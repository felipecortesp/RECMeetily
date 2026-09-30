# Spike: pyannote Community-1 for the final diarization pass

Date: 2026-09-29. Branch: `recmeetily/02-community1-spike`. Nothing here ships.

## Question

Replace the current offline diarization (segmentation-3.0 + WeSpeaker
ResNet34-LM + LDA + own agglomerative clustering, threshold 0.60) with
pyannote `speaker-diarization-community-1` (powerset segmentation + WeSpeaker +
PLDA + VBx). Two integration paths:

- **A**: export segmentation and the ResNet embedding (fbank stays in Rust) to
  ONNX and port PLDA + VBx to Rust.
- **B**: FluidAudio (Swift, CoreML/ANE) through the `fluidaudio-rs` crate.
- A third option, `speakrs` (pure Rust community-1), was dropped: 0 stars and
  4 days old at the time of the spike.

## Data

One real 3-person meeting (2026-09-28, 30:28): `system.wav` (the two remote
speakers) and `audio.wav` (mixed, all three). The app diarizes `system` and
`mic` separately when both tracks exist, so **`system` is the case that
matters**. No human ground truth: the reference is community-1 run in Python
(pyannote.audio 4.0.7, CPU, exclusive output). Old block-level labels from the
app were too coarse to score anything (a one-speaker hypothesis scored the
same as the others).

## Results

DER against the community-1 reference (collar 0.25 s), exclusive output:

| Track | Speakers given | Current app | FluidAudio (B) |
|---|---|---|---|
| system | 2 | 38.2 % (confusion 31.4 %) | **10.6 % (confusion 4.4 %)** |
| system | none | 34.8 % | 35.8 % |
| mixed | 3 | **11.3 %** | 34.2 % |
| mixed | none | 23.2 % | 54.8 % |

The reference itself is unstable on this audio: community-1 finds 3 remote
voices without a hint, and its own no-hint vs 2-speaker outputs differ by
28.9 % DER. Remote voices through a call codec are hard for every model.

Cost for 30 min of audio on this Mac (Apple Silicon):

| | Time | Peak memory | Models |
|---|---|---|---|
| Current app (Rust + ONNX, CPU) | ~17 s | not isolated | 30 MB |
| Community-1 in Python (CPU) | ~22 min | ~3 GB | 34 MB |
| FluidAudio (CoreML/ANE) | ~6 s (+1 s warm prep, 11 s first run) | ~0.5 GB | 21 MB |

Known current-app defect: forcing the speaker count makes agglomerative
clustering split off a single outlier (742 s vs 3 s on `system`).

## Path facts

**B, FluidAudio / fluidaudio-rs**
- Licenses: FluidAudio Apache-2.0, fluidaudio-rs MIT, CoreML models
  (`FluidInference/speaker-diarization-coreml`) CC-BY-4.0 derived from
  community-1 → attribution required (README + third-party notices).
- `fluidaudio-rs` exposes the offline community-1 diarizer but only the
  clustering threshold; FluidAudio itself supports `numSpeakers`,
  `minSpeakers`, `maxSpeakers` (~20 lines of bridge to expose). Output is
  exclusive by default, which is what stage 1's row splitter wants.
- Build: `build.rs` runs `swift build` and fetches FluidAudio over the network
  → needs full Xcode on every build machine; vendor/pin for reproducibility.
  macOS 14+, Apple Silicon (matches the app). +~5 MB binary.
- Runtime: models land in `~/Library/Application Support/FluidAudio/`, and a
  CoreML cache under `~/Library/Caches/<exe>/`; the app keeps models
  install-local, so the model directory must be made configurable. Library
  prints profiling noise to stdout.

**A, ONNX + PLDA/VBx in Rust**
- Segmentation exports directly; the embedding exports only without fbank,
  which fits `diarization/dsp.rs`. PLDA/VBx reference: grikdotnet
  (Apache-2.0, ~11 KB of Python, byte-identical RTTM vs pyannote), VBx
  Fa=0.07, Fb=0.8, loopP=0.9.
- Not measured: it is a port, i.e. weeks of work plus a parity harness, and its
  best case is "equal to the Python reference", which B already matches on
  the case that matters when given the speaker count.

## Recommendation: B (FluidAudio via fluidaudio-rs), with conditions

1. On the `system` track with the right speaker count, B reproduces
   community-1 (4.4 % confusion) where the current pipeline does not (31 %),
   and it is ~3x faster than today with less memory. The app already asks for
   the participant count, so the hint is available (remote = count − 1).
2. A would at best reach the same quality with far more work and ownership of
   a numeric port.
3. Conditions for the integration stage:
   - expose `numSpeakers/min/max` (and keep exclusive output) in the bridge:
     patch or fork `fluidaudio-rs`, pinned;
   - pin/vendor FluidAudio and the CoreML models; host the models in a new
     release with SHA-256 like `diarization-models-v1`; load them from the app's
     model directory, offline;
   - keep the current pipeline as fallback when models/ANE are unavailable and
     for the mixed-audio path until B is validated there;
   - CC-BY-4.0 attribution for community-1 in the README and notices;
   - validate on a few minutes of human-labelled audio before release; this
     spike only measured agreement with the reference on one meeting.
