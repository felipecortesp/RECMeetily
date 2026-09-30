//! Automatic finalizer for forgotten recordings.
//!
//! A recording is stopped on its own when either
//!   * no speech was transcribed (from the mic or the system track) for
//!     `auto_stop_idle_minutes`, or
//!   * it has been running for `auto_stop_max_hours` (safety cap, 0 = off).
//!
//! Audio level is deliberately NOT used: background noise would keep a
//! forgotten recording alive forever. Only non-empty transcript segments count.
//!
//! When a limit is reached the user gets a 120 s warning
//! (`recording-auto-stop-warning` event, a macOS notification and an in-app
//! toast with a "Keep recording" button). If nothing cancels it, the canonical
//! stop path runs (the same one the Stop button uses), with
//! `stop_reason` recorded in `metadata.json`.
//!
//! The decision logic lives in the pure [`AutoStopMachine`] (time is passed in
//! as seconds), so it is unit-tested without a clock. [`spawn_watchdog`] drives
//! it from a 5 s tokio tick.
//!
//! Testing in the real app: `auto_stop_idle_minutes` is normally clamped to
//! 5-60. Launching with `RECMEETILY_AUTOSTOP_TEST=1` lowers the minimum to 1.

use log::{info, warn};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Runtime};
use tauri_plugin_notification::NotificationExt;
use tokio::task::JoinHandle;

/// Seconds between the warning and the automatic stop.
pub const WARNING_SECS: f64 = 120.0;
/// How much "Keep recording" extends the max-duration cap.
pub const KEEP_ALIVE_EXTENSION_SECS: f64 = 3600.0;
const TICK: Duration = Duration::from_secs(5);

pub const DEFAULT_IDLE_MINUTES: u32 = 15;
pub const DEFAULT_MAX_HOURS: u32 = 8;

/// Clamp the idle limit. `allow_short` (test env var) lowers the minimum to 1.
pub fn clamp_idle_minutes_with(minutes: u32, allow_short: bool) -> u32 {
    minutes.clamp(if allow_short { 1 } else { 5 }, 60)
}

/// Idle limit clamped to 5-60 (1-60 when `RECMEETILY_AUTOSTOP_TEST=1`).
pub fn clamp_idle_minutes(minutes: u32) -> u32 {
    let allow_short = std::env::var("RECMEETILY_AUTOSTOP_TEST").map_or(false, |v| v == "1");
    clamp_idle_minutes_with(minutes, allow_short)
}

