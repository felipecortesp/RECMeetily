# RECMeetily

<p align="center">
  <img src="docs/images/recmeetily-logo.png" alt="RECMeetily logo" width="480" />
</p>

A privacy-hardened, fully local meeting recorder for macOS Apple Silicon. Records meetings from the microphone and system audio (via Core Audio process tap) without bots or virtual drivers. Microphone and system audio are kept as separate tracks; microphone speech is labelled "You" and remote voices become "Speaker N" via on-device diarization. Transcription with Parakeet (25 European languages, automatic switching) or whisper.cpp (via Metal/CoreML). Local LLM summaries via bundled llama.cpp sidecar or your chosen cloud provider.

## Why This Fork

RECMeetily grew out of Meetily - Actually Free to deliver a macOS-focused privacy-hardened recorder:

- **No telemetry, no updater, no network exposure:** The app never contacts GitHub or any cloud service unless you explicitly add an API key. No auto-updater; no analytics module.
- **Hash-verified downloads:** Every model and binary (Parakeet, Whisper, summary GGUF, diarization, ffmpeg) is verified against SHA-256 hashes embedded in the app.
- **Content-free logs:** Logs never contain spoken text, summaries, or personally identifiable information.
- **Bundled ffmpeg only:** The sidecar is the exclusive ffmpeg source; system PATH is never consulted.
- **Microphone and system audio separation:** Kept as separate MP4 tracks before diarization, enabling precise speaker attribution.

## Features

- **Speaker-aware transcripts:** Microphone speech stays "You"; remote voices become "Speaker 1", "Speaker 2", etc.; overlapped speech renders as "You + Speaker 1".
- **Separate mic and system audio:** Retained as `mic.mp4` and `system.mp4` alongside the mixed playback track `audio.mp4`.
- **Parakeet or Whisper transcription:** Parakeet TDT 0.6B v3 (NVIDIA, ONNX) for live and post-call with 25 European languages and automatic language switching; whisper.cpp for post-call retranscription with Metal/CoreML acceleration and vocabulary hints.
- **Local diarization:** On-device speaker identification via pyannote segmentation 3.0 + WeSpeaker ResNet34 embeddings (ONNX Runtime, no external API).
- **Local LLM summaries:** Bundled llama.cpp sidecar with Metal support; Qwen 3.5 (2B/4B) or Gemma 3 (1B/4B) models; or bring your own provider (Ollama, OpenAI, Anthropic, Groq, OpenRouter, any OpenAI-compatible endpoint).
- **Automatic meeting detection:** Watches for Zoom, Teams, Slack, Webex, and other meeting apps; prompts to start recording.
- **Live audio levels:** Separate microphone and system meters show pre-mix activity during recording.
- **Floating recording bar:** Compact minibar with timer and pause/resume/stop controls.
- **Meeting memory:** Global search, reusable people profiles, speaker naming, and person Q&A.
- **Native exports:** PDF, DOCX, Markdown, JSON, and clipboard.
- **Automatic post-call processing:** Retranscribes retained source tracks independently, diarizes, and optionally summarizes.
- **Custom summary templates:** Define your own summary structure and ask custom questions.
- **Dark and light themes:** Cohesive theming across recording, transcripts, summaries, and settings.

## Requirements

- **macOS 14.2 or later** (Sonoma or newer) on Apple Silicon (M1 or newer).
- **Microphone and System Audio Recording permissions.** Prompted on first use.
- **Internet connection:** Required only for downloading models (first launch) and, optionally, cloud providers if configured.

## Install

