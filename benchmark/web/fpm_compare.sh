#!/usr/bin/env bash
# Compare an Elephc origin with an Nginx + PHP-FPM reference endpoint.
#
# Both endpoints must already be running. This script deliberately owns only the
# client side: it does not start, stop, or configure either server. Point
# `--elephc-url` at the Elephc origin (directly or through the edge under test)
# and `--fpm-url` at the Nginx endpoint that serves the equivalent fixture.
#
# It reports two distinct shapes:
#
#   * HTTP keep-alive (`wrk`) for steady-state throughput and latency;
#   * one new HTTP connection per request, sequentially, to expose
#     connection-setup costs without the client-side TIME_WAIT noise of a
#     high-concurrency churn run.
#
# The script is a comparison tool, not a historical benchmark sample. CI keeps
# its process-execution benchmarks separate.
#
# `wrk` is required. `curl` is required for the new-connection scenario.

set -o pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=lib.sh
source "$SCRIPT_DIR/lib.sh"

WRK_SCENARIO="HTTP keep-alive"
CONNECTION_SCENARIO="new HTTP connection"
WORK_DIR=""

# Removes the temporary work directory.
cleanup() {
  if [[ -n $WORK_DIR && -d $WORK_DIR ]]; then
    rm -rf "$WORK_DIR"
  fi
}

trap cleanup EXIT INT TERM

# Prints the command-line help.
usage() {
  cat <<'EOF'
Usage: benchmark/web/fpm_compare.sh --elephc-url URL --fpm-url URL [options]

Options:
  --elephc-url URL        Elephc origin (required)
  --fpm-url URL           Nginx + PHP-FPM origin (required)
  --threads N             wrk threads (default: 4)
  --connections N         wrk connections (default: 100)
  --duration DURATION     wrk duration per sample (default: 10s)
  --repeat N              samples per endpoint (default: 3)
  --new-connections N     sequential fresh connections per sample (default: 500)
  --skip-keepalive        Skip the wrk keep-alive scenario
  --skip-new-connection   Skip the sequential new-connection scenario
  --json PATH             Write the JSON report
  --markdown PATH         Write the Markdown report
  --timeout SECONDS       curl timeout for one new connection (default: 5)
  --wrk-timeout SECONDS   watchdog for one keep-alive sample (default: 120)
  -h, --help              Show this help
EOF
}