/// Max hours: 0 (off) or 2-12.
pub fn clamp_max_hours(hours: u32) -> u32 {
    if hours == 0 {
        0
    } else {
        hours.clamp(2, 12)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutoStopSettings {
    pub enabled: bool,
    pub idle_minutes: u32,
    /// 0 = no cap.
    pub max_hours: u32,
}

impl Default for AutoStopSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            idle_minutes: DEFAULT_IDLE_MINUTES,
            max_hours: DEFAULT_MAX_HOURS,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    Inactivity,
    MaxDuration,
}

impl StopReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            StopReason::Inactivity => "inactivity",
            StopReason::MaxDuration => "max_duration",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Decision {
    Idle,
    Warn { reason: StopReason, seconds_remaining: f64 },
    Stop { reason: StopReason },
}

/// Pure state machine. All times are seconds on any monotonic clock.
#[derive(Debug, Clone)]
pub struct AutoStopMachine {
    started: f64,
    paused_total: f64,
    paused_since: Option<f64>,
    /// Active (unpaused) time at which the idle clock last restarted.
    idle_ref: f64,
    /// Extra seconds granted to the max-duration cap by "Keep recording".
    extension: f64,
    /// Active time at which the current warning began.
    warning: Option<(StopReason, f64)>,
}

impl AutoStopMachine {
    pub fn new(start: f64) -> Self {
        Self {
            started: start,
            paused_total: 0.0,
            paused_since: None,
            idle_ref: 0.0,
            extension: 0.0,
            warning: None,
        }
    }

    /// Recording time excluding pauses.
    fn active(&self, now: f64) -> f64 {
        let paused_now = self.paused_since.map_or(0.0, |p| (now - p).max(0.0));
        (now - self.started - self.paused_total - paused_now).max(0.0)
    }

    /// New transcribed speech: restarts the idle clock and cancels an
    /// inactivity warning (a max-duration warning needs "Keep recording").
    pub fn on_speech(&mut self, now: f64) {
        self.idle_ref = self.active(now);
        if matches!(self.warning, Some((StopReason::Inactivity, _))) {
            self.warning = None;
        }
    }

    /// The user pressed "Keep recording".
    pub fn on_keep_alive(&mut self, now: f64, settings: &AutoStopSettings) {
        let active = self.active(now);
        self.idle_ref = active;
        self.warning = None;
        if settings.max_hours > 0 {
            let cap = settings.max_hours as f64 * 3600.0 + self.extension;
            if active >= cap {
                // One more hour from now.
                self.extension = active - settings.max_hours as f64 * 3600.0
                    + KEEP_ALIVE_EXTENSION_SECS;
            }
        }
    }

    pub fn on_pause(&mut self, now: f64) {
        if self.paused_since.is_none() {
            self.paused_since = Some(now);
            // The user is clearly present.
            self.warning = None;
        }
    }

    /// Resuming counts as activity: the idle clock restarts.
    pub fn on_resume(&mut self, now: f64) {
        if let Some(p) = self.paused_since.take() {
            self.paused_total += (now - p).max(0.0);
        }
        self.idle_ref = self.active(now);
    }

    pub fn tick(&mut self, now: f64, settings: &AutoStopSettings) -> Decision {
        if !settings.enabled {
            self.warning = None;
            return Decision::Idle;
        }
        if self.paused_since.is_some() {
            return Decision::Idle;
        }
        let active = self.active(now);

        if let Some((reason, since)) = self.warning {
            let waited = active - since;
            return if waited >= WARNING_SECS {
                Decision::Stop { reason }
            } else {
                Decision::Warn { reason, seconds_remaining: WARNING_SECS - waited }
            };
        }

        let max_reached = settings.max_hours > 0
            && active >= settings.max_hours as f64 * 3600.0 + self.extension;
        let idle_reached = active - self.idle_ref >= settings.idle_minutes as f64 * 60.0;
        let reason = if max_reached {
            StopReason::MaxDuration
        } else if idle_reached {
            StopReason::Inactivity
        } else {
            return Decision::Idle;
        };
        self.warning = Some((reason, active));
        Decision::Warn { reason, seconds_remaining: WARNING_SECS }
    }
}

// ---------------------------------------------------------------------------
// Runtime glue
// ---------------------------------------------------------------------------

static ENABLED: AtomicBool = AtomicBool::new(true);
static IDLE_MINUTES: AtomicU32 = AtomicU32::new(DEFAULT_IDLE_MINUTES);
static MAX_HOURS: AtomicU32 = AtomicU32::new(DEFAULT_MAX_HOURS);
/// Bumped for every non-empty transcript segment.
static SPEECH_COUNTER: AtomicU64 = AtomicU64::new(0);
/// Bumped by `keep_recording_alive`.
static KEEP_ALIVE_COUNTER: AtomicU64 = AtomicU64::new(0);
static WATCHDOG: std::sync::Mutex<Option<JoinHandle<()>>> = std::sync::Mutex::new(None);
static STOP_REASON: std::sync::Mutex<Option<StopReason>> = std::sync::Mutex::new(None);

/// Publish the saved preferences to the running watchdog.
pub fn set_runtime_settings(settings: AutoStopSettings) {
    ENABLED.store(settings.enabled, Ordering::Relaxed);
    IDLE_MINUTES.store(clamp_idle_minutes(settings.idle_minutes), Ordering::Relaxed);
    MAX_HOURS.store(clamp_max_hours(settings.max_hours), Ordering::Relaxed);
}

fn runtime_settings() -> AutoStopSettings {
    AutoStopSettings {
        enabled: ENABLED.load(Ordering::Relaxed),
        idle_minutes: IDLE_MINUTES.load(Ordering::Relaxed),
        max_hours: MAX_HOURS.load(Ordering::Relaxed),
    }
}

/// Call for every transcript segment; empty text is ignored.
pub fn note_transcript_text(text: &str) {
    if !text.trim().is_empty() {
        SPEECH_COUNTER.fetch_add(1, Ordering::Relaxed);
    }
}

/// Reason of the auto-stop currently in progress, consumed by the saver.
pub fn take_stop_reason() -> Option<&'static str> {
    STOP_REASON.lock().ok().and_then(|mut r| r.take()).map(|r| r.as_str())
}

