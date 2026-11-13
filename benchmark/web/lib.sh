#!/usr/bin/env bash
# Shared helpers for the bash web benchmarks.
#
# Sourced by `run.sh` and `fpm_compare.sh`. The helpers deliberately avoid
# bash-4 features so they run under the bash 3.2 shipped with macOS.

# Prints a fatal error to stderr and exits.
die() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

# Fails early when a required executable is missing.
require_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "$1 is required${2:+: $2}"
}

# Requires a value after an option, exiting in the CALLER's shell on failure.
# Do not call this through a command substitution: `exit` there only leaves the
# subshell and the caller would loop forever.
require_value() {
  [[ $# -ge 2 ]] || die "missing value for $1"
}

# Returns success when the argument looks like a JSON number.
is_number() {
  [[ $1 =~ ^-?[0-9]+([.][0-9]+)?([eE][-+]?[0-9]+)?$ ]]
}

# Prints the median of the numbers on stdin, or `null` when empty.
median_values() {
  sort -n | awk '
    { values[NR] = $1 }
    END {
      if (NR == 0) { print "null"; exit }
      if (NR % 2) { printf "%.10f\n", values[(NR + 1) / 2] }
      else { printf "%.10f\n", (values[NR / 2] + values[NR / 2 + 1]) / 2 }
    }'
}

# Prints the arithmetic mean of the numbers on stdin, or `null` when empty.
mean_values() {
  awk '
    { sum += $1; count += 1 }
    END {
      if (count == 0) print "null"
      else printf "%.10f\n", sum / count
    }'
}

# Prints the smallest number on stdin, or `null` when empty.
min_values() {
  sort -n | awk 'NR == 1 { print; found = 1 } END { if (!found) print "null" }'
}

# Prints the largest number on stdin, or `null` when empty.
max_values() {
  sort -n | awk '{ value = $1; found = 1 } END { if (found) print value; else print "null" }'
}

# Escapes one string for embedding in JSON.
#
# The character loop avoids `gsub` replacement-string rules, which differ
# between BSD awk and the Linux awks and can leave a single backslash before an
# arbitrary character.
json_escape() {
  awk '
    BEGIN { ORS = "" }
    {
      if (NR > 1) printf "\\n"
      out = ""
      for (i = 1; i <= length($0); i += 1) {
        c = substr($0, i, 1)
        if (c == "\\") out = out "\\\\"
        else if (c == "\"") out = out "\\\""
        else if (c == "\t") out = out "\\t"
        else if (c == "\r") out = out "\\r"
        else out = out c
      }
      printf "%s", out
    }'
}

# Prints a JSON string literal for the argument.
json_string() {
  printf '"%s"' "$(printf '%s' "$1" | json_escape)"
}

# Prints a JSON number, or `null` when the value is missing/invalid.
json_number_or_null() {
  if [[ -n $1 && $1 != "null" ]] && is_number "$1"; then
    printf '%s' "$1"
  else
    printf 'null'
  fi
}

# Converts a wrk duration (`us`, `ms`, or `s`) to milliseconds.
parse_ms() {
  awk -v value="$1" 'BEGIN {
    if (value ~ /us$/) { sub(/us$/, "", value); printf "%.6f", value / 1000 }
    else if (value ~ /ms$/) { sub(/ms$/, "", value); printf "%.6f", value }
    else if (value ~ /s$/) { sub(/s$/, "", value); printf "%.6f", value * 1000 }
    else printf "%.6f", value
  }'
}

# Converts a `ps` time (`M:SS.ss` or `H:MM:SS`) to seconds.
ps_time_to_seconds() {
  awk -v value="$1" 'BEGIN {
    parts = split(value, field, ":")
    if (parts == 2) printf "%.6f", field[1] * 60 + field[2]
    else if (parts == 3) printf "%.6f", field[1] * 3600 + field[2] * 60 + field[3]
    else printf "%.6f", value
  }'
}

