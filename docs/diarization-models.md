# Diarization model provenance

The speaker-diarization pipeline (`frontend/src-tauri/src/diarization/`) downloads
three ONNX/NumPy assets on first use. This note records where they come from and
why, so the pinned hashes in `diarization/download.rs` can be audited later
without redoing the research.

## Current source: RECMeetily GitHub release

All three files are downloaded from the RECMeetily GitHub release
`diarization-models-v1`
(<https://github.com/felipecortesp/RECMeetily/releases/download/diarization-models-v1>),
each verified against a pinned exact size and SHA-256 in
`diarization/download.rs::ASSETS` before it's trusted. This release is
byte-identical to the original `diarization-models-v1` release published by
the Meetily - Actually Free project — same files, same pinned hashes:

| File | Size | SHA-256 |
|---|---|---|
| `segmentation-3.0-fp16.onnx` | 2,977,738 bytes | `b0acba8e4cc30e8ec2bd33075ce95f282d66acab86fb155860d8deddbfacd30c` |
| `wespeaker-resnet34-LM.onnx` | 26,544,003 bytes | `992a5632618f11644608dbfbd28d401cd8480713207dc2db9af1c4cfc2c8652e` |
| `xvec_transform.npz` | 134,376 bytes | `325f1ce8e48f7e55e9c8aa47e05d2766b7c48c4b25b8de8dd751e7a4cc5fbe8f` |

This release exists so setup needs no third-party bandwidth and no Hugging Face
account/token — the tradeoff is that the models are re-hosted rather than
downloaded directly from their original publishers, so the hash pin above is
the only thing standing between a user and a tampered release asset. Treat
that table as the source of truth; it was last cross-checked 2026-09-25.

## Public candidates for a future direct-from-publisher switch

Two of the three assets have an accessible, first-party Hugging Face
equivalent that a future phase could switch to, removing the re-hosting step
entirely:

| Candidate | Repo | File | Size | SHA-256 | Accessible |
|---|---|---|---|---|---|
| Segmentation | `onnx-community/pyannote-segmentation-3.0` | `onnx/model_fp16.onnx` | 3,000,918 bytes | `f3dba0c91270b923e9ce66e4cef123820cb73c9b07e47e03a3d7d4267e4fbec4` | Yes |
| Embedding | `Wespeaker/wespeaker-voxceleb-resnet34-LM` (commit `f0c48c298fd835726c27956a5d617bad7115627e`) | `voxceleb_resnet34_LM.onnx` | 26,530,309 bytes | `7bb2f06e9df17cdf1ef14ee8a15ab08ed28e8d0ef5054ee135741560df2ec068` | Yes |

Note the sizes differ slightly from the currently-used files (e.g.
`wespeaker-resnet34-LM.onnx` at 26,544,003 bytes vs. `voxceleb_resnet34_LM.onnx`
at 26,530,309 bytes) — these are not guaranteed to be byte-identical exports,
so switching is not a drop-in hash swap; it needs its own validation pass
against real recordings before it can replace the current pipeline.

The third asset, the VBx x-vector LDA transform (`xvec_transform.npz`), does
**not** have an accessible first-party equivalent: `pyannote/speaker-diarization-community-1`
(commit `3533c8cf8e369892e6b79bf1bf80f7b0286a54ee`), file `plda/xvec_transform.npz`,
returned HTTP 401 — it's a gated repository requiring Hugging Face
authentication. Any switch would need either a token-gated download flow (which
conflicts with this fork's no-account-required goal) or would have to keep
re-hosting this one file regardless of what happens to the other two.

## Validation result (2026-09-25): not a drop-in — kept the RECMeetily release

The two public candidates above were downloaded, hash-verified against the
table, and inspected with `onnx` (Python) to compare their graph I/O against
what `diarization/models.rs` hardcodes:

| Model | Used today (input → output) | Public candidate (input → output) |
|---|---|---|
| Segmentation | `waveform` `[1,1,samples]` → `segmentation` `[1,frames,7]` | `input_values` `[batch,channels,samples]` → `logits` `[batch,frames,7]` |
| Embedding | `fbank` `[1,t,128]`*(t,80 in practice)* → `embedding` `[256]` | `feats` `[B,T,80]` → `embs` `[B,256]` |

Both public exports use different ONNX tensor names than the pinned files,
even though the tensor shapes and roles line up. `models.rs` calls
`ort::inputs!["waveform" => ...]` and `outputs.get("segmentation")` /
`outputs.get("embedding")` by literal name, so it cannot load these files
unmodified — and renaming/adapting the loader to match was explicitly out of
scope for this decision (the point was to check whether the *existing* pinned
contract has a direct public replacement, not to fork the loader per source).

This was confirmed empirically, not just by reading the graph: running the
existing `diarization::tests::diarize_sample` test (see `diarization/mod.rs`)
against a model directory built from the public segmentation + public
embedding models (keeping the current `xvec_transform.npz`, since the
pyannote x-vector transform is gated — see above) fails immediately:

```
thread 'diarization::tests::diarize_sample' panicked at frontend/src-tauri/src/diarization/mod.rs:1333:10:
diarization failed: Invalid input name: waveform
```

For reference, the same test against the current three-file set on a
~107s PT/FR/EN 3-speaker sample (`pt-fr-en.wav`, `num_speakers=3`) runs in
~2.9s wall time and finds 3 speakers / 13 segments — this is the working
baseline the public files were being checked against.

**Decision: keep the current release-hosted assets.** `download.rs` and
`frontend/src-tauri/resources/diarization/` are unchanged. A future switch
would require either finding public exports that keep the original
`waveform`/`segmentation` and `fbank`/`embedding` tensor names (unlikely,
since these are re-exports of the same upstream models under different
conversion tooling), or extending `models.rs` to probe/accept either name —
which is a real code change with its own risk, not a config swap, and should
get its own issue if it's ever pursued.


## Core ML models (pyannote community-1 via FluidAudio)

On macOS the offline diarizer (`diarization/fluid.rs`, backed by the vendored
FluidAudio Swift package) runs pyannote community-1 as Core ML models. Unlike the
ONNX assets above, these are committed in the repository and bundled with the app
under `frontend/src-tauri/resources/diarization-coreml/` (Tauri resource glob
`resources/diarization-coreml/**/*`); nothing is downloaded at runtime.

- Source: Hugging Face `FluidInference/speaker-diarization-coreml`, public (not
  gated), pinned to revision `df2625ac79a7ac6b65ad868fee6d80f320da4232`.
- License: CC-BY-4.0 (Hugging Face tag `scoped-cc-by-4.0`). The models are modified
  Core ML conversions of `pyannote/speaker-diarization-community-1` (CC-BY-4.0);
  the PLDA parameters originate from BUT Speech@FIT (CC-BY-4.0). The upstream
  `LICENSE`, `NOTICE.md` and `PROVENANCE.md` are kept unchanged next to the models.
- Only `Segmentation`, `FBank`, `Embedding` and `PldaRho` (`.mlmodelc`) and
  `plda-parameters.json` are bundled; the mlpackages, the unused `PLDA.mlmodelc`,
  legacy models and images from that repository are not.
- Integrity: `diarization::fluid::MODEL_FILES` pins the size and SHA-256 of every
  file below. `verify_models` checks them once per process; `usable_models_dir`
  prefers a user copy in `<models>/diarization-coreml` if it verifies, else the
  bundled copy. A unit test verifies the committed files against this table.

| File | Size (bytes) | SHA-256 |
|---|---|---|
| `Embedding.mlmodelc/analytics/coremldata.bin` | 243 | `8d6706436639b53830b4dbe8aaf9c9a843f7f582d63e16f3cb8bb7c6ccd58682` |
| `Embedding.mlmodelc/coremldata.bin` | 704 | `4a705bac27d151d9642f37609296042a15602a42253039e0921dc9e75da7e004` |
| `Embedding.mlmodelc/metadata.json` | 2818 | `1854371eb6b438fb8aeac96afb45c999af7902581c06afdfcd7ff3cb1ce66be5` |
| `Embedding.mlmodelc/model.mil` | 78432 | `22fa958aef72a561c21f874a07cbdcd30fdf40ee961c0bc2fb67c119273b46d3` |
| `Embedding.mlmodelc/weights/weight.bin` | 13412288 | `99356b2985b8d43880a657024d941d450b38820451ccff903f76ed4e52d1868b` |
| `FBank.mlmodelc/analytics/coremldata.bin` | 243 | `0e8bd3a8b82ac123580989f490e4d9245127c535857630b543311268accc3f0a` |
| `FBank.mlmodelc/coremldata.bin` | 853 | `57ac436bb0671cbb5527a339134d695f752eb77f7a18966b93c6835335595759` |
| `FBank.mlmodelc/metadata.json` | 3409 | `2623785f5d186893b82d01e84aa33a7704ef763c3309e02055f22dc9d871ce9a` |
| `FBank.mlmodelc/model.mil` | 15667 | `27aaeb21569e81bdbe2eef87789f50a37cfea800039bd134448a9417de2f30ed` |
| `FBank.mlmodelc/weights/weight.bin` | 1776896 | `9e83fdd3ea78064b078069e4d9141603c61c47a27fd19e7e3142ff7476f8db36` |
| `PldaRho.mlmodelc/analytics/coremldata.bin` | 243 | `8940ea6044dbcbefa22da8cc41e0b485e1fb5ed89aecaf37c6e0c483a97ddcd7` |
| `PldaRho.mlmodelc/coremldata.bin` | 763 | `4d9741477f721c79b09fcdfe455110c4b7d4272e2de3496bf1729d966d3ee418` |
| `PldaRho.mlmodelc/metadata.json` | 2749 | `b314cf25a93e46b4076883a6f5a2f8848b73c3851bd9d36074d067f35a1c7945` |
| `PldaRho.mlmodelc/model.mil` | 7613 | `83aee2e5310d19b5f202aea97d07a0e12102556d1b32ef3ed08b36f7f9725041` |
| `PldaRho.mlmodelc/weights/weight.bin` | 200192 | `80f7d229202636d372428c90596f11a91545f07da77259f07153aaf225914a36` |
| `Segmentation.mlmodelc/analytics/coremldata.bin` | 243 | `64265f8e7ad41a5f68d630c15288c2499cca5892ad49e20096819cdeac004cdb` |
| `Segmentation.mlmodelc/coremldata.bin` | 812 | `ea51481b8bd3e496ad3cf16f066ddaa37f20e8772eaac76b3393c28de20e06bc` |
| `Segmentation.mlmodelc/metadata.json` | 3410 | `88dbf0b07208fe142e1729c2b4c974ad3599fcb2ae5d5f18fce782b225384124` |
| `Segmentation.mlmodelc/model.mil` | 43063 | `d37e4ce30b406a6b34f765f769b9baed3178cc0c2b2e299c641daa43a052dd3f` |
| `Segmentation.mlmodelc/weights/weight.bin` | 5959360 | `c3189a64946c75bc24fcb98afe89ad78c52bdbadfdf65e857fb1b81e2cc9fbb2` |
| `plda-parameters.json` | 89416 | `38ee28d4269c076cef254ee760bbd811f0738a92e0f01f9699ad372828c5de8f` |

### Vendored FluidAudio

`vendor/FluidAudio` is FluidAudio v0.14.8 (Apache-2.0, license and third-party
notices unchanged) with a small patch for deterministic k-means restarts. What was
trimmed and why is in `vendor/FluidAudio/RECMEETILY_PATCHES.md`.
