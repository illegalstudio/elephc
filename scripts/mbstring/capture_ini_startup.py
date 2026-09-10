#!/usr/bin/env python3
"""Capture mbstring startup state and ordered diagnostics in fresh PHP 8.5.10 processes."""

import gzip
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]


def binary(value):
    """Preserve raw configuration and diagnostic text without JSON encoding assumptions."""
    return {"bytes": value.hex()}


def capture(overrides, defaults):
    """Apply raw startup assignments and collect PHP's state before any public setter runs."""
    core = [("internal_encoding", defaults[0]), ("input_encoding", defaults[1]), ("output_encoding", defaults[2])]
    command = ["php", "-d", "display_startup_errors=1", "-d", "display_errors=stderr", "-d", "log_errors=0"]
    # Core directive deprecations are unrelated to the mbstring handlers under test.
    for key, value in core + overrides:
        # Quote text so PHP's INI scanner does not replace On/yes/no before the handler sees it.
        command.extend(["-d", key + "=" + json.dumps(value)])
    command.extend([str(ROOT / "scripts/mbstring/capture_ini.php"), "--startup"])
    result = subprocess.run(command, capture_output=True, check=True)
    warnings = []
    for line in result.stderr.splitlines():
        if not line:
            continue
        severity, message = line.split(b": ", 1)
        message = message.removesuffix(b" in Unknown on line 0")
        if message.startswith(b"PHP Startup: Use of ") and b"mbstring." not in message:
            continue
        warnings.append([{b"Warning": 2, b"Deprecated": 8192}[severity], binary(message)])
    return {"overrides": [[binary(key.encode()), binary(value.encode())] for key, value in overrides],
        "defaults": [binary(value.encode()) for value in defaults], "warnings": warnings, "state": json.loads(result.stdout)}


def main():
    """Exercise defaults, registration order, rejected overrides, and inherited core encodings."""
    profiles = [[], [("mbstring.detect_order", "auto"), ("mbstring.language", "Japanese")],
        [("mbstring.language", "Korean"), ("mbstring.language", "Japanese"), ("mbstring.detect_order", "auto,ASCII")]]
    entries = {
        "mbstring.language": ["Korean", "Japanese", "unknown", ""],
        "mbstring.detect_order": ["auto", "ASCII,SJIS", "bogus", ""],
        "mbstring.http_input": ["pass", "auto", "ASCII,SJIS", "bogus", ""],
        "mbstring.http_output": ["pass", "p", "SJIS", "bogus", ""],
        "mbstring.internal_encoding": ["SJIS", "bogus", ""],
        "mbstring.substitute_character": ["entity", "-1", "0b10", "garbage", ""],
        "mbstring.encoding_translation": ["On", "yes", "no", ""],
        "mbstring.strict_detection": ["On", "0", "garbage", ""],
        "mbstring.regex_stack_limit": ["2K", "0x20", "1badK", "18446744073709551616", ""],
        "mbstring.regex_retry_limit": ["-1", "3M", "bogus"],
        "mbstring.http_output_conv_mimetypes": ["^application/", ""],
    }
    profiles.extend([(key, value)] for key, values in entries.items() for value in values)
    count = 0
    with gzip.open(ROOT / "crates/elephc-mbstring/tests/fixtures/ini_startup.jsonl.gz", "wt", compresslevel=9) as output:
        for defaults in [("UTF-8", "UTF-8", "UTF-8"), ("SJIS", "ASCII,SJIS", "ISO-8859-1"), ("bogus", "bogus", "bogus")]:
            for overrides in profiles:
                output.write(json.dumps(capture(overrides, defaults), separators=(",", ":")) + "\n")
                count += 1
    print(f"Captured {count} INI startup cases")


if __name__ == "__main__":
    main()
