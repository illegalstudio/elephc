#!/usr/bin/env python3
"""Capture warning-time mbstring mutation and restore behavior in isolated PHP processes."""

import gzip
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]


def identity_cases():
    """Distinguish getter aliases, equal copied bytes, empty/one-byte normalization, and interned values."""
    for key, initial, changed in [("mbstring.internal_encoding", "ASCII", "SJIS"),
                                  ("mbstring.regex_stack_limit", "3", "1badK")]:
        for configured in [False, True]:
            startup = [[key, initial]] if configured else []
            for old in [None, initial, "", ["literal", initial], ["literal", ""]]:
                before = [] if old is None else [["set", key, old]]
                for expression in [["get", key], ["flat", key], ["local", key], ["global", key],
                                   ["copy", ["get", key]], ["literal", initial], ["literal", ""], initial,
                                   ["lower", ["get", key]], ["upper", ["get", key]], ["reverse", ["get", key]]]:
                    for throwing in [False, True]:
                        yield {"startup": startup, "before": before, "outer": ["set", key, changed],
                               "callback": [["set", key, expression]], "throw": throwing}


def capture(output, case):
    """Run one independent PHP process with explicitly quoted startup text and append its observations."""
    command = ["php", "-d", "display_errors=0", "-d", "display_startup_errors=0", "-d", "log_errors=0"]
    for name, raw in case["startup"]:
        command.extend(["-d", name + "=" + json.dumps(raw)])
    command.extend([str(ROOT / "scripts/mbstring/capture_ini_reentry.php"), json.dumps(case)])
    result = subprocess.run(command, capture_output=True, check=True)
    output.write(json.dumps(json.loads(result.stdout), separators=(",", ":")) + "\n")


def main():
    """Cross successful and failed outer handlers with nested setters, restores, and exceptions."""
    settings = {"mbstring.internal_encoding": ("ASCII", ["SJIS", "bogus", ""], "ISO-8859-1"),
        "mbstring.http_output": ("ASCII", ["SJIS", "bogus", ""], "pass"),
        "mbstring.http_input": ("ASCII", ["SJIS", "bogus", ""], "pass"),
        "mbstring.detect_order": ("ASCII", ["bogus"], "SJIS"),
        "mbstring.regex_stack_limit": ("32", ["1badK"], "2K"),
        "mbstring.regex_retry_limit": ("64", ["foo"], "3K"),
        "mbstring.http_output_conv_mimetypes": ("^text/", ["["], "^application/")}
    count = 0
    destination = ROOT / "crates/elephc-mbstring/tests/fixtures/ini_reentry.jsonl.gz"
    with gzip.open(destination, "wt", compresslevel=9) as output:
        for configured in [False, True]:
            startup = [(key, values[0]) for key, values in settings.items()] if configured else []
            for key, (initial, values, replacement) in settings.items():
                for value in values:
                    for restore in [False, True]:
                        before = [["internal", "UTF-16LE"], ["output", "SJIS"]]
                        if restore:
                            before.append(["set", key, value])
                        outer = ["restore", key] if restore else ["set", key, value]
                        actions = [[], [["internal", "ISO-8859-1"]], [["output", "pass"]], [["language", "Japanese"], ["detect", "auto"]],
                            [["set", key, replacement]], [["restore", key]], [["set", key, ""]],
                            [["set", key, replacement], ["restore", key]], [["set", "mbstring.strict_detection", "On"]]]
                        for callback in actions:
                            for throwing in [False, True]:
                                case = {"startup": startup, "before": before, "outer": outer, "callback": callback, "throw": throwing}
                                capture(output, case)
                                count += 1
        for case in identity_cases():
            capture(output, case)
            count += 1
    print(f"Captured {count} INI reentry cases")


if __name__ == "__main__":
    main()
