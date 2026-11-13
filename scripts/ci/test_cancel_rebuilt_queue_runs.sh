#!/usr/bin/env bash
# Focused tests for the rebuilt merge group cleanup, with a fake `gh` on PATH, so no API
# access is needed. The fake applies `--jq` with the real `jq`, and filters and pages the run
# listing the way the API does, so the listing itself is tested too.
set -euo pipefail

script_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
fixture_dir=$(mktemp -d "$PWD/.queue-rebuild-test.XXXXXX")
trap 'rm -rf "$fixture_dir"' EXIT HUP INT TERM

# The fake gh answers by API path and records each call as "METHOD PATH" in calls:
#   .../workflows/W/runs  the runs of runs-W.json with the requested status, at most
#                         FAKE_PAGE_SIZE of them (default 100) unless --paginate is passed
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
    *) body='{}' ;;
esac
if [[ -n $filter ]]; then
    jq -r "$filter" <<< "$body"
else
    echo "$body"
fi
EOF
chmod +x "$fixture_dir/bin/gh"

# This run is 44, the third group of PR 12 (base sha ccc). The cleanup lists the statuses in
# order, `queued` before `in_progress`, so it cancels run 42 before run 41. In ci.yml: runs 41 and 42 test
# the two earlier groups of the same entry, run 43 another entry, run 45 a newer group of
# PR 12 that already replaced this one, run 46 the entry of PR 120 (the same digits, one more),
# and run 47 an earlier group of PR 12 that already finished.
cat > "$fixture_dir/runs-ci.yml.json" <<'EOF'
{"workflow_runs": [
  {"id": 47, "head_branch": "gh-readonly-queue/main/pr-12-zzz", "status": "completed"},
  {"id": 46, "head_branch": "gh-readonly-queue/main/pr-120-aaa", "status": "in_progress"},
  {"id": 45, "head_branch": "gh-readonly-queue/main/pr-12-ddd", "status": "queued"},
  {"id": 44, "head_branch": "gh-readonly-queue/main/pr-12-ccc", "status": "in_progress"},
  {"id": 43, "head_branch": "gh-readonly-queue/main/pr-7-aaa", "status": "in_progress"},
  {"id": 42, "head_branch": "gh-readonly-queue/main/pr-12-bbb", "status": "queued"},
  {"id": 41, "head_branch": "gh-readonly-queue/main/pr-12-aaa", "status": "in_progress"}
]}
EOF
# A base branch whose name contains a hyphen: only the last `-<base sha>` names the group.
cat > "$fixture_dir/runs-hyphen.yml.json" <<'EOF'
{"workflow_runs": [
  {"id": 52, "head_branch": "gh-readonly-queue/release-1.0/pr-30-aaa", "status": "in_progress"},
  {"id": 51, "head_branch": "gh-readonly-queue/release-1.0/pr-3-aaa", "status": "in_progress"}
]}
EOF
echo '{"workflow_runs": []}' > "$fixture_dir/runs-empty.yml.json"
# Newest first, as the API lists them: four finished runs, then two earlier groups of PR 12
# still waiting. With pages of one run, the status filter reaches the first of them and only
# paging reaches the second.
cat > "$fixture_dir/runs-paged.yml.json" <<'EOF'
{"workflow_runs": [
  {"id": 64, "head_branch": "gh-readonly-queue/main/pr-12-zzz", "status": "completed"},
  {"id": 63, "head_branch": "gh-readonly-queue/main/pr-12-yyy", "status": "completed"},
  {"id": 62, "head_branch": "gh-readonly-queue/main/pr-12-xxx", "status": "completed"},
  {"id": 61, "head_branch": "gh-readonly-queue/main/pr-12-www", "status": "completed"},
  {"id": 36, "head_branch": "gh-readonly-queue/main/pr-12-bbb", "status": "queued"},
  {"id": 35, "head_branch": "gh-readonly-queue/main/pr-12-aaa", "status": "queued"}
]}
EOF

