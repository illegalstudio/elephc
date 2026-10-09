#!/usr/bin/env bash
#
# php-src upstream watcher.
#
# Detects newly published stable releases of php/php-src, enriches each one with
# its NEWS/UPGRADING content and a compare-based module summary, then opens one
# tracking issue per release tag on the elephc repository. The last seen tag per
# maintenance branch is stored in a committed JSON file, so re-runs never open
# duplicates.
#
# Called from:
#   .github/workflows/php-upstream-watch.yml (weekly schedule + manual dispatch)
#   .github/scripts/test_php-upstream-watch.sh (sources this file and drives the
#   pure helpers directly)
#
# Requirements: bash, gh (authenticated via GH_TOKEN), jq, awk.
#
# Key details:
#   - Only stable php-X.Y.Z releases on watched branches are considered;
#     pre-releases (RC/beta/alpha) are ignored on purpose.
#   - First run (no state file) records the branch pointers silently; pass
#     --kickoff to also open one issue for each watched branch's current latest
#     release. Later runs open one issue per newly published tag. --seed-only
#     records pointers without opening.
#   - --dry-run performs no writes: no issues and no state update.
#   - Any issue-creation failure makes the run exit non-zero without advancing
#     state, so the next run retries; already-opened issues are skipped by the
#     dedup search on their body marker.

UPSTREAM_REPO="php/php-src"
DEFAULT_REPO="illegalstudio/elephc"
DEFAULT_STATE=".github/upstream/php-src-watch.json"
DEFAULT_BASELINE="scripts/docs/php_baseline.json"
DEFAULT_BUILTIN_REGISTRY="scripts/docs/builtin_registry.json"
DEFAULT_SYMBOL_REGISTRY="scripts/docs/symbol_registry.json"
RELEASES_PER_PAGE=100
# php-src publishes ~1 release/ month/branch plus prereleases; two pages cover
# several years, so the oldest watched branch's latest cannot fall off page one.
MAX_RELEASE_PAGES=2
# Keep embedded NEWS/UPGRADING blocks bounded: a .0 release can embed five
# UPGRADING sections plus NEWS, and GitHub caps issue bodies at 65536 chars.
MAX_EMBED_BYTES=8000

# Configured by parse_args / environment.
TARGET_REPO="${TARGET_REPO:-$DEFAULT_REPO}"
STATE_FILE="${STATE_FILE:-$DEFAULT_STATE}"
BASELINE_FILE="${BASELINE_FILE:-$DEFAULT_BASELINE}"
BUILTIN_REGISTRY="${BUILTIN_REGISTRY:-$DEFAULT_BUILTIN_REGISTRY}"
SYMBOL_REGISTRY="${SYMBOL_REGISTRY:-$DEFAULT_SYMBOL_REGISTRY}"
MIN_MINOR="${MIN_MINOR:-8.2}"
MAX_ISSUES="${MAX_ISSUES:-20}"
DRY_RUN=0
SEED_ONLY=0
KICKOFF="${KICKOFF:-0}"
MILESTONE="${MILESTONE:-1}"

log() { printf '%s\n' "$*" >&2; }

die() { printf 'error: %s\n' "$*" >&2; exit 1; }

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"
}

# version_gt A B -> exit 0 when A > B, exit 1 otherwise.
version_gt() {
  awk -v a="$1" -v b="$2" 'BEGIN {
    n = split(a, A, "."); m = split(b, B, ".");
    for (i = 1; i <= 3; i++) {
      x = A[i] + 0; y = B[i] + 0;
      if (x > y) exit 0;
      if (x < y) exit 1;
    }
    exit 1;
  }'
}

# baseline_kind RELEASE_VERSION BASELINE_VERSION -> current | newer-patch | newer-minor
baseline_kind() {
  local release="$1" baseline="$2"
  if ! version_gt "$release" "$baseline"; then
    printf 'current\n'
    return 0
  fi
  if [ "${release%.*}" = "${baseline%.*}" ]; then
    printf 'newer-patch\n'
  else
    printf 'newer-minor\n'
  fi
}