# Parses one wrk report into the `wrk_*` globals below.
#
# Mirrors the previous Python parser: only fields present in the report are
# set, so callers can distinguish a missing percentile from a zero.
parse_wrk_file() {
  local file=$1
  local key value
  wrk_raw=$(cat "$file")
  wrk_latency_avg_ms=""
  wrk_latency_stdev_ms=""
  wrk_latency_max_ms=""
  wrk_req_per_sec_avg=""
  wrk_latency_p50_ms=""
  wrk_latency_p75_ms=""
  wrk_latency_p90_ms=""
  wrk_latency_p99_ms=""
  wrk_requests_per_sec=""
  wrk_transfer_per_sec=""
  wrk_socket_errors=""
  wrk_non_2xx=""
  wrk_summary=""
  wrk_requests=""
  while IFS=$'\t' read -r key value; do
    case $key in
      latency_avg_ms) wrk_latency_avg_ms=$value ;;
      latency_stdev_ms) wrk_latency_stdev_ms=$value ;;
      latency_max_ms) wrk_latency_max_ms=$value ;;
      req_per_sec_avg) wrk_req_per_sec_avg=$value ;;
      latency_p50_ms) wrk_latency_p50_ms=$value ;;
      latency_p75_ms) wrk_latency_p75_ms=$value ;;
      latency_p90_ms) wrk_latency_p90_ms=$value ;;
      latency_p99_ms) wrk_latency_p99_ms=$value ;;
      requests_per_sec) wrk_requests_per_sec=$value ;;
      transfer_per_sec) wrk_transfer_per_sec=$value ;;
      socket_errors) wrk_socket_errors=$value ;;
      non_2xx) wrk_non_2xx=$value ;;
      summary) wrk_summary=$value ;;
      requests) wrk_requests=$value ;;
    esac
  done < <(awk '
    {
      line = $0
      sub(/^[ \t]+/, "", line)
      sub(/[ \t]+$/, "", line)
      fields = split(line, part, /[ \t]+/)
      if (part[1] == "Latency" && fields >= 3) {
        print "latency_avg_ms\t" part[2]
        print "latency_stdev_ms\t" part[3]
        print "latency_max_ms\t" part[4]
      } else if (part[1] == "Req/Sec" && fields >= 3) {
        print "req_per_sec_avg\t" part[2]
      } else if (fields == 2 && part[1] == "50%") {
        print "latency_p50_ms\t" part[2]
      } else if (fields == 2 && part[1] == "75%") {
        print "latency_p75_ms\t" part[2]
      } else if (fields == 2 && part[1] == "90%") {
        print "latency_p90_ms\t" part[2]
      } else if (fields == 2 && part[1] == "99%") {
        print "latency_p99_ms\t" part[2]
      } else if (part[1] == "Requests/sec:") {
        print "requests_per_sec\t" part[2]
      } else if (part[1] == "Transfer/sec:") {
        sub(/^Transfer\/sec:[ \t]*/, "", line)
        print "transfer_per_sec\t" line
      } else if (line ~ /^Socket errors:/) {
        sub(/^Socket errors:[ \t]*/, "", line)
        print "socket_errors\t" line
      } else if (line ~ /^Non-2xx or 3xx responses:/) {
        sub(/^Non-2xx or 3xx responses:[ \t]*/, "", line)
        print "non_2xx\t" line
      } else if (index(line, " requests in ") > 0 && index(line, " read") > 0) {
        print "summary\t" line
        print "requests\t" part[1]
      }
    }' "$file")
  [[ -n $wrk_requests_per_sec ]] || die "could not parse wrk output from $file"
  [[ -n $wrk_latency_avg_ms ]] && wrk_latency_avg_ms=$(parse_ms "$wrk_latency_avg_ms")
  [[ -n $wrk_latency_stdev_ms ]] && wrk_latency_stdev_ms=$(parse_ms "$wrk_latency_stdev_ms")
  [[ -n $wrk_latency_max_ms ]] && wrk_latency_max_ms=$(parse_ms "$wrk_latency_max_ms")
  [[ -n $wrk_latency_p50_ms ]] && wrk_latency_p50_ms=$(parse_ms "$wrk_latency_p50_ms")
  [[ -n $wrk_latency_p75_ms ]] && wrk_latency_p75_ms=$(parse_ms "$wrk_latency_p75_ms")
  [[ -n $wrk_latency_p90_ms ]] && wrk_latency_p90_ms=$(parse_ms "$wrk_latency_p90_ms")
  [[ -n $wrk_latency_p99_ms ]] && wrk_latency_p99_ms=$(parse_ms "$wrk_latency_p99_ms")
}

