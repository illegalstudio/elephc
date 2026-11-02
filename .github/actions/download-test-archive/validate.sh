#!/bin/bash
# Reject a missing, truncated or corrupt nextest frame before archive extraction.
set -euo pipefail

if [[ "$#" -ne 1 ]]; then
    echo 'usage: validate.sh <artifact-directory>' >&2
    exit 2
fi

zstd --test --quiet -- "$1/nextest-archive.tar.zst"
