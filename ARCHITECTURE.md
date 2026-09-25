# Architecture Notes

Practical notes on how this app fits together, aimed at someone (or some model)
opening the repo with no prior context. It deliberately focuses on the things
that are **not** obvious from reading the code — the traps, the "why is it like
this", and the places where a reasonable-looking change silently does nothing.

For macOS build commands see [.github/workflows/build-macos.yml](.github/workflows/build-macos.yml).

---

## 1. Shape of the app

A Tauri 2 desktop app. Rust owns audio, transcription, storage and AI
orchestration; a Next.js 14 frontend renders the UI inside the webview. There is
no server — everything runs on the user's machine.

```
Next.js UI  ──invoke()──▶  Tauri commands (Rust)
     ▲                            │
     └──────── events ────────────┘      e.g. transcript-update,
                                          recording-audio-levels
```

The `backend/` directory in the upstream project (Python/FastAPI) is **not used**
and has been removed from this fork.

---

## 2. Audio pipeline

VAD, Parakeet, and diarization use ONNX Runtime on macOS, initialized by the
Core Audio capture backend in `src-tauri/src/audio/capture/core_audio.rs`. The
global process tap for system audio depends on macOS 14.2+ APIs.
`src-tauri/src/diarization/download.rs` verifies exact model hashes and stages
them for both packaging and plain cargo builds. Bundled diarization models are
stored in `resources/diarization/`; macOS must include them.
Record must initialize both source VADs before starting any saver or exposing a
chunk sender. Runtime failures return a distinct startup error, while recording
destination failures remain storage errors. All three source tracks, mute
alignment, and the 800 ms/2,000 ms live/offline policies still apply.

Capture, recording, and transcription deliberately split into parallel paths in
`src-tauri/src/audio/`:

```
mic ─▶ normalize/limit ─┬─▶ mic VAD ─────▶ transcription worker
                       ├─▶ mic.mp4
                       └─┐
                         ├─▶ professional mix ─▶ audio.mp4 (playback)
sys ───────────────────┬─┘
                      ├─▶ system VAD ───▶ transcription worker
                      └─▶ system.mp4
```

### The mixed-audio trap ⚠️

Current live transcription receives **separate mic and system VAD segments**.
Mixing is only for `audio.mp4`, the user-facing playback track. Do not collapse
live STT back onto that mixed track: overlapping system speech can mask the
local user even when `mic.mp4` clearly contains their voice.

The old mixed-chunk `device_type = Microphone` placeholder can still appear in
legacy/import paths and must never be interpreted as speaker identity. Current
live source chunks preserve their actual capture source; final identity
refinement still comes from diarization (§4).

### Live audio levels

`pipeline.rs` computes per-source RMS/peak *before* mixing and emits them as
`recording-audio-levels` events (~25/sec per source), which drive the meters in
`RecordingControls`.

Mic and system speech use separate VAD instances. The mic path is intentionally
more permissive (`0.20/0.10`) than hot digital loopback (`0.50/0.35`). These mic
values were calibrated against the retained Logitech G733 track from the
"Vehicle Trade-In Discussion" failure: `0.42/0.30` and `0.30/0.20` found zero
speech, while `0.20/0.10` recovered 8 plausible segments / 16.9 seconds. Before
VAD, mic loudness normalization targets -20 LUFS, limits automatic gain to
0.5x-2.0x, smooths gain changes, and applies the user's gain before one final
-1 dB limiter. Do not move user gain after that limiter or restore unbounded
normalization; retained mic tracks showed hard 0 dBFS clipping under that design.

Microphone and system gain are separate persisted Rust-owned values. System
gain is applied once before source meters, system VAD/transcription, retained
`system.mp4`, and playback mixing. Over-range chunks are attenuated to a -1 dBFS
peak while preserving waveform shape, sample count, and source timing. Repeated
limiter activity is included in `recording-audio-levels` so both recording
windows can warn that the system gain or playback volume is too high. Do not
reimplement either gain only in the webview or in the final mixer.

Note: the webview **cannot** capture system audio itself, so browser-side
`getUserMedia` visualizers can only ever show the microphone. That is why the
levels come from Rust.

### Source-specific mute