# min_minor_num MAJOR.MINOR -> integer key for comparisons (major*1000+minor).
min_minor_num() {
  local major minor
  major="${1%%.*}"
  minor="${1##*.}"
  printf '%s\n' "$(( major * 1000 + minor ))"
}

# normalize_releases -> canonical stable watched releases, ascending per branch.
# Reads the releases JSON array on stdin.
normalize_releases() {
  local min_key="$1"
  jq -c --argjson min "$min_key" '
    def parse_tag:
      (.tag_name // "") as $t
      | ($t | capture("^php-(?<maj>[0-9]+)\\.(?<min>[0-9]+)\\.(?<pat>[0-9]+)$")? // null) as $v
      | select($v != null)
      | {
          tag: $t,
          maj: ($v.maj | tonumber),
          min: ($v.min | tonumber),
          pat: ($v.pat | tonumber),
          version: ($v.maj + "." + $v.min + "." + $v.pat),
          branch: ($v.maj + "." + $v.min),
          url: (.html_url // ("https://github.com/php/php-src/releases/tag/" + $t)),
          published: ((.published_at // "") | .[0:10]),
          body: (.body // "")
        };
    [ .[]
      | select((.draft // false) == false and (.prerelease // false) == false)
      | parse_tag
      | select(((.maj * 1000) + .min) >= $min)
    ]
    | sort_by([.maj, .min, .pat])
  '
}

# plan_releases NORMALIZED_JSON STATE_BRANCHES_JSON SEED_ONLY -> planned items.
# Each item carries tag/version/branch/prev_tag/first plus release metadata.
plan_releases() {
  local normalized="$1" state="$2" seed="$3"
  jq -c --argjson state "$state" --arg seed "$seed" '
    def seed_flag: ($seed == "1");
    def chain($rec; $rs):
      ($rec | capture("^php-[0-9]+\\.[0-9]+\\.(?<pat>[0-9]+)$")? // null) as $rp
      | if $rp == null then []
        else
          reduce ($rs[] | select(.pat > ($rp.pat | tonumber))) as $r
            ({acc: [], base: $rec};
             .acc += [ ($r + {prev_tag: .base, first: false}) ]
             | .base = $r.tag)
          | .acc
        end;
    [ (group_by(.branch) | .[])
      | . as $rs
      | ($rs | last) as $latest
      | ($state[$latest.branch] // null) as $rec
      | if $rec != null then chain($rec; $rs)
        elif seed_flag then []
        else [ $latest + {
                 prev_tag: (if ($rs | length) > 1 then $rs[-2].tag else null end),
                 first: (($rs | length) == 1)
               } ]
        end
    ] | add // []
  ' <<<"$normalized"
}

# extract_news_section FILE VERSION -> the NEWS block for VERSION on stdout,
# with leading/trailing blank lines trimmed.
extract_news_section() {
  local file="$1" version="$2"
  awk -v ver="$version" '
    /^[0-9][0-9]? [A-Za-z][A-Za-z][A-Za-z] [0-9][0-9][0-9][0-9], PHP / {
      if (grab) { done = 1; exit }
      if ($0 ~ ("PHP " ver "$")) { grab = 1; next }
    }
    grab && /^\|+[ \t]*$/ { done = 1; exit }
    grab { L[++n] = $0 }
    END {
      s = 1;
      while (s <= n && L[s] == "") s++;
      e = n;
      while (e >= s && L[e] == "") e--;
      for (i = s; i <= e; i++) print L[i];
    }
  ' "$file"
}

# emit_upgrading_sections DIR FILE -> writes DIR/upg_NN.txt per kept section and
# prints "index<TAB>title" lines for the sections worth embedding.
emit_upgrading_sections() {
  local dir="$1" file="$2"
  awk -v dir="$dir" '
    { L[NR] = $0 }
    END {
      n = NR; s = 0;
      for (i = 1; i <= n - 2; i++) {
        if (L[i] ~ /^=+[ \t]*$/ && L[i+1] ~ /^[0-9]+\. / && L[i+2] ~ /^=+[ \t]*$/) {
          starts[++s] = i;
        }
      }
      starts[++s] = n + 1;
      c = 0;
      for (k = 1; k < s; k++) {
        st = starts[k]; en = starts[k+1] - 1;
        title = L[st+1];
        sub(/^[0-9]+\.[ \t]*/, "", title);
        t = tolower(title);
        if (t == "backward incompatible changes" || t == "new features" ||
            t == "new functions" || t == "new classes and interfaces" ||
            t == "new global constants") {
          c++;
          fn = sprintf("%s/upg_%02d.txt", dir, c);
          printf "" > fn;
          for (j = st + 3; j <= en; j++) print L[j] >> fn;
          close(fn);
          printf "%02d\t%s\n", c, title;
        }
      }
    }
  ' "$file"
}

# modules_summary COMPARE_JSON_FILE -> "ext/dom x8, core (Zend) x12, ..." on stdout.
# Only code-bearing trees are counted (ext/, Zend/, sapi/, main/); docs, tests and
# CI churn are ignored so the summary reflects PHP's implementation surface.
modules_summary() {
  jq -r '
    [ .files[]?.filename | select(test("^(ext|Zend|sapi|main)/")) ]
    | map(
        if startswith("ext/") then ("ext/" + (split("/")[1]))
        elif startswith("Zend/") then "core (Zend)"
        elif startswith("sapi/") then ("sapi/" + (split("/")[1]))
        else (split("/")[0])
        end)
    | group_by(.) | map({ name: .[0], count: length })
    | sort_by(-.count, .name)
    | .[0:18]
    | map("\(.name) x\(.count)")
    | join(", ")
  ' "$1"
}

# extract_function_mentions FILE -> lowercased unique bare name() tokens.
extract_function_mentions() {
  grep -oE '(^|[^:>[:alnum:]_])[a-zA-Z_][a-zA-Z0-9_]*\(\)' "$1" \
    | sed -E 's/^[^a-zA-Z_]*//' \
    | sed -E 's/\(\)$//' \
    | tr 'A-Z' 'a-z' \
    | sort -u
}

# fence_file FILE -> a ```text fenced block, capped at MAX_EMBED_BYTES.
fence_file() {
  local file="$1"
  printf '```text\n'
  if [ "$(wc -c <"$file" | tr -d ' ')" -gt "$MAX_EMBED_BYTES" ]; then
    head -c "$MAX_EMBED_BYTES" "$file"
    printf '\n… (truncated)\n'
  else
    cat "$file"
  fi
  printf '```\n'
}

usage() {
  cat <<'EOF'
Usage: php-upstream-watch.sh [options]

  --repo OWNER/NAME     target repository (default: illegalstudio/elephc)
  --state PATH          watch state file (default: .github/upstream/php-src-watch.json)
  --baseline PATH       vendored PHP baseline (default: scripts/docs/php_baseline.json)
  --builtin-registry PATH  elephc builtin registry (coverage cross-reference)
  --symbol-registry PATH   elephc symbol registry (coverage cross-reference)
  --min-minor X.Y       oldest maintained branch to watch (default: 8.2)
  --max-issues N        flood guard, at most N issues per run (default: 20)
  --dry-run             render but open no issues and write no state
  --seed-only           record branch pointers without opening issues
  --kickoff             on a first run, open one issue per watched branch's latest
  --no-milestone        do not group issues under a "PHP X.Y" milestone
  -h, --help            show this help
EOF
}

parse_args() {
  while [ $# -gt 0 ]; do
    case "$1" in
      --repo) TARGET_REPO="$2"; shift 2 ;;
      --state) STATE_FILE="$2"; shift 2 ;;
      --baseline) BASELINE_FILE="$2"; shift 2 ;;
      --builtin-registry) BUILTIN_REGISTRY="$2"; shift 2 ;;
      --symbol-registry) SYMBOL_REGISTRY="$2"; shift 2 ;;
      --min-minor) MIN_MINOR="$2"; shift 2 ;;
      --max-issues) MAX_ISSUES="$2"; shift 2 ;;
      --dry-run) DRY_RUN=1; shift ;;
      --seed-only) SEED_ONLY=1; shift ;;
      --kickoff) KICKOFF=1; shift ;;
      --no-milestone) MILESTONE=0; shift ;;
      -h|--help) usage; exit 0 ;;
      *) die "unknown argument: $1" ;;
    esac
  done
}

