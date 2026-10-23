#!/usr/bin/env bash
#
# Tests for php-upstream-watch.sh.
#
# Runs pure-helper unit checks against in-memory fixtures, then a full
# end-to-end pass with a fake `gh` on PATH (dry-run and real issue creation,
# including dedup/state), so the watcher can be validated without network access.
#
# Called from:
#   .github/workflows/ci.yml, `upstream-watch-tests` job (ubuntu-latest)
#   Locally: bash .github/scripts/test_php-upstream-watch.sh
#
# Key details:
#   - Portable across bash 3.2 (macOS) and bash 5 (Ubuntu); no associative
#     arrays and no ${var,,} case folding.
#   - The watcher is sourced WITHOUT set -e, so individual assertions can fail
#     without aborting the suite; a non-zero exit at the end reports failure.

set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=php-upstream-watch.sh
. "$HERE/php-upstream-watch.sh"

failures=0
pass() { printf 'ok   - %s\n' "$1"; }
fail() { printf 'FAIL - %s\n' "$1" >&2; failures=$(( failures + 1 )); }

assert_eq() { # desc expected actual
  if [ "$2" = "$3" ]; then pass "$1"; else fail "$1 (expected [$2], got [$3])"; fi
}

assert_contains() { # desc needle haystack
  case "$3" in
    *"$2"*) pass "$1" ;;
    *) fail "$1 (missing [$2] in output)" ;;
  esac
}

assert_file_contains() { # desc needle file
  if [ -f "$3" ] && grep -qF -- "$2" "$3"; then
    pass "$1"
  else
    fail "$1 (missing [$2] in $3)"
  fi
}

assert_file_lacks() { # desc needle file
  if [ -f "$3" ] && grep -qF -- "$2" "$3"; then
    fail "$1 (unexpected [$2] in $3)"
  else
    pass "$1"
  fi
}

WORK="$(mktemp -d "${TMPDIR:-/tmp}/php-watch-test.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT
FIX="$WORK/fixtures"
mkdir -p "$FIX" "$WORK/bin"

# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------

cat >"$FIX/releases.json" <<'JSON'
[
 {"tag_name":"php-8.5.11","draft":false,"prerelease":false,"html_url":"https://example/8511","published_at":"2026-09-24T12:00:00Z","body":"This is a security release."},
 {"tag_name":"php-8.5.10","draft":false,"prerelease":false,"html_url":"https://example/8510","published_at":"2026-08-28T12:00:00Z","body":"This is a bug fix release."},
 {"tag_name":"php-8.4.26","draft":false,"prerelease":false,"html_url":"https://example/8426","published_at":"2026-09-24T12:00:00Z","body":"This is a bug fix release."},
 {"tag_name":"php-8.4.25","draft":false,"prerelease":false,"html_url":"https://example/8425","published_at":"2026-08-28T12:00:00Z","body":"release"},
 {"tag_name":"php-8.6.0RC3","draft":false,"prerelease":true,"html_url":"https://example/86rc3","published_at":"2026-09-01T12:00:00Z","body":"rc"},
 {"tag_name":"php-8.1.99","draft":false,"prerelease":false,"html_url":"https://example/8199","published_at":"2025-01-01T12:00:00Z","body":"old"}
]
JSON

cat >"$FIX/NEWS" <<'NEWS'
PHP                                                                        NEWS
|||||||||||||||||||||||||||||||||||||||||||||||||||||||||||||||||||||||||||||||
24 Sep 2026, PHP 8.5.11

- Core:
  . Fixed a thing in array_first(). (Someone)

|||||||||||||||||||||||||||||||||||||||||||||||||||||||||||||||||||||||||||||||
28 Aug 2026, PHP 8.5.10

- Core:
  . Fixed an older thing. (Someone)

|||||||||||||||||||||||||||||||||||||||||||||||||||||||||||||||||||||||||||||||
24 Sep 2026, PHP 8.4.26

- GD:
  . Fixed a gd thing. (Someone)
NEWS

cat >"$FIX/UPGRADING" <<'UPG'
PHP 8.5 UPGRADE NOTES

1. New Functions
2. New Global Constants

========================================
1. New Functions
========================================

- Core:
  . Added array_first() and array_last().

========================================
2. New Global Constants
========================================