Live microphone and system-audio mute are independent of each other and Pause.
Rust owns both atomic mute states, and the main window and separate minibar
recover them through `get_recording_state` plus their source-specific change
broadcasts. Capture streams stay open; muted-source samples are replaced with
zeros before levels, VAD, retained source tracks, and the mixer. Normal-sized
silent chunks must keep flowing so the other source remains live and
`mic.mp4`, `system.mp4`, and `audio.mp4` retain the same timeline. Never
implement either mute by dropping callbacks, stopping a device, or only hiding
the frontend meter.

### Live segmentation and shutdown invariants

- Live VAD redemption is 800 ms. Offline retranscription uses 2,000 ms because
  it has no live-latency constraint.
- Some capture backends suppress callbacks throughout exact-zero silence. Live
  inactivity is tracked per source; after 800 ms Rust force-finalizes only that
  source's active utterance without synthesizing audio. It then replaces the
  Silero session to release internal history and offsets the fresh session's
  timestamps onto the continuous meeting timeline. Do not use wall-time-sized
  silence buffers or one shared mic/system activity clock.
- Every window, including exact zero-filled padding, must handle the segments
  returned by `ContinuousVadProcessor`. Speech-end commonly arrives during the
  trailing silent window; discarding that return drops the whole utterance.
- Whisper's current `confidence` is text-length-derived, not a token
  probability. It is diagnostic and must not filter short valid phrases.
- During shutdown `RecordingManager` is temporarily outside its global slot.
  The transcript listener owns a cloned shared segment handle so final events
  remain writable until the worker finishes.
- Timed-out workers are aborted and awaited before listener removal and model
  unload. Dropping a Tokio `JoinHandle` only detaches it.
- Transcript-only sessions still write their final segment vector even when
  audio auto-save is disabled.

### Compact recording bar ownership

The floating bar is a separate `minibar` webview, so it cannot share React
recording state with `main`. Rust owns its lifecycle in `src-tauri/src/minibar.rs`:

- `MINIBAR_LIFECYCLE` serializes create/hide/destroy transitions because Windows can
  report one minimize through more than one observer.
- A new bar is built hidden; Rust hides `main` before showing the bar so both
  control surfaces never flash onscreen together.
- `recording_commands::stop_recording_inner` is the only native stop owner and
  the only source of `recording-stop-complete`. A minibar-origin Stop hides the
  bar immediately after claiming `IS_STOPPING`, then Rust destroys it after the
  command returns so Tauri can safely send the IPC response. Tray and frontend
  code must not emit a second completion event.
- Only Stop originating in the minibar restores the main window it hid. A tray
  or already-visible main-window stop must not steal focus.
- The minibar polls Rust `RecordingState.active_duration` every 500 ms rather
  than incrementing a webview-local timer. This keeps it aligned across startup
  delay, duplicate minimize events, and pauses. `recording_duration` is only a
  compatibility fallback.

Do not restore cross-webview stop-request events, elapsed-time reseeding, or a
frontend close fallback. Those mechanisms caused zombie bars, frozen
`Finishing` states, timer resets, and duplicate post-processing.

Dragging uses one bubbling mouse handler that excludes buttons and their children,
plus a minibar-only `core:window:allow-start-dragging` capability. Container-only
`data-tauri-drag-region` attributes miss child targets in Tauri's native handler.

### Windows runtime ownership

The workspace pins a vendored `tauri-runtime-wry` 2.11.4 through the root
`Cargo.toml`. A v0.2.12 long-recording crash matched the upstream Windows
`Rc<EventLoopRunner>` clone/drop race: ordinary background IPC traffic can
corrupt the event-loop reference count. This is not fixed by slowing the
minibar timer, changing transcription models, or replacing Tao's `Rc` with
an `Arc` while ignoring thread-affine destruction.

The Windows patch keeps the strong target owner inside the non-Send `Wry`
runtime; cloned contexts hold private atomic weak references. Only UI-thread
dispatch upgrades them, monitor queries are routed to that thread, and the
owner expires before native-loop destruction, including panic unwinding.
Windows display handles borrow no data. Non-Windows behavior is unchanged.
Windows runtime `tracing` is deliberately rejected because its separate
`ActiveTraceSpanStore` still contains another cross-thread `Rc`.

See `vendor/tauri-runtime-wry/PATCH.md` for provenance, regression commands,
scope, and removal criteria. Do not remove this override on a version bump
without verifying an equivalent upstream ownership fix. Short native stress
tests do not establish multi-hour recording/minibar stability. For v0.2.14 the
reporting user approved release after a successful patched-build trial so far;
its duration is unconfirmed and no completed multi-hour soak is claimed.