# Measures one fresh HTTP connection with curl and prints `elapsed_ms status`.
new_connection_sample() {
  local url=$1 timeout=$2 output status seconds host port target
  {
    read -r host
    read -r port
    read -r target
  } < <(parse_origin "$url")
  output=$(curl -sS -o /dev/null -w '%{http_code} %{time_total}' \
    --max-time "$timeout" -H "Host: $host" -H 'Connection: close' "$url" 2>/dev/null) || return 1
  status=${output%% *}
  seconds=${output##* }
  [[ -n $status && -n $seconds ]] || return 1
  printf '%s %s\n' "$(awk -v seconds="$seconds" 'BEGIN { printf "%.6f", seconds * 1000 }')" "$status"
}

# Runs one sequential new-connection sample and writes its JSON plus a TSV row.
measure_new_connections() {
  local url=$1 count=$2 timeout=$3 json=$4
  local samples_file attempt elapsed status failures=0
  local requests=0 requests_per_sec=0 p50="null" p99="null"
  local p50_row p99_row line
  samples_file="$WORK_DIR/new-samples.$$"
  : >"$samples_file"
  for ((attempt = 1; attempt <= count; attempt += 1)); do
    if ! line=$(new_connection_sample "$url" "$timeout"); then
      failures=$((failures + 1))
      continue
    fi
    read -r elapsed status <<<"$line"
    if [[ $status != 200 ]]; then
      failures=$((failures + 1))
      continue
    fi
    printf '%s\n' "$elapsed" >>"$samples_file"
  done
  if [[ -s $samples_file ]]; then
    sort -n "$samples_file" -o "$samples_file"
    requests=$(wc -l <"$samples_file" | tr -d ' ')
    requests_per_sec=$(awk -v count="$requests" '{ sum += $1 } END { if (sum > 0) printf "%.6f", 1000 * count / sum; else print "0" }' "$samples_file")
    p50_row=$((requests / 2 + 1))
    p99_row=$(awk -v count="$requests" 'BEGIN { value = int(count * 0.99) + 1; if (value > count) value = count; print value }')
    p50=$(sed -n "${p50_row}p" "$samples_file")
    p99=$(sed -n "${p99_row}p" "$samples_file")
  fi
  rm -f "$samples_file"
  printf '{ "requests": %s, "failures": %s, "requests_per_sec": %s, "latency_p50_ms": %s, "latency_p99_ms": %s }\n' \
    "$(json_number_or_null "$requests")" \
    "$(json_number_or_null "$failures")" \
    "$(json_number_or_null "$requests_per_sec")" \
    "$(json_number_or_null "$p50")" \
    "$(json_number_or_null "$p99")" >"$json"
  printf '%s\t%s\t%s\t%s\t%s\n' \
    "$requests" "$failures" "$requests_per_sec" \
    "${p50:--}" "${p99:--}" >>"${json%/*}/samples.tsv"
}

# Runs one wrk sample and writes its JSON plus a TSV row.
measure_wrk() {
  local url=$1 threads=$2 connections=$3 duration=$4 timeout=$5 json=$6 output
  output="$WORK_DIR/wrk-$$.txt"
  run_wrk_one "$url" "$threads" "$connections" "$duration" "$timeout" "$output" || return 1
  wrk_json_object >"$json"
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
    "$wrk_requests_per_sec" \
    "${wrk_latency_avg_ms:--}" \
    "${wrk_latency_p50_ms:--}" \
    "${wrk_latency_p99_ms:--}" \
    "${wrk_requests:-0}" \
    "${wrk_non_2xx:-0}" \
    "0" \
    "${wrk_socket_errors:-none}" >>"${json%/*}/samples.tsv"
  rm -f "$output"
}

# Builds a JSON array from the sample files named 1.json .. count.json.
sample_array() {
  local dir=$1 count=$2 index=1 json=""
  while [[ $index -le $count ]]; do
    if [[ -f "$dir/$index.json" ]]; then
      if [[ -n $json ]]; then
        json+=","
      fi
      json+=$(cat "$dir/$index.json")
    fi
    index=$((index + 1))
  done
  printf '[%s]' "$json"
}

# Summarizes the wrk sample TSV and JSON files for one endpoint.
summarize_wrk_samples() {
  local dir=$1 count=$2
  local rps_file="$WORK_DIR/wrk-rps.$$"
  local avg_file="$WORK_DIR/wrk-avg.$$"
  local p50_file="$WORK_DIR/wrk-p50.$$"
  local p99_file="$WORK_DIR/wrk-p99.$$"
  local requests_file="$WORK_DIR/wrk-requests.$$"
  local failures=0 socket_errors="" index value
  : >"$rps_file"
  : >"$avg_file"
  : >"$p50_file"
  : >"$p99_file"
  : >"$requests_file"
  while IFS=$'\t' read -r rps avg p50 p99 requests non_2xx sample_failures errors; do
    [[ -n $rps ]] || continue
    printf '%s\n' "$rps" >>"$rps_file"
    [[ $avg != "-" && $avg != "null" ]] && printf '%s\n' "$avg" >>"$avg_file"
    [[ $p50 != "-" && $p50 != "null" ]] && printf '%s\n' "$p50" >>"$p50_file"
    [[ $p99 != "-" && $p99 != "null" ]] && printf '%s\n' "$p99" >>"$p99_file"
    [[ $requests != "-" && $requests != "null" ]] && printf '%s\n' "$requests" >>"$requests_file"
    failures=$((failures + ${non_2xx:-0} + ${sample_failures:-0}))
    if [[ -n $socket_errors ]]; then
      socket_errors+=","
    fi
    socket_errors+=$(json_string "${errors:-none}")
  done <"$dir/samples.tsv"
  local summary
  summary=$(printf '{ "requests_per_sec": %s, "latency_avg_ms": %s, "latency_p50_ms": %s, "latency_p99_ms": %s, "non_2xx": %s, "requests": %s, "failures": %s, "socket_errors": [%s], "raw": %s }' \
    "$(json_number_or_null "$(median_values <"$rps_file")")" \
    "$(json_number_or_null "$(median_values <"$avg_file")")" \
    "$(json_number_or_null "$(median_values <"$p50_file")")" \
    "$(json_number_or_null "$(median_values <"$p99_file")")" \
    "0" \
    "$(json_number_or_null "$(median_values <"$requests_file")")" \
    "$failures" \
    "$socket_errors" \
    "$(sample_array "$dir" "$count")")
  rm -f "$rps_file" "$avg_file" "$p50_file" "$p99_file" "$requests_file"
  printf '%s\n' "$summary"
}

# Summarizes the sequential new-connection sample TSV for one endpoint.
summarize_new_samples() {
  local dir=$1 count=$2
  local rps_file="$WORK_DIR/new-rps.$$"
  local p50_file="$WORK_DIR/new-p50.$$"
  local p99_file="$WORK_DIR/new-p99.$$"
  local requests_file="$WORK_DIR/new-requests.$$"
  local failures=0 index
  : >"$rps_file"
  : >"$p50_file"
  : >"$p99_file"
  : >"$requests_file"
  while IFS=$'\t' read -r requests sample_failures rps p50 p99; do
    [[ -n $requests ]] || continue
    [[ $rps != "-" && $rps != "null" ]] && printf '%s\n' "$rps" >>"$rps_file"
    [[ $p50 != "-" && $p50 != "null" ]] && printf '%s\n' "$p50" >>"$p50_file"
    [[ $p99 != "-" && $p99 != "null" ]] && printf '%s\n' "$p99" >>"$p99_file"
    [[ $requests != "-" && $requests != "null" ]] && printf '%s\n' "$requests" >>"$requests_file"
    failures=$((failures + ${sample_failures:-0}))
  done <"$dir/samples.tsv"
  local summary
  summary=$(printf '{ "requests": %s, "failures": %s, "requests_per_sec": %s, "latency_p50_ms": %s, "latency_p99_ms": %s, "socket_errors": %s, "raw": %s }' \
    "$(json_number_or_null "$(median_values <"$requests_file")")" \
    "$failures" \
    "$(json_number_or_null "$(median_values <"$rps_file")")" \
    "$(json_number_or_null "$(median_values <"$p50_file")")" \
    "$(json_number_or_null "$(median_values <"$p99_file")")" \
    "$(awk -v count="$count" 'BEGIN { printf "["; for (i = 1; i <= count; i += 1) { if (i > 1) printf ","; printf "\"none\"" } printf "]" }')" \
    "$(sample_array "$dir" "$count")")
  rm -f "$rps_file" "$p50_file" "$p99_file" "$requests_file"
  printf '%s\n' "$summary"
}

# Builds the enriched comparison JSON for one scenario.
compare_scenario() {
  local elephc=$1 fpm=$2
  local left_rps right_rps left_p50 right_p50 left_p99 right_p99
  left_rps=$(printf '%s' "$elephc" | jq -r '.requests_per_sec // empty' 2>/dev/null || true)
  right_rps=$(printf '%s' "$fpm" | jq -r '.requests_per_sec // empty' 2>/dev/null || true)
  left_p50=$(printf '%s' "$elephc" | jq -r '.latency_p50_ms // empty' 2>/dev/null || true)
  right_p50=$(printf '%s' "$fpm" | jq -r '.latency_p50_ms // empty' 2>/dev/null || true)
  left_p99=$(printf '%s' "$elephc" | jq -r '.latency_p99_ms // empty' 2>/dev/null || true)
  right_p99=$(printf '%s' "$fpm" | jq -r '.latency_p99_ms // empty' 2>/dev/null || true)
  printf '{ "elephc": %s, "fpm": %s, "throughput_ratio": %s, "latency_p50_ratio": %s, "latency_p99_ratio": %s }' \
    "$elephc" "$fpm" \
    "$(json_string "$(ratio "$left_rps" "$right_rps")")" \
    "$(json_string "$(ratio "$left_p50" "$right_p50")")" \
    "$(json_string "$(ratio "$left_p99" "$right_p99")")"
}

# Prints the Markdown comparison table.
markdown_report() {
  local elephc_url=$1 fpm_url=$2 repeat=$3 comparison=$4
  local scenario label metric value left right ratio_value
  printf '# Elephc vs Nginx + PHP-FPM\n\n'
  printf -- '- Elephc: `%s`\n' "$elephc_url"
  printf -- '- FPM: `%s`\n' "$fpm_url"
  printf -- '- Repeats: %s\n\n' "$repeat"
  printf '| Scenario | Metric | Elephc | FPM | FPM / Elephc |\n'
  printf '|---|---:|---:|---:|---:|\n'
  for scenario in "$WRK_SCENARIO" "$CONNECTION_SCENARIO"; do
    printf '%s' "$comparison" | jq -e --arg scenario "$scenario" 'has($scenario)' >/dev/null 2>&1 || continue
    for metric in requests_per_sec latency_p50_ms latency_p99_ms; do
      case $metric in
        requests_per_sec) label="Throughput (req/s)" ;;
        latency_p50_ms) label="p50 (ms)" ;;
        latency_p99_ms) label="p99 (ms)" ;;
      esac
      left=$(printf '%s' "$comparison" | jq -r --arg scenario "$scenario" --arg metric "$metric" '.[$scenario].elephc[$metric] // "n/a"' 2>/dev/null)
      right=$(printf '%s' "$comparison" | jq -r --arg scenario "$scenario" --arg metric "$metric" '.[$scenario].fpm[$metric] // "n/a"' 2>/dev/null)
      case $metric in
        requests_per_sec) ratio_value=$(printf '%s' "$comparison" | jq -r --arg scenario "$scenario" '.[$scenario].throughput_ratio' 2>/dev/null) ;;
        latency_p50_ms) ratio_value=$(printf '%s' "$comparison" | jq -r --arg scenario "$scenario" '.[$scenario].latency_p50_ratio' 2>/dev/null) ;;
        latency_p99_ms) ratio_value=$(printf '%s' "$comparison" | jq -r --arg scenario "$scenario" '.[$scenario].latency_p99_ratio' 2>/dev/null) ;;
      esac
      printf '| %s | %s | %s | %s | %s |\n' "$scenario" "$label" "$left" "$right" "$ratio_value"
    done
  done
  printf '\n'
}

