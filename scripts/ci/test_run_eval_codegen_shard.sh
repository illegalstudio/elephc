#!/usr/bin/env bash
# Focused tests for the eval shard runner's exit status around its watchdog, with a fake
# libtest binary and a fake cargo-nextest, so no Rust build is needed.
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
fixture_dir=$(mktemp -d "$PWD/.eval-shard-test.XXXXXX")
trap 'rm -rf "$fixture_dir"' EXIT HUP INT TERM

# The fake test binary lists two eval tests. In run mode FAKE_MODE picks the behaviour:
#   exit_late    both pass, and the process exits 0 after 3s: past the 2s budget but inside
#                the watchdog's 5s sleep (the CI failure shape)
#   crash_after  both pass, and the process exits 3 after 3s
#   crash_racing both pass, then the process lingers and answers the watchdog's TERM by exiting
#                3: to the runner, the same as a crash landing between the watchdog's liveness
#                check and its signal, which no sleep can time reliably
#   oom_killed   both pass, then an outside SIGKILL ends the process while a child it started
#                keeps stdout open past the budget
#   slow_exit    both pass, then the process lingers holding stdout
#   stall_before only the first passes, then the process lingers
cat > "$fixture_dir/codegen_tests" <<'EOF'
#!/bin/sh
if [ "$1" = "--list" ]; then
    echo "codegen::eval::first: test"
    echo "codegen::eval::second: test"
    echo
    echo "2 tests, 0 benchmarks"
    exit 0
fi
echo "running 2 tests"
echo "test codegen::eval::first ... ok"
case "$FAKE_MODE" in
    exit_late)
        echo "test codegen::eval::second ... ok"
        echo "test result: ok. 2 passed; 0 failed"
        sleep 3
        exit 0
        ;;
    slow_exit)
        echo "test codegen::eval::second ... ok"
        echo "test result: ok. 2 passed; 0 failed"
        sleep 30
        ;;
    crash_after)
        echo "test codegen::eval::second ... ok"
        echo "test result: ok. 2 passed; 0 failed"
        sleep 3
        exit 3
        ;;
    crash_racing)
        echo "test codegen::eval::second ... ok"
        echo "test result: ok. 2 passed; 0 failed"
        trap 'exit 3' TERM
        while :; do sleep 1; done
        ;;
    oom_killed)
        echo "test codegen::eval::second ... ok"
        echo "test result: ok. 2 passed; 0 failed"
        sleep 8 &
        kill -KILL $$
        ;;
    stall_before)
        sleep 30
        ;;
esac
EOF
chmod +x "$fixture_dir/codegen_tests"

# The fake nextest records the pass-2 arguments and exits 7, so pass 2 is observable.
cat > "$fixture_dir/cargo-nextest" <<EOF
#!/bin/sh
printf '%s\n' "\$@" > "$fixture_dir/nextest-args"
exit 7
EOF
chmod +x "$fixture_dir/cargo-nextest"

# Runs the shard runner as shard 1/1 with a 2s total budget and the fakes above, and prints
# its exit status. Its own output goes to $fixture_dir/out.
run_shard() {
    local status=0
    FAKE_MODE=$1 EVAL_SHARD_BUDGET=2 EVAL_SHARD_STALL_TIMEOUT=100 \
        EVAL_SHARD_NEXTEST_BIN="$fixture_dir/cargo-nextest" \
        bash "$script_dir/run_eval_codegen_shard.sh" "$fixture_dir/codegen_tests" 1 1 \
        > "$fixture_dir/out" 2>&1 || status=$?
    echo "$status"
}

# A pass that exits by itself while the watchdog sleeps keeps its own status: no watchdog
# stop, no warning, and pass 2 never runs.
status=$(run_shard exit_late)
if [ "$status" -ne 0 ]; then
    echo "a pass that exited during the watchdog's sleep exited $status, expected 0" >&2
    cat "$fixture_dir/out" >&2
    exit 1
fi
if grep -q 'watchdog' "$fixture_dir/out"; then
    echo "the watchdog judged a pass that had already exited" >&2
    cat "$fixture_dir/out" >&2
    exit 1
fi
test ! -e "$fixture_dir/nextest-args"

# Every selected test passed but the process never exited: the watchdog stops it, which is a
# warning, not a failure, and pass 2 never runs.
status=$(run_shard slow_exit)
if [ "$status" -ne 0 ]; then
    echo "watchdog stop after every test passed exited $status, expected 0" >&2
    cat "$fixture_dir/out" >&2
    exit 1
fi
grep -q '::warning::eval shard 1/1: every selected test passed' "$fixture_dir/out"
test ! -e "$fixture_dir/nextest-args"

# A crash after the last test is not hidden by the all-passed shortcut.
status=$(run_shard crash_after)
if [ "$status" -ne 3 ]; then
    echo "crash after every test passed exited $status, expected 3" >&2
    cat "$fixture_dir/out" >&2
    exit 1
fi
test ! -e "$fixture_dir/nextest-args"

# A status the pass returns by itself while the watchdog stops it is kept, not replaced by the
# watchdog's 124 and then waved through by the all-passed warning.
status=$(run_shard crash_racing)
if [ "$status" -ne 3 ]; then
    echo "a crash racing the watchdog's stop exited $status, expected 3" >&2
    cat "$fixture_dir/out" >&2
    exit 1
fi
test ! -e "$fixture_dir/nextest-args"

# A pass an outside SIGKILL ended keeps its 137 even when a leftover child holds the pipeline
# open past the budget: the watchdog stopped nothing, so it is neither a 124 nor a warning.
status=$(run_shard oom_killed)
if [ "$status" -ne 137 ]; then
    echo "a pass killed from outside exited $status, expected 137" >&2
    cat "$fixture_dir/out" >&2
    exit 1
fi
if grep -q '::warning::' "$fixture_dir/out"; then
    echo "a pass killed from outside was reported as a watchdog stop" >&2
    cat "$fixture_dir/out" >&2
    exit 1
fi
test ! -e "$fixture_dir/nextest-args"

# A watchdog stop before every test reported still hands the rest to pass 2.
status=$(run_shard stall_before)
if [ "$status" -ne 7 ]; then
    echo "watchdog stop before the last test exited $status, expected pass 2's 7" >&2
    cat "$fixture_dir/out" >&2
    exit 1
fi
grep -q 'test(=codegen::eval::second)' "$fixture_dir/nextest-args"
if grep -q 'test(=codegen::eval::first)' "$fixture_dir/nextest-args"; then
    echo "pass 2 reran a test that already passed" >&2
    exit 1
fi

echo 'eval shard runner tests passed'