---

## 3. Where things are stored

Core writable data is under `~/Library/Application Support/RECMeetily`;
Tauri plugin stores use the app identifier directory `~/Library/Application Support/dev.felipecortes.recmeetily`.
Both are outside the signed `.app`, because writing inside that bundle invalidates its
signature.

`src-tauri/src/paths.rs` is the single source of truth:

| What | Where |
|---|---|
| Database, templates, models, file-backed settings | `~/Library/Application Support/RECMeetily/…` |
| Tauri plugin stores (recording preferences/onboarding) | `~/Library/Application Support/dev.felipecortes.recmeetily/…` |
| Bundled diarization models | The app bundle's `resources/diarization/` directory |
| **Audio recordings** | `~/Movies/recmeetily-recordings/<meeting>` (configurable) |

Two gotchas:

1. **Recordings are the exception.** They follow the user's configured
   recordings folder (`audio/recording_preferences.rs`), *not* the data root,
   and they are **`.mp4`, not `.wav`**. New recordings retain aligned
   `audio.mp4` (mixed playback), `mic.mp4` (local user), and `system.mp4`
   (remote/computer audio). Code that looks for playback must prefer
   `audio.mp4` — see `diarization::find_meeting_audio`.
2. RECMeetily always uses the OS data directory on macOS as specified in `paths.rs`.

Recording-folder creation is intentionally collision-safe. Sanitized titles are
not unique, so `audio_processing.rs` creates the directory atomically and adds a
suffix if it already exists. The configured custom recordings root must flow
through `RecordingManager` into every saver; falling back to the default root at
save time makes Settings appear to work while files go elsewhere. The short-take
discard command canonicalizes paths, rejects every allowed root itself, and only
deletes descendants of the configured, default, or legacy portable roots. Keep
the equality rejection separate from descendant checks because roots may be
nested.

The native folder picker persists a destination only after it can be created and
written. On macOS, validation must reject the `.app` bundle and every descendant
before creating anything so a writable user-installed application cannot
invalidate its signature.

Required mixed-audio failures are not allowed to fail silently. FFmpeg process
failures, timeouts, and missing mixed outputs propagate through
`RecordingManager` into the `recording-stop-complete.audio_save_error` payload.
The frontend keeps the transcript flow running and shows a ten-second error
toast. Empty or failed optional mic/system tracks are currently logged and
omitted because either source may remain silent for a whole meeting. Concat
demuxer list entries use `ffconcat_file_line`; raw absolute paths are unsafe
because an apostrophe in a custom root or user name terminates FFmpeg's quoted
path.

A one-time migration (`paths::migrate_legacy_data`) copies data from a previous
Meetily installation on first run, leaving the original untouched so upgrading users keep their history.

### Crash reports

`src-tauri/src/crash_report.rs` writes a small session marker under
`<data>/crash-reports`. A marker left behind after process termination becomes a
pending unexpected-exit report on the next launch; the panic hook also writes a
source-relative location and anonymous fingerprint. Clean shutdown removes only the current session's
marker. This detects abrupt exits without trying to allocate a ZIP inside a
failing process. If failures repeat before the prompt is resolved, the newest
failure replaces the older pending report.

After onboarding is resolved, `app/layout.tsx` checks for that pending report
before mounting the application provider tree. This prevents service probes,
meeting loads, and transcript/audio recovery from running behind the prompt.
The dialog has exactly **Send Report**, **Save
ZIP**, and **Ignore**. Send Report creates the same local ZIP and opens a
prefilled public GitHub issue for manual attachment because the project has no
report-upload server; nothing is uploaded automatically.

The in-app ZIP is deliberately allowlist-only: crash classification/time,
version/backend, OS family and major version, architecture, bucketed CPU core
count, rounded memory size, source-relative panic file/line details, and a
panic-location fingerprint. It
never includes ordinary logs, recordings, checkpoints, transcript/summary data,
meeting identity, SQLite/WebView/settings storage, credentials, usernames,
hostnames, or device names. Do not replace it with the much broader manual
`scripts/collect-meetily-crash-diagnostics.bat` support bundle.

---

## 4. Speaker diarization ("who spoke when")

Implemented from scratch on the ONNX Runtime already in the build (`ort`),
deliberately **not** by linking sherpa-onnx — that would pull in a second
onnxruntime and risk duplicate-symbol failures at link time.