1. Download the DMG from the [latest release](https://github.com/felipecortesp/RECMeetily/releases).
2. Open the DMG and drag **RECMeetily** to Applications.
3. The build is not Apple-notarized, so macOS blocks the first launch. Click Done, open System Settings > Privacy & Security, scroll to the Security section and click **Open Anyway** next to the RECMeetily message, then launch it again. (Alternative in Terminal: `xattr -dr com.apple.quarantine /Applications/RECMeetily.app`.)
4. Grant Microphone and System Audio Recording permissions when prompted.

## Permissions

RECMeetily requests:

- **Microphone:** Captures local audio from the default input device.
- **System Audio Recording:** Captures audio from the system output using Core Audio process tap (no bot joins the call, no virtual drivers).

No camera, screen capture, location, contacts, or calendar permissions are needed or requested.

## Local Data

| Data | Location |
| --- | --- |
| Database, templates, models | `~/Library/Application Support/RECMeetily` |
| Recordings | `~/Movies/recmeetily-recordings` (configurable in Settings) |
| Playback and retained tracks | `audio.mp4` (mixed), `mic.mp4` (you), `system.mp4` (remote) |

On first launch, the app copies existing models and recordings from a previous Meetily installation (if present), leaving the original untouched.

## Privacy

- **No telemetry:** No analytics or usage tracking. Crash reports are written to a local file only and never sent.
- **No auto-updater:** Download updates manually from the GitHub releases page.
- **No backend:** Everything runs on your machine. There is no server, no cloud sync, and no account required.
- **Hash-verified downloads:** Every model and binary is verified against embedded SHA-256 hashes. Diarization models are fetched from pinned URLs and verified against SHA-256 hashes embedded in the app; see [docs/diarization-models.md](docs/diarization-models.md) for their provenance.
- **Content-free logs:** Logs never include transcripts, summaries, audio data, or other sensitive content.
- **Only bundled ffmpeg:** The app uses only its bundled ffmpeg sidecar; system PATH is never consulted.

## Build From Source

Requires: Xcode (full), Rust 1.97+, pnpm 12, Homebrew openssl@3.

1. Build the sidecar:

```bash
cargo build --release --package llama-helper --target aarch64-apple-darwin --features metal
cp target/aarch64-apple-darwin/release/llama-helper frontend/src-tauri/binaries/llama-helper-aarch64-apple-darwin
```

2. Build the frontend and package the app:

```bash
cd frontend
pnpm install --frozen-lockfile
pnpm exec tauri build --target aarch64-apple-darwin --bundles app
```

3. Create a DMG (optional):

```bash
hdiutil create -volname RECMeetily -srcfolder "<path to RECMeetily.app>" -ov -format UDZO RECMeetily.dmg
```

Environment variables:

```bash
export DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer
export MACOSX_DEPLOYMENT_TARGET=14.2
```

See [ARCHITECTURE.md](ARCHITECTURE.md) and [.github/workflows/MACOS_RELEASE.md](.github/workflows/MACOS_RELEASE.md) for more details.

## Acknowledgements

It grew out of [Meetily - Actually Free](https://github.com/TylerBuza/Meetily-ActuallyFree) by Tyler Buza, itself based on [Meetily](https://github.com/Zackriya-Solutions/meetily) by Zackriya Solutions. Thanks to both projects.

Security and bundling ideas came from [Talkkeeper](https://github.com/leyvanah/Talkkeeper): bundled-only ffmpeg, content-free logs, and removing dead backend commands.

Built with open-source models and engines:

- [Parakeet TDT 0.6B v3](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3) by NVIDIA; ONNX conversion by [istupakov](https://github.com/istupakov).
- [Whisper](https://github.com/openai/whisper) and [whisper.cpp](https://github.com/ggerganov/whisper.cpp) by OpenAI and ggerganov.
- [pyannote.audio](https://github.com/pyannote/pyannote-audio) for speaker segmentation.
- [WeSpeaker](https://github.com/wenet-e2e/wespeaker) for speaker embeddings.
- [llama.cpp](https://github.com/ggerganov/llama.cpp) by ggerganov.
- [Qwen](https://github.com/QwenLM/Qwen) by Alibaba and [Gemma](https://github.com/google/gemma) by Google for summary models.
- [Tauri](https://tauri.app) and [ONNX Runtime](https://onnxruntime.ai).

## License

MIT licensed. See [LICENSE.md](LICENSE.md). Original copyright notices and license terms from Meetily and Meetily - Actually Free are retained.
