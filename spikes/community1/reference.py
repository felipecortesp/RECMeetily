"""Run pyannote community-1 on a wav file and dump RTTM + metrics."""
import argparse, json, os, resource, sys, time
from pathlib import Path

import numpy as np
from scipy.io import wavfile
import torch
from dotenv import load_dotenv
from pyannote.audio import Pipeline

MODEL = "pyannote/speaker-diarization-community-1"


def talk_time(annotation, total):
    per = {}
    for seg, _, spk in annotation.itertracks(yield_label=True):
        per[spk] = per.get(spk, 0.0) + seg.duration
    return {s: {"seconds": round(t, 2), "percent": round(100 * t / total, 2)}
            for s, t in sorted(per.items())}


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("wav")
    ap.add_argument("--out", required=True)
    ap.add_argument("--num-speakers", type=int)
    ap.add_argument("--min-speakers", type=int)
    ap.add_argument("--max-speakers", type=int)
    ap.add_argument("--device", choices=["cpu", "mps"], default="cpu")
    a = ap.parse_args()

    load_dotenv(Path(__file__).resolve().parents[2] / ".env")
    token = os.environ.get("HF_TOKEN")
    if not token:
        sys.exit("HF_TOKEN missing: set it in the repo-root .env")

    try:
        pipeline = Pipeline.from_pretrained(MODEL, token=token)
    except Exception as e:  # never echo the exception text blindly (may embed headers)
        sys.exit(f"Failed to load {MODEL}: {type(e).__name__}")
    pipeline.to(torch.device(a.device))

    sr, data = wavfile.read(a.wav)  # wav only; avoids decoder issues
    if data.dtype.kind in "iu":
        data = data.astype("float32") / float(np.iinfo(data.dtype).max)
    data = data.astype("float32")
    if data.ndim == 1:
        data = data[:, None]
    waveform = torch.from_numpy(data.T.copy())  # (channel, time)
    duration = waveform.shape[1] / sr

    t0 = time.perf_counter()
    result = pipeline({"waveform": waveform, "sample_rate": sr},
                      num_speakers=a.num_speakers,
                      min_speakers=a.min_speakers,
                      max_speakers=a.max_speakers)
    wall = time.perf_counter() - t0

    out = Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    stem = Path(a.wav).stem
    reg, exc = result.speaker_diarization, result.exclusive_speaker_diarization
    for ann, name in ((reg, f"{stem}.rttm"), (exc, f"{stem}.exclusive.rttm")):
        ann.uri = stem
        with open(out / name, "w") as f:
            ann.write_rttm(f)

    if result.speaker_embeddings is not None:
        np.save(out / f"{stem}.embeddings.npy", result.speaker_embeddings)

    rss = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss / 1e6  # bytes on macOS
    summary = {
        "device": a.device,
        "wall_time_s": round(wall, 2),
        "audio_duration_s": round(duration, 2),
        "real_time_factor": round(wall / duration, 4),
        "peak_rss_mb": round(rss, 1),
        "regular": {"num_speakers": len(reg.labels()), "talk_time": talk_time(reg, duration)},
        "exclusive": {"num_speakers": len(exc.labels()), "talk_time": talk_time(exc, duration)},
    }
    (out / f"{stem}.json").write_text(json.dumps(summary, indent=2))
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