`src-tauri/src/diarization/`:

| File | Role |
|---|---|
| `dsp.rs` | WAV reader + Kaldi-compatible 80-dim log-mel fbank (Povey window, pre-emphasis 0.97, HTK mel, per-utterance CMN) |
| `models.rs` | pyannote `segmentation-3.0` (7-class powerset) + WeSpeaker ResNet34 embeddings + VBx LDA transform; includes a minimal `.npz`/`.npy` reader |
| `clustering.rs` | Agglomerative clustering, cosine distance, average linkage |
| `mod.rs` | Offline pipeline + Tauri commands |
| `online.rs` | Streaming diarization for live transcription |
| `download.rs` | Repair-path model download from the project's own GitHub release |

### Offline pipeline

1. New recordings load `mic.mp4` and `system.mp4` independently. The mic track
   is deterministically the local user; only system audio is clustered into
   remote speakers. Older meetings fall back to mixed `audio.mp4` plus the
   enrolled user voiceprint. Decode/downmix/resample inputs to 16 kHz.
2. Slide a 10 s window; run segmentation; decode the powerset output into
   per-frame activity for up to 3 *local* speakers.
3. Embed each local speaker's audio in each window.
4. Cluster embeddings globally → *global* speaker identities.
5. Merge adjacent same-speaker regions.

The mic track updates `<data>/voiceprint/user_voiceprint.json`; live
diarization seeds the user centroid from that profile on later calls.
Overlapped speech remains excluded from embeddings, but overlap timing is
retained. Transcript rows can carry combined labels such as
`You + Speaker 1` rather than being forced to one voice.

Two non-obvious refinements, both from real failure cases:

- **Overlapped speech is excluded from embeddings.** Powerset segmentation
  assigns simultaneous frames to *every* active speaker; including them blends
  two voices into both embeddings.
- **Only turns ≥1.5 s may define a cluster.** Short fragments (usually speech
  clipped by a window edge) have noisy embeddings and used to spawn phantom
  speakers. They are still labelled, by nearest centroid.

### Calibration, and its limits

`DEFAULT_THRESHOLD = 0.60`, chosen by measurement against recordings with known
speaker counts, not by guess:

| Recording | Truth | Auto |
|---|---|---|
| Solo presenter | 1 | 1 ✅ |
| Team call | 5 | 5 ✅ |
| Panel | 6 | 8 |
| Interview | 3 | 2 |

**A single threshold cannot fit every recording** — how far apart two voices
land depends on mic, codec and room. The chosen value never invents speakers in
single-speaker audio, which is the worst failure mode. When the count is known,
passing `num_speakers` bypasses the threshold and resolved *every* test case
exactly. The post-call dialog therefore recommends an entered count, but offers
threshold-based **Auto-detect** when the user does not know it.

A silhouette-based automatic speaker-count search was tried and **removed** — it
consistently preferred the maximum candidate count and did worse than a fixed
threshold. Don't re-add it without evidence.

### Headless evaluation

Diarization can be evaluated without launching the GUI on macOS:

```bash
export DIARIZE_WAV="/path/to/audio.wav"
export DIARIZE_MODELS="frontend/src-tauri/resources/diarization"
cargo test --package llama-helper diarize_sample -- --nocapture
```

Optional: `DIARIZE_SWEEP=0.55,0.60,0.65` (threshold sweep in one process),
`DIARIZE_SPEAKERS=4` (force count), `DIARIZE_DIAG=1` (embedding-distance
histogram — should be clearly bimodal if features are healthy).

Performance varies with hardware; Metal acceleration on Apple Silicon provides
significant speedup.

---

## 5. Transcript rendering (frontend)

The most error-prone area in the codebase, because of duplicated components.

```
live screen        → app/_components/TranscriptPanel.tsx        ─┐
                                                                 ├─▶ VirtualizedTranscriptView
meeting details    → components/MeetingDetails/TranscriptPanel  ─┘
```

Two traps, both of which have bitten:

1. **Two components named `TranscriptPanel`.** Editing the wrong one compiles
   and does nothing visible.
2. **Three separate transcript→segment converters**, and every one must copy
   `speaker` or labels silently vanish on that screen:
   - `app/_components/TranscriptPanel.tsx` (live)
   - `components/MeetingDetails/TranscriptPanel.tsx` (non-paginated)
   - `hooks/usePaginatedTranscripts.ts` (paginated)

