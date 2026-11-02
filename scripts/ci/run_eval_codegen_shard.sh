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
# EVAL_SHARD_NEXTEST_BIN names a cargo-nextest binary to run pass 2 with, for a
# runner that packages the binary with the archive instead of installing one
# (the macOS shards); without it pass 2 uses the `cargo nextest` on PATH.
# EVAL_SHARD_JUNIT_FILE, when set, receives a JUnit report of pass 1: one testcase
# per selected test with its libtest outcome. Pass 2 writes nextest's own report
# through `[profile.ci.junit]`. libtest on stable reports no per-test durations,
# so the `time` of a pass-1 testcase is the interval since the previous completion
# in the two-thread process, a shard-tuning signal rather than the test's own
# duration; the report says so in a suite property.

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
pass1_times=$log_dir/pass1.times

echo "Running ${#selected[@]} eval tests in shard $shard/$shard_count"

# --- pass 1: one process, progress-watched -----------------------------------
pass1_status_file=$log_dir/pass1.status
pass1_pid_file=$log_dir/pass1.pid
# Every line is stamped with the epoch second it completed, which is all the per-test
# timing stable libtest allows (see EVAL_SHARD_JUNIT_FILE above). perl rather than awk:
# the macOS runners' awk has no systime(), and both runners ship perl.
{
    # `${skipped[@]+...}`: a single-shard run skips nothing, and macOS's bash 3.2 treats an
    # empty "${skipped[@]}" as an unbound variable under `set -u`.
    "$test_binary" codegen::eval ${skipped[@]+"${skipped[@]}"} --test-threads 2 2>&1 &
    echo $! > "$pass1_pid_file"
    if wait $!; then echo 0 > "$pass1_status_file"; else echo $? > "$pass1_status_file"; fi
} | tee "$pass1_log" | perl -pe 'BEGIN { $| = 1 } $_ = time . "\t" . $_' > "$pass1_times" &
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
    # The pass may have exited on its own during the sleep. Judging the budget then would stop
    # a process that already finished and replace its real status with 124: macOS shards
    # reported "75 passed ... finished in 1797.91s" and failed with 124 exactly that way.
    kill -0 "$tee_pid" 2>/dev/null || break
    # The subshell writes the status file as soon as it reaps libtest, so the file means libtest
    # already ended even while a child it left behind still holds the pipe open. Its status is its
    # own, whatever ended it (an outside SIGKILL included), and the watchdog has nothing to stop.
    [[ -e $pass1_status_file ]] && break
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
    # 143 and 137 are the watchdog's own TERM and KILL. Any other status is one the pass returned
    # by itself after the checks above but before the signal landed, and it stands: a crash
    # racing the budget must not become the all-passed warning below.
    case $pass1_status in
        137 | 143) pass1_status=124 ;;
        *)
            echo "eval shard: the single-process pass exited with status $pass1_status before the watchdog's signal; keeping that status" >&2
            watchdog_reason=""
            ;;
    esac
fi

# --- pass 1 JUnit report ------------------------------------------------------
write_pass1_junit() {
    [[ -n ${EVAL_SHARD_JUNIT_FILE:-} ]] || return 0
    mkdir -p "$(dirname "$EVAL_SHARD_JUNIT_FILE")"
    awk -F '\t' -v started="$started" -v shard="$shard/$shard_count" '
        BEGIN { previous = started; tests = 0; failures = 0; skipped = 0; cases = "" }
        $2 ~ /^test [^ ]+ \.\.\. (ok|FAILED|ignored)$/ {
            split($2, parts, " ")
            name = parts[2]
            status = parts[4]
            elapsed = $1 - previous
            if (elapsed < 0) elapsed = 0
            previous = $1
            tests++
            body = ""
            if (status == "FAILED") { failures++; body = "<failure message=\"failed in the single-process pass\"/>" }
            if (status == "ignored") { skipped++; body = "<skipped/>" }
            cases = cases sprintf("    <testcase classname=\"elephc::codegen_tests\" name=\"%s\" time=\"%d\">%s</testcase>\n", name, elapsed, body)
        }
        END {
            printf "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"
            printf "<testsuites name=\"eval shard %s pass 1\" tests=\"%d\" failures=\"%d\" skipped=\"%d\">\n", shard, tests, failures, skipped
            printf "  <testsuite name=\"elephc::codegen_tests\" tests=\"%d\" failures=\"%d\" skipped=\"%d\">\n", tests, failures, skipped
            printf "    <properties>\n"
            printf "      <property name=\"elephc.eval_shard.time\" value=\"seconds since the previous completion in one two-thread libtest process, not the test duration itself\"/>\n"
            printf "    </properties>\n"
            printf "%s", cases
            printf "  </testsuite>\n</testsuites>\n"
        }
    ' "$pass1_times" > "$EVAL_SHARD_JUNIT_FILE"
}
write_pass1_junit

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
    # The watchdog stopped a pass that had already reported every selected test ok but did not
    # exit, e.g. because something it started still holds its output open. No test failed, so
    # report the stop as a warning instead of failing the shard. A non-zero status the pass
    # returned on its own (a crash after the last test, for one) still fails the shard.
    if [[ -n $watchdog_reason ]]; then
        echo "::warning::eval shard $shard/$shard_count: every selected test passed before the watchdog stopped the single-process pass ($watchdog_reason)"
        exit 0
    fi
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
if [[ -n ${EVAL_SHARD_NEXTEST_BIN:-} ]]; then
    # The Cargo subcommand protocol: a `cargo-<name>` binary receives `<name>` as its
    # first argument, exactly as `cargo nextest` would pass it.
    exec "$EVAL_SHARD_NEXTEST_BIN" nextest run "${nextest_args[@]}"
fi
exec cargo nextest run "${nextest_args[@]}"