# ensure_label NAME COLOR DESCRIPTION -> create the label if it is missing, so
# issues are never silently created unlabeled. Never overwrites an existing
# label (the pr-labels catalog remains authoritative for its definition).
ensure_label() {
  local name="$1" color="$2" description="$3"
  if gh label list --repo "$TARGET_REPO" --limit 200 --json name --jq '.[].name' 2>/dev/null | grep -qxF "$name"; then
    return 0
  fi
  gh label create "$name" --repo "$TARGET_REPO" --color "$color" --description "$description" >/dev/null 2>&1 || true
}

# ensure_milestone TITLE -> create the "PHP X.Y" milestone if it is missing.
# Returns 0 when the milestone exists (or was created), 1 otherwise, so a
# milestone failure never blocks opening the issue itself.
ensure_milestone() {
  local title="$1" existing
  existing="$(gh api -X GET "repos/$TARGET_REPO/milestones" -f state=all -f per_page=100 2>/dev/null \
    | jq -r --arg t "$title" '.[] | select(.title == $t) | .title' 2>/dev/null | head -1 || true)"
  if [ "$existing" = "$title" ]; then
    return 0
  fi
  gh api --method POST "repos/$TARGET_REPO/milestones" -f title="$title" >/dev/null 2>&1
}

# render_provenance BODY VERSION TAG URL BRANCH PREV_TAG FIRST KIND PUBLISHED
render_provenance() {
  local body="$1" version="$2" tag="$3" url="$4" branch="$5" prev_tag="$6" first="$7" kind="$8" published="$9"
  {
    printf '<!-- php-src-watch:%s -->\n' "$tag"
    printf '## PHP %s\n\n' "$version"
    printf 'Upstream release: %s\n\n' "$url"
    printf -- '- Published: %s\n' "${published:-unknown}"
    if [ "$first" = "true" ]; then
      printf -- '- Branch: `%s` · first stable release seen on this branch\n' "$branch"
    else
      printf -- '- Branch: `%s` · previous on branch: `%s`\n' "$branch" "$prev_tag"
    fi
    printf -- '- Kind: %s\n\n' "$kind"
    printf '_Opened automatically by `.github/workflows/php-upstream-watch.yml`, one issue per stable php-src release tag._\n'
  } >"$body"
}

