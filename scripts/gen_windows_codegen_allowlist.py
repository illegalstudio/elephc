#!/usr/bin/env python3
"""Verify exact successful coverage of the native Windows codegen shards.

The aggregate CI job supplies the runnable ``ci``-profile inventory and every
native Windows shard's JUnit report. This tool rejects missing, duplicate,
unexpected, or failing testcases so neither a failed test nor a truncated
partition can masquerade as a green Windows result.
"""

from __future__ import annotations

import argparse
import json
import sys
import xml.etree.ElementTree as ET
from collections import Counter
from pathlib import Path

# The nextest test binary whose fixtures make up the strict native suite.
CODEGEN_SUITE_ID = "elephc::codegen_tests"


def _local_tag(tag: str) -> str:
    """Return an XML element tag without its ``{namespace}`` prefix, if any."""
    return tag.rsplit("}", 1)[-1]


def parse_junit_failures(path: Path) -> tuple[set[str], int]:
    """Parse a nextest JUnit report into (failing_test_names, tests_run).

    A ``<testcase>`` counts as failing iff it has a direct child element named
    ``failure`` or ``error`` (nextest emits ``<failure>`` for panics, non-zero
    exits, and slow-timeout terminations). Passing tests carry no such child;
    skipped/ignored tests are not emitted at all. The returned name is the
    testcase ``name`` attribute, which nextest sets to the full test path
    (e.g. ``codegen::arrays::callbacks::test_array_all``) — the exact form used
    throughout these lists.
    """
    root = ET.parse(path).getroot()
    failures: set[str] = set()
    ran = 0
    for testcase in root.iter("testcase"):
        name = testcase.attrib.get("name")
        if name is None:
            continue
        ran += 1
        if any(_local_tag(child.tag) in ("failure", "error") for child in testcase):
            failures.add(name)
    return failures, ran


def parse_junit_cases(path: Path) -> list[str]:
    """Return every named testcase in report order, retaining duplicates."""
    root = ET.parse(path).getroot()
    return [
        name
        for testcase in root.iter("testcase")
        if (name := testcase.attrib.get("name")) is not None
    ]


def load_runnable(list_json_path: Path) -> set[str]:
    """Extract the CI-profile *runnable* codegen tests from a nextest list dump.

    Reads ``cargo nextest list --profile ci --test codegen_tests
    --message-format json`` output. A test is runnable when it is not
    ``#[ignore]``d and matches the profile's filter (``filter-match.status ==
    "matches"``). Returns the set of full test names.
    """
    data = json.loads(Path(list_json_path).read_text())
    suites = data.get("rust-suites", {})
    suite = suites.get(CODEGEN_SUITE_ID)
    if suite is None:
        raise SystemExit(
            f"error: nextest list JSON has no '{CODEGEN_SUITE_ID}' suite "
            f"(found: {sorted(suites)})"
        )
    runnable: set[str] = set()
    for name, tc in suite.get("testcases", {}).items():
        if tc.get("ignored"):
            continue
        fm = tc.get("filter-match")
        matched = fm.get("status") == "matches" if isinstance(fm, dict) else bool(fm)
        if matched:
            runnable.add(name)
    return runnable


def cmd_verify_complete(args: argparse.Namespace) -> int:
    """Reject incomplete, duplicate, stale, or optionally failing shard coverage."""
    if (
        args.expected_junit_count is not None
        and len(args.junit) != args.expected_junit_count
    ):
        print(
            "WINDOWS CODEGEN INCOMPLETE: expected "
            f"{args.expected_junit_count} JUnit report(s), found {len(args.junit)}.",
            file=sys.stderr,
        )
        return 1

    runnable = load_runnable(Path(args.list_json))
    observed_list = [
        name
        for junit_path in args.junit
        for name in parse_junit_cases(Path(junit_path))
    ]
    failures = sorted(
        {
            name
            for junit_path in args.junit
            for name in parse_junit_failures(Path(junit_path))[0]
        }
    )
    observed = set(observed_list)
    missing = sorted(runnable - observed)
    unexpected = sorted(observed - runnable)
    duplicates = sorted(
        name for name, count in Counter(observed_list).items() if count > 1
    )

    if missing or unexpected or duplicates or (args.require_success and failures):
        print(
            "WINDOWS CODEGEN INCOMPLETE: aggregate shard coverage does not "
            "provide one successful execution of every runnable CI test.",
            file=sys.stderr,
        )
        for label, names in (
            ("missing", missing),
            ("unexpected", unexpected),
            ("duplicated", duplicates),
            ("failed", failures if args.require_success else []),
        ):
            if not names:
                continue
            print(f"{label} testcase(s): {len(names)}", file=sys.stderr)
            for name in names[:50]:
                print(f"  {name}", file=sys.stderr)
            if len(names) > 50:
                print(f"  ... and {len(names) - 50} more", file=sys.stderr)
        return 1

    print(
        f"OK: {len(observed)} Windows codegen testcase(s) exactly cover "
        "the runnable CI set once each."
    )
    return 0


def build_parser() -> argparse.ArgumentParser:
    """Construct the CLI for strict native Windows coverage verification."""
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="command", required=True)

    c = sub.add_parser(
        "verify-complete",
        help="verify aggregate shard coverage against the runnable test set",
    )
    c.add_argument("--list-json", required=True, help="nextest runnable-list JSON")
    c.add_argument(
        "--junit",
        action="append",
        required=True,
        help="per-shard nextest JUnit report",
    )
    c.add_argument(
        "--expected-junit-count",
        type=int,
        help="reject the aggregate unless exactly this many JUnit reports are supplied",
    )
    c.add_argument(
        "--require-success",
        action="store_true",
        help="also reject every failure/error recorded in the shard reports",
    )
    c.set_defaults(func=cmd_verify_complete)
    return p


def main(argv: list[str]) -> int:
    """CLI entry point: parse arguments and dispatch to the chosen subcommand."""
    args = build_parser().parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