- Core:
  . Added PHP_NEW_CONST.
UPG

cat >"$FIX/compare.json" <<'JSON'
{"files":[{"filename":"ext/dom/a.c"},{"filename":"ext/dom/b.c"},{"filename":"Zend/zend.c"},{"filename":"README.md"},{"filename":"tests/dom/foo.phpt"}]}
JSON

cat >"$FIX/baseline.json" <<'JSON'
{"php_version":"8.5.10"}
JSON

cat >"$WORK/bin/gh" <<'GH'
#!/usr/bin/env bash
set -u
fix="${FIXTURE_DIR:?}"
cmd="${1:-}"; shift || true
case "$cmd" in
  api)
    # Guard: `gh api` with a field defaults to POST, which 404s on read-only
    # endpoints. The watcher must always pair `-f` with `-X GET`.
    has_field=0; has_post=0; explicit_method=0; prev=""
    for a in "$@"; do
      case "$a" in
        -f|--field|-F|--raw-field|-f=*|--field=*|-F=*|--raw-field=*) has_field=1 ;;
        -X|-X=*|--method|--method=*) explicit_method=1 ;;
        --method=POST|-X=POST) has_post=1 ;;
      esac
      if [ "$prev" = "-X" ] || [ "$prev" = "--method" ]; then
        [ "$a" = "POST" ] && has_post=1
      fi
      prev="$a"
    done
    if [ "$has_field" -eq 1 ] && [ "$explicit_method" -ne 1 ]; then
      printf 'fake gh: `gh api` with a field must set the method explicitly (add -X GET)\n' >&2
      exit 1
    fi
    # Find the endpoint: the first argument that is neither a flag nor a flag value.
    endpoint=""; expect_value=0
    for a in "$@"; do
      if [ "$expect_value" -eq 1 ]; then expect_value=0; continue; fi
      case "$a" in
        -X|--method) expect_value=1; continue ;;
        -X=*|--method=*) continue ;;
        -*) continue ;;
      esac
      endpoint="$a"; break
    done
    case "$endpoint" in
      *"/releases"*) cat "$fix/releases.json" ;;
      *"/contents/NEWS"*) cat "$fix/NEWS" ;;
      *"/contents/UPGRADING"*) cat "$fix/UPGRADING" ;;
      *"/compare/"*) cat "$fix/compare.json" ;;
      *"/milestones"*)
        if [ "$has_post" -eq 1 ]; then printf '{"number": 1}\n'; else printf '[]\n'; fi ;;
      *"search/issues"*)
        if [ "${SEARCH_FAILED:-0}" = "1" ]; then
          printf 'fake gh: issue search unavailable\n' >&2
          exit 1
        fi
        printf '%s\n' "${SEARCH_COUNT:-0}" ;;
      *) printf '{}\n' ;;
    esac
    ;;
  label) printf 'type:chore\ntopic:php-compat\n' ;;
  issue)
    sub="${1:-}"; shift || true
    if [ "$sub" = "create" ]; then
      printf '%s\n' "$*" >>"$fix/issue_args.log"
      prev=""
      for a in "$@"; do
        if [ "$prev" = "--title" ]; then printf '%s\n' "$a" >>"$fix/titles.log"; fi
        if [ "$prev" = "--body-file" ]; then cp "$a" "$fix/body_$(basename "$a")"; fi
        prev="$a"
      done
      printf 'https://github.com/example/issues/1\n'
    fi
    ;;
  *) printf '{}\n' ;;
esac
exit 0
GH
chmod +x "$WORK/bin/gh"
export FIXTURE_DIR="$FIX"
export PATH="$WORK/bin:$PATH"
export GH_TOKEN=fake-token

# ---------------------------------------------------------------------------
# Pure helpers
# ---------------------------------------------------------------------------

assert_eq "version_gt true" "yes" "$(version_gt 8.5.11 8.5.10 && echo yes || echo no)"
assert_eq "version_gt false" "no" "$(version_gt 8.5.10 8.5.11 && echo yes || echo no)"
assert_eq "baseline newer-patch" "newer-patch" "$(baseline_kind 8.5.11 8.5.10)"
assert_eq "baseline current" "current" "$(baseline_kind 8.5.10 8.5.10)"
assert_eq "baseline older" "current" "$(baseline_kind 8.4.26 8.5.10)"
assert_eq "baseline newer-minor" "newer-minor" "$(baseline_kind 8.6.0 8.5.10)"