# Builds one JSON object from the parsed wrk globals.
wrk_json_object() {
  local json="{"
  json+="\"raw\": $(json_string "$wrk_raw")"
  [[ -n $wrk_latency_avg_ms ]] && json+=", \"latency_avg_ms\": $wrk_latency_avg_ms"
  [[ -n $wrk_latency_stdev_ms ]] && json+=", \"latency_stdev_ms\": $wrk_latency_stdev_ms"
  [[ -n $wrk_latency_max_ms ]] && json+=", \"latency_max_ms\": $wrk_latency_max_ms"
  [[ -n $wrk_req_per_sec_avg ]] && json+=", \"req_per_sec_avg\": $(json_string "$wrk_req_per_sec_avg")"
  [[ -n $wrk_latency_p50_ms ]] && json+=", \"latency_p50_ms\": $wrk_latency_p50_ms"
  [[ -n $wrk_latency_p75_ms ]] && json+=", \"latency_p75_ms\": $wrk_latency_p75_ms"
  [[ -n $wrk_latency_p90_ms ]] && json+=", \"latency_p90_ms\": $wrk_latency_p90_ms"
  [[ -n $wrk_latency_p99_ms ]] && json+=", \"latency_p99_ms\": $wrk_latency_p99_ms"
  [[ -n $wrk_requests_per_sec ]] && json+=", \"requests_per_sec\": $wrk_requests_per_sec"
  [[ -n $wrk_transfer_per_sec ]] && json+=", \"transfer_per_sec\": $(json_string "$wrk_transfer_per_sec")"
  [[ -n $wrk_socket_errors ]] && json+=", \"socket_errors\": $(json_string "$wrk_socket_errors")"
  [[ -n $wrk_non_2xx ]] && json+=", \"non_2xx\": $wrk_non_2xx"
  [[ -n $wrk_summary ]] && json+=", \"summary\": $(json_string "$wrk_summary")"
  [[ -n $wrk_requests ]] && json+=", \"requests\": $wrk_requests"
  [[ -n $wrk_command ]] && json+=", \"command\": $(json_string "$wrk_command")"
  json+="}"
  printf '%s' "$json"
}

# Counts non-2xx responses and socket errors from the last parsed wrk report.
#
# A benchmark that silently averages failed responses is worse than no
# benchmark: callers must treat a non-zero count as an invalid run.
wrk_failure_count() {
  local total=${wrk_non_2xx:-0} socket_total
  socket_total=$(printf '%s' "${wrk_socket_errors:-}" | tr -cs '0-9' '\n' | awk '{ sum += $1 } END { print sum + 0 }')
  printf '%s\n' "$((total + socket_total))"
}

# Runs one wrk process with a watchdog and parses its report.
run_wrk_one() {
  local url=$1 threads=$2 connections=$3 duration=$4 timeout=$5 output=$6
  local pid watchdog status
  wrk_command="wrk --latency -t$threads -c$connections -d$duration $url"
  wrk --latency -t "$threads" -c "$connections" -d "$duration" "$url" >"$output" 2>&1 &
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
    printf 'wrk exited with %s:\n' "$status" >&2
    cat "$output" >&2
    return 1
  fi
  parse_wrk_file "$output"
  local failures
  failures=$(wrk_failure_count)
  if [[ $failures -ne 0 ]]; then
    printf 'wrk reported %s failed response(s) or socket error(s):\n' "$failures" >&2
    cat "$output" >&2
    return 1
  fi
}

