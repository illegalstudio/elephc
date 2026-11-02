#!/usr/bin/env python3
"""Differential test run: the same plain-PHP suite under php-src PHPUnit and under elephc.

Runs ``vendor/bin/phpunit`` and ``elephc test`` on one project, both with a JUnit report,
and compares the outcome of every test PHPUnit reports against what the compiled suite
reports for the same ``Class::name``. A test PHPUnit ran that elephc did not run, or ran
with a different outcome, is a mismatch and the script exits 1. Tests only elephc runs
(from ``.lfc`` files PHPUnit cannot parse) are listed as elephc-only and never fail the
comparison.

Outcomes are normalized to ``passed``, ``failed``, ``error`` and ``skipped`` (PHPUnit
reports incomplete tests as skipped in JUnit, and so does elephc).

Usage:
    scripts/test-differential.py PROJECT [--elephc BIN] [--phpunit BIN] [--tests DIR]

PROJECT holds ``composer.json`` with the suite's autoload and a ``vendor/`` with PHPUnit
installed (``composer require --dev phpunit/phpunit``). ``--tests`` is the suite path
relative to PROJECT (default ``tests``).
"""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ElementTree
from pathlib import Path

NORMALIZED = {"failure": "failed", "error": "error", "skipped": "skipped"}


def parse_junit(path: Path) -> dict[tuple[str, str, int], str]:
    """Maps ``(classname, name, occurrence)`` to the normalized outcome of every testcase.

    The occurrence index keeps repeated names apart: two providers on one test, both with
    list rows, report ``testX with data set #0`` twice, and keying on the name alone would
    let the second outcome overwrite the first.
    """
    outcomes: dict[tuple[str, str, int], str] = {}
    seen: dict[tuple[str, str], int] = {}
    root = ElementTree.parse(path).getroot()
    for case in root.iter("testcase"):
        classname = case.get("class") or case.get("classname") or ""
        name = case.get("name") or ""
        outcome = "passed"
        for child in case:
            tag = child.tag.split("}")[-1]
            if tag in NORMALIZED:
                outcome = NORMALIZED[tag]
                break
        base = (classname.replace(".", "\\"), name)
        occurrence = seen.get(base, 0)
        seen[base] = occurrence + 1
        outcomes[(base[0], base[1], occurrence)] = outcome
    return outcomes


def run(command: list[str], cwd: Path) -> int:
    """Runs ``command`` in ``cwd`` with inherited output and returns its exit code."""
    print("$", " ".join(command), flush=True)
    return subprocess.call(command, cwd=cwd)


def main() -> int:
    """Runs both suites, compares the reports, prints the differences, returns 0 or 1."""
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("project", type=Path, help="project directory with composer.json and vendor/")
    parser.add_argument("--elephc", default="elephc", help="the elephc binary (default: from PATH)")
    parser.add_argument("--phpunit", default=None, help="the PHPUnit binary (default: PROJECT/vendor/bin/phpunit)")
    parser.add_argument("--tests", default="tests", help="suite path relative to PROJECT (default: tests)")
    args = parser.parse_args()

    project = args.project.resolve()
    # Both suites run inside PROJECT, so a relative binary path is resolved from here.
    elephc = str(Path(args.elephc).resolve()) if os.sep in args.elephc else args.elephc
    # Both suites run inside PROJECT, so explicit paths are resolved from the caller's
    # directory first: a relative path would otherwise pass `exists()` here and then fail.
    phpunit = Path(args.phpunit).resolve() if args.phpunit else project / "vendor" / "bin" / "phpunit"
    if not phpunit.exists():
        print(f"error: PHPUnit not found at {phpunit}; run `composer require --dev phpunit/phpunit` in {project}", file=sys.stderr)
        return 2

    with tempfile.TemporaryDirectory(prefix="elephc-differential-") as scratch:
        reference = Path(scratch) / "phpunit.xml"
        compiled = Path(scratch) / "elephc.xml"
        # PHPUnit's own exit code reflects failing tests; the comparison is what decides.
        run(
            [str(phpunit), "--bootstrap", "vendor/autoload.php", "--log-junit", str(reference), "--no-progress", args.tests],
            project,
        )
        run([elephc, "test", args.tests, "--junit", str(compiled)], project)
        if not reference.exists():
            print("error: PHPUnit wrote no JUnit report", file=sys.stderr)
            return 2
        if not compiled.exists():
            print("error: elephc test wrote no JUnit report (compile error above?)", file=sys.stderr)
            return 2
        expected = parse_junit(reference)
        actual = parse_junit(compiled)

    if not expected:
        # An empty reference compares nothing: it would report every outcome as matching.
        print("error: PHPUnit's report has no test cases; nothing to compare", file=sys.stderr)
        return 2

    mismatches = []
    for key, outcome in sorted(expected.items()):
        label = f"{key[0]}::{key[1]}" + (f" (occurrence {key[2] + 1})" if key[2] else "")
        if key not in actual:
            mismatches.append(f"{label}: phpunit={outcome}, elephc=not run")
        elif actual[key] != outcome:
            mismatches.append(f"{label}: phpunit={outcome}, elephc={actual[key]}")
    elephc_only = sorted(
        f"{key[0]}::{key[1]}" + (f" (occurrence {key[2] + 1})" if key[2] else "")
        for key in actual
        if key not in expected
    )

    print()
    print(f"compared {len(expected)} PHPUnit tests against {len(actual)} elephc tests")
    for label in elephc_only:
        print(f"elephc-only: {label}")
    if mismatches:
        print(f"{len(mismatches)} mismatch(es):")
        for line in mismatches:
            print(f"  {line}")
        return 1
    print("every PHPUnit outcome matches the compiled suite")
    return 0


if __name__ == "__main__":
    os.environ.setdefault("PYTHONDONTWRITEBYTECODE", "1")
    sys.exit(main())
