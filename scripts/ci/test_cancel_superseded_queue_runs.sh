#!/usr/bin/env bash
# Focused tests for the merge queue cleanup, with a fake `gh` on PATH, so no API access is
# needed. The fake applies `--jq` with the real `jq`, and filters and pages the run listing
# the way the API does, so the listing itself is tested too.
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
fixture_dir=$(mktemp -d "$PWD/.queue-cleanup-test.XXXXXX")
trap 'rm -rf "$fixture_dir"' EXIT HUP INT TERM

# The fake gh answers by API path and records each call as "METHOD PATH" in calls:
#   .../workflows/W/runs  the runs of runs-W.json with the requested status, at most
#                         FAKE_PAGE_SIZE of them (default 100) unless --paginate is passed
#   .../compare/B...H     the status FAKE_COMPARE gives B ("aaa=ahead bbb=identical ...")
#   .../runs/N/cancel     an empty object
# FAKE_FAIL names a path fragment that fails, as the API does with a 502 or a 409.
# FAKE_MOVE=STATE:ID:TO moves run ID to status TO right after the STATE runs are listed, the
# way a run starts while the cleanup is still listing.
mkdir "$fixture_dir/bin"
cat > "$fixture_dir/bin/gh" <<'EOF'
#!/usr/bin/env bash
method=GET path="" filter="" paginate=false
while [[ $# -gt 0 ]]; do
    case "$1" in
        --method) method=$2; shift ;;
        --jq) filter=$2; shift ;;
        --paginate) paginate=true ;;
        repos/*) path=$1 ;;
    esac
    shift
done
echo "$method $path" >> "$FAKE_DIR/calls"
if [[ -n ${FAKE_FAIL:-} && $path == *"$FAKE_FAIL"* ]]; then
    echo "HTTP 502" >&2
    exit 1
fi
case "$path" in
    */actions/workflows/*/runs*)
        workflow=${path#*/workflows/}
        state=""
        if [[ $path == *status=* ]]; then
            state=${path#*status=}
            state=${state%%&*}
        fi
        size=${FAKE_PAGE_SIZE:-100}
        $paginate && size=1000000
        body=$(jq -c --arg state "$state" --argjson size "$size" \
            '{workflow_runs: ([.workflow_runs[] | select($state == "" or .status == $state)] | .[:$size])}' \
            "$FAKE_DIR/runs-${workflow%%/*}.json")
        if [[ -n ${FAKE_MOVE:-} && $state == "${FAKE_MOVE%%:*}" ]]; then
            move=${FAKE_MOVE#*:}
            file="$FAKE_DIR/runs-${workflow%%/*}.json"
            jq --argjson id "${move%%:*}" --arg to "${move#*:}" \
                '.workflow_runs |= map(if .id == $id then .status = $to else . end)' "$file" > "$file.next"
            mv "$file.next" "$file"
        fi
        ;;
    */compare/*)
        base=${path#*/compare/}
        base=${base%%...*}
        status=unknown
        for pair in ${FAKE_COMPARE:-}; do
            [[ ${pair%%=*} == "$base" ]] && status=${pair#*=}
        done
        body="{\"status\":\"$status\"}"
        ;;
    *) body='{}' ;;
esac
if [[ -n $filter ]]; then
    jq -r "$filter" <<< "$body"
else
    echo "$body"
fi
EOF
chmod +x "$fixture_dir/bin/gh"

# One push to main at SHA fff. In ci.yml: run 11 tests an entry that landed with it (aaa),
# run 12 the commit main now points to (bbb), run 13 a later entry built on top (ccc), run 14
# an entry rebuilt on another base (ddd), and run 15 an earlier entry that already finished.
# In pdo-live.yml, run 21 is waiting for a runner on the same landed entry as run 11.
cat > "$fixture_dir/runs-ci.yml.json" <<'EOF'
{"workflow_runs": [
  {"id": 11, "head_sha": "aaa", "status": "in_progress"},
  {"id": 12, "head_sha": "bbb", "status": "in_progress"},
  {"id": 13, "head_sha": "ccc", "status": "queued"},
  {"id": 14, "head_sha": "ddd", "status": "in_progress"},
  {"id": 15, "head_sha": "eee", "status": "completed"}
]}
EOF
cat > "$fixture_dir/runs-pdo-live.yml.json" <<'EOF'
{"workflow_runs": [
  {"id": 21, "head_sha": "aaa", "status": "waiting"}
]}
EOF
echo '{"workflow_runs": []}' > "$fixture_dir/runs-empty.yml.json"
# Newest first, as the API lists them: four finished runs, then two older ones still waiting
# on an entry that landed. With pages of one run, the status filter reaches the first of them
# and only paging reaches the second.
cat > "$fixture_dir/runs-paged.yml.json" <<'EOF'
{"workflow_runs": [
  {"id": 31, "head_sha": "eee", "status": "completed"},
  {"id": 32, "head_sha": "eee", "status": "completed"},
  {"id": 33, "head_sha": "eee", "status": "completed"},
  {"id": 34, "head_sha": "eee", "status": "completed"},
  {"id": 35, "head_sha": "aaa", "status": "queued"},
  {"id": 36, "head_sha": "aaa", "status": "queued"}
]}
EOF

# Writes the fixture of the cases where a run starts during the listing: run 37, on an entry
# that landed, still waiting for a runner when the cleanup begins.
write_moving() {
    echo '{"workflow_runs": [{"id": 37, "head_sha": "aaa", "status": "queued"}]}' > "$fixture_dir/runs-moving.yml.json"
}

# Runs the cleanup for WORKFLOW_FILES with the remaining NAME=VALUE overrides. Prints the
# exit status and the IDs of the runs it asked to cancel; its own output goes to out.
cleanup() {
    local workflows=$1 status=0
    shift
    : > "$fixture_dir/calls"
    env PATH="$fixture_dir/bin:$PATH" FAKE_DIR="$fixture_dir" GH_TOKEN=x REPO=o/r SHA=fff \
        WORKFLOW_FILES="$workflows" FAKE_COMPARE="aaa=ahead bbb=identical ccc=behind ddd=diverged eee=ahead" \
        "$@" bash "$script_dir/cancel_superseded_queue_runs.sh" > "$fixture_dir/out" 2>&1 || status=$?
    local ids
    ids=$(sed -n 's|^POST repos/o/r/actions/runs/\([0-9]*\)/cancel$|\1|p' "$fixture_dir/calls" | tr '\n' ' ')
    echo "exit=$status cancel=${ids% }"
}

# Asserts that a cleanup case produced the expected exit status and cancellations.
expect() {
    local name=$1 want=$2 got=$3
    if [[ $got != "$want" ]]; then
        echo "$name: expected '$want', got '$got'" >&2
        cat "$fixture_dir/out" >&2
        exit 1
    fi
}

expect "landed entries" "exit=0 cancel=11 21" "$(cleanup "ci.yml pdo-live.yml")"
if grep -q 'compare/eee' "$fixture_dir/calls"; then
    echo "a completed run was compared" >&2
    exit 1
fi
expect "no active runs" "exit=0 cancel=" "$(cleanup empty.yml)"
expect "active runs past the first page" "exit=0 cancel=35 36" "$(cleanup paged.yml FAKE_PAGE_SIZE=1)"
# Run 37 starts right after the in_progress request: an order that lists in_progress before
# queued finds it under neither status.
write_moving
expect "a run starts after in_progress is listed" "exit=0 cancel=37" \
    "$(cleanup moving.yml FAKE_MOVE=in_progress:37:in_progress)"
# Run 37 starts right after the queued request, so it is listed under both statuses.
write_moving
expect "a run listed under two statuses" "exit=0 cancel=37" "$(cleanup moving.yml FAKE_MOVE=queued:37:in_progress)"
expect "listing failure" "exit=0 cancel=21" "$(cleanup "ci.yml pdo-live.yml" FAKE_FAIL=workflows/ci.yml)"
grep -q '::warning::could not list the in_progress merge_group runs of ci.yml' "$fixture_dir/out"
expect "one status fails to list" "exit=0 cancel=21" "$(cleanup "ci.yml pdo-live.yml" FAKE_FAIL=status=in_progress)"
expect "compare failure" "exit=0 cancel=" "$(cleanup "ci.yml pdo-live.yml" FAKE_FAIL=compare/aaa)"
grep -q '::warning::could not compare ci.yml run 11' "$fixture_dir/out"
expect "cancel failure" "exit=0 cancel=11 21" "$(cleanup "ci.yml pdo-live.yml" FAKE_FAIL=runs/11/cancel)"
grep -q '::warning::could not cancel ci.yml run 11' "$fixture_dir/out"
grep -q '^cancelled 1 superseded' "$fixture_dir/out"

echo 'merge queue cleanup tests passed'