news="$(extract_news_section "$FIX/NEWS" 8.5.11)"
assert_contains "news section body" "Fixed a thing in array_first()" "$news"
case "$news" in
  *"older thing"*) fail "news section must stop at the next release" ;;
  *) pass "news section stops at the next release" ;;
esac

titles="$(emit_upgrading_sections "$WORK" "$FIX/UPGRADING")"
assert_contains "upgrading title" "New Functions" "$titles"

assert_eq "modules summary" "ext/dom x2, core (Zend) x1" "$(modules_summary "$FIX/compare.json")"

# render_upgrading: embeds the section and reports elephc coverage.
mkdir -p "$WORK/render"
printf '01\tNew Functions\n' >"$WORK/render/titles.tsv"
printf -- '- Core:\n  . Added array_first() and array_unknown().\n' >"$WORK/render/upg_01.txt"
printf 'array_first\n' >"$WORK/covered.txt"
: >"$WORK/upg_body.md"
render_upgrading "$WORK/upg_body.md" "$WORK/render/titles.tsv" "$WORK/render" "$WORK/covered.txt"
assert_file_contains "upgrading embeds section" "**New Functions**" "$WORK/upg_body.md"
assert_file_contains "upgrading reports coverage" "elephc covers 1" "$WORK/upg_body.md"
assert_file_contains "upgrading lists uncovered" "array_unknown" "$WORK/upg_body.md"

normalized="$(normalize_releases "$(min_minor_num 8.2)" <"$FIX/releases.json")"
assert_eq "normalize keeps watched stable" "2" "$(printf '%s' "$normalized" | jq '[.[] | select(.tag == "php-8.5.11" or .tag == "php-8.4.26")] | length')"
assert_eq "normalize drops prerelease" "0" "$(printf '%s' "$normalized" | jq '[.[] | select(.tag == "php-8.6.0RC3")] | length')"
assert_eq "normalize drops below min-minor" "0" "$(printf '%s' "$normalized" | jq '[.[] | select(.tag == "php-8.1.99")] | length')"

assert_eq "plan fresh opens latest per branch" "2" "$(plan_releases "$normalized" '{}' 0 0 | jq 'length')"
assert_eq "plan seed opens nothing" "0" "$(plan_releases "$normalized" '{}' 1 0 | jq 'length')"
assert_eq "plan state current opens nothing" "0" "$(plan_releases "$normalized" '{"8.5":"php-8.5.11","8.4":"php-8.4.26"}' 0 1 | jq 'length')"
assert_eq "plan state behind opens both" "2" "$(plan_releases "$normalized" '{"8.5":"php-8.5.10","8.4":"php-8.4.25"}' 0 1 | jq 'length')"
assert_eq "plan chains compare base" "php-8.5.10" "$(plan_releases "$normalized" '{"8.5":"php-8.5.10","8.4":"php-8.4.26"}' 0 1 | jq -r '.[] | select(.tag == "php-8.5.11") | .prev_tag')"
# Seed mode must plan nothing even when an existing branch pointer is behind:
# --seed-only promises "record pointers, open nothing" (review regression:
# it used to open the backlog between the recorded pointer and the latest).
assert_eq "plan seed with existing state opens nothing" "0" \
  "$(plan_releases "$normalized" '{"8.5":"php-8.5.10","8.4":"php-8.4.25"}' 1 1 | jq 'length')"

# A branch missing from an existing state plans its whole fetched backlog
# (a late-discovered branch must not lose the releases before its newest);
# a true first run (no state file) stays latest-only for the kickoff guard.
newbranch='[{"tag_name":"php-8.6.1","draft":false,"prerelease":false,"html_url":"https://example/861","published_at":"2026-11-19T12:00:00Z","body":"This is a bug fix release."},{"tag_name":"php-8.6.0","draft":false,"prerelease":false,"html_url":"https://example/860","published_at":"2026-10-22T12:00:00Z","body":"This is a security release."}]'
newnorm="$(normalize_releases "$(min_minor_num 8.2)" <<<"$newbranch")"
assert_eq "plan unknown branch opens the fetched backlog" "2" \
  "$(plan_releases "$newnorm" '{"8.4":"php-8.4.26"}' 0 1 | jq 'length')"
