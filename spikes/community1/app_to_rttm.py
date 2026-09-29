"""Convert `diarize_sample` output lines to RTTM.

Line format: `1755.53 -> 1758.25  ( 2.72s)  Speaker 1`
"""
import argparse
import re
from pathlib import Path

LINE = re.compile(r"^\s*(\d+(?:\.\d+)?)\s*->\s*(\d+(?:\.\d+)?)\s*\(\s*[\d.]+s\)\s+(.+?)\s*$")


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("input")
    ap.add_argument("output")
    ap.add_argument("--uri", default=None, help="default: output file stem")
    a = ap.parse_args()
    uri = a.uri or Path(a.output).stem
    n = 0
    with open(a.input) as fi, open(a.output, "w") as fo:
        for line in fi:
            m = LINE.match(line)
            if not m:
                continue
            start, end, label = float(m[1]), float(m[2]), m[3].replace(" ", "_")
            fo.write(f"SPEAKER {uri} 1 {start:.3f} {end - start:.3f} <NA> <NA> {label} <NA> <NA>\n")
            n += 1
    print(f"wrote {n} segments to {a.output}")


if __name__ == "__main__":
    main()
