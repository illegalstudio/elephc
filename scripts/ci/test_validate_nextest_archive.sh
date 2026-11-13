#!/bin/bash
# Reproduce successful downloads whose inner nextest frame is incomplete.
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
validator="$script_dir/../../.github/actions/download-test-archive/validate.sh"
fixture_dir=$(mktemp -d "$PWD/.nextest-frame-test.XXXXXX")
trap 'rm -rf "$fixture_dir"' EXIT HUP INT TERM

mkdir "$fixture_dir/artifact with spaces"
artifact_dir="$fixture_dir/artifact with spaces"
archive="$artifact_dir/nextest-archive.tar.zst"
printf 'nextest fixture\n' | zstd --quiet -o "$archive"
bash "$validator" "$artifact_dir"
cp "$archive" "$fixture_dir/complete.zst"

# The download action can report success for this exact pre-test failure.
python3 - "$archive" <<'PY'
from pathlib import Path
import sys

archive = Path(sys.argv[1])
archive.write_bytes(archive.read_bytes()[:-1])
PY
if bash "$validator" "$artifact_dir"; then
    echo 'truncated archive unexpectedly accepted' >&2
    exit 1
fi

# A complete frame downloaded on the next attempt must recover normally.
cp "$fixture_dir/complete.zst" "$archive"
bash "$validator" "$artifact_dir"

printf 'not a zstd frame\n' > "$archive"
if bash "$validator" "$artifact_dir"; then
    echo 'corrupt archive unexpectedly accepted' >&2
    exit 1
fi

rm "$archive"
if bash "$validator" "$artifact_dir"; then
    echo 'missing archive unexpectedly accepted' >&2
    exit 1
fi

echo 'nextest archive frame validation tests passed'
