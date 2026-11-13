#!/usr/bin/env bash
# Execute a filter from an extracted nextest archive without invoking Cargo builds.
set -euo pipefail

: "${ELEPHC_CI_TEST_ROOT:?restore the test archive before selecting tests}"
if [[ $# -lt 1 ]]; then
    echo 'usage: run_prebuilt_tests.sh <nextest filter> [nextest run options...]' >&2
    exit 2
fi
filter=$1
shift
runner=${ELEPHC_CI_NEXTEST:-cargo-nextest}
export ELEPHC_TEST_PREBUILT_BRIDGES=1
export CARGO_TARGET_DIR="$ELEPHC_CI_TEST_ROOT/target"
export RUST_MIN_STACK=${RUST_MIN_STACK:-33554432}

exec "$runner" nextest run --profile ci \
    --binaries-metadata "$ELEPHC_CI_TEST_ROOT/target/nextest/binaries-metadata.json" \
    --cargo-metadata "$ELEPHC_CI_TEST_ROOT/target/nextest/cargo-metadata.json" \
    --target-dir-remap "$ELEPHC_CI_TEST_ROOT/target" --workspace-remap "$PWD" \
    --no-tests fail --no-fail-fast --retries 1 --flaky-result pass \
    -E "$filter" "$@"
