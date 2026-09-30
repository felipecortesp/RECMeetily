"""DER between two RTTM files (URIs normalised)."""
import argparse

from pyannote.core import Annotation
from pyannote.database.util import load_rttm
from pyannote.metrics.diarization import DiarizationErrorRate


def load(path):
    out = Annotation(uri="x")
    for ann in load_rttm(path).values():
        for seg, _, spk in ann.itertracks(yield_label=True):
            out[seg, out.new_track(seg)] = spk
    return out


def talk(ann):
    per = {}
    for seg, _, spk in ann.itertracks(yield_label=True):
        per[spk] = per.get(spk, 0.0) + seg.duration
    return per


def table(title, per):
    total = sum(per.values()) or 1.0
    print(f"\n{title}")
    for s, t in sorted(per.items(), key=lambda kv: -kv[1]):
        print(f"  {s:<16}{t:10.1f}s {100 * t / total:6.1f}%")


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("reference")
    ap.add_argument("hypothesis")
    ap.add_argument("--collar", type=float, default=0.25)
    ap.add_argument("--skip-overlap", action="store_true")
    a = ap.parse_args()

    ref, hyp = load(a.reference), load(a.hypothesis)
    metric = DiarizationErrorRate(collar=a.collar, skip_overlap=a.skip_overlap)
    comp = metric(ref, hyp, detailed=True)
    total = comp["total"] or 1.0
    print(f"DER        {100 * comp['diarization error rate']:.2f}%")
    for k, name in (("false alarm", "false alarm"), ("missed detection", "missed"),
                    ("confusion", "confusion")):
        print(f"  {name:<11}{comp[k]:9.1f}s {100 * comp[k] / total:6.2f}%")
    print(f"  reference speech {total:.1f}s")

    table("Reference talk time", talk(ref))
    table("Hypothesis talk time", talk(hyp))
    mapping = {r: h for h, r in metric.optimal_mapping(ref, hyp).items()}  # returns hyp -> ref
    print("\nOptimal mapping (reference -> hypothesis)")
    for r in sorted(ref.labels()):
        print(f"  {r:<16}-> {mapping.get(r, '(none)')}")


if __name__ == "__main__":
    main()
