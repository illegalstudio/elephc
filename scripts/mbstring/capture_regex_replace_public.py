#!/usr/bin/env python3
"""Capture exact public replacement fixtures with portable PHP diagnostic locations."""

from pathlib import Path
import re
import subprocess


def main():
    """Preserve all output bytes and diagnostic text, excluding PHP's source-location suffix."""
    root = Path(__file__).resolve().parents[2]
    version = subprocess.check_output(["php", "-r", "echo PHP_VERSION, ':', MB_ONIGURUMA_VERSION;"])
    assert version == b"8.5.10:6.9.10", version
    for name in ["public", "errors"]:
        source = root / f"tests/codegen/strings/fixtures/mbstring_replace_{name}.php"
        result = subprocess.run(["php", "-d", "display_errors=stderr", "-d", "log_errors=0", str(source)],
                                capture_output=True, check=True, timeout=10)
        result.stdout.decode("utf-8")
        source.with_suffix(".out").write_bytes(result.stdout)
        if name == "errors":
            errors = re.sub(rb" in " + re.escape(str(source).encode()) + rb" on line [0-9]+(?=\n)", b"", result.stderr)
            source.with_suffix(".err").write_bytes(errors)
        else:
            assert not result.stderr, result.stderr
        print(name, len(result.stdout), "stdout bytes")


if __name__ == "__main__":
    main()
