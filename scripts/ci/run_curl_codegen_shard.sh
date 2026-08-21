#!/usr/bin/env bash
# Run one deterministic curl codegen shard inside a single libtest process, and fail
# loudly if the managed-native skip gate fires instead of letting a silently-skipped
# suite report green.
#
# `curl_native::skip_without_curl_native` (tests/codegen/support/curl_native.rs) makes a
# skipped curl fixture return early WITHOUT panicking, so it still reports as a passing
# test to the harness -- this is exactly the failure mode this dedicated CI job exists to
# catch (see ROADMAP.md's v0.30.x entry). The only observable signal that a "pass" was
# actually a skip is the eprintln! message the gate prints, which requires `--nocapture`
# to reach this script's log: nextest's own success-output default (and this repo's `ci`
# profile) hides captured stdout/stderr for passing tests, so running under nextest would
# hide the very message this script greps for. Invoking the extracted test binary's plain
# libtest CLI directly, as this script does, sidesteps that.
#
# WHAT IS MATCHED IS A STABLE TOKEN, NOT PROSE. `skip_gate_marker` below is
# `curl_native::SKIP_GATE_MARKER` verbatim, and the Rust side prints it as a fixed prefix
# ahead of the human-readable half. This script used to grep the readable half instead
# ("managed native curl/openssl/zlib are not installed"), which named the packages -- so
# when libssh2 and nghttp2 joined the set and the message was correctly updated, the grep
# went on matching nothing and THE GATE WAS DEAD: a shard with an incomplete cache would
# skip every fixture, pass, and report green with zero curl coverage. A gate whose pattern
# tracks something that is supposed to change is not a gate.

set -euo pipefail

export RUST_MIN_STACK=${RUST_MIN_STACK:-33554432}

if [[ $# -ne 3 ]]; then
    echo "usage: run_curl_codegen_shard.sh <test-binary> <shard> <shard-count>" >&2
    exit 2
fi

test_binary=$1
shard=$2
shard_count=$3

if [[ ! -f $test_binary || ! $shard =~ ^[0-9]+$ || ! $shard_count =~ ^[0-9]+$ \
    || $shard -lt 1 || $shard -gt $shard_count ]]; then
    echo "invalid test binary or shard selection" >&2
    exit 2
fi

selected=()
skipped=()
while IFS= read -r line; do
    [[ $line == codegen::curl*:" test" ]] || continue
    test_name=${line%: test}
    checksum_line=$(printf '%s' "$test_name" | cksum)
    checksum=${checksum_line%% *}
    test_shard=$((checksum % shard_count + 1))
    if [[ $test_shard -eq $shard ]]; then
        selected+=("$test_name")
    else
        skipped+=(--skip "$test_name")
    fi
done < <("$test_binary" --list)

if [[ ${#selected[@]} -eq 0 ]]; then
    echo "curl shard $shard/$shard_count selected no tests" >&2
    exit 2
fi

echo "Running ${#selected[@]} curl tests in shard $shard/$shard_count"

output_log=$(mktemp)
trap 'rm -f "$output_log"' EXIT

"$test_binary" codegen::curl "${skipped[@]}" --test-threads 4 --nocapture 2>&1 | tee "$output_log"

# Keep in sync with `curl_native::SKIP_GATE_MARKER` (the only place that spelling is
# allowed to live besides this line).
skip_gate_marker='ELEPHC_CURL_NATIVE_SKIP_GATE'

if grep -q "$skip_gate_marker" "$output_log"; then
    echo "error: curl fixtures hit the managed-native skip gate -- the managed" >&2
    echo "curl/libssh2/nghttp2/openssl/zlib archives were not linked, so this shard did" >&2
    echo "not really exercise ext/curl." >&2
    echo "See tests/codegen/support/curl_native.rs::skip_without_curl_native." >&2
    exit 1
fi
