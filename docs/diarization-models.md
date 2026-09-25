# Diarization model provenance

The speaker-diarization pipeline (`frontend/src-tauri/src/diarization/`) downloads
three ONNX/NumPy assets on first use. This note records where they come from and
why, so the pinned hashes in `diarization/download.rs` can be audited later
without redoing the research.

## Current source: Tyler's GitHub release

All three files are downloaded from this fork's own GitHub release
`diarization-models-v1` (`TylerBuza/Meetily-ActuallyFree`), each verified
against a pinned exact size and SHA-256 in `diarization/download.rs::ASSETS`
before it's trusted:

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

## Validation result (2026-09-25): not a drop-in — kept Tyler's release

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
embedding models (keeping Tyler's `xvec_transform.npz`, since the pyannote
x-vector transform is gated — see above) fails immediately:

```
thread 'diarization::tests::diarize_sample' panicked at frontend/src-tauri/src/diarization/mod.rs:1333:10:
diarization failed: Invalid input name: waveform
```

For reference, the same test against the current (Tyler) three-file set on a
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