assert_eq "plan unknown branch starts at its oldest" "true" \
  "$(plan_releases "$newnorm" '{"8.4":"php-8.4.26"}' 0 1 | jq -r '.[] | select(.tag == "php-8.6.0") | .first | tostring')"
assert_eq "plan unknown branch chains prev" "php-8.6.0" \
  "$(plan_releases "$newnorm" '{"8.4":"php-8.4.26"}' 0 1 | jq -r '.[] | select(.tag == "php-8.6.1") | .prev_tag')"
assert_eq "plan fresh stays latest-only" "1" "$(plan_releases "$newnorm" '{}' 0 0 | jq 'length')"

# ---------------------------------------------------------------------------
# End-to-end: dry run (no writes)
# ---------------------------------------------------------------------------

dry_out="$WORK/dry.log"
bash "$HERE/php-upstream-watch.sh" --repo example/elephc --state "$WORK/state.json" \
  --baseline "$FIX/baseline.json" --dry-run --kickoff >"$dry_out" 2>&1 || true
assert_file_contains "dry-run plans 8.5.11" "would open: [php-src] PHP 8.5.11 released" "$dry_out"
assert_file_contains "dry-run plans 8.4.26" "would open: [php-src] PHP 8.4.26 released" "$dry_out"
if [ -f "$WORK/state.json" ]; then
  fail "dry-run must not write state"
else
  pass "dry-run writes no state"
fi

# Dry-run must honour the (read-only) dedup search.
SEARCH_COUNT=1 bash "$HERE/php-upstream-watch.sh" --repo example/elephc \
  --state "$WORK/state-dry2.json" --baseline "$FIX/baseline.json" --dry-run --kickoff \
  >"$WORK/dry2.log" 2>&1 || true
assert_file_contains "dry-run reports an existing issue" "already exists" "$WORK/dry2.log"
if grep -qF "would open:" "$WORK/dry2.log"; then
  fail "dry-run must not claim it would open an existing issue"
else
  pass "dry-run does not claim to open existing issues"
fi

# ---------------------------------------------------------------------------
# End-to-end: silent first run, then a real run with fake gh, then a second run
# ---------------------------------------------------------------------------

# A first run without --kickoff seeds silently (no issue burst).
: >"$FIX/titles.log"
bash "$HERE/php-upstream-watch.sh" --repo example/elephc --state "$WORK/state-silent.json" \
  --baseline "$FIX/baseline.json" >"$WORK/silent.log" 2>&1 || fail "silent first run exited non-zero"
if [ -s "$FIX/titles.log" ]; then
  fail "silent first run must open nothing"
else
  pass "silent first run opens nothing"
fi
assert_file_contains "silent first run seeds pointers" "recording branch pointers only" "$WORK/silent.log"
assert_eq "silent first run records state" "2" "$(jq '.branches | length' "$WORK/state-silent.json")"

: >"$FIX/titles.log"
: >"$FIX/issue_args.log"
run1="$WORK/run1.log"
bash "$HERE/php-upstream-watch.sh" --repo example/elephc --state "$WORK/state.json" \
  --baseline "$FIX/baseline.json" --kickoff >"$run1" 2>&1 || fail "first real run exited non-zero"
assert_file_contains "first run opens 8.5.11" "[php-src] PHP 8.5.11 released" "$FIX/titles.log"
assert_file_contains "first run opens 8.4.26" "[php-src] PHP 8.4.26 released" "$FIX/titles.log"
assert_eq "state records both branches" "2" "$(jq '.branches | length' "$WORK/state.json")"
assert_eq "state records 8.5.11" "php-8.5.11" "$(jq -r '.branches["8.5"]' "$WORK/state.json")"
assert_file_contains "issue body carries marker" "<!-- php-src-watch:php-8.5.11 -->" \
  "$FIX/body_body_php-8.5.11.md"
assert_file_contains "issue body links the changelog" "ChangeLog-8.php#8.5.11" \
  "$FIX/body_body_php-8.5.11.md"
assert_file_contains "first run sets the PHP milestone" "--milestone PHP 8.5" "$FIX/issue_args.log"

