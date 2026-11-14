#!/usr/bin/env bash
# Unit checks for the bash web benchmark helpers.
#
# Run directly with `bash benchmark/tests/test_web_bash.sh`. These cover the
# parsing and formatting that used to live in `test_web_runner.py` and
# `test_fpm_compare.py`; the end-to-end runners are exercised by the benchmark
# smoke commands in the README.

set -o pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
WEB_DIR=$(cd "$SCRIPT_DIR/../web" && pwd)
# shellcheck source=../web/lib.sh
source "$WEB_DIR/lib.sh"

failures=0

# Asserts that two strings are equal.
assert_eq() {
  local expected=$1 actual=$2 label=$3
  if [[ $expected != "$actual" ]]; then
    printf 'FAIL %s: expected %q, got %q\n' "$label" "$expected" "$actual" >&2
    failures=$((failures + 1))
  fi
}

assert_eq "1.500000" "$(parse_ms "1500us")" "parse_ms us"
assert_eq "250.000000" "$(parse_ms "250ms")" "parse_ms ms"
assert_eq "2000.000000" "$(parse_ms "2s")" "parse_ms s"
assert_eq "2.0000000000" "$(printf '1\n2\n3\n' | median_values)" "median odd"
assert_eq "2.5000000000" "$(printf '1\n2\n3\n4\n' | median_values)" "median even"
assert_eq 'a\"b\\c' "$(printf '%s' 'a"b\c' | json_escape)" "json escape"
assert_eq "0.50x" "$(ratio 10 5)" "ratio"
assert_eq "n/a" "$(ratio 0 5)" "ratio zero"

parse_origin "http://127.0.0.1:8080/base?x=1" >"$SCRIPT_DIR/.origin-test"
{
  read -r host
  read -r port
  read -r target
} <"$SCRIPT_DIR/.origin-test"
rm -f "$SCRIPT_DIR/.origin-test"
assert_eq "127.0.0.1" "$host" "origin host"
assert_eq "8080" "$port" "origin port"
assert_eq "/base?x=1" "$target" "origin target"

parse_origin "http://127.0.0.1:8080?x=1" >"$SCRIPT_DIR/.origin-test"
{
  read -r host
  read -r port
  read -r target
} <"$SCRIPT_DIR/.origin-test"
rm -f "$SCRIPT_DIR/.origin-test"
assert_eq "127.0.0.1" "$host" "origin query host"
assert_eq "8080" "$port" "origin query port"
assert_eq "/?x=1" "$target" "origin query target"

wrk_sample="$SCRIPT_DIR/.wrk-sample"
cat >"$wrk_sample" <<'EOF'
Running 1s test @ http://127.0.0.1:8080/
  2 threads and 16 connections
  Thread Stats   Avg      Stdev     Max   +/- Stdev
    Latency     8.35ms   17.35ms 146.54ms   88.14%
    Req/Sec    26.24k    17.57k   69.12k    60.00%
  Latency Distribution
     50%  112.00us
     75%    7.73ms
     90%   30.37ms
     99%   77.96ms
  52333 requests in 1.01s, 5.79MB read
  Socket errors: connect 0, read 2, write 0, timeout 0
  Non-2xx or 3xx responses: 3
Requests/sec:  52059.32
Transfer/sec:      5.76MB
EOF
wrk_command=""
parse_wrk_file "$wrk_sample"
rm -f "$wrk_sample"
assert_eq "52059.32" "$wrk_requests_per_sec" "wrk rps"
assert_eq "8.350000" "$wrk_latency_avg_ms" "wrk avg"
assert_eq "0.112000" "$wrk_latency_p50_ms" "wrk p50"
assert_eq "77.960000" "$wrk_latency_p99_ms" "wrk p99"
assert_eq "52333" "$wrk_requests" "wrk requests"
assert_eq "26.24k" "$wrk_req_per_sec_avg" "wrk req/sec avg"
assert_eq "3" "$wrk_non_2xx" "wrk non-2xx"
case $wrk_socket_errors in
  *"read 2"*) ;;
  *) printf 'FAIL wrk socket errors: %s\n' "$wrk_socket_errors" >&2; failures=$((failures + 1)) ;;
esac
assert_eq "2" "$(wrk_socket_error_count)" "wrk socket error count"
assert_eq "5" "$(wrk_failure_count)" "wrk failure count"
wrk_non_2xx=0
wrk_socket_errors="connect 0, read 0, write 0, timeout 0"
assert_eq "0" "$(wrk_socket_error_count)" "wrk clean socket error count"
assert_eq "0" "$(wrk_failure_count)" "wrk clean failure count"

if "$WEB_DIR/run.sh" --workers >/dev/null 2>&1; then
  printf 'FAIL run.sh accepted a missing option value\n' >&2
  failures=$((failures + 1))
fi
if "$WEB_DIR/fpm_compare.sh" --threads >/dev/null 2>&1; then
  printf 'FAIL fpm_compare.sh accepted a missing option value\n' >&2
  failures=$((failures + 1))
fi

if ! "$WEB_DIR/run.sh" --help >/dev/null 2>&1; then
  printf 'FAIL run.sh --help\n' >&2
  failures=$((failures + 1))
fi
if ! "$WEB_DIR/fpm_compare.sh" --help >/dev/null 2>&1; then
  printf 'FAIL fpm_compare.sh --help\n' >&2
  failures=$((failures + 1))
fi

# A socket-error-only sample must still be saved, reported, and rejected.
tmp=$(mktemp -d "${TMPDIR:-/tmp}/elephc-web-bash.XXXXXX")
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/bin"
cat >"$tmp/bin/wrk" <<'EOF'
#!/usr/bin/env bash
cat <<'REPORT'
Running 1s test @ http://127.0.0.1:1/
  2 threads and 4 connections
  Thread Stats   Avg      Stdev     Max   +/- Stdev
    Latency     1.00ms  500.00us   2.00ms   90.00%
    Req/Sec     1.00k   100.00     1.20k    80.00%
  Latency Distribution
     50%    1.00ms
     75%    1.20ms
     90%    1.50ms
     99%    2.00ms
  1000 requests in 1.00s, 1.00MB read
  Socket errors: connect 0, read 2, write 0, timeout 3
  Non-2xx or 3xx responses: 0
Requests/sec:   1000.00
Transfer/sec:      1.00MB
REPORT
EOF
chmod +x "$tmp/bin/wrk"
PATH="$tmp/bin:$PATH" "$WEB_DIR/fpm_compare.sh" \
  --elephc-url http://127.0.0.1:1/ \
  --fpm-url http://127.0.0.1:2/ \
  --skip-new-connection --repeat 1 --duration 1s \
  --json "$tmp/report.json" --markdown "$tmp/report.md" >"$tmp/stdout" 2>&1
socket_status=$?
if [[ $socket_status -eq 0 ]]; then
  printf 'FAIL fpm_compare accepted a socket-error-only run\n' >&2
  failures=$((failures + 1))
fi
if ! grep -q 'Invalid run' "$tmp/report.md" 2>/dev/null; then
  printf 'FAIL fpm_compare lost the invalid-run banner\n' >&2
  failures=$((failures + 1))
fi
if ! grep -q '| HTTP keep-alive | Failures | 5 | 5 | - |' "$tmp/report.md" 2>/dev/null; then
  printf 'FAIL fpm_compare lost the socket-error counts\n' >&2
  failures=$((failures + 1))
fi

if [[ $failures -ne 0 ]]; then
  printf '%s bash benchmark test(s) failed\n' "$failures" >&2
  exit 1
fi
printf 'bash web benchmark tests passed\n'
