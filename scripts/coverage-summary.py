#!/usr/bin/env python3
"""Per-crate coverage table for the CI job summary.

Reads `cargo llvm-cov --json` output and prints a Markdown table, one row per
workspace crate. Exits 1 when a crate is below its floor in
backend/coverage-floors.toml (a missing entry means no floor).

  coverage-summary.py coverage.json backend/coverage-floors.toml
"""
import json
import re
import sys
from collections import defaultdict


def parse_floors(text):
    floors = {}
    for line in text.splitlines():
        m = re.match(r'\s*([A-Za-z0-9_-]+)\s*=\s*([0-9.]+)', line)
        if m:
            floors[m.group(1)] = float(m.group(2))
    return floors


def crate_of(path):
    m = re.search(r'/crates/([^/]+)/', path)
    return m.group(1) if m else None


def summarize(report):
    agg = defaultdict(lambda: {"lines": [0, 0], "regions": [0, 0], "functions": [0, 0]})
    for f in report["data"][0]["files"]:
        c = crate_of(f["filename"])
        if not c:
            continue
        for k in ("lines", "regions", "functions"):
            agg[c][k][0] += f["summary"][k]["covered"]
            agg[c][k][1] += f["summary"][k]["count"]
    return agg


def pct(pair):
    return 100.0 * pair[0] / pair[1] if pair[1] else None


def render(agg, floors):
    rows = ["| Crate | Lines | Functions | Regions | Floor | |", "|---|---:|---:|---:|---:|:--:|"]
    failed = []
    tl = [0, 0]
    for c in sorted(agg):
        a = agg[c]
        lp = pct(a["lines"])
        floor = floors.get(c)
        ok = floor is None or (lp or 0.0) >= floor
        if not ok:
            failed.append(c)
        f = lambda p: "n/a" if p is None else f"{p:.1f}%"
        rows.append(
            f"| `{c}` | {f(lp)} | {f(pct(a['functions']))} | {f(pct(a['regions']))} | "
            f"{'' if floor is None else f'{floor:.0f}%'} | {'✅' if ok else '❌'} |"
        )
        tl[0] += a["lines"][0]
        tl[1] += a["lines"][1]
    total = pct(tl)
    rows.append(f"| **workspace** | **{'n/a' if total is None else f'{total:.1f}%'}** | | | | |")
    return "\n".join(rows), failed


def main():
    report = json.load(open(sys.argv[1]))
    floors = parse_floors(open(sys.argv[2]).read()) if len(sys.argv) > 2 else {}
    table, failed = render(summarize(report), floors)
    print("### Backend test coverage\n")
    print(table)
    if failed:
        print(f"\n**Below floor:** {', '.join(failed)}")
        sys.exit(1)


if __name__ == "__main__":
    main()