# render_baseline BODY KIND RELEASE BASELINE BRANCH
render_baseline() {
  local body="$1" kind="$2" release="$3" baseline="$4" branch="$5"
  {
    printf '\n### elephc baseline\n\n'
    case "$kind" in
      current)
        printf 'Pinned baseline is **PHP %s**; this release does not move it.\n' "$baseline"
        return 0
        ;;
      newer-patch)
        printf 'Pinned baseline is **PHP %s**; `%s` is a newer patch on the **same minor**.\n\n' "$baseline" "$release"
        printf 'The baseline Docker image tracks the branch (`FROM php:%s-cli`), so this is a snapshot refresh with no Dockerfile change:\n\n' "$branch"
        ;;
      newer-minor)
        printf 'Pinned baseline is **PHP %s**; `%s` opens the **%s** branch. This needs a wider review than a patch bump:\n\n' "$baseline" "$release" "$branch"
        printf -- '- [ ] Add the `%s` profile to `--php-version` in `src/cli.rs` if not present (currently 8.0 through 8.6).\n' "$branch"
        printf -- '- [ ] Point `scripts/docs/php_baseline/Dockerfile` at `php:%s-cli` and review the installed extension list, `BUNDLED_EXTENSIONS` in `scripts/docs/extract_php_baseline.py`, and any new bundled extension.\n' "$branch"
        printf -- '- [ ] Refresh the snapshot and regenerate the compatibility page:\n\n'
        ;;
    esac
    printf '    1. `docker build -t elephc-php-baseline scripts/docs/php_baseline`\n'
    printf '    2. `printf '"'"'#!/bin/sh\\nexec docker run --rm -i elephc-php-baseline php "$@"\\n'"'"' > /tmp/php && chmod +x /tmp/php`\n'
    printf '    3. `python3 scripts/docs/extract_php_baseline.py --php /tmp/php`\n'
    printf '    4. `python3 scripts/docs/gen_module_sections.py`\n'
    printf '    5. `python3 scripts/docs/gen_php_comparison.py`\n'
    printf '    6. Commit `scripts/docs/php_baseline.json` and the regenerated pages.\n'
  } >>"$body"
}