(A third, unrendered `components/TranscriptView.tsx` used to exist and caused
exactly this confusion; it has been deleted along with `SettingTabs.tsx`,
`CustomDialog.tsx` and the never-compiled `src-tauri/src/audio_v2/`.)

The label also has to survive the Rust side: `MeetingTranscript` must include
`speaker`, and every place constructing it must set it.

`InsightTabs` renders the complete stored Markdown as its authoritative summary.
English-keyword action/topic shortcuts are supplemental only; never make them the
sole visible representation, because custom and non-English headings do not map
reliably to those buckets. Preserve Markdown whitespace, nesting, and table syntax.

### Stable speaker colors

`VirtualizedTranscriptView.tsx` owns identity colors for both screens. `You` and
labels ending in `(You)` normalize to the same reserved blue, right-aligned user
identity. Blue is excluded from remote colors: Speaker 1 is purple, Speaker 2
emerald, followed by amber, pink, and cyan. Dot and text colors share one index
function, and normalized identity is also used when adjacent turns merge.

### Global search and durable people

`GlobalSearchDialog.tsx` is the single global search surface. The expanded
sidebar field, collapsed Search icon, and `Ctrl/Cmd+K` dispatch/open that same
dialog; the old sidebar-only meeting filter has been removed. It invokes
`api_global_search`, which returns typed `person`, `meeting`, `transcript`, and
`summary` rows. Summary matching parses only user-visible markdown/BlockNote/
legacy content, never raw JSON or hidden `english_cache`. `/person?id=...` is a
query-driven static route because local SQLite people cannot be known during a
Next static export.

The migration `20260811000000_add_people.sql` adds:

```text
people(id, display_name, normalized_name, notes, created_at, updated_at)
person_speakers(person_id, meeting_id, speaker_label)
```

Migration files are immutable byte-for-byte after release. SQLx hashes the raw
SQL, including line endings. Historical migrations through 2025 intentionally
retain their platform checkout behavior because Windows and macOS releases
embedded different line endings; never normalize those files retroactively.
Every new migration must instead receive an explicit LF entry in
`.gitattributes` before release. Windows `0.2.5` and `0.2.6` accidentally
embedded LF and CRLF variants of the same people migration. Database startup
may repair only those two known hashes, and only after verifying the exact
people table/index definitions; never generalize that exception to arbitrary
migration mismatches.

Identity is explicit through `person_speakers`; never infer one person from
`Speaker N`, capture-source placeholders (`mic`, `system`, etc.), `Guest`, `You`,
or combined overlap labels. Existing custom labels are backfilled and exact
normalized names auto-link across meetings. A meeting-local rename must split a
shared profile when the new name is not already a known person; it must not
silently rename that person in every other meeting. Removing a name deletes the
mapping and restores the lowest available `Speaker N` label. Blank Save and the
explicit **Remove name** action share this operation.

Retranscription replaces transcript rows, so it also deletes that meeting's
person mappings and prunes orphan people in the same transaction. Meeting
deletion performs the same explicit cleanup even if SQLite foreign-key
enforcement differs across legacy databases.

Person Q&A is loaded in Rust by `ask_person`; React never supplies its own
corpus. Context contains only the person's mapped messages plus user-visible
summaries for those meetings, is capped conservatively at about 24,000
characters, treats source text as untrusted, and asks for meeting/date/time
citations. Cloud model selections receive that selected context, which the
profile UI discloses; local providers keep it on-device.

### Automatic post-call pipeline

`source=recording` transfers control to `PostCallProcessingDialog.tsx`:

```text
save live meeting and retained tracks
  -> ask for total speakers (exact count recommended; Auto-detect optional)
  -> retranscribe mic.mp4 and system.mp4 independently, unless skipped with X
  -> refetch replaced transcript rows
  -> diarize using the selected exact count or threshold-based Auto-detect
  -> refetch persisted labels
  -> if Auto Summary is enabled, summarize fresh SQLite transcript rows
  -> otherwise leave the summary empty for manual generation
```

Enhanced retranscription must carry retained-track provenance into every new
row: mic speech starts as `You`, system speech starts as `Guest`, and mixed-only
fallback speech remains unlabeled. The later dual-track diarization pass may
refine `Guest` to `Speaker N`, but it must not erase the deterministic mic
identity when segmentation finds only part of a short utterance. When source
speech overlaps, canonical combined labels begin with `You` (for example,
`You + Speaker 1`) so user styling and identity normalization remain stable.
These hints are trusted only when that diarization run actually used both
retained source tracks; legacy mixed recordings must still rely on clustering
and voiceprint evidence.

