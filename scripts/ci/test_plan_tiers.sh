#!/usr/bin/env bash
# Focused tests for the CI tier planner, with a fake `gh` on PATH, so no API access is needed.
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
fixture_dir=$(mktemp -d "$PWD/.plan-tiers-test.XXXXXX")
trap 'rm -rf "$fixture_dir"' EXIT HUP INT TERM

# The fake gh answers by API path. FAKE_RUNS is the merge_group run count, FAKE_LABELS and
# FAKE_FILES are newline-separated answers; FAKE_FAIL names a path fragment that fails.
mkdir "$fixture_dir/bin"
cat > "$fixture_dir/bin/gh" <<'EOF'
#!/usr/bin/env bash
path=""
for arg in "$@"; do
    case "$arg" in repos/*) path=$arg ;; esac
done
if [[ -n ${FAKE_FAIL:-} && $path == *"$FAKE_FAIL"* ]]; then
    echo "HTTP 502" >&2
    exit 1
fi
case "$path" in
    */actions/workflows/*/runs*) echo "${FAKE_RUNS:-0}" ;;
    */pulls/*/files*) printf '%s\n' "${FAKE_FILES:-}" ;;
    */pulls/*) [[ -n ${FAKE_LABELS:-} ]] && printf '%s\n' "$FAKE_LABELS" ;;
esac
exit 0
EOF
chmod +x "$fixture_dir/bin/gh"

# Runs the planner for EVENT with the remaining NAME=VALUE overrides and prints the
# `run=... extended=...` pair it wrote to GITHUB_OUTPUT.
plan() {
    local event=$1
    shift
    : > "$fixture_dir/output"
    env PATH="$fixture_dir/bin:$PATH" GH_TOKEN=x REPO=o/r EVENT="$event" SHA=abc123 PR_NUMBER=7 \
        WORKFLOW_FILE=ci.yml EXTENDED_LABEL=ci:macos \
        EXTENDED_PATHS='^(\.github/workflows/ci\.yml|scripts/ci/)' \
        GITHUB_OUTPUT="$fixture_dir/output" "$@" \
        bash "$script_dir/plan_tiers.sh" > /dev/null 2>&1
    tr '\n' ' ' < "$fixture_dir/output" | sed 's/ $//'
}

# Asserts that a planner case produced the expected output pair.
expect() {
    local name=$1 want=$2 got=$3
    if [[ $got != "$want" ]]; then
        echo "$name: expected '$want', got '$got'" >&2
        exit 1
    fi
}

expect "merge queue" "run=true extended=true" "$(plan merge_group)"
expect "schedule" "run=true extended=true" "$(plan schedule)"
expect "push already verified" "run=false extended=false" "$(plan push FAKE_RUNS=1)"
expect "push not verified" "run=true extended=true" "$(plan push FAKE_RUNS=0)"
expect "push lookup failure" "run=true extended=true" "$(plan push FAKE_FAIL=actions)"
expect "plain pull request" "run=true extended=false" \
    "$(plan pull_request FAKE_LABELS=$'type:fix\nsize:s' FAKE_FILES=$'src/main.rs\ntests/a.rs')"
expect "labelled pull request" "run=true extended=true" \
    "$(plan pull_request FAKE_LABELS=$'type:fix\nci:macos' FAKE_FILES=src/main.rs)"
expect "label is matched exactly" "run=true extended=false" \
    "$(plan pull_request FAKE_LABELS=ci:macos-later FAKE_FILES=src/main.rs)"
expect "scheduling path" "run=true extended=true" \
    "$(plan pull_request FAKE_FILES=$'src/main.rs\nscripts/ci/run_eval_codegen_shard.sh')"
expect "label lookup failure" "run=true extended=true" "$(plan pull_request FAKE_FAIL=pulls/7)"
expect "file lookup failure" "run=true extended=true" "$(plan pull_request FAKE_FAIL=files)"
expect "unknown event" "run=true extended=true" "$(plan release)"

echo 'CI tier planner tests passed'
