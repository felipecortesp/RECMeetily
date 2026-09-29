# community-1 spike (stage 2)

Compares ways to run `pyannote/speaker-diarization-community-1` in the app.
Nothing here ships.

The model is CC-BY-4.0 and gated on Hugging Face: accept its terms, then put
`HF_TOKEN=...` in the repo-root `.env` (gitignored). The token is only read via
python-dotenv and never printed.

Use the venv `~/.venvs/recmeetily-pyannote/bin/python`.

- `reference.py <wav> --out <dir> [--num-speakers N] [--min-speakers N] [--max-speakers N] [--device cpu|mps]`
  runs the reference pipeline; writes `<stem>.rttm`, `<stem>.exclusive.rttm`,
  `<stem>.json` (timing, RSS, talk time) and `<stem>.embeddings.npy`.
- `compare.py <ref.rttm> <hyp.rttm> [--collar 0.25] [--skip-overlap]`
  prints DER (false alarm / missed / confusion), talk-time tables, optimal mapping.
- `app_to_rttm.py <diarize_sample.txt> <out.rttm> [--uri NAME]`
  converts output of the Rust `diarize_sample` test into RTTM.