Do not start automatic diarization without the user's explicit Auto-detect
choice, or generate a summary before this sequence. The count-prompt X skips
only retranscription: it keeps the live rows, still runs the selected exact or
automatic diarization, refetches labels, and then releases the post-call
completion gate. The Auto Summary preference decides whether that completion
starts a summary.
Register retranscription listeners before invoking Rust,
filter events by meeting ID, and clean them once. Model fallback stays within the
configured local provider. Timeout cancellation waits for the native reservation
to clear before retry. Pre-diarization and post-diarization refresh failures are
distinct retry stages. Workflow refetch failures reject without replacing the
already-mounted page with its initial fatal-load screen.

When Auto Summary is enabled, summary generation is gated by both the initial
summary lookup and post-call completion. It fetches all current rows directly
from SQLite, not the paginated React page. With Auto Summary disabled, post-call
processing still completes and the empty summary remains available for manual
generation. Both success-toast and delayed navigation paths must preserve
`source=recording`. Completed meeting IDs are recorded in `sessionStorage` to
prevent React remount duplication.

Post-call summary start uses two levels of ownership: a module-level in-flight
promise deduplicates React remounts/StrictMode while setup is running, and
`post-call-summary-started:<meeting>` is persisted only after
`api_process_transcript` returns a valid process ID. Preflight, model, transcript
fetch, or invoke failures clear the provisional claim so retry remains possible.
Do not mark summary start before backend acceptance.

If audio saving was disabled or retained audio is unavailable, enhancement and
diarization are impossible. The error state offers **Use live transcript**, which
refetches the saved live rows and unblocks the sequence instead of trapping the
user in a retry loop. Those rows are summarized only when Auto Summary is
enabled or the user generates a summary manually.

Claude summary output uses the optional `summaryMaxTokens` setting and a shared
Rust/UI catalog in `src/lib/claude-output-limits.json`. The application default
is 8,192 tokens (4,096 for original Claude 3), not the model's maximum. Explicit
values must be positive integers within the catalog/application limit. Resolve
the effective default before cache fingerprinting; do not let this setting leak
into Custom Server or other providers. Claude responses must report a completed
stop reason; `max_tokens`, refusal, or missing completion must fail before an
incomplete summary is persisted or cached.

Summary prompts serialize every persisted transcript row with timestamp,
speaker label, and text. This is required for a regeneration after speaker
rename to actually expose the renamed identities to the model. Generated
meeting titles are written through `api_save_meeting_title` before meeting and
sidebar refetches; a local-only title is otherwise immediately replaced by the
stale database value. Meeting export owns content selection first (transcript,
summary, or both) and format selection second. It always fetches all transcript
rows from SQLite, and summary export must support markdown, BlockNote
`summary_json`, and legacy section representations.

File export is native-owned after format selection. `exportSummary.ts` creates
PDF/DOCX/text bytes, opens `@tauri-apps/plugin-dialog` Save, then writes only the
user-selected path through `@tauri-apps/plugin-fs`; cancellation is silent.
Clipboard export remains immediate. Do not restore browser Blob/anchor downloads,
which bypass the requested destination and behave inconsistently in a webview.

---

## 6. Model sources

No dependency on any third party's hosting:

| Model | Source |
|---|---|
| Whisper | HuggingFace (`ggerganov/whisper.cpp`) |
| Parakeet v2 | HuggingFace (`istupakov/…-v2-onnx`) |
| Parakeet v3 | This project's own GitHub release |
| Qwen / Gemma | HuggingFace (`unsloth`, `bartowski`) |
| Diarization | Bundled in the app (`resources/diarization/`) |

Parakeet v3 originally pointed at the upstream project's server; it was mirrored
so this fork doesn't consume someone else's bandwidth.

---

## 7. Building

RECMeetily builds for macOS Apple Silicon only.

Gotchas:

- A plain `cargo build` does **not** stage bundled resources the way Tauri's
  packaging does. The sidecar and bundled diarization models must be present
  in their expected locations for the app to function.
- The frontend is embedded at compile time. After changing frontend code you
  must rebuild the frontend *and* relink the app.