# Runs one wrk round (possibly several concurrent clients) and writes:
#   round-N.wrk.json  parsed wrk object
#   round-N.stats     rps<TAB>avg<TAB>p99<TAB>requests
run_wrk_round() {
  local url=$1 threads=$2 connections=$3 duration=$4 timeout=$5 clients=$6 dir=$7 index=$8
  local client output i failed=0 failures=0 json
  local -a pids outputs client_jsons
  local rps_sum=0 requests_sum=0 avg_sum=0 p99_max=0 avg_count=0 p99_count=0
  local json_out
  wrk_command="wrk --latency -t$threads -c$connections -d$duration $url"
  for ((client = 1; client <= clients; client += 1)); do
    output="$dir/round-$index-client-$client.txt"
    wrk --latency -t "$threads" -c "$connections" -d "$duration" "$url" >"$output" 2>&1 &
    pids+=("$!")
    outputs+=("$output")
  done
  (
    sleep "$timeout"
    for i in "${pids[@]}"; do
      kill -TERM "$i" 2>/dev/null || true
    done
    sleep 1
    for i in "${pids[@]}"; do
      kill -KILL "$i" 2>/dev/null || true
    done
  ) >/dev/null 2>&1 &
  local watchdog=$!
  for ((i = 0; i < clients; i += 1)); do
    if ! wait "${pids[$i]}"; then
      failed=1
    fi
  done
  kill -TERM "$watchdog" 2>/dev/null || true
  wait "$watchdog" 2>/dev/null || true
  if [[ $failed -ne 0 ]]; then
    printf 'one or more wrk clients failed:\n' >&2
    for output in "${outputs[@]}"; do
      cat "$output" >&2
    done
    return 1
  fi
  for ((client = 1; client <= clients; client += 1)); do
    output="$dir/round-$index-client-$client.txt"
    parse_wrk_file "$output"
    failures=$((failures + $(wrk_failure_count)))
    json=$(wrk_json_object)
    client_jsons+=("$json")
    rps_sum=$(awk -v a="$rps_sum" -v b="$wrk_requests_per_sec" 'BEGIN { printf "%.6f", a + b }')
    requests_sum=$(awk -v a="$requests_sum" -v b="${wrk_requests:-0}" 'BEGIN { printf "%.6f", a + b }')
    if [[ -n $wrk_latency_avg_ms ]]; then
      avg_sum=$(awk -v a="$avg_sum" -v b="$wrk_latency_avg_ms" 'BEGIN { printf "%.6f", a + b }')
      avg_count=$((avg_count + 1))
    fi
    if [[ -n $wrk_latency_p99_ms ]]; then
      p99_max=$(awk -v a="$p99_max" -v b="$wrk_latency_p99_ms" 'BEGIN { print (b > a) ? b : a }')
      p99_count=$((p99_count + 1))
    fi
  done
  if [[ $failures -ne 0 ]]; then
    printf 'wrk reported %s failed response(s) or socket error(s):\n' "$failures" >&2
    for output in "${outputs[@]}"; do
      cat "$output" >&2
    done
    return 1
  fi
  if [[ $clients -eq 1 ]]; then
    parse_wrk_file "$dir/round-$index-client-1.txt"
    json_out="${client_jsons[0]%?},\"clients\": 1}"
    printf '%s\n' "$json_out" >"$dir/round-$index.wrk.json"
    printf '%s\t%s\t%s\t%s\n' \
      "$wrk_requests_per_sec" \
      "${wrk_latency_avg_ms:--}" \
      "${wrk_latency_p99_ms:--}" \
      "${wrk_requests:-0}" >"$dir/round-$index.stats"
    return 0
  fi
  local per_client
  per_client=$(IFS=,; printf '%s' "${client_jsons[*]}")
  local raw_all=""
  for ((client = 1; client <= clients; client += 1)); do
    raw_all+=$(cat "$dir/round-$index-client-$client.txt")$'\n'
  done
  local avg="null" p99="null"
  if [[ $avg_count -gt 0 ]]; then
    avg=$(awk -v sum="$avg_sum" -v count="$avg_count" 'BEGIN { printf "%.6f", sum / count }')
  fi
  if [[ $p99_count -gt 0 ]]; then
    p99=$p99_max
  fi
  json_out="{\"clients\": $clients, \"requests\": $requests_sum, \"requests_per_sec\": $rps_sum, \"latency_avg_ms\": $avg, \"latency_p99_ms\": $p99, \"per_client\": [$per_client], \"raw\": $(json_string "$raw_all")}"
  printf '%s\n' "$json_out" >"$dir/round-$index.wrk.json"
  printf '%s\t%s\t%s\t%s\n' "$rps_sum" "$avg" "$p99" "$requests_sum" >"$dir/round-$index.stats"
}

# Sums the user and system CPU seconds of a process tree.
process_tree() {
  ps -axo pid=,ppid= | awk -v root="$1" '
    { children[$2] = children[$2] " " $1 }
    END {
      queue[1] = root
      count = 1
      head = 1
      while (head <= count) {
        pid = queue[head]
        print pid
        parts = split(children[pid], child, " ")
        for (i = 1; i <= parts; i += 1) {
          if (child[i] != "") {
            count += 1
            queue[count] = child[i]
          }
        }
        head += 1
      }
    }'
}

# Reads one process's `(user, system)` CPU seconds, best effort.
process_cpu_split() {
  local pid=$1 system_name
  system_name=$(uname -s)
  if [[ $system_name == Darwin ]]; then
    local times user_time system_time
    times=$(ps -o utime=,stime= -p "$pid" 2>/dev/null) || { printf '0 0\n'; return; }
    read -r user_time system_time <<<"$times"
    printf '%s %s\n' \
      "$(ps_time_to_seconds "$user_time")" \
      "$(ps_time_to_seconds "$system_time")"
    return
  fi
  local stat rest ticks
  stat=$(cat "/proc/$pid/stat" 2>/dev/null) || { printf '0 0\n'; return; }
  rest=${stat##*) }
  ticks=$(getconf CLK_TCK 2>/dev/null || echo 100)
  awk -v ticks="$ticks" '{
    printf "%.6f %.6f\n", $12 / ticks, $13 / ticks
  }' <<<"$rest"
}

