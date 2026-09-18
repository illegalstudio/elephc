#!/usr/bin/env bash
# Run one deterministic eval shard inside a single libtest process so the test
# harness can reuse assembled runtime objects and bridge discovery caches, while
# keeping the protections the nextest shards have:
#
#   1. Pass 1 runs the shard in ONE libtest process. A watchdog kills it when no
#      test has completed for EVAL_SHARD_STALL_TIMEOUT seconds (the nextest
#      budget for `codegen::eval`: 60s x terminate-after 3) or after
#      EVAL_SHARD_BUDGET seconds in total, so a hang cannot hold the job for its
#      whole timeout.
#   2. Pass 2 reruns under nextest only what pass 1 did not report as ok — the
#      failures, and whatever was cut off by the watchdog — each test in its own
#      process, with nextest's per-test slow-timeout and the same
#      `--retries 1 --flaky-result pass` policy the other shards use.
#
# EVAL_SHARD_NEXTEST_ARCHIVE / EVAL_SHARD_NEXTEST_EXTRACT_DIR point pass 2 at the
# CI test archive; without them pass 2 runs nextest against the local workspace.

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
stall_timeout=${EVAL_SHARD_STALL_TIMEOUT:-180}
total_budget=${EVAL_SHARD_BUDGET:-1800}

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

log_dir=$(mktemp -d)
trap 'rm -rf "$log_dir"' EXIT
pass1_log=$log_dir/pass1.log

echo "Running ${#selected[@]} eval tests in shard $shard/$shard_count"

# --- pass 1: one process, progress-watched -----------------------------------
pass1_status_file=$log_dir/pass1.status
pass1_pid_file=$log_dir/pass1.pid
{
    "$test_binary" codegen::eval "${skipped[@]}" --test-threads 2 2>&1 &
    echo $! > "$pass1_pid_file"
    if wait $!; then echo 0 > "$pass1_status_file"; else echo $? > "$pass1_status_file"; fi
} | tee "$pass1_log" &
tee_pid=$!
# The libtest pid is what the watchdog kills; wait for the subshell to record it rather than
# hoping a fixed sleep was long enough on a loaded runner.
test_pid=""
for ((attempt = 0; attempt < 100; attempt++)); do
    test_pid=$(cat "$pass1_pid_file" 2>/dev/null || true)
    [[ -n $test_pid ]] && break
    sleep 0.1
done
if [[ -z $test_pid ]]; then
    echo "eval shard: the single-process pass did not record its pid" >&2
fi

completed_count() {
    grep -cE '^test [^ ]+ \.\.\. (ok|FAILED|ignored)' "$pass1_log" 2>/dev/null || true
}

started=$(date +%s)
last_progress=$started
last_completed=$(completed_count)
watchdog_reason=""
while kill -0 "$tee_pid" 2>/dev/null; do
    sleep 5
    now=$(date +%s)
    completed=$(completed_count)
    if [[ $completed -ne $last_completed ]]; then
        last_completed=$completed
        last_progress=$now
    fi
    if [[ $((now - last_progress)) -ge $stall_timeout ]]; then
        watchdog_reason="no test completed for ${stall_timeout}s"
    elif [[ $((now - started)) -ge $total_budget ]]; then
        watchdog_reason="shard exceeded its ${total_budget}s budget"
    fi
    if [[ -n $watchdog_reason ]]; then
        echo "eval shard watchdog: $watchdog_reason; stopping the single-process pass" >&2
        if [[ -n ${test_pid:-} ]]; then
            pkill -TERM -P "$test_pid" 2>/dev/null || true
            kill -TERM "$test_pid" 2>/dev/null || true
            sleep 5
            pkill -KILL -P "$test_pid" 2>/dev/null || true
            kill -KILL "$test_pid" 2>/dev/null || true
        fi
        break
    fi
done
wait "$tee_pid" || true
pass1_status=$(cat "$pass1_status_file" 2>/dev/null || echo 1)
if [[ -n $watchdog_reason ]]; then
    pass1_status=124
fi

# libtest's --skip filters are substring matches; the prefix grouping above keeps the known
# shapes together, but a green pass 1 is only trusted when every selected test was reported.
# Anything missing (a name skipped by a shorter sibling's filter) is rerun in pass 2 instead
# of silently running in no shard at all.
reported_count=$(grep -cE '^test [^ ]+ \.\.\. (ok|ignored)$' "$pass1_log" 2>/dev/null || true)
if [[ $pass1_status -eq 0 && $reported_count -eq ${#selected[@]} ]]; then
    exit 0
fi
if [[ $pass1_status -eq 0 ]]; then
    echo "eval shard: pass 1 reported $reported_count of ${#selected[@]} selected tests; rerunning the rest" >&2
    pass1_status=1
fi

# --- pass 2: nextest for everything pass 1 did not pass -----------------------
remaining=()
for test_name in "${selected[@]}"; do
    if grep -qE "^test ${test_name//./\\.} \.\.\. (ok|ignored)$" "$pass1_log"; then
        continue
    fi
    remaining+=("$test_name")
done

if [[ ${#remaining[@]} -eq 0 ]]; then
    echo "single-process pass exited with status $pass1_status but every selected test passed" >&2
    exit "$pass1_status"
fi

echo "Rerunning ${#remaining[@]} test(s) under nextest after the single-process pass (status $pass1_status):"
printf '  %s\n' "${remaining[@]}"

filter="binary(codegen_tests) and ("
separator=""
for test_name in "${remaining[@]}"; do
    filter+="${separator}test(=${test_name})"
    separator=" or "
done
filter+=")"

nextest_args=(--profile ci -E "$filter" --no-fail-fast --retries 1 --flaky-result pass -j 2)
if [[ -n ${EVAL_SHARD_NEXTEST_ARCHIVE:-} ]]; then
    nextest_args+=(--archive-file "$EVAL_SHARD_NEXTEST_ARCHIVE" --workspace-remap .)
    if [[ -n ${EVAL_SHARD_NEXTEST_EXTRACT_DIR:-} ]]; then
        nextest_args+=(--extract-to "$EVAL_SHARD_NEXTEST_EXTRACT_DIR" --extract-overwrite)
    fi
fi
exec cargo nextest run "${nextest_args[@]}"