- Build with `--features metal,custom-protocol`. Metal enables GPU acceleration
  on Apple Silicon. Without `custom-protocol` the app tries to load a dev
  server on localhost instead of the embedded UI.
- The llama.cpp sidecar (`llama-helper`) must be built separately with `--features metal`
  and copied to `frontend/src-tauri/binaries/llama-helper-aarch64-apple-darwin`.
- Diarization models must be present in `frontend/src-tauri/resources/diarization/`
  at build time (included in the app bundle).

---

## 8. Onboarding and App lifecycle

Onboarding is rendered by `frontend/src/components/onboarding/OnboardingFlow.tsx`.
On macOS, permission testing is serialized in the Audio Test step because
the native Core Audio monitor is a process-global singleton. See section 9
for permission and capture details.

The main window has `center: true` in `tauri.conf.json` so first-run onboarding
opens on the center of the active display.

Model downloads are sequential: the required Parakeet transcription model owns
the connection until it reaches 100%, then the selected summary model starts.
The download step labels these as Step 1 and Step 2 and renders only the active
step's progress bar. After the user continues, active transfers appear as a
compact top-right indicator that expands on hover or keyboard focus; it must not
reserve space or shift later onboarding pages. Completion and errors use short
bottom-right Sonner notifications.

Built-in summary models use exact published byte sizes. Smaller files are
`Incomplete`, retained, and resumed with a validated HTTP `Content-Range`; only
an exact-size file with valid GGUF magic becomes `Available`. Progress reaching
100 is not completion until validation succeeds and Rust emits `completed`.

### Branding assets

`frontend/src-tauri/icon-source.png` is the canonical high-resolution logo.
Tauri's icon generator produces the platform family under `src-tauri/icons/`.
The macOS icon is `icons/icon.icns`, which feeds the app bundle and native
notifications.

The README intentionally renders the logo from the repository so GitHub uses
the same canonical art.

The macOS app identifier is `dev.felipecortes.recmeetily`: it is Tauri's
bundle identifier and used in Tauri plugin store paths.

---

## 9. RECMeetily macOS Apple Silicon capture and releases

The supported macOS target is Apple Silicon on macOS 14.2 Sonoma or later. The
minimum is not merely a product choice: the native Core Audio process-tap path
depends on APIs introduced in 14.2. Keep these declarations aligned:

| Declaration | Responsibility |
|---|---|
| `frontend/src-tauri/.cargo/config.toml` | Rust linker deployment target |
| `frontend/src-tauri/tauri.macos.conf.json` | Tauri bundle minimum system version |
| `.github/workflows/build-macos.yml` | CI `MACOSX_DEPLOYMENT_TARGET` |
| `README.md` | User-visible support statement |

### Native capture and permission model

Responsibility map:

| File | macOS responsibility |
|---|---|
| `audio/capture/core_audio.rs` | Create the global process tap, aggregate device, stream, and audible probe |
| `audio/permissions.rs` | Preserve legacy IPC names while routing verification to the Core Audio probe |
| `audio/capture/backend_config.rs` | Expose only implemented backends for the current platform |
| `components/onboarding/steps/AudioTestStep.tsx` | Serialize the native singleton monitor and guide audible testing |
| `hooks/usePermissionCheck.ts` | Session-scoped verification state and deduplicated manual Recheck |
| `audio/recording_preferences.rs` | Custom-root safety and backend metadata returned to the UI |
| `.github/workflows/build-macos.yml` | Build and verify a candidate without publishing |
| `.github/workflows/publish-macos.yml` | Promote one exact successful candidate artifact |
| `.github/workflows/smoke-test-macos-release.yml` | Download and verify the exact public DMG |

macOS uses one implemented system-audio backend: the Core Audio global process
tap in `audio/capture/core_audio.rs`. `AudioCaptureBackend::available_backends()`
therefore exposes only Core Audio on macOS. The `ScreenCaptureKit` enum variant
still exists for cross-platform compatibility and old settings, but it must not
be offered as a selectable macOS backend.

The global tap follows the current default output route. A listed output-device
name proves that an output exists; it does not prove Audio Capture authorization,
and a non-default output name cannot retarget the global tap. The macOS UI keeps
this route read-only and directs users to Sound settings. Do not restore a
selectable output list unless capture actually binds to that route.