/// The user chose to keep the recording going after an auto-stop warning.
#[tauri::command]
pub async fn keep_recording_alive() -> Result<(), String> {
    if !super::recording_commands::is_recording_active() {
        return Err("No recording is currently active".to_string());
    }
    KEEP_ALIVE_COUNTER.fetch_add(1, Ordering::SeqCst);
    info!("Auto-stop: user chose to keep recording");
    Ok(())
}

/// Abort the watchdog (called when recording stops).
pub fn stop_watchdog() {
    if let Ok(mut guard) = WATCHDOG.lock() {
        if let Some(handle) = guard.take() {
            handle.abort();
        }
    }
}

/// Start the 5 s watchdog for a new recording, replacing any previous one.
pub fn spawn_watchdog<R: Runtime>(app: AppHandle<R>) {
    stop_watchdog();
    if let Ok(mut reason) = STOP_REASON.lock() {
        *reason = None;
    }
    let handle = tokio::spawn(async move {
        let clock = Instant::now();
        let mut machine = AutoStopMachine::new(0.0);
        let mut seen_speech = SPEECH_COUNTER.load(Ordering::Relaxed);
        let mut seen_keep_alive = KEEP_ALIVE_COUNTER.load(Ordering::SeqCst);
        let mut was_paused = false;
        let mut warned = false;
        let mut interval = tokio::time::interval(TICK);
        loop {
            interval.tick().await;
            let now = clock.elapsed().as_secs_f64();
            let settings = runtime_settings();

            let paused = super::recording_commands::is_paused_now();
            if paused && !was_paused {
                machine.on_pause(now);
            } else if !paused && was_paused {
                machine.on_resume(now);
            }
            was_paused = paused;

            let speech = SPEECH_COUNTER.load(Ordering::Relaxed);
            if speech != seen_speech {
                seen_speech = speech;
                machine.on_speech(now);
            }
            let keep_alive = KEEP_ALIVE_COUNTER.load(Ordering::SeqCst);
            if keep_alive != seen_keep_alive {
                seen_keep_alive = keep_alive;
                machine.on_keep_alive(now, &settings);
            }

            match machine.tick(now, &settings) {
                Decision::Idle => {
                    if warned {
                        warned = false;
                        let _ = app.emit("recording-auto-stop-cancelled", serde_json::json!({}));
                    }
                }
                Decision::Warn { reason, seconds_remaining } => {
                    if !warned {
                        warned = true;
                        announce_warning(&app, reason, seconds_remaining, &settings);
                    }
                }
                Decision::Stop { reason } => {
                    info!("Auto-stop: stopping recording ({})", reason.as_str());
                    if let Ok(mut r) = STOP_REASON.lock() {
                        *r = Some(reason);
                    }
                    // Run the stop on its own task: the stop path aborts this
                    // watchdog and must not cancel itself midway.
                    let app = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let result = super::recording_commands::stop_recording(
                            app.clone(),
                            super::recording_commands::RecordingArgs { save_path: String::new() },
                        )
                        .await;
                        if let Err(e) = result {
                            warn!("Auto-stop: stop failed: {}", e);
                        }
                    });
                    return;
                }
            }
        }
    });
    if let Ok(mut guard) = WATCHDOG.lock() {
        *guard = Some(handle);
    }
}

