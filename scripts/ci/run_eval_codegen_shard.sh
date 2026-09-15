#!/usr/bin/env bash
# Run one deterministic eval shard inside a single libtest process so the test
# harness can reuse assembled runtime objects and bridge discovery caches.

set -euo pipefail

export RUST_MIN_STACK=${RUST_MIN_STACK:-33554432}
export LC_ALL=C

if [[ $# -ne 3 ]]; then
    echo "usage: run_eval_codegen_shard.sh <test-binary> <shard> <shard-count>" >&2
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

tests=()
while IFS= read -r line; do
    [[ $line == codegen::eval*:" test" ]] || continue
    test_name=${line%: test}
    tests+=("$test_name")
done < <("$test_binary" --list)

selected=()
skipped=()
for ((test_index = 0; test_index < ${#tests[@]}; test_index++)); do
    test_name=${tests[$test_index]}
    # libtest's --skip filters are substring matches. Keep prefix-related test
    # names in the same shard so skipping a shorter name cannot silently skip
    # a selected longer name.
    shard_key_index=$test_index
    for ((candidate_index = 0; candidate_index < ${#tests[@]}; candidate_index++)); do
        candidate=${tests[$candidate_index]}
        if [[ $test_name == "$candidate"_* ]]; then
            shard_key_index=$candidate_index
            break
        fi
    done

    # Libtest lists names deterministically. Round-robin the canonical group
    # indexes to keep shard sizes balanced without a separate manifest.
    test_shard=$((shard_key_index % shard_count + 1))
    if [[ $test_shard -eq $shard ]]; then
        selected+=("$test_name")
    else
        skipped+=(--skip "$test_name")
    fi
done

if [[ ${#selected[@]} -eq 0 ]]; then
    echo "eval shard $shard/$shard_count selected no tests" >&2
    exit 2
fi

echo "Running ${#selected[@]} eval tests in shard $shard/$shard_count"
exec "$test_binary" codegen::eval "${skipped[@]}" --test-threads 2
