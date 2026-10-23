#!/usr/bin/env python3
"""Summarize a php-src curl audit JSON into pass/fail/skip buckets and a
heuristic root-cause breakdown of the failures.

Usage: categorize_results.py full-results.json
"""
import json
import re
import sys
from collections import Counter, defaultdict


def cause(rec):
    if rec["status"] == "skip":
        return "skipped by SKIPIF"
    if rec["status"] == "run-timeout":
        return "run timeout"
    if rec["status"] == "compile-error":
        detail = rec.get("detail", "")
        m = re.search(r"Undefined function: (\w+)", detail)
        if m:
            return f"missing core function: {m.group(1)}"
        if "constant() first argument" in detail:
            return "dynamic constant() in AOT"
        if "Undefined variable: $GLOBALS" in detail:
            return "$GLOBALS unsupported"
        if re.search(r"Undefined variable", detail):
            return "checker dataflow: undefined variable"
        if "strlen() argument must be string" in detail:
            return "strict null/non-string argument"
        if "by-reference parameter writes back" in detail:
            return "by-ref output must be pre-initialized"
        if "Unexpected token" in detail:
            return "parser: unsupported syntax"
        if "Cannot access private constructor" in detail:
            return "private constructor is compile-time"
        if "Undefined constant: exit" in detail:
            return "bare exit statement"
        return "other compile error"
    # fail: classify by the emitted diagnostic / shape
    return "runtime/behavior mismatch"


def main():
    path = sys.argv[1]
    recs = json.load(open(path))
    counts = Counter(r["status"] for r in recs)
    print("status counts:", dict(counts), "total:", len(recs))
    by_cause = defaultdict(list)
    for r in recs:
        if r["status"] in ("pass",):
            continue
        by_cause[cause(r)].append(r["name"])
    print("\nnon-pass grouped by heuristic cause:")
    for c, names in sorted(by_cause.items(), key=lambda kv: -len(kv[1])):
        print(f"\n## {c} ({len(names)})")
        for n in names:
            print("  -", n)


if __name__ == "__main__":
    main()