# Writes the fixture of the cases where a run starts during the listing: run 37, an earlier
# group of PR 12, still waiting for a runner when the cleanup begins.
write_moving() {
    echo '{"workflow_runs": [{"id": 37, "head_branch": "gh-readonly-queue/main/pr-12-aaa", "status": "queued"}]}' > "$fixture_dir/runs-moving.yml.json"
}

# Runs the cleanup for WORKFLOW_FILE as run RUN_ID on HEAD_REF with the remaining NAME=VALUE
# overrides. Prints the exit status and the IDs of the runs it asked to cancel; its own
# output goes to out and its step summary to summary.
cleanup() {
    local workflow=$1 head_ref=$2 run_id=$3 status=0
    shift 3
    : > "$fixture_dir/calls"
    : > "$fixture_dir/summary"
    env PATH="$fixture_dir/bin:$PATH" FAKE_DIR="$fixture_dir" GH_TOKEN=x REPO=o/r \
        WORKFLOW_FILE="$workflow" HEAD_REF="$head_ref" RUN_ID="$run_id" \
        GITHUB_STEP_SUMMARY="$fixture_dir/summary" \
        "$@" bash "$script_dir/cancel_rebuilt_queue_runs.sh" > "$fixture_dir/out" 2>&1 || status=$?
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

this_ref=refs/heads/gh-readonly-queue/main/pr-12-ccc

expect "earlier groups of the same entry" "exit=0 cancel=42 41" "$(cleanup ci.yml "$this_ref" 44)"
grep -q '^cancelled 2 rebuilt merge_group run(s) of gh-readonly-queue/main/pr-12$' "$fixture_dir/out"
grep -q 'run 45 (gh-readonly-queue/main/pr-12-ddd) is this run or a newer group' "$fixture_dir/out"
grep -q 'cancelled 2 earlier merge_group run(s) of gh-readonly-queue/main/pr-12 replaced by gh-readonly-queue/main/pr-12-ccc' "$fixture_dir/summary"
if grep -q 'status=completed' "$fixture_dir/calls"; then
    echo "completed runs were listed" >&2
    exit 1
fi
expect "not a merge queue ref" "exit=0 cancel=" "$(cleanup ci.yml refs/heads/feature/x 44)"
grep -q '::warning::refs/heads/feature/x is not a merge queue ref' "$fixture_dir/out"
if [[ -s $fixture_dir/calls ]]; then
    echo "a run was listed for a ref outside the queue" >&2
    exit 1
fi
expect "no active runs" "exit=0 cancel=" "$(cleanup empty.yml "$this_ref" 44)"
expect "base branch with a hyphen" "exit=0 cancel=51" \
    "$(cleanup hyphen.yml refs/heads/gh-readonly-queue/release-1.0/pr-3-bbb 53)"
expect "active runs past the first page" "exit=0 cancel=36 35" "$(cleanup paged.yml "$this_ref" 70 FAKE_PAGE_SIZE=1)"
# Run 37 starts right after the in_progress request: an order that lists in_progress before
# queued finds it under neither status.
write_moving
expect "a run starts after in_progress is listed" "exit=0 cancel=37" \
    "$(cleanup moving.yml "$this_ref" 44 FAKE_MOVE=in_progress:37:in_progress)"
# Run 37 starts right after the queued request, so it is listed under both statuses.
write_moving
expect "a run listed under two statuses" "exit=0 cancel=37" \
    "$(cleanup moving.yml "$this_ref" 44 FAKE_MOVE=queued:37:in_progress)"
expect "one status fails to list" "exit=0 cancel=42" "$(cleanup ci.yml "$this_ref" 44 FAKE_FAIL=status=in_progress)"
grep -q '::warning::could not list the in_progress merge_group runs of ci.yml' "$fixture_dir/out"
expect "cancel failure" "exit=0 cancel=42 41" "$(cleanup ci.yml "$this_ref" 44 FAKE_FAIL=runs/41/cancel)"
grep -q '::warning::could not cancel ci.yml run 41' "$fixture_dir/out"
grep -q '^cancelled 1 rebuilt' "$fixture_dir/out"

echo 'rebuilt merge group cleanup tests passed'
