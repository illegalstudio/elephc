#!/usr/bin/env bash
# HTTP throughput benchmark for an elephc `--web` binary.
#
# Compiles one PHP fixture with `--web`, starts the prefork server, drives it
# with `wrk` (and optionally k6), and reports per-round samples plus a median.
# The default fixture is the minimal handler used by the FrankenPHP-vs-PHP-FPM
# article: one echo and one `$_SERVER` read. A round with non-2xx responses or
# socket errors is a failed benchmark and exits non-zero.
#
# This is deliberately separate from `benchmark/run.py`: that runner times
# short-lived processes, while this one owns a long-running server and an
# external load generator. Results are not appended to the process-execution
# history.
#
# Examples:
#   ./benchmark/web/run.sh
#   ./benchmark/web/run.sh --workers 4 --threads 4 --connections 100 --duration 10s
#   ./benchmark/web/run.sh --isolation pool --workers 8 --threads 4 --connections 32
#   ./benchmark/web/run.sh --compiler target/release/elephc --no-build \
#       --json web-results.json --markdown web-results.md
#   ./benchmark/web/run.sh --k6 vus --k6 constant
#   ./benchmark/web/run.sh --php-baseline
#
# `--k6` requires k6 and jq. `--php-baseline` requires php. `wrk` is always
# required.

set -o pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd "$SCRIPT_DIR/../.." && pwd)
# shellcheck source=lib.sh
source "$SCRIPT_DIR/lib.sh"

DEFAULT_FIXTURE="$SCRIPT_DIR/hello.php"
WORK_DIR=""
SERVER_PID=""
SERVER_URL=""
PHP_PID=""

# Removes the server, the PHP baseline, and the temporary work directory.
cleanup() {
  stop_process "$PHP_PID"
  PHP_PID=""
  stop_process "$SERVER_PID"
  SERVER_PID=""
  if [[ -n $WORK_DIR && -d $WORK_DIR ]]; then
    rm -rf "$WORK_DIR"
  fi
}

trap cleanup EXIT INT TERM

# Prints the command-line help.
usage() {
  cat <<'EOF'
Usage: benchmark/web/run.sh [options]

Options:
  --fixture PATH          PHP fixture to compile (default: benchmark/web/hello.php)
  --compiler PATH         elephc binary (default: target/release/elephc)
  --no-build              Do not run `cargo build --release`
  --workers N             Prefork web workers (default: 4)
  --isolation MODE        worker | pool | request (default: worker)
  --clients N             Concurrent wrk processes (default: 1)
  --threads N             wrk threads per client (default: 4)
  --connections N         wrk connections per client (default: 100)
  --duration DURATION     wrk duration per round (default: 10s)
  --warmup DURATION       warmup duration; 0 skips (default: 2s)
  --repeat N              measured rounds (default: 3)
  --cpu                   Sample server user/system CPU per request
  --k6 SCENARIO           Also run k6: vus or constant (repeatable)
  --php-baseline          Compare with the single-process `php -S` server
  --json PATH             Write the JSON report
  --markdown PATH         Write the Markdown report
  --timeout SECONDS       kill a hung wrk/k6 process after this (default: 120)
  -h, --help              Show this help
EOF
}