state_snapshot="$(cat "$WORK/state.json")"
: >"$FIX/titles.log"
run2="$WORK/run2.log"
bash "$HERE/php-upstream-watch.sh" --repo example/elephc --state "$WORK/state.json" \
  --baseline "$FIX/baseline.json" >"$run2" 2>&1 || fail "second real run exited non-zero"
if [ -s "$FIX/titles.log" ]; then
  fail "second run must not re-open issues"
else
  pass "second run opens nothing (state is current)"
fi
assert_eq "second run does not rewrite state" "$state_snapshot" "$(cat "$WORK/state.json")"

# ---------------------------------------------------------------------------
# End-to-end: flood guard must not advance a pointer past a deferred release
# ---------------------------------------------------------------------------

# planned.json is branch-grouped with patches ascending per branch, so the
# flood guard keeps the OLDEST planned tags and defers the newest ones. With
# --max-issues 1, run one opens 8.4.26 and must leave 8.5 unplanned: the 8.5
# pointer stays absent so the next run retries the deferred 8.5.11 tag.
: >"$FIX/titles.log"
bash "$HERE/php-upstream-watch.sh" --repo example/elephc --state "$WORK/state-flood.json" \
  --baseline "$FIX/baseline.json" --kickoff --max-issues 1 >"$WORK/flood1.log" 2>&1 \
  || fail "flood-guard run exited non-zero"
assert_file_contains "flood guard opens the oldest planned issue" "PHP 8.4.26 released" "$FIX/titles.log"
assert_file_lacks "flood guard defers the newest issue" "PHP 8.5.11 released" "$FIX/titles.log"
assert_file_contains "flood guard logs the deferral" "deferred to a later run" "$WORK/flood1.log"
assert_eq "flood guard records the opened branch" "php-8.4.26" "$(jq -r '.branches["8.4"]' "$WORK/state-flood.json")"
assert_eq "flood guard leaves the deferred branch pointer unadvanced" "false" \
  "$(jq -r '.branches | has("8.5")' "$WORK/state-flood.json")"

# The deferred branch is retried on the next run, and — because it is unknown
# to the state — its whole fetched backlog is filed: both 8.5.10 and 8.5.11.
: >"$FIX/titles.log"
bash "$HERE/php-upstream-watch.sh" --repo example/elephc --state "$WORK/state-flood.json" \
  --baseline "$FIX/baseline.json" >"$WORK/flood2.log" 2>&1 \
  || fail "flood-guard retry run exited non-zero"
assert_file_contains "flood guard retries the deferred branch" "PHP 8.5.11 released" "$FIX/titles.log"
assert_file_contains "flood guard files the branch's older patch too" "PHP 8.5.10 released" "$FIX/titles.log"
assert_eq "flood guard records the retried pointer" "php-8.5.11" "$(jq -r '.branches["8.5"]' "$WORK/state-flood.json")"

# ---------------------------------------------------------------------------
# End-to-end: flood guard must cut a branch mid-chain without losing a tag
# ---------------------------------------------------------------------------

# State records php-8.4.0 / php-8.5.0 while every stable patch on both branches
# is waiting: 8.4.1..8.4.5 plus 8.5.1. --max-issues 3 cuts planned.json in the
# middle of the 8.4 chain, so run one must open the contiguous oldest prefix
# (8.4.1-8.4.3) and leave the rest pending. A pointer that jumped past an
# unopened tag would hide 8.4.4/8.4.5 forever; the two runs must file all six
# tags and end with every pointer on the branch's latest release.
CHAIN="$WORK/fixtures-chain"
mkdir -p "$CHAIN"
cp "$FIX/NEWS" "$FIX/UPGRADING" "$FIX/compare.json" "$FIX/baseline.json" "$CHAIN/"
cat >"$CHAIN/releases.json" <<'JSON'
[
 {"tag_name":"php-8.5.1","draft":false,"prerelease":false,"html_url":"https://example/85p1","published_at":"2026-01-22T12:00:00Z","body":"This is a bug fix release."},
 {"tag_name":"php-8.4.5","draft":false,"prerelease":false,"html_url":"https://example/84p5","published_at":"2026-01-22T12:00:00Z","body":"This is a bug fix release."},
 {"tag_name":"php-8.4.4","draft":false,"prerelease":false,"html_url":"https://example/84p4","published_at":"2025-12-18T12:00:00Z","body":"This is a security release."},
 {"tag_name":"php-8.4.3","draft":false,"prerelease":false,"html_url":"https://example/84p3","published_at":"2025-11-20T12:00:00Z","body":"This is a bug fix release."},
 {"tag_name":"php-8.4.2","draft":false,"prerelease":false,"html_url":"https://example/84p2","published_at":"2025-10-16T12:00:00Z","body":"This is a bug fix release."},
 {"tag_name":"php-8.4.1","draft":false,"prerelease":false,"html_url":"https://example/84p1","published_at":"2025-09-18T12:00:00Z","body":"This is a bug fix release."}
]
JSON
printf '{"schema":1,"updated_at":"2025-09-01","branches":{"8.4":"php-8.4.0","8.5":"php-8.5.0"}}\n' \
  >"$WORK/state-chain.json"