# Parses the CLI, measures both endpoints, and writes the reports.
main() {
  local elephc_url=""
  local fpm_url=""
  local threads=4
  local connections=100
  local duration=10s
  local repeat=3
  local new_connections=500
  local skip_keepalive=0
  local skip_new_connection=0
  local json_out=""
  local markdown_out=""
  local timeout=5
  local wrk_timeout=120
  while (($#)); do
    case $1 in
      --elephc-url) elephc_url=$2; shift 2 ;;
      --fpm-url) fpm_url=$2; shift 2 ;;
      --threads) threads=$2; shift 2 ;;
      --connections) connections=$2; shift 2 ;;
      --duration) duration=$2; shift 2 ;;
      --repeat) repeat=$2; shift 2 ;;
      --new-connections) new_connections=$2; shift 2 ;;
      --skip-keepalive) skip_keepalive=1; shift ;;
      --skip-new-connection) skip_new_connection=1; shift ;;
      --json) json_out=$2; shift 2 ;;
      --markdown) markdown_out=$2; shift 2 ;;
      --timeout) timeout=$2; shift 2 ;;
      --wrk-timeout) wrk_timeout=$2; shift 2 ;;
      -h|--help) usage; exit 0 ;;
      *) die "unknown argument: $1" ;;
    esac
  done
  [[ -n $elephc_url ]] || die "--elephc-url is required"
  [[ -n $fpm_url ]] || die "--fpm-url is required"
  [[ $repeat -ge 1 ]] || die "--repeat must be >= 1"
  [[ $new_connections -ge 1 ]] || die "--new-connections must be >= 1"
  require_cmd wrk "install it (brew install wrk) and retry"
  require_cmd curl "install curl for the new-connection scenario"
  require_cmd jq "install jq for the comparison report"
  [[ $skip_new_connection -eq 1 ]] || require_cmd curl "install curl for the new-connection scenario"
  # Validate both URLs before any measurement.
  parse_origin "$elephc_url" >/dev/null
  parse_origin "$fpm_url" >/dev/null

  WORK_DIR=$(mktemp -d "${TMPDIR:-/tmp}/elephc-fpm-compare.XXXXXX")
  mkdir -p "$WORK_DIR/keepalive-elephc" "$WORK_DIR/keepalive-fpm" "$WORK_DIR/new-elephc" "$WORK_DIR/new-fpm"

  local index pair url dir sample
  if [[ $skip_keepalive -eq 0 ]]; then
    for ((index = 0; index < repeat; index += 1)); do
      if ((index % 2 == 0)); then
        pair=("$elephc_url|$WORK_DIR/keepalive-elephc" "$fpm_url|$WORK_DIR/keepalive-fpm")
      else
        pair=("$fpm_url|$WORK_DIR/keepalive-fpm" "$elephc_url|$WORK_DIR/keepalive-elephc")
      fi
      for sample in "${pair[@]}"; do
        url=${sample%%|*}
        dir=${sample#*|}
        measure_wrk "$url" "$threads" "$connections" "$duration" "$wrk_timeout" "$dir/$((index + 1)).json" || die "wrk failed for $url"
        sleep 0.5
      done
    done
  fi

  if [[ $skip_new_connection -eq 0 ]]; then
    for ((index = 0; index < repeat; index += 1)); do
      if ((index % 2 == 0)); then
        pair=("$elephc_url|$WORK_DIR/new-elephc" "$fpm_url|$WORK_DIR/new-fpm")
      else
        pair=("$fpm_url|$WORK_DIR/new-fpm" "$elephc_url|$WORK_DIR/new-elephc")
      fi
      for sample in "${pair[@]}"; do
        url=${sample%%|*}
        dir=${sample#*|}
        measure_new_connections "$url" "$new_connections" "$timeout" "$dir/$((index + 1)).json"
        sleep 0.5
      done
    done
  fi

  local keepalive_elephc="null" keepalive_fpm="null" new_elephc="null" new_fpm="null"
  local scenarios_json="{}" comparison_json="{}"
  if [[ $skip_keepalive -eq 0 ]]; then
    keepalive_elephc=$(summarize_wrk_samples "$WORK_DIR/keepalive-elephc" "$repeat")
    keepalive_fpm=$(summarize_wrk_samples "$WORK_DIR/keepalive-fpm" "$repeat")
    scenarios_json="{ \"$WRK_SCENARIO\": { \"elephc\": $keepalive_elephc, \"fpm\": $keepalive_fpm } }"
    comparison_json="{ \"$WRK_SCENARIO\": $(compare_scenario "$keepalive_elephc" "$keepalive_fpm") }"
  fi
  if [[ $skip_new_connection -eq 0 ]]; then
    new_elephc=$(summarize_new_samples "$WORK_DIR/new-elephc" "$repeat")
    new_fpm=$(summarize_new_samples "$WORK_DIR/new-fpm" "$repeat")
    scenarios_json=$(printf '%s' "$scenarios_json" | jq -c --argjson elephc "$new_elephc" --argjson fpm "$new_fpm" --arg scenario "$CONNECTION_SCENARIO" '. + {($scenario): {elephc: $elephc, fpm: $fpm}}')
    comparison_json=$(printf '%s' "$comparison_json" | jq -c --argjson scenario "$(compare_scenario "$new_elephc" "$new_fpm")" --arg name "$CONNECTION_SCENARIO" '. + {($name): $scenario}')
  fi

  local report
  report=$(markdown_report "$elephc_url" "$fpm_url" "$repeat" "$comparison_json")
  printf '%s\n' "$report"
  if [[ -n $json_out ]]; then
    {
      printf '{\n'
      printf '  "elephc_url": %s,\n' "$(json_string "$elephc_url")"
      printf '  "fpm_url": %s,\n' "$(json_string "$fpm_url")"
      printf '  "threads": %s,\n' "$threads"
      printf '  "connections": %s,\n' "$connections"
      printf '  "duration": %s,\n' "$(json_string "$duration")"
      printf '  "repeat": %s,\n' "$repeat"
      printf '  "new_connections": %s,\n' "$new_connections"
      printf '  "scenarios": %s,\n' "$scenarios_json"
      printf '  "comparison": %s\n' "$comparison_json"
      printf '}\n'
    } >"$json_out"
  fi
  if [[ -n $markdown_out ]]; then
    printf '%s\n' "$report" >"$markdown_out"
  fi
}

main "$@"
