//! Bridge to FluidAudio's offline diarizer (pyannote community-1 + VBx) through the
//! `RecDiarizer` Swift static library (see `swift/RecDiarizer`).
//!
//! The Swift side never downloads anything: it only loads the CoreML files that
//! already exist in `models_dir`.

use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::ffi::{c_char, CString};
use std::path::Path;

use super::DiarizationSegment;

extern "C" {
    fn rec_diarize(
        samples: *const f32,
        count: usize,
        num_speakers: i32,
        min_speakers: i32,
        max_speakers: i32,
        models_dir: *const c_char,
        out_json: *mut *mut c_char,
    ) -> i32;
    fn rec_diarize_free(ptr: *mut c_char);
}

/// Model files/directories the offline diarizer needs inside the models folder.
pub const REQUIRED_FILES: &[&str] = &[
    "Segmentation.mlmodelc",
    "FBank.mlmodelc",
    "Embedding.mlmodelc",
    "PldaRho.mlmodelc",
    "plda-parameters.json",
];

pub fn models_present(dir: &Path) -> bool {
    REQUIRED_FILES.iter().all(|f| dir.join(f).exists())
}

/// Speaker-count hints; `None` leaves the value to the clusterer.
#[derive(Debug, Clone, Copy, Default)]
pub struct SpeakerCount {
    pub exact: Option<usize>,
    pub min: Option<usize>,
    pub max: Option<usize>,
}

fn hint(v: Option<usize>) -> i32 {
    v.map_or(0, |n| n.clamp(1, i32::MAX as usize) as i32)
}

/// Diarize 16 kHz mono samples. Blocks until done (a few seconds per hour of audio).
pub fn diarize_samples(
    samples: &[f32],
    count: SpeakerCount,
    models_dir: &Path,
) -> Result<Vec<DiarizationSegment>> {
    let dir = CString::new(models_dir.to_string_lossy().as_bytes())
        .context("models dir contains a NUL byte")?;
    let mut out: *mut c_char = std::ptr::null_mut();
    // SAFETY: pointers are valid for the call; the returned string is copied and
    // released with `rec_diarize_free` below.
    let code = unsafe {
        rec_diarize(
            samples.as_ptr(),
            samples.len(),
            hint(count.exact),
            hint(count.min),
            hint(count.max),
            dir.as_ptr(),
            &mut out,
        )
    };
    let json = if out.is_null() {
        None
    } else {
        // SAFETY: `out` is a NUL-terminated string allocated by the Swift side.
        let s = unsafe { std::ffi::CStr::from_ptr(out) }
            .to_string_lossy()
            .into_owned();
        unsafe { rec_diarize_free(out) };
        Some(s)
    };

    let detail = || {
        json.as_deref()
            .and_then(|j| parse_output(j).err())
            .map(|e| e.to_string())
            .unwrap_or_default()
    };
    match code {
        0 => parse_output(json.as_deref().ok_or_else(|| anyhow!("empty FluidAudio output"))?),
        1 => bail!("FluidAudio: invalid arguments ({})", detail()),
        2 => bail!("FluidAudio models missing in {} ({})", models_dir.display(), detail()),
        3 => bail!("FluidAudio diarization failed: {}", detail()),
        c => bail!("FluidAudio returned unexpected code {c}: {}", detail()),
    }
}

#[derive(Deserialize)]
struct RawOutput {
    #[serde(default)]
    segments: Vec<RawSegment>,
    error: Option<String>,
}

#[derive(Deserialize)]
struct RawSegment {
    speaker: String,
    start: f32,
    end: f32,
}