CHAIN_ARGS=(--repo example/elephc --state "$WORK/state-chain.json" --baseline "$CHAIN/baseline.json")

: >"$CHAIN/titles.log"
FIXTURE_DIR="$CHAIN" bash "$HERE/php-upstream-watch.sh" "${CHAIN_ARGS[@]}" \
  --max-issues 3 >"$WORK/chain1.log" 2>&1 || fail "chain run 1 exited non-zero"
cp "$CHAIN/titles.log" "$WORK/chain1-titles.txt"
assert_eq "chain run 1 opens the flood cap" "3" "$(wc -l <"$WORK/chain1-titles.txt" | tr -d ' ')"
assert_file_contains "chain run 1 opens the chain head" "PHP 8.4.1 released" "$WORK/chain1-titles.txt"
assert_file_contains "chain run 1 opens the second tag" "PHP 8.4.2 released" "$WORK/chain1-titles.txt"
assert_file_contains "chain run 1 opens the third tag" "PHP 8.4.3 released" "$WORK/chain1-titles.txt"
assert_file_lacks "chain run 1 skips the mid-chain tail" "PHP 8.4.4 released" "$WORK/chain1-titles.txt"
assert_file_lacks "chain run 1 skips the newest patch" "PHP 8.4.5 released" "$WORK/chain1-titles.txt"
assert_file_lacks "chain run 1 skips the other branch" "PHP 8.5.1 released" "$WORK/chain1-titles.txt"
assert_eq "chain run 1 advances the pointer through opened tags only" "php-8.4.3" \
  "$(jq -r '.branches["8.4"]' "$WORK/state-chain.json")"
assert_eq "chain run 1 leaves the untouched branch behind" "php-8.5.0" \
  "$(jq -r '.branches["8.5"]' "$WORK/state-chain.json")"

# Run 2: the mid-chain remainder (8.4.4-8.4.5) and 8.5.1 all fit under the cap.
# prev_tag must chain from the new pointer, not restart from the branch head.
: >"$CHAIN/titles.log"
FIXTURE_DIR="$CHAIN" bash "$HERE/php-upstream-watch.sh" "${CHAIN_ARGS[@]}" \
  --max-issues 3 >"$WORK/chain2.log" 2>&1 || fail "chain run 2 exited non-zero"
cp "$CHAIN/titles.log" "$WORK/chain2-titles.txt"
assert_eq "chain run 2 opens the remainder" "3" "$(wc -l <"$WORK/chain2-titles.txt" | tr -d ' ')"
assert_file_contains "chain run 2 opens the deferred patch" "PHP 8.4.4 released" "$WORK/chain2-titles.txt"
assert_file_contains "chain run 2 opens the last 8.4 patch" "PHP 8.4.5 released" "$WORK/chain2-titles.txt"
assert_file_contains "chain run 2 opens the deferred branch" "PHP 8.5.1 released" "$WORK/chain2-titles.txt"
assert_file_lacks "chain run 2 does not re-open filed tags" "PHP 8.4.1 released" "$WORK/chain2-titles.txt"
assert_eq "chain run 2 ends 8.4 on its latest" "php-8.4.5" \
  "$(jq -r '.branches["8.4"]' "$WORK/state-chain.json")"