# Sums `(user, system)` CPU seconds over the server master and descendants.
server_cpu_split() {
  local master=$1 pid
  process_tree "$master" | while read -r pid; do
    process_cpu_split "$pid"
  done | awk '{ user += $1; sys += $2 } END { printf "%.6f %.6f\n", user, sys }'
}

# Returns success when the local TCP port has no listener.
port_is_free() {
  local port=$1
  if command -v lsof >/dev/null 2>&1; then
    ! lsof -nP -iTCP:"$port" -sTCP:LISTEN >/dev/null 2>&1
  elif command -v ss >/dev/null 2>&1; then
    ! ss -H -ltn "sport = :$port" 2>/dev/null | grep -q .
  else
    ! (echo >"/dev/tcp/127.0.0.1/$port") >/dev/null 2>&1
  fi
}

# Picks a free ephemeral localhost port.
free_port() {
  local attempt port
  for attempt in $(seq 1 100); do
    port=$((20000 + RANDOM % 40000))
    if port_is_free "$port"; then
      printf '%s\n' "$port"
      return 0
    fi
  done
  die "could not find a free local port"
}

# Polls a URL until the server answers, failing fast if the process exits.
wait_for_server() {
  local pid=$1 url=$2 log=$3 attempt
  for attempt in $(seq 1 300); do
    if ! kill -0 "$pid" 2>/dev/null; then
      printf 'server exited before answering:\n' >&2
      cat "$log" >&2
      return 1
    fi
    if curl -fsS --max-time 0.5 "$url" >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.05
  done
  printf 'server did not answer within 15s:\n' >&2
  cat "$log" >&2
  return 1
}

# Stops one background process, escalating to SIGKILL after a grace period.
stop_process() {
  local pid=$1 attempt
  [[ -n $pid ]] || return 0
  kill -TERM "$pid" 2>/dev/null || true
  for attempt in $(seq 1 50); do
    kill -0 "$pid" 2>/dev/null || return 0
    sleep 0.1
  done
  kill -KILL "$pid" 2>/dev/null || true
}

# Prints the host load average as JSON numbers.
load_average_json() {
  local values
  if [[ $(uname -s) == Darwin ]]; then
    values=$(sysctl -n vm.loadavg 2>/dev/null | tr -d '{}')
  else
    values=$(awk '{ print $1, $2, $3 }' /proc/loadavg 2>/dev/null)
  fi
  set -- $values
  printf '[%s, %s, %s]' \
    "$(json_number_or_null "${1:-0}")" \
    "$(json_number_or_null "${2:-0}")" \
    "$(json_number_or_null "${3:-0}")"
}

# Returns the platform's CPU count.
cpu_count() {
  if [[ $(uname -s) == Darwin ]]; then
    sysctl -n hw.ncpu 2>/dev/null || echo 0
  else
    getconf _NPROCESSORS_ONLN 2>/dev/null || echo 0
  fi
}

# Parses `http://host[:port][/path][?query]` into host, port, and target.
parse_origin() {
  local url=$1 rest authority host port target
  [[ $url == http://* ]] || die "only plain HTTP origins are supported: $url"
  rest=${url#http://}
  authority=${rest%%[/?]*}
  [[ -n $authority ]] || die "missing host in URL: $url"
  host=${authority%%:*}
  if [[ $authority == *:* ]]; then
    port=${authority##*:}
    [[ $port =~ ^[0-9]+$ ]] || die "invalid port in URL: $url"
  else
    port=80
  fi
  [[ -n $host ]] || die "missing host in URL: $url"
  if [[ $rest == */* ]]; then
    target=/${rest#*/}
  elif [[ $rest == *\?* ]]; then
    target=/?${rest#*\?}
  else
    target=/
  fi
  printf '%s\n%s\n%s\n' "$host" "$port" "$target"
}

# Prints a right/left ratio, or `n/a` when either side is missing.
ratio() {
  local left=$1 right=$2
  if [[ -z $left || $left == "null" || $left == "0" || -z $right || $right == "null" ]]; then
    printf 'n/a'
    return
  fi
  awk -v left="$left" -v right="$right" 'BEGIN { printf "%.2fx", right / left }'
}
