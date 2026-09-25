# Privacy Policy — RECMeetily

**Short version: your data stays on your machine unless you explicitly choose to
send it to a cloud provider or submit a crash report.**

## What we collect

RECMeetily has **no automatic analytics, telemetry, or phone-home
behavior**. There is no account, no sign-in, no auto-updater, and no usage tracking.
The app does not automatically send your audio, transcripts, summaries, crash reports,
or usage data to us or anyone else, and makes no automatic network access beyond the
model and provider downloads described below.

## Where your data lives

- **Audio recordings, transcripts, summaries, and settings** are stored **locally** on your
  device (a SQLite database and files in your app data directory).
- **API keys** you enter (if you choose to use a cloud AI or transcription provider) are
  stored locally and used only to talk directly to that provider.

## When data leaves your machine

Data leaves your machine only when **you** explicitly choose an action that sends it:

- If you select a **cloud AI provider** (Claude, OpenAI, Groq, OpenRouter, or a custom
  endpoint), the transcript text you summarize is sent **directly to that provider** using
  **your** key, subject to **their** privacy policy and data-retention terms.
- If you select a **cloud transcription provider**, your audio is sent to that provider.
- Local model downloads (summarization, transcription, and diarization models) are
  fetched from **huggingface.co** and, for diarization assets, from
  **github.com/felipecortesp/RECMeetily**; every downloaded file is SHA-256 verified
  before use.

If RECMeetily detects that the previous session ended unexpectedly, it can create a
redacted crash-report ZIP at your request. The report excludes recordings, transcripts,
summaries, meeting names, the database, settings, credentials, usernames, hostnames,
and device names. It contains crash type/time, app version and backend, OS family and
major version, architecture, bucketed CPU core count, rounded memory size, and a
source-relative panic file/line location with a location fingerprint when available.
The ZIP is written **locally only**; it is shared with us only if you choose
**Send Report**, which opens a pre-filled GitHub issue and lets you attach the ZIP
yourself. Opening GitHub sends normal request data to GitHub, and GitHub uploads the
attachment only once you add it and submit the issue.

Using the **built-in local model**, **local Whisper/Parakeet**, and **Ollama** keeps
everything **100% offline** — nothing leaves your device.

## Your control

Delete a meeting in the app and its data is removed locally. Uninstalling the app and
deleting its data directory removes everything.

## Contact

This is an open-source project. For questions or issues, open an issue at
<https://github.com/felipecortesp/RECMeetily/issues>.