assert_eq "chain run 2 ends 8.5 on its latest" "php-8.5.1" \
  "$(jq -r '.branches["8.5"]' "$WORK/state-chain.json")"

# Run 3: state is current, so nothing is planned and state is untouched.
state_snapshot="$(cat "$WORK/state-chain.json")"
: >"$CHAIN/titles.log"
FIXTURE_DIR="$CHAIN" bash "$HERE/php-upstream-watch.sh" "${CHAIN_ARGS[@]}" \
  >"$WORK/chain3.log" 2>&1 || fail "chain run 3 exited non-zero"
if [ -s "$CHAIN/titles.log" ]; then
  fail "chain run 3 must open nothing once state is current"
else
  pass "chain run 3 opens nothing once state is current"
fi
assert_eq "chain run 3 does not rewrite state" "$state_snapshot" "$(cat "$WORK/state-chain.json")"

assert_eq "chain files every pending tag exactly once" "6" "$(cat "$WORK/chain1-titles.txt" "$WORK/chain2-titles.txt" | wc -l | tr -d ' ')"

# ---------------------------------------------------------------------------
# End-to-end: --seed-only must open nothing even with outdated state
# ---------------------------------------------------------------------------

# --seed-only with an existing, behind state promises "record pointers, open
# nothing": both branches are recorded as current, and any pending backlog
# (8.4.26, 8.5.11) is skipped deliberately instead of being opened.
printf '{"schema":1,"updated_at":"2026-08-01","branches":{"8.5":"php-8.5.10","8.4":"php-8.4.25"}}\n' \
  >"$WORK/state-seed.json"
: >"$FIX/titles.log"
bash "$HERE/php-upstream-watch.sh" --repo example/elephc --state "$WORK/state-seed.json" \
  --baseline "$FIX/baseline.json" --seed-only >"$WORK/seed-existing.log" 2>&1 \
  || fail "seed-only run with existing state exited non-zero"
if [ -s "$FIX/titles.log" ]; then
  fail "seed-only must not open issues with existing state"
else
  pass "seed-only opens no issue with existing state"
fi
assert_file_contains "seed-only logs its silent re-record" "recording branch pointers without opening issues" "$WORK/seed-existing.log"
assert_eq "seed-only re-records the behind 8.5 pointer" "php-8.5.11" "$(jq -r '.branches["8.5"]' "$WORK/state-seed.json")"
assert_eq "seed-only re-records the behind 8.4 pointer" "php-8.4.26" "$(jq -r '.branches["8.4"]' "$WORK/state-seed.json")"

# The re-seeded state makes the next normal run plan nothing.
: >"$FIX/titles.log"
bash "$HERE/php-upstream-watch.sh" --repo example/elephc --state "$WORK/state-seed.json" \
  --baseline "$FIX/baseline.json" >"$WORK/seed-after.log" 2>&1 \
  || fail "post-seed run exited non-zero"
if [ -s "$FIX/titles.log" ]; then
  fail "post-seed run must not re-open the skipped backlog"
else
  pass "post-seed run plans nothing after the re-seed"
fi

# ---------------------------------------------------------------------------
# End-to-end: a late-discovered branch files its full backlog, including .0
# ---------------------------------------------------------------------------

# State knows 8.4/8.5 but not 8.6. The fetched releases include BOTH 8.6.0 and
# 8.6.1: the run must file both — 8.6.0 keeps its UPGRADING highlights, its
# coverage count and the new-minor baseline checklist, and 8.6.1 chains from
# it — instead of jumping the pointer straight to 8.6.1 and losing 8.6.0.
NEWB="$WORK/fixtures-new-branch"
mkdir -p "$NEWB"
cp "$FIX/NEWS" "$FIX/UPGRADING" "$FIX/compare.json" "$FIX/baseline.json" "$NEWB/"
# Deterministic coverage: the "New Functions" section mentions array_first()
# and array_last(); this synthetic registry covers only array_first().
printf '[{"name":"array_first"}]\n' >"$NEWB/registry.json"
cat >"$NEWB/releases.json" <<'JSON'
[
 {"tag_name":"php-8.6.1","draft":false,"prerelease":false,"html_url":"https://example/new861","published_at":"2026-11-19T12:00:00Z","body":"This is a bug fix release."},
 {"tag_name":"php-8.6.0","draft":false,"prerelease":false,"html_url":"https://example/new860","published_at":"2026-10-22T12:00:00Z","body":"This is a security release."}
]
JSON
printf '{"schema":1,"updated_at":"2026-10-01","branches":{"8.4":"php-8.4.26","8.5":"php-8.5.11"}}\n' \
  >"$WORK/state-newbranch.json"