fn announce_warning<R: Runtime>(
    app: &AppHandle<R>,
    reason: StopReason,
    seconds_remaining: f64,
    settings: &AutoStopSettings,
) {
    let body = match reason {
        StopReason::Inactivity => format!(
            "No speech for {} minutes. The recording will stop in 2 minutes unless you keep it going.",
            settings.idle_minutes
        ),
        StopReason::MaxDuration => format!(
            "The recording reached {} hours. It will stop in 2 minutes unless you keep it going.",
            settings.max_hours
        ),
    };
    info!("Auto-stop warning ({}): {}", reason.as_str(), body);
    let _ = app.emit(
        "recording-auto-stop-warning",
        serde_json::json!({
            "reason": reason.as_str(),
            "seconds_remaining": seconds_remaining,
            "message": body,
        }),
    );
    if let Err(e) = app
        .notification()
        .builder()
        .title("RECMeetily: recording will stop")
        .body(&body)
        .show()
    {
        warn!("Auto-stop: could not show notification: {}", e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s() -> AutoStopSettings {
        AutoStopSettings { enabled: true, idle_minutes: 15, max_hours: 8 }
    }
    const IDLE: f64 = 15.0 * 60.0;

    #[test]
    fn no_speech_warns_at_limit_and_stops_120s_later() {
        let mut m = AutoStopMachine::new(0.0);
        assert_eq!(m.tick(IDLE - 1.0, &s()), Decision::Idle);
        assert_eq!(
            m.tick(IDLE, &s()),
            Decision::Warn { reason: StopReason::Inactivity, seconds_remaining: 120.0 }
        );
        assert!(matches!(m.tick(IDLE + 60.0, &s()), Decision::Warn { .. }));
        assert_eq!(
            m.tick(IDLE + 120.0, &s()),
            Decision::Stop { reason: StopReason::Inactivity }
        );
    }

    #[test]
    fn speech_resets_idle_clock() {
        let mut m = AutoStopMachine::new(0.0);
        m.on_speech(600.0);
        assert_eq!(m.tick(IDLE, &s()), Decision::Idle);
        assert!(matches!(m.tick(600.0 + IDLE, &s()), Decision::Warn { .. }));
    }

    #[test]
    fn speech_during_warning_cancels_it() {
        let mut m = AutoStopMachine::new(0.0);
        assert!(matches!(m.tick(IDLE, &s()), Decision::Warn { .. }));
        m.on_speech(IDLE + 30.0);
        assert_eq!(m.tick(IDLE + 31.0, &s()), Decision::Idle);
        assert_eq!(m.tick(IDLE + 200.0, &s()), Decision::Idle);
        assert!(matches!(m.tick(IDLE + 30.0 + IDLE, &s()), Decision::Warn { .. }));
    }

    #[test]
    fn keep_alive_cancels_and_resets() {
        let mut m = AutoStopMachine::new(0.0);
        assert!(matches!(m.tick(IDLE, &s()), Decision::Warn { .. }));
        m.on_keep_alive(IDLE + 10.0, &s());
        assert_eq!(m.tick(IDLE + 11.0, &s()), Decision::Idle);
        assert_eq!(m.tick(IDLE + 130.0, &s()), Decision::Idle);
        assert!(matches!(m.tick(IDLE + 10.0 + IDLE, &s()), Decision::Warn { .. }));
    }

    #[test]
    fn paused_time_is_not_counted() {
        let mut m = AutoStopMachine::new(0.0);
        m.on_pause(300.0);
        assert_eq!(m.tick(10_000.0, &s()), Decision::Idle);
        m.on_resume(10_000.0);
        // Resume restarts the idle clock.
        assert_eq!(m.tick(10_000.0 + IDLE - 1.0, &s()), Decision::Idle);
        assert!(matches!(m.tick(10_000.0 + IDLE, &s()), Decision::Warn { .. }));
    }

    #[test]
    fn pause_cancels_a_pending_warning() {
        let mut m = AutoStopMachine::new(0.0);
        assert!(matches!(m.tick(IDLE, &s()), Decision::Warn { .. }));
        m.on_pause(IDLE + 10.0);
        assert_eq!(m.tick(IDLE + 500.0, &s()), Decision::Idle);
        m.on_resume(IDLE + 500.0);
        assert_eq!(m.tick(IDLE + 501.0, &s()), Decision::Idle);
    }

    #[test]
    fn max_duration_warns_and_stops() {
        let mut m = AutoStopMachine::new(0.0);
        let max = 8.0 * 3600.0;
        // Regular speech keeps the idle clock fresh.
        m.on_speech(max - 10.0);
        assert_eq!(
            m.tick(max, &s()),
            Decision::Warn { reason: StopReason::MaxDuration, seconds_remaining: 120.0 }
        );
        // Speech does not cancel a max-duration warning.
        m.on_speech(max + 5.0);
        assert!(matches!(m.tick(max + 6.0, &s()), Decision::Warn { .. }));
        assert_eq!(
            m.tick(max + 120.0, &s()),
            Decision::Stop { reason: StopReason::MaxDuration }
        );
    }

    #[test]
    fn keep_alive_extends_max_duration_by_one_hour() {
        let mut m = AutoStopMachine::new(0.0);
        let max = 8.0 * 3600.0;
        m.on_speech(max - 1.0);
        assert!(matches!(m.tick(max, &s()), Decision::Warn { .. }));
        m.on_keep_alive(max + 10.0, &s());
        m.on_speech(max + 3000.0);
        assert_eq!(m.tick(max + 3000.0, &s()), Decision::Idle);
        m.on_speech(max + 3600.0 + 5.0 - 1.0);
        assert!(matches!(
            m.tick(max + 10.0 + 3600.0, &s()),
            Decision::Warn { reason: StopReason::MaxDuration, .. }
        ));
    }

    #[test]
    fn max_duration_excludes_paused_time() {
        let mut m = AutoStopMachine::new(0.0);
        let max = 8.0 * 3600.0;
        m.on_pause(100.0);
        m.on_resume(100.0 + 3600.0);
        m.on_speech(max + 3500.0);
        assert_eq!(m.tick(max + 3500.0, &s()), Decision::Idle);
        m.on_speech(max + 3600.0);
        assert!(matches!(m.tick(max + 3600.0, &s()), Decision::Warn { .. }));
    }

    #[test]
    fn max_hours_zero_disables_cap() {
        let mut m = AutoStopMachine::new(0.0);
        let settings = AutoStopSettings { max_hours: 0, ..s() };
        m.on_speech(40_000.0);
        assert_eq!(m.tick(40_100.0, &settings), Decision::Idle);
    }

    #[test]
    fn disabled_never_fires() {
        let mut m = AutoStopMachine::new(0.0);
        let settings = AutoStopSettings { enabled: false, ..s() };
        assert_eq!(m.tick(100_000.0, &settings), Decision::Idle);
    }

    #[test]
    fn settings_are_clamped() {
        assert_eq!(clamp_idle_minutes_with(1, false), 5);
        assert_eq!(clamp_idle_minutes_with(15, false), 15);
        assert_eq!(clamp_idle_minutes_with(999, false), 60);
        assert_eq!(clamp_idle_minutes_with(1, true), 1);
        assert_eq!(clamp_idle_minutes_with(0, true), 1);
        assert_eq!(clamp_max_hours(0), 0);
        assert_eq!(clamp_max_hours(1), 2);
        assert_eq!(clamp_max_hours(8), 8);
        assert_eq!(clamp_max_hours(99), 12);
    }
}