# render_surface BODY COMPARE_FILE VERSION
render_surface() {
  local body="$1" compare_file="$2" version="$3"
  local summary=""
  if [ -s "$compare_file" ]; then
    summary="$(modules_summary "$compare_file")"
  fi
  {
    printf '\n### PHP surface\n\n'
    if [ -n "$summary" ]; then
      printf 'Modules touched upstream (changed files, by module): %s.\n\n' "$summary"
    elif [ -s "$compare_file" ]; then
      printf 'No extension/Zend/SAPI/core files changed in this release.\n\n'
    else
      printf 'No comparable upstream file activity for this release.\n\n'
    fi
    printf 'Full upstream change list: https://www.php.net/ChangeLog-8.php#%s\n' "$version"
  } >>"$body"
}

# render_news BODY NEWS_BLOCK_FILE VERSION
render_news() {
  local body="$1" block="$2" version="$3"
  if [ ! -s "$block" ]; then
    printf '\n> Upstream NEWS block was not found at this tag; use the ChangeLog link above.\n' >>"$body"
    return 0
  fi
  {
    printf '\n### Upstream changelog (NEWS)\n\n'
    printf '<details>\n<summary>NEWS block for PHP %s</summary>\n\n' "$version"
    fence_file "$block"
    printf '\n</details>\n'
  } >>"$body"
}

# render_upgrading BODY TITLES_TSV TMP_DIR COVERED_FUNCTIONS_FILE
render_upgrading() {
  local body="$1" titles="$2" dir="$3" covered="$4"
  [ -s "$titles" ] || return 0
  printf '\n### UPGRADING highlights\n' >>"$body"
  while IFS=$'\t' read -r idx title; do
    [ -n "${idx:-}" ] || continue
    local section_file="$dir/upg_$idx.txt"
    printf '\n**%s**' "$title" >>"$body"
    if [ "$title" = "New Functions" ]; then
      local mentions covered_count missing
      mentions="$(extract_function_mentions "$section_file" | grep -c . || true)"
      if [ "$mentions" -gt 0 ]; then
        if [ -s "$covered" ]; then
          covered_count="$(extract_function_mentions "$section_file" | grep -Fxf "$covered" | grep -c . || true)"
        else
          covered_count=0
        fi
        printf ' — %s function mention(s); elephc covers %s.' "$mentions" "$covered_count" >>"$body"
        missing="$(extract_function_mentions "$section_file" | grep -Fvxf "$covered" 2>/dev/null | head -40 | sed 's/^/`/; s/$/()`/' | paste -sd ', ' - || true)"
        if [ -n "$missing" ]; then
          printf '\nNot yet covered: %s.' "$missing" >>"$body"
        fi
      fi
    fi
    {
      printf '\n\n<details>\n<summary>%s</summary>\n\n' "$title"
      fence_file "$section_file"
      printf '\n</details>\n'
    } >>"$body"
  done <"$titles"
}

