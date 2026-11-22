#!/usr/bin/env bash
# Apps and scripts demo: builds two versions of a worker that send their
# transactions to a listening collector (--with-apm --apm-collector), starts the
# collector with an on-disk history and the monitor page, then runs instances of
# both versions in turns until Ctrl-C. v1.0 rarely fails and exits 0; v2.0 fails
# one job in five and exits 3, so the page's Apps section flags it and its
# Scripts section shows each run's exit code.
#
#   examples/apm-apps/demo.sh            # monitor page on http://127.0.0.1:9100/
#   ELEPHC=target/release/elephc examples/apm-apps/demo.sh
#
# Ports: collector 9475, page 9100 (override with COLLECTOR_PORT, DASHBOARD_PORT).
# History goes to STORE (default: a temp dir).
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
elephc=${ELEPHC:-$root/target/debug/elephc}
collector_port=${COLLECTOR_PORT:-9475}
dashboard_port=${DASHBOARD_PORT:-9100}
store=${STORE:-$(mktemp -d "${TMPDIR:-/tmp}/elephc-apm-apps.XXXXXX")}
work=$(mktemp -d "${TMPDIR:-/tmp}/elephc-apm-apps-build.XXXXXX")

if [[ ! -x "$elephc" ]]; then
  echo "building elephc..."
  (cd "$root" && cargo build)
fi

# One key for both versions: the collector authenticates every instance with it.
export ELEPHC_PROBE_KEY
ELEPHC_PROBE_KEY=$(od -An -tx1 -N32 /dev/urandom | tr -d ' \n')
for version in 1.0 2.0; do
  echo "compiling shopkit v$version with --with-apm --apm-collector..."
  mkdir -p "$work/v$version"
  cp "$here/main.php" "$work/v$version/shopkit.php"
  ELEPHC_BUILD_ID="v$version" "$elephc" build --with-apm --apm-collector "127.0.0.1:$collector_port" \
    --apm-transaction handle_job "$work/v$version/shopkit.php"
done

pids=()
cleanup() {
  for pid in "${pids[@]}"; do kill -INT "$pid" 2>/dev/null || true; done
  wait 2>/dev/null || true
  rm -rf "$work"
  echo
  echo "stopped. history kept in $store"
}
trap cleanup EXIT
trap 'exit 130' INT TERM

"$elephc" monitor --apm --listen "127.0.0.1:$collector_port" --key "$work/v1.0/shopkit.key" \
  --store "$store" --serve "127.0.0.1:$dashboard_port" &
pids+=($!)
for _ in $(seq 100); do
  curl -s -o /dev/null "http://127.0.0.1:$dashboard_port/" && break
  sleep 0.1
done

echo
echo "monitor page: http://127.0.0.1:$dashboard_port/   (Ctrl-C to stop)"
echo

# Turns of runs: two v1.0 instances, then two v2.0 ones beside them. A failed
# run is part of the demo, not a reason to stop.
set +e
while true; do
  JOBS=40 FAIL_EVERY=50 "$work/v1.0/shopkit" >/dev/null 2>&1 &
  JOBS=25 FAIL_EVERY=50 "$work/v1.0/shopkit" >/dev/null 2>&1 &
  sleep 3
  JOBS=30 FAIL_EVERY=5 EXIT_CODE=3 "$work/v2.0/shopkit" >/dev/null 2>&1 &
  JOBS=20 FAIL_EVERY=5 EXIT_CODE=3 "$work/v2.0/shopkit" >/dev/null 2>&1 &
  wait_runs=$(jobs -p | grep -v -x -F "$(printf '%s\n' "${pids[@]}")")
  for pid in $wait_runs; do wait "$pid"; done
done
