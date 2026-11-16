#!/bin/bash
# Reject a missing, truncated or corrupt nextest frame before archive extraction.
set -euo pipefail

if [[ "$#" -ne 1 ]]; then
    echo 'usage: validate.sh <artifact-directory>' >&2
    exit 2
fi

if [[ ${VALIDATE_PAYLOAD:-false} == true ]]; then
    python3 -B - "$1" <<'PYTHON'
import hashlib
import json
import sys
from pathlib import Path
root = Path(sys.argv[1])
manifest = json.loads((root / "manifest.json").read_text())
checksum = hashlib.sha256()
with (root / "payload.tar.gz").open("rb") as stream:
    for chunk in iter(lambda: stream.read(1024 * 1024), b""):
        checksum.update(chunk)
if checksum.hexdigest() != manifest["archive_sha256"]:
    raise SystemExit("Build payload checksum mismatch")
PYTHON
    exit 0
fi

zstd --test --quiet -- "$1/nextest-archive.tar.zst"