: >"$NEWB/titles.log"
FIXTURE_DIR="$NEWB" bash "$HERE/php-upstream-watch.sh" --repo example/elephc \
  --state "$WORK/state-newbranch.json" --baseline "$NEWB/baseline.json" \
  --builtin-registry "$NEWB/registry.json" >"$WORK/newbranch.log" 2>&1 \
  || fail "late-branch run exited non-zero"
assert_eq "late branch files every fetched release" "2" "$(wc -l <"$NEWB/titles.log" | tr -d ' ')"
assert_file_contains "late branch opens the .0 release" "PHP 8.6.0 released" "$NEWB/titles.log"
assert_file_contains "late branch opens the next patch" "PHP 8.6.1 released" "$NEWB/titles.log"
assert_file_contains "late branch .0 body carries marker" "<!-- php-src-watch:php-8.6.0 -->" "$NEWB/body_body_php-8.6.0.md"
assert_file_contains "late branch .0 flags first stable on branch" "first stable release seen on this branch" "$NEWB/body_body_php-8.6.0.md"
assert_file_contains "minor .0 body embeds UPGRADING highlights" "UPGRADING highlights" "$NEWB/body_body_php-8.6.0.md"
assert_file_contains "minor .0 body embeds the New Functions section" "**New Functions**" "$NEWB/body_body_php-8.6.0.md"
assert_file_contains "minor .0 body reports the coverage count" "elephc covers 1" "$NEWB/body_body_php-8.6.0.md"
assert_file_contains "minor .0 body lists uncovered functions" "array_last" "$NEWB/body_body_php-8.6.0.md"
assert_file_contains "minor .0 body carries the new-minor checklist" "opens the **8.6** branch" "$NEWB/body_body_php-8.6.0.md"
assert_file_contains "minor .0 checklist names the profile bump" 'profile to `--php-version`' "$NEWB/body_body_php-8.6.0.md"
assert_eq "late branch ends on its latest" "php-8.6.1" "$(jq -r '.branches["8.6"]' "$WORK/state-newbranch.json")"
assert_eq "late branch run leaves recorded branches untouched" "php-8.5.11" "$(jq -r '.branches["8.5"]' "$WORK/state-newbranch.json")"

# ---------------------------------------------------------------------------
# End-to-end: a failed dedup search aborts instead of risking duplicates
# ---------------------------------------------------------------------------

# A transient search outage must not read as "no duplicates": the run stops
# with a non-zero exit, opens nothing, and leaves state untouched, so the next
# run retries cleanly once the search API answers again.
printf '{"schema":1,"updated_at":"2026-09-01","branches":{"8.5":"php-8.5.10","8.4":"php-8.4.25"}}\n' \
  >"$WORK/state-searchfail.json"
searchfail_snapshot="$(cat "$WORK/state-searchfail.json")"
: >"$FIX/titles.log"
rc=0
SEARCH_FAILED=1 bash "$HERE/php-upstream-watch.sh" --repo example/elephc \
  --state "$WORK/state-searchfail.json" --baseline "$FIX/baseline.json" \
  >"$WORK/searchfail.log" 2>&1 || rc=$?
if [ "$rc" -eq 0 ]; then
  fail "failed dedup search must abort the run (exit non-zero)"
else
  pass "failed dedup search aborts the run"
fi
assert_file_lacks "failed dedup search opens no issue" "PHP 8.5.11 released" "$FIX/titles.log"
assert_file_contains "failed dedup search explains the abort" "duplicate search failed" "$WORK/searchfail.log"
assert_eq "failed dedup search leaves state untouched" "$searchfail_snapshot" "$(cat "$WORK/state-searchfail.json")"

echo
if [ "$failures" -eq 0 ]; then
  echo "All watcher tests passed."
  exit 0
fi
echo "$failures watcher test(s) failed." >&2
exit 1