Creating the process tap triggers the OS Audio Capture prompt. A successful tap
creation is still not enough because a denied tap can produce an all-zero stream.
`trigger_system_audio_permission_command` starts the real tap for up to five
seconds and returns early only after receiving an audible sample. UI text must
ask the user to play audio during that interval. The result is stored in
`sessionStorage`, not durable app storage: authorization can be revoked between
launches, so each new app session must not trust an old success forever.

`AudioTestStep` serializes transitions because `start_audio_level_monitoring`
controls one process-global native monitor. Retests, device changes, React
StrictMode cleanup, and unmount can overlap asynchronous permission calls. Keep
the generation checks after every await and keep the transition chain; a stale
probe must not start or stop the next probe's monitor.

Core Audio can change sample rate when the default route changes. The live
resampler is configured at stream creation and cannot safely absorb that change,
so the stream reports a fatal error and uses the command-owned stop/final-save
path. Unexpected stream completion does the same. Never reduce either condition
to a log line while the global recording flag remains true.

Recording destination initialization is also part of startup, not a best-effort
background operation. The custom root is created and write-probed before it is
persisted; `RecordingSaver` creates its meeting folder/checkpoint savers before
returning a chunk sender. Otherwise the channel accepts and silently discards
all audio while the UI appears to record.

The two required privacy strings are `NSMicrophoneUsageDescription` and
`NSAudioCaptureUsageDescription` in `Info.plist`. The shipped entitlement file
contains only the audio-input entitlement currently needed by this build. Do not
add speculative screen-capture, audio-output, or temporary-exception keys; CI
parses both files and verifies the packaged values.

### Bundle integrity and writable data

A signed `.app` is immutable at runtime. Writing a database, settings, models,
or logs beneath `RECMeetily.app/Contents/MacOS` changes the sealed bundle and makes
`codesign --verify --deep --strict` fail after first launch. macOS therefore
uses `~/Library/Application Support/RECMeetily` for core data,
`~/Library/Application Support/dev.felipecortes.recmeetily` for Tauri plugin stores, and
`~/Movies/recmeetily-recordings` (or the configured root) for recordings. Bundled
diarization models and sidecars are read-only resources.

The build and published-release smoke tests deliberately launch the installed
app twice and verify its signature after runtime writes. The first launch covers
fresh database/onboarding initialization; the second covers reopening existing
state. Keep both launches and the post-launch signature checks. A build-only
signature check cannot detect this class of regression.

### Release topology

RECMeetily uses a separate, immutable macOS release:

- Release tags are `vX.Y.Z-macos` and contain the Apple Silicon DMG, checksum,
  and release provenance metadata.
- `.github/workflows/build-macos.yml` is the only production Apple Silicon build
  workflow. It never publishes. It compiles macOS-only Rust, executes sidecars,
  validates architecture/metadata/resources, installs the app, launches it twice,
  and uploads the DMG, checksum, and build-identity metadata as one candidate.
- `.github/workflows/publish-macos.yml` accepts a successful candidate run ID,
  requires physical macOS 14.2 signoff and verifies the immutable-release
  setting with a protected read-only Administration token before reserving the
  tag. It validates the canonical workflow ID, current `main` commit, sole
  artifact archive digest, run attempt, checksum, physical-test DMG digest,
  public API digests, immutable status, then promotes those exact bytes.
  It must not rebuild from mutable `main`.
- After publishing, run `smoke-test-macos-release.yml` against the exact public
  tag. This independently downloads and launches what users receive.
- RECMeetily has no auto-updater; users download updates manually from GitHub.

Unsigned CI builds are ad-hoc signed and are not notarized. First launch may
require Control-click and Open. Notarization is enabled only when all Apple
certificate/account secrets are configured and `sign-and-notarize=true`; never
claim notarization based on ad-hoc `codesign` success.

CI runs on an explicit Apple Silicon macOS 15 image while retaining and checking
the 14.2 deployment target. The runner has no meaningful microphone, speaker
route, Bluetooth device, or
interactive privacy UI. Native CI proves build, package, sidecar, startup,
storage, and signature invariants, but a physical Apple Silicon Mac must still
test on macOS 14.2: microphone input, audible system capture, permission
denial/retry, output-route changes, pause/resume/stop, and the three retained MP4
tracks. The publisher's attestation inputs make this a release gate.

The operational commands, immutable rollback procedure, CI assertions, and
physical-device checklist are in `.github/workflows/MACOS_RELEASE.md`.