render_checklist() {
  local body="$1"
  {
    printf '\n### Triage checklist\n\n'
    printf -- '- [ ] Read the upstream changelog and note any behavior elephc must match.\n'
    printf -- '- [ ] If a fix or change touches a covered elephc module, add or adjust a regression test.\n'
    printf -- '- [ ] Update `php_baseline.json` if the baseline checklist above applies.\n'
    printf -- '- [ ] Close once triaged, linking any resulting issue or pull request.\n'
  } >>"$body"
}

# load_covered_functions REGISTRY_FILE -> lowercased function names, one per line.
load_covered_functions() {
  local registry="$1"
  [ -f "$registry" ] || return 0
  jq -r '.[] | .name // empty' "$registry" 2>/dev/null | tr 'A-Z' 'a-z' | sort -u || true
}

main() {
  parse_args "$@"
  require_cmd gh
  require_cmd jq
  require_cmd awk

  if [ "$DRY_RUN" -eq 0 ] && [ -z "${GH_TOKEN:-${GITHUB_TOKEN:-}}" ]; then
    die "GH_TOKEN or GITHUB_TOKEN must be set (or pass --dry-run)"
  fi

  case "$MIN_MINOR" in
    [0-9]*.[0-9]*) ;;
    *) die "--min-minor must be MAJOR.MINOR (got: $MIN_MINOR)" ;;
  esac

  local tmp
  WATCH_TMP="$(mktemp -d "${TMPDIR:-/tmp}/php-watch.XXXXXX")"
  trap 'rm -rf "$WATCH_TMP"' EXIT
  tmp="$WATCH_TMP"

  log "fetching recent releases from $UPSTREAM_REPO"
  local page count
  page=1
  while [ "$page" -le "$MAX_RELEASE_PAGES" ]; do
    gh api "repos/$UPSTREAM_REPO/releases?per_page=$RELEASES_PER_PAGE&page=$page" >"$tmp/releases_page_$page.json"
    count="$(jq 'length' "$tmp/releases_page_$page.json")"
    if [ "$count" -lt "$RELEASES_PER_PAGE" ]; then
      break
    fi
    page=$(( page + 1 ))
  done
  jq -s 'add // []' "$tmp"/releases_page_*.json >"$tmp/releases.json"

  local min_key normalized
  min_key="$(min_minor_num "$MIN_MINOR")"
  normalize_releases "$min_key" <"$tmp/releases.json" >"$tmp/normalized.json"

  local state_branches seed state_existed
  state_existed=0
  if [ -f "$STATE_FILE" ]; then
    state_existed=1
    state_branches="$(jq -c '.branches // {}' "$STATE_FILE" 2>/dev/null || printf '{}')"
  else
    state_branches="{}"
  fi
  seed=0
  if [ "$SEED_ONLY" -eq 1 ]; then
    seed=1
  fi
  # A first run seeds silently by default so it does not open one issue per
  # watched branch at once; --kickoff opts into opening them.
  if [ "$state_existed" -eq 0 ] && [ "$KICKOFF" -eq 0 ]; then
    seed=1
  fi

  local baseline_version baseline_kind_val
  baseline_version=""
  if [ -f "$BASELINE_FILE" ]; then
    baseline_version="$(jq -r '.php_version // empty' "$BASELINE_FILE" 2>/dev/null || true)"
  fi
  if [ -z "$baseline_version" ]; then
    log "warning: no usable PHP baseline at $BASELINE_FILE"
    baseline_version="0.0.0"
  fi

  if [ "$state_existed" -eq 0 ]; then
    if [ "$seed" -eq 1 ]; then
      log "first run: recording branch pointers only (pass --kickoff to open the current latest)"
    else
      log "first run: opening one issue per watched branch's latest release"
    fi
  fi

  local planned_total plan_file
  plan_file="$tmp/planned.jsonl"
  plan_releases "$(cat "$tmp/normalized.json")" "$state_branches" "$seed" >"$tmp/planned.json"
  planned_total="$(jq 'length' "$tmp/planned.json")"
  if [ "$planned_total" -gt "$MAX_ISSUES" ]; then
    log "warning: $(( planned_total - MAX_ISSUES )) older release(s) skipped by --max-issues"
    jq -c ".[-$MAX_ISSUES:] | .[]" "$tmp/planned.json" >"$plan_file"
  else
    jq -c '.[]' "$tmp/planned.json" >"$plan_file"
  fi

  local covered_functions
  covered_functions="$tmp/covered_functions.txt"
  load_covered_functions "$BUILTIN_REGISTRY" >"$covered_functions"

  # Ensure the labels we apply exist and resolve the label arguments once.
  local label_args=()
  if [ "$DRY_RUN" -eq 0 ]; then
    ensure_label "type:chore" "6E7781" "Updates maintenance, tooling, dependencies, or housekeeping."
    ensure_label "topic:php-compat" "D73A4A" "Changes PHP compatibility or observable PHP semantics."
  fi
  local labels_present label
  labels_present="$(gh label list --repo "$TARGET_REPO" --limit 200 --json name --jq '.[].name' 2>/dev/null || true)"
  for label in "type:chore" "topic:php-compat"; do
    if printf '%s\n' "$labels_present" | grep -qxF "$label"; then
      label_args+=("--label" "$label")
    fi
  done

  local created=0 already=0 failures=0
  # Tags whose issues were opened (or already existed) this run, one JSON object
  # per line; the state pointers below advance only through these.
  local opened_log
  opened_log="$tmp/opened.jsonl"
  : >"$opened_log"
  local line tag version branch prev_tag first url published pat kind
  while IFS= read -r line; do
    [ -n "$line" ] || continue
    tag="$(jq -r '.tag' <<<"$line")"
    version="$(jq -r '.version' <<<"$line")"
    branch="$(jq -r '.branch' <<<"$line")"
    prev_tag="$(jq -r '.prev_tag // ""' <<<"$line")"
    first="$(jq -r '.first | tostring' <<<"$line")"
    url="$(jq -r '.url' <<<"$line")"
    published="$(jq -r '.published' <<<"$line")"
    pat="$(jq -r '.pat' <<<"$line")"
    kind="$(jq -r 'if (.body | test("security release"; "i")) then "security release" elif (.body | test("bug fix release"; "i")) then "bug fix release" else "release" end' <<<"$line")"

    local marker body title
    marker="php-src-watch:$tag"
    body="$tmp/body_$tag.md"
    title="[php-src] PHP $version released"

    # NEWS section (always).
    local news_block="$tmp/news_$tag.txt"
    : >"$news_block"
    if gh api -X GET "repos/$UPSTREAM_REPO/contents/NEWS" -f ref="$tag" -H "Accept: application/vnd.github.raw" >"$tmp/news_raw_$tag.txt" 2>/dev/null; then
      extract_news_section "$tmp/news_raw_$tag.txt" "$version" >"$news_block" || true
    fi

    # UPGRADING highlights (only for a minor's .0 release).
    local titles="$tmp/titles_$tag.tsv"
    : >"$titles"
    if [ "$pat" -eq 0 ]; then
      if gh api -X GET "repos/$UPSTREAM_REPO/contents/UPGRADING" -f ref="$tag" -H "Accept: application/vnd.github.raw" >"$tmp/upg_raw_$tag.txt" 2>/dev/null; then
        emit_upgrading_sections "$tmp" "$tmp/upg_raw_$tag.txt" >"$titles"
      fi
    fi

    # Compare-based module summary.
    local compare_file="$tmp/compare_$tag.json"
    : >"$compare_file"
    if [ -n "$prev_tag" ] && [ "$first" != "true" ]; then
      if gh api "repos/$UPSTREAM_REPO/compare/$prev_tag...$tag" >"$compare_file" 2>/dev/null; then
        :
      else
        : >"$compare_file"
      fi
    fi

    baseline_kind_val="$(baseline_kind "$version" "$baseline_version")"

    render_provenance "$body" "$version" "$tag" "$url" "$branch" "$prev_tag" "$first" "$kind" "$published"
    render_baseline "$body" "$baseline_kind_val" "$version" "$baseline_version" "$branch"
    render_surface "$body" "$compare_file" "$version"
    render_news "$body" "$news_block" "$version"
    render_upgrading "$body" "$titles" "$tmp" "$covered_functions"
    render_checklist "$body"

    # Dedup search runs in both modes (read-only), so --dry-run reports the same
    # exists/would-open decision the real run would make.
    local existing
    existing="$(gh api -X GET "search/issues" -f q="repo:$TARGET_REPO in:body \"$marker\"" --jq '.total_count' 2>/dev/null || printf '0')"
    if [ "${existing:-0}" -gt 0 ]; then
      log "skip $tag: issue already exists"
      if [ "$DRY_RUN" -eq 0 ]; then
        already=$(( already + 1 ))
        jq -cn --arg b "$branch" --arg t "$tag" '{branch: $b, tag: $t}' >>"$opened_log"
      fi
      continue
    fi

    if [ "$DRY_RUN" -eq 1 ]; then
      local dry_extra=""
      if [ "$MILESTONE" -eq 1 ]; then dry_extra=" milestone=PHP $branch"; fi
      log "[dry-run] would open: $title ($(wc -c <"$body" | tr -d ' ') chars)$dry_extra"
      continue
    fi

    local milestone_args=()
    if [ "$MILESTONE" -eq 1 ]; then
      if ensure_milestone "PHP $branch"; then
        milestone_args=(--milestone "PHP $branch")
      else
        log "warning: could not ensure milestone 'PHP $branch'; opening without it"
      fi
    fi

    if gh issue create --repo "$TARGET_REPO" --title "$title" --body-file "$body" \
        "${milestone_args[@]+"${milestone_args[@]}"}" "${label_args[@]+"${label_args[@]}"}"; then
      log "opened: $title"
      created=$(( created + 1 ))
      jq -cn --arg b "$branch" --arg t "$tag" '{branch: $b, tag: $t}' >>"$opened_log"
    else
      log "error: could not open issue for $tag"
      failures=$(( failures + 1 ))
      break
    fi
  done <"$plan_file"

  # Advance each branch pointer only through tags whose issues were opened (or
  # already existed) this run. A seed run opens nothing by design, so it records
  # the latest tag per branch instead. This keeps a branch whose releases were
  # skipped — by --max-issues, or dropped entirely by the flood guard — pointing
  # at its last opened tag, so the next run retries them rather than treating
  # them as already seen.
  if [ "$DRY_RUN" -eq 0 ] && [ "$failures" -eq 0 ]; then
    local new_map changed
    if [ "$seed" -eq 1 ]; then
      new_map="$(jq -c 'group_by(.branch) | map({ key: .[0].branch, value: (last.tag) }) | from_entries' "$tmp/normalized.json")"
    elif [ -s "$opened_log" ]; then
      new_map="$(jq -cs 'reduce .[] as $r ({}; .[$r.branch] = $r.tag)' "$opened_log")"
    else
      new_map='{}'
    fi
    changed="$(jq -n --argjson old "$state_branches" --argjson new "$new_map" '$new | to_entries | any(.value != ($old[.key] // null))')"
    if [ ! -f "$STATE_FILE" ] || [ "$changed" = "true" ]; then
      local updated
      updated="$(jq -n --argjson old "$state_branches" --argjson new "$new_map" --arg today "$(date -u +%F)" \
        '{schema: 1, updated_at: $today, branches: ($old + $new)}')"
      mkdir -p "$(dirname "$STATE_FILE")"
      printf '%s\n' "$updated" >"$STATE_FILE"
      log "state updated: $STATE_FILE"
    else
      log "state unchanged"
    fi
  fi

  log "summary: planned=$planned_total created=$created already=$already failures=$failures"
  if [ "$failures" -gt 0 ]; then
    return 1
  fi
  return 0
}

if [ "${BASH_SOURCE[0]:-$0}" = "$0" ]; then
  set -euo pipefail
  main "$@"
fi
