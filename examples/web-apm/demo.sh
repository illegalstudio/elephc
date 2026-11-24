#!/usr/bin/env bash
# Transaction monitoring demo: builds the shop with --with-monitoring, starts it
# with --apm, starts the collector with an on-disk history and a live dashboard,
# then sends a mixed load until Ctrl-C.
#
#   examples/web-apm/demo.sh            # dashboard on http://127.0.0.1:9000/
#   ELEPHC=target/release/elephc examples/web-apm/demo.sh
#
# Ports: HTTP 8080, probe 9411, dashboard 9000 (override with HTTP_PORT,
# PROBE_PORT, DASHBOARD_PORT). History goes to STORE (default: a temp dir).
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
elephc=${ELEPHC:-$root/target/debug/elephc}
http_port=${HTTP_PORT:-8080}
probe_port=${PROBE_PORT:-9411}
dashboard_port=${DASHBOARD_PORT:-9000}
store=${STORE:-$(mktemp -d "${TMPDIR:-/tmp}/elephc-apm-demo.XXXXXX")}

if [[ ! -x "$elephc" ]]; then
  echo "building elephc..."
  (cd "$root" && cargo build)
fi

echo "compiling the shop with --web --with-monitoring..."
"$elephc" build --web --with-monitoring "$here/main.php"

pids=()
cleanup() {
  for pid in "${pids[@]}"; do kill -INT "$pid" 2>/dev/null || true; done
  wait 2>/dev/null || true
  echo
  echo "stopped. history kept in $store"
}
# Ctrl-C leaves the script; leaving runs the cleanup, once.
trap cleanup EXIT
trap 'exit 130' INT TERM

ELEPHC_PROBE_ADDR=127.0.0.1:$probe_port "$here/main" --listen 127.0.0.1:$http_port --workers 2 --apm &
pids+=($!)
for _ in $(seq 100); do
  curl -s -o /dev/null "http://127.0.0.1:$http_port/" && break
  sleep 0.1
done

"$elephc" monitor 127.0.0.1:$probe_port --apm --key "$here/main.key" \
  --store "$store" --resolution 5 --serve 127.0.0.1:$dashboard_port --capture-every 5 \
  --alert 'p95:GET /products<=100' --alert 'queries:GET /orders/{id}<=3' \
  --alert 'error_pct:GET /reports/{id}<=5' --alert 'p99:POST /checkout<=50' &
pids+=($!)

echo
echo "monitor page: http://127.0.0.1:$dashboard_port/   (Ctrl-C to stop)"
echo

# Mixed traffic with a slow phase every minute, so the charts have something to
# show. A failed request is part of the traffic, not a reason to stop.
set +e
i=0
while true; do
  base="http://127.0.0.1:$http_port"
  # Only this batch is waited for: a bare `wait` would also wait for the
  # server and the collector started above, and never return.
  batch=()
  curl -s -o /dev/null "$base/products/$((i % 60))" & batch+=($!)
  curl -s -o /dev/null "$base/products/$(((i * 7) % 60))" & batch+=($!)
  curl -s -o /dev/null "$base/products" & batch+=($!)
  curl -s -o /dev/null "$base/reports/$i" & batch+=($!)
  curl -s -o /dev/null "$base/orders/$((i % 25))" & batch+=($!)
  if ((i % 3 == 0)); then curl -s -o /dev/null -X POST "$base/checkout" & batch+=($!); fi
  if ((i % 20 == 0)); then curl -s -o /dev/null "$base/old-page" & batch+=($!); fi
  wait "${batch[@]}"
  if (((SECONDS / 30) % 2 == 1)); then sleep 0.05; fi
  i=$((i + 1))
done