fn parse_output(json: &str) -> Result<Vec<DiarizationSegment>> {
    let raw: RawOutput = serde_json::from_str(json).context("invalid FluidAudio JSON")?;
    if let Some(e) = raw.error {
        bail!("{e}");
    }
    let mut segs: Vec<RawSegment> = raw.segments.into_iter().filter(|s| s.end > s.start).collect();
    segs.sort_by(|a, b| a.start.total_cmp(&b.start));

    // Number speakers by first appearance so labels are stable across runs.
    let mut ids: HashMap<String, usize> = HashMap::new();
    Ok(segs
        .into_iter()
        .map(|s| {
            let next = ids.len();
            let speaker = *ids.entry(s.speaker).or_insert(next);
            DiarizationSegment { start: s.start, end: s.end, speaker, overlapped: false }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_orders_and_maps_speakers() {
        let json = r#"{"segments":[
            {"speaker":"S2","start":5.0,"end":6.0},
            {"speaker":"S7","start":1.0,"end":2.0},
            {"speaker":"S2","start":3.0,"end":4.0}]}"#;
        let segs = parse_output(json).unwrap();
        let got: Vec<(f32, usize)> = segs.iter().map(|s| (s.start, s.speaker)).collect();
        assert_eq!(got, vec![(1.0, 0), (3.0, 1), (5.0, 1)]);
        assert!(segs.iter().all(|s| !s.overlapped));
    }

    #[test]
    fn parse_drops_empty_segments() {
        let json = r#"{"segments":[{"speaker":"S1","start":2.0,"end":2.0},
            {"speaker":"S1","start":3.0,"end":2.5},{"speaker":"S1","start":4.0,"end":5.0}]}"#;
        assert_eq!(parse_output(json).unwrap().len(), 1);
    }

    #[test]
    fn parse_error_json() {
        let err = parse_output(r#"{"error":"boom"}"#).unwrap_err();
        assert_eq!(err.to_string(), "boom");
    }

    #[test]
    fn parse_empty_and_invalid() {
        assert!(parse_output(r#"{"segments":[]}"#).unwrap().is_empty());
        assert!(parse_output("not json").is_err());
    }

    #[test]
    fn models_present_false_for_empty_dir() {
        assert!(!models_present(Path::new("/nonexistent-models-dir")));
    }

    /// Runs the real FluidAudio pipeline. Skipped unless `FLUID_WAV` and
    /// `FLUID_MODELS` are set (optional `FLUID_SPEAKERS`).
    #[test]
    fn fluid_diarize_sample() {
        let (wav, models) = match (std::env::var("FLUID_WAV"), std::env::var("FLUID_MODELS")) {
            (Ok(w), Ok(m)) => (w, m),
            _ => {
                eprintln!("skipping: set FLUID_WAV and FLUID_MODELS");
                return;
            }
        };
        let (samples, sr) = super::super::dsp::read_wav(Path::new(&wav)).expect("read wav");
        let samples = if sr != super::super::dsp::SAMPLE_RATE {
            crate::audio::audio_processing::resample_audio(&samples, sr, super::super::dsp::SAMPLE_RATE)
        } else {
            samples
        };
        let count = SpeakerCount {
            exact: std::env::var("FLUID_SPEAKERS").ok().and_then(|v| v.parse().ok()),
            ..Default::default()
        };

        let started = std::time::Instant::now();
        let segs = diarize_samples(&samples, count, Path::new(&models)).expect("diarize");
        let elapsed = started.elapsed();

        let mut talk: std::collections::BTreeMap<usize, f32> = Default::default();
        for s in &segs {
            *talk.entry(s.speaker).or_default() += s.end - s.start;
        }
        println!("\n=== FLUIDAUDIO DIARIZATION ===");
        println!("audio: {:.1}s, segments: {}", samples.len() as f32 / 16_000.0, segs.len());
        for (spk, secs) in &talk {
            println!("speaker {spk}: {secs:.1}s");
        }
        println!("elapsed: {:.2}s", elapsed.as_secs_f32());

        if let Ok(path) = std::env::var("FLUID_RTTM") {
            let rttm: String = segs
                .iter()
                .map(|s| {
                    format!(
                        "SPEAKER system 1 {:.3} {:.3} <NA> <NA> S{} <NA> <NA>\n",
                        s.start,
                        s.end - s.start,
                        s.speaker + 1
                    )
                })
                .collect();
            std::fs::write(&path, rttm).expect("write rttm");
        }
    }
}
