#!/usr/bin/env python3
"""Compiler wrapper that records exact php-src build invocations as JSON Lines."""

from __future__ import annotations

import fcntl
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path


def main() -> int:
    """Append one invocation atomically, then execute the configured real compiler."""
    log_name = os.environ.get("ELEPHC_ORACLE_CC_LOG")
    if not log_name:
        print("ELEPHC_ORACLE_CC_LOG is required", file=sys.stderr)
        return 2
    real_cc = os.environ.get("ELEPHC_ORACLE_REAL_CC", "cc")
    resolved_cc = shutil.which(real_cc)
    if resolved_cc is None:
        print(f"real compiler not found: {real_cc}", file=sys.stderr)
        return 2

    record = {
        "directory": str(Path.cwd().resolve()),
        "compiler": str(Path(resolved_cc).resolve()),
        "arguments": sys.argv[1:],
    }
    log_path = Path(log_name)
    log_path.parent.mkdir(parents=True, exist_ok=True)
    with log_path.open("a", encoding="utf-8") as handle:
        fcntl.flock(handle.fileno(), fcntl.LOCK_EX)
        handle.write(json.dumps(record, sort_keys=True, ensure_ascii=False) + "\n")
        handle.flush()
        fcntl.flock(handle.fileno(), fcntl.LOCK_UN)

    result = subprocess.run([resolved_cc, *sys.argv[1:]], check=False)
    return result.returncode


if __name__ == "__main__":
    raise SystemExit(main())