# Builds the release compiler unless `--no-build` was requested.
build_compiler() {
  local no_build=$1 compiler=$2
  if [[ -n $compiler ]]; then
    if [[ $compiler != */* ]]; then
      compiler=$(command -v "$compiler" || true)
      [[ -n $compiler ]] || die "compiler not found: $2"
    fi
    compiler=$(cd "$(dirname "$compiler")" && pwd)/$(basename "$compiler")
    [[ -x $compiler ]] || die "compiler is not executable: $compiler"
    printf '%s\n' "$compiler"
    return
  fi
  compiler="$ROOT/target/release/elephc"
  if [[ $no_build -eq 0 ]]; then
    (cd "$ROOT" && cargo build --release >/dev/null) || die "cargo build --release failed"
  fi
  [[ -x $compiler ]] || die "--no-build requested but $compiler does not exist"
  printf '%s\n' "$compiler"
}

# Compiles the fixture with the selected isolation mode and returns the binary.
compile_fixture() {
  local compiler=$1 fixture=$2 work=$3 isolation=$4
  cp "$fixture" "$work/app.php"
  if ! (cd "$work" && "$compiler" --web --web-isolation "$isolation" --quiet app.php >compile.log 2>&1); then
    cat "$work/compile.log" >&2
    die "compilation failed"
  fi
  [[ -x $work/app ]] || die "compilation did not produce $work/app"
  printf '%s\n' "$work/app"
}

# Starts the compiled server on a free port and waits until it answers.
start_server() {
  local binary=$1 workers=$2 work=$3 attempt port log url
  for attempt in 1 2 3; do
    port=$(free_port)
    log="$work/server.log"
    url="http://127.0.0.1:$port/"
    "$binary" --listen "127.0.0.1:$port" --workers "$workers" >"$log" 2>&1 &
    SERVER_PID=$!
    if wait_for_server "$SERVER_PID" "$url" "$log"; then
      SERVER_URL=$url
      return 0
    fi
    stop_process "$SERVER_PID"
    SERVER_PID=""
  done
  die "could not start the web server"
}

# Builds the environment JSON recorded in the report.
environment_json() {
  local compiler=$1 version wrk_path k6_path platform machine processor
  version=$("$compiler" --version 2>/dev/null | head -1)
  wrk_path=$(command -v wrk 2>/dev/null || echo missing)
  k6_path=$(command -v k6 2>/dev/null || echo missing)
  platform=$(uname -srm)
  machine=$(uname -m)
  processor=$(uname -p 2>/dev/null || uname -m)
  printf '{ "platform": %s, "machine": %s, "processor": %s, "cpu_count": %s, "bash": %s, "compiler": %s, "compiler_version": %s, "wrk": %s, "k6": %s, "load_average": %s }' \
    "$(json_string "$platform")" \
    "$(json_string "$machine")" \
    "$(json_string "$processor")" \
    "$(json_number_or_null "$(cpu_count)")" \
    "$(json_string "${BASH_VERSION:-unknown}")" \
    "$(json_string "$compiler")" \
    "$(json_string "$version")" \
    "$(json_string "$wrk_path")" \
    "$(json_string "$k6_path")" \
    "$(load_average_json)"
}

# Reduces the `round-N.stats` files to the JSON summary and the `SUMMARY_*`
# globals used by the Markdown report.
summarize_stats() {
  local dir=$1
  local stats=()
  local file
  local rps_file="$dir/summary-rps"
  local avg_file="$dir/summary-avg"
  local p99_file="$dir/summary-p99"
  local cpu_file="$dir/summary-cpu"
  local user_file="$dir/summary-user"
  local system_file="$dir/summary-system"
  : >"$rps_file"
  : >"$avg_file"
  : >"$p99_file"
  : >"$cpu_file"
  : >"$user_file"
  : >"$system_file"
  for file in "$dir"/round-*.stats; do
    [[ -e $file ]] || continue
    stats+=("$file")
    awk -F'\t' '{ print $1 }' "$file" >>"$rps_file"
    awk -F'\t' '$2 != "-" && $2 != "null" { print $2 }' "$file" >>"$avg_file"
    awk -F'\t' '$3 != "-" && $3 != "null" { print $3 }' "$file" >>"$p99_file"
    awk -F'\t' 'NF >= 7 && $5 != "-" { print $5 }' "$file" >>"$cpu_file"
    awk -F'\t' 'NF >= 7 && $6 != "-" { print $6 }' "$file" >>"$user_file"
    awk -F'\t' 'NF >= 7 && $7 != "-" { print $7 }' "$file" >>"$system_file"
  done
  SUMMARY_ROUNDS=${#stats[@]}
  SUMMARY_RPS_MEDIAN=$(median_values <"$rps_file")
  SUMMARY_RPS_MIN=$(min_values <"$rps_file")
  SUMMARY_RPS_MAX=$(max_values <"$rps_file")
  SUMMARY_AVG_MEDIAN=$(median_values <"$avg_file")
  SUMMARY_P99_MEDIAN=$(median_values <"$p99_file")
  SUMMARY_CPU_MEDIAN=$(median_values <"$cpu_file")
  SUMMARY_USER_MEDIAN=$(median_values <"$user_file")
  SUMMARY_SYSTEM_MEDIAN=$(median_values <"$system_file")
  printf '{ "rounds": %s, "requests_per_sec_median": %s, "requests_per_sec_min": %s, "requests_per_sec_max": %s, "latency_avg_ms_median": %s, "latency_p99_ms_median": %s, "server_cpu_us_per_request_median": %s, "server_user_us_per_request_median": %s, "server_system_us_per_request_median": %s }' \
    "$SUMMARY_ROUNDS" \
    "$(json_number_or_null "$SUMMARY_RPS_MEDIAN")" \
    "$(json_number_or_null "$SUMMARY_RPS_MIN")" \
    "$(json_number_or_null "$SUMMARY_RPS_MAX")" \
    "$(json_number_or_null "$SUMMARY_AVG_MEDIAN")" \
    "$(json_number_or_null "$SUMMARY_P99_MEDIAN")" \
    "$(json_number_or_null "$SUMMARY_CPU_MEDIAN")" \
    "$(json_number_or_null "$SUMMARY_USER_MEDIAN")" \
    "$(json_number_or_null "$SUMMARY_SYSTEM_MEDIAN")"
}

# Renders the Markdown report for one run.
markdown_report() {
  local fixture=$1 isolation=$2 workers=$3 threads=$4 connections=$5 duration=$6 clients=$7
  local stats_dir=$8 k6_tsv=$9 php_tsv=${10}
  local index=0 file rps avg p99 p99_text cpu_text
  printf '# elephc web benchmark: %s\n\n' "$fixture"
  printf -- '- Server: `elephc --web` (isolation=%s), workers=%s\n' "$isolation" "$workers"
  printf -- '- Load: `wrk -t%s -c%s -d%s` x%s client(s)\n' "$threads" "$connections" "$duration" "$clients"
  printf -- '- Platform: %s (%s CPUs)\n\n' "$(uname -srm)" "$(cpu_count)"
  printf '| Repeat | Requests/sec | Avg latency | p99 latency |\n'
  printf '|---:|---:|---:|---:|\n'
  for file in "$stats_dir"/round-*.stats; do
    [[ -e $file ]] || continue
    index=$((index + 1))
    rps=$(awk -F'\t' '{ print $1 }' "$file")
    avg=$(awk -F'\t' '{ print $2 }' "$file")
    p99=$(awk -F'\t' '{ print $3 }' "$file")
    if [[ $p99 == "-" || $p99 == "null" ]]; then
      p99_text="n/a"
    else
      p99_text=$(printf '%.2f ms' "$p99")
    fi
    printf '| %s | %s | %s | %s |\n' \
      "$index" \
      "$(awk -v value="$rps" 'BEGIN { printf "%.0f", value }')" \
      "$(awk -v value="$avg" 'BEGIN { printf "%.2f ms", value }')" \
      "$p99_text"
  done
  p99_text="n/a"
  if [[ $SUMMARY_P99_MEDIAN != "null" ]]; then
    p99_text=$(printf '%.2f ms' "$SUMMARY_P99_MEDIAN")
  fi
  printf '\nMedian: **%s req/s**, average latency **%s ms**, p99 **%s**.\n' \
    "$(awk -v value="$SUMMARY_RPS_MEDIAN" 'BEGIN { printf "%.0f", value }')" \
    "$(awk -v value="$SUMMARY_AVG_MEDIAN" 'BEGIN { printf "%.2f", value }')" \
    "$p99_text"
  if [[ $cpu_enabled -eq 1 && $SUMMARY_CPU_MEDIAN != "null" ]]; then
    printf '\nServer CPU: **%.2f µs/request** (user %.2f, sys %.2f; median).\n' \
      "$SUMMARY_CPU_MEDIAN" "$SUMMARY_USER_MEDIAN" "$SUMMARY_SYSTEM_MEDIAN"
  fi
  if [[ -s $k6_tsv ]]; then
    printf '\n## k6\n\n| Scenario | HTTP req/s | avg | p95 | failed |\n|---|---:|---:|---:|---:|\n'
    while IFS=$'\t' read -r scenario rate avg_ms p95_ms failed; do
      printf '| %s | %s | %s ms | %s ms | %.2f%% |\n' \
        "$scenario" "$rate" "$avg_ms" "$p95_ms" \
        "$(awk -v value="$failed" 'BEGIN { printf "%.2f", value * 100 }')"
    done <"$k6_tsv"
  fi
  if [[ -s $php_tsv ]]; then
    printf '\n## PHP built-in server baseline\n\n'
    printf '`php -S` median: **%s req/s** (single process, not PHP-FPM).\n' \
      "$(awk -F'\t' '{ printf "%.0f", $1 }' "$php_tsv")"
  fi
}

# Runs one k6 scenario and writes its normalized report plus a TSV row.
run_k6_scenario() {
  local scenario=$1 url=$2 duration=$3 work=$4 timeout=$5
  require_cmd k6 "install k6 and retry"
  require_cmd jq "install jq for k6 summary parsing"
  local script="$SCRIPT_DIR/k6-$scenario.js"
  local summary="$work/k6-$scenario.json"
  local log="$work/k6-$scenario.log"
  local report="$work/k6-$scenario.report.json"
  local pid watchdog status
  local command="k6 run --quiet --summary-export $summary --env TARGET=$url --env DURATION=$duration $script"
  k6 run --quiet --summary-export "$summary" --env "TARGET=$url" --env "DURATION=$duration" "$script" >"$log" 2>&1 &
  pid=$!
  (
    sleep "$timeout"
    kill -TERM "$pid" 2>/dev/null || true
    sleep 1
    kill -KILL "$pid" 2>/dev/null || true
  ) >/dev/null 2>&1 &
  watchdog=$!
  wait "$pid"
  status=$?
  kill -TERM "$watchdog" 2>/dev/null || true
  wait "$watchdog" 2>/dev/null || true
  if [[ $status -ne 0 ]]; then
    cat "$log" >&2
    die "k6 exited with $status"
  fi
  local rate avg p95 failed metrics raw
  rate=$(jq -r '.metrics.http_reqs.rate // .metrics.http_reqs.values.rate // 0' "$summary")
  avg=$(jq -r '.metrics.http_req_duration.avg // .metrics.http_req_duration.values.avg // 0' "$summary")
  p95=$(jq -r '.metrics.http_req_duration["p(95)"] // .metrics.http_req_duration.values["p(95)"] // 0' "$summary")
  failed=$(jq -r '.metrics.http_req_failed.value // .metrics.http_req_failed.values.rate // 0' "$summary")
  metrics=$(jq -c '.metrics' "$summary")
  raw=$(cat "$log")
  printf '{ "scenario": %s, "command": %s, "metrics": %s, "http_reqs_per_sec": %s, "http_req_avg_ms": %s, "http_req_p95_ms": %s, "http_req_failed_rate": %s, "raw": %s }\n' \
    "$(json_string "$scenario")" \
    "$(json_string "$command")" \
    "$metrics" \
    "$(json_number_or_null "$rate")" \
    "$(json_number_or_null "$avg")" \
    "$(json_number_or_null "$p95")" \
    "$(json_number_or_null "$failed")" \
    "$(json_string "$raw")" >"$report"
  printf '%s\t%s\t%s\t%s\t%s\n' "$scenario" "$rate" "$avg" "$p95" "$failed" >>"$work/k6.tsv"
}

# Runs the single-process PHP built-in server baseline and writes its summary.
run_php_baseline() {
  local fixture=$1 work=$2 threads=$3 connections=$4 duration=$5 timeout=$6 clients=$7
  require_cmd php "install php for --php-baseline"
  local document_root="$work/php-root"
  local rounds="$work/php-rounds"
  local port url log
  mkdir -p "$document_root" "$rounds"
  cp "$fixture" "$document_root/index.php"
  port=$(free_port)
  url="http://127.0.0.1:$port/"
  log="$work/php-server.log"
  php -S "127.0.0.1:$port" -t "$document_root" >"$log" 2>&1 &
  PHP_PID=$!
  if ! wait_for_server "$PHP_PID" "$url" "$log"; then
    die "php -S did not start"
  fi
  run_wrk_round "$url" "$threads" "$connections" "$duration" "$timeout" "$clients" "$rounds" 1 || die "php baseline wrk failed"
  cp "$rounds/round-1.wrk.json" "$rounds/round-1.json"
  stop_process "$PHP_PID"
  PHP_PID=""
  summarize_stats "$rounds" >"$work/php-summary.json"
  printf '%s\n' "$SUMMARY_RPS_MEDIAN" >"$work/php.tsv"
}

# Parses the CLI, runs the benchmark, and writes the reports.
main() {
  local fixture=$DEFAULT_FIXTURE
  local compiler_arg=""
  local no_build=0
  local workers=4
  local isolation=worker
  local clients=1
  local threads=4
  local connections=100
  local duration=10s
  local warmup=2s
  local repeat=3
  local cpu_enabled=0
  local php_enabled=0
  local json_out=""
  local markdown_out=""
  local timeout=120
  local -a k6_scenarios
  local scenario
  while (($#)); do
    case $1 in
      --fixture) require_value "$@"; fixture=$2; shift 2 ;;
      --compiler) require_value "$@"; compiler_arg=$2; shift 2 ;;
      --no-build) no_build=1; shift ;;
      --workers) require_value "$@"; workers=$2; shift 2 ;;
      --isolation) require_value "$@"; isolation=$2; shift 2 ;;
      --clients) require_value "$@"; clients=$2; shift 2 ;;
      --threads) require_value "$@"; threads=$2; shift 2 ;;
      --connections) require_value "$@"; connections=$2; shift 2 ;;
      --duration) require_value "$@"; duration=$2; shift 2 ;;
      --warmup) require_value "$@"; warmup=$2; shift 2 ;;
      --repeat) require_value "$@"; repeat=$2; shift 2 ;;
      --cpu) cpu_enabled=1; shift ;;
      --k6) require_value "$@"; k6_scenarios+=("$2"); shift 2 ;;
      --php-baseline) php_enabled=1; shift ;;
      --json) require_value "$@"; json_out=$2; shift 2 ;;
      --markdown) require_value "$@"; markdown_out=$2; shift 2 ;;
      --timeout) require_value "$@"; timeout=$2; shift 2 ;;
      -h|--help) usage; exit 0 ;;
      *) die "unknown argument: $1" ;;
    esac
  done

  require_cmd wrk "install it (brew install wrk) and retry"
  [[ $clients -ge 1 ]] || die "--clients must be at least 1"
  [[ $repeat -ge 1 ]] || die "--repeat must be at least 1"
  case $isolation in
    worker|pool|request) ;;
    *) die "--isolation must be worker, pool, or request" ;;
  esac
  for scenario in "${k6_scenarios[@]}"; do
    case $scenario in
      vus|constant) ;;
      *) die "--k6 must be vus or constant" ;;
    esac
  done
  [[ -f $fixture ]] || die "fixture not found: $fixture"

  local compiler
  compiler=$(build_compiler "$no_build" "$compiler_arg")
  fixture=$(cd "$(dirname "$fixture")" && pwd)/$(basename "$fixture")
  WORK_DIR=$(mktemp -d "${TMPDIR:-/tmp}/elephc-web-bench.XXXXXX")
  local rounds="$WORK_DIR/rounds"
  mkdir -p "$rounds"
  local binary
  binary=$(compile_fixture "$compiler" "$fixture" "$WORK_DIR" "$isolation")
  start_server "$binary" "$workers" "$WORK_DIR" || die "could not start the web server"
  local url=$SERVER_URL

  if [[ $warmup != 0 ]]; then
    mkdir -p "$WORK_DIR/warmup"
    run_wrk_round "$url" "$threads" "$connections" "$warmup" "$timeout" "$clients" "$WORK_DIR/warmup" warmup >/dev/null || die "warmup failed"
    rm -rf "$WORK_DIR/warmup"
  fi

  local round stats_file wrk_json round_json before_user before_system after_user after_system
  local user sys requests cpu_seconds cpu_us user_us sys_us cpu_fields stats_line
  local -a round_jsons
  for ((round = 1; round <= repeat; round += 1)); do
    before_user=0
    before_system=0
    if [[ $cpu_enabled -eq 1 ]]; then
      read -r before_user before_system <<<"$(server_cpu_split "$SERVER_PID")"
    fi
    run_wrk_round "$url" "$threads" "$connections" "$duration" "$timeout" "$clients" "$rounds" "$round" || die "wrk round $round failed"
    stats_file="$rounds/round-$round.stats"
    wrk_json="$rounds/round-$round.wrk.json"
    round_json="$rounds/round-$round.json"
    if [[ $cpu_enabled -eq 1 ]]; then
      read -r after_user after_system <<<"$(server_cpu_split "$SERVER_PID")"
      user=$(awk -v before="$before_user" -v after="$after_user" 'BEGIN { value = after - before; if (value < 0) value = 0; printf "%.6f", value }')
      sys=$(awk -v before="$before_system" -v after="$after_system" 'BEGIN { value = after - before; if (value < 0) value = 0; printf "%.6f", value }')
      requests=$(awk -F'\t' '{ print $4 }' "$stats_file")
      if awk -v value="$requests" 'BEGIN { exit !(value > 0) }'; then
        cpu_seconds=$(awk -v user="$user" -v sys="$sys" 'BEGIN { printf "%.6f", user + sys }')
        cpu_us=$(awk -v seconds="$cpu_seconds" -v requests="$requests" 'BEGIN { printf "%.6f", seconds / requests * 1000000 }')
        user_us=$(awk -v seconds="$user" -v requests="$requests" 'BEGIN { printf "%.6f", seconds / requests * 1000000 }')
        sys_us=$(awk -v seconds="$sys" -v requests="$requests" 'BEGIN { printf "%.6f", seconds / requests * 1000000 }')
        stats_line=$(awk -F'\t' -v cpu_us="$cpu_us" -v user_us="$user_us" -v sys_us="$sys_us" 'BEGIN { OFS = "\t" } { print $1, $2, $3, $4, cpu_us, user_us, sys_us }' "$stats_file")
        printf '%s\n' "$stats_line" >"$stats_file"
        cpu_fields="\"server_user_cpu_seconds\": $user, \"server_system_cpu_seconds\": $sys, \"server_cpu_seconds\": $cpu_seconds, \"server_cpu_us_per_request\": $cpu_us, \"server_user_us_per_request\": $user_us, \"server_system_us_per_request\": $sys_us"
        sed "s/}$/,$cpu_fields}/" "$wrk_json" >"$round_json"
      else
        cp "$wrk_json" "$round_json"
      fi
    else
      cp "$wrk_json" "$round_json"
    fi
    round_jsons+=("$round_json")
  done

  local k6_enabled=0
  : >"$WORK_DIR/k6.tsv"
  if [[ ${#k6_scenarios[@]} -gt 0 ]]; then
    k6_enabled=1
    for scenario in "${k6_scenarios[@]}"; do
      run_k6_scenario "$scenario" "$url" "$duration" "$WORK_DIR" "$timeout"
    done
  fi

  stop_process "$SERVER_PID"
  SERVER_PID=""
  : >"$WORK_DIR/php.tsv"
  if [[ $php_enabled -eq 1 ]]; then
    run_php_baseline "$fixture" "$WORK_DIR" "$threads" "$connections" "$duration" "$timeout" "$clients"
  fi

  local wrk_summary
  summarize_stats "$rounds" >"$WORK_DIR/wrk-summary.json"
  wrk_summary=$(cat "$WORK_DIR/wrk-summary.json")
  local environment
  environment=$(environment_json "$compiler")
  local raw_rounds="" round_json_file
  for round_json_file in "${round_jsons[@]}"; do
    if [[ -n $raw_rounds ]]; then
      raw_rounds+=$',\n'
    fi
    raw_rounds+=$(cat "$round_json_file")
  done
  local k6_json=""
  if [[ $k6_enabled -eq 1 ]]; then
    local k6_file
    for k6_file in "$WORK_DIR"/k6-*.report.json; do
      [[ -e $k6_file ]] || continue
      if [[ -n $k6_json ]]; then
        k6_json+=$','
      fi
      k6_json+=$(cat "$k6_file")
    done
  fi
  local report
  report=$(markdown_report "$fixture" "$isolation" "$workers" "$threads" "$connections" "$duration" "$clients" "$rounds" "$WORK_DIR/k6.tsv" "$WORK_DIR/php.tsv")
  printf '%s\n' "$report"

  if [[ -n $json_out ]]; then
    {
      printf '{\n'
      printf '  "fixture": %s,\n' "$(json_string "$fixture")"
      printf '  "isolation": %s,\n' "$(json_string "$isolation")"
      printf '  "workers": %s,\n' "$workers"
      printf '  "clients": %s,\n' "$clients"
      printf '  "cpu": %s,\n' "$([[ $cpu_enabled -eq 1 ]] && echo true || echo false)"
      printf '  "threads": %s,\n' "$threads"
      printf '  "connections": %s,\n' "$connections"
      printf '  "duration": %s,\n' "$(json_string "$duration")"
      printf '  "environment": %s,\n' "$environment"
      printf '  "wrk": { "summary": %s, "raw_rounds": [\n' "$wrk_summary"
      printf '%s\n' "$raw_rounds"
      printf '  ] }'
      if [[ $k6_enabled -eq 1 ]]; then
        printf ',\n  "k6": [%s]' "$k6_json"
      fi
      if [[ $php_enabled -eq 1 ]]; then
        printf ',\n  "php": { "summary": %s, "raw_rounds": [%s] }' \
          "$(cat "$WORK_DIR/php-summary.json")" \
          "$(cat "$WORK_DIR/php-rounds/round-1.json")"
      fi
      printf '\n}\n'
    } >"$json_out"
  fi
  if [[ -n $markdown_out ]]; then
    printf '%s\n' "$report" >"$markdown_out"
  fi
}

main "$@"
