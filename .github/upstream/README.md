# php-src upstream watch

Tracks stable releases of [`php/php-src`](https://github.com/php/php-src) and
opens one GitHub issue per release tag, so elephc can follow PHP's evolution
between minor releases.

- Workflow: `.github/workflows/php-upstream-watch.yml` (weekly cron + manual
  dispatch).
- Script: `.github/scripts/php-upstream-watch.sh` (bash, `gh` + `jq` + `awk`).
- Tests: `.github/scripts/test_php-upstream-watch.sh` (run in CI by the
  `upstream-watch-tests` job).

## State

`php-src-watch.json` in this directory records the last seen stable tag per
maintenance branch:

```json
{
  "schema": 1,
  "updated_at": "2026-09-28",
  "branches": { "8.4": "php-8.4.26", "8.5": "php-8.5.11" }
}
```

The watcher only opens an issue for a release newer than its branch's recorded
tag, so re-runs never duplicate. The file is committed by the workflow and only
rewritten when a branch pointer advances — a run that finds nothing new leaves
it untouched. The first run seeds the pointers **silently** (no issue burst);
pass `--kickoff` (or the workflow's `kickoff` input) to also open one issue for
each branch's current latest release.

## What an issue contains

- Release metadata (branch, publish date, security/bug-fix kind) and the
  upstream release link.
- An **elephc baseline** section: whether the vendored
  `scripts/docs/php_baseline.json` snapshot is behind, plus a bump checklist
  (patch vs. new-minor differ).
- A **surface** section: the php-src modules touched, derived from the GitHub
  compare API, with a link to the upstream ChangeLog.
- The `NEWS` block for the release, and — for a minor's `.0` release — the
  `UPGRADING` highlights (with a quick "how much does elephc already cover"
  count for the New Functions section).

## Running it by hand

Requires an authenticated `gh` and `jq`:

```bash
# See what would be opened; writes nothing.
GH_TOKEN=... bash .github/scripts/php-upstream-watch.sh --dry-run

# Record the current branch pointers without opening any issue.
GH_TOKEN=... bash .github/scripts/php-upstream-watch.sh --seed-only
```

Useful flags: `--kickoff` (open the current latest per branch on a first run),
`--seed-only` (never open issues this run), `--no-milestone`, `--min-minor 8.2`
(oldest branch to watch), `--max-issues 20` (flood guard), `--state PATH`,
`--repo OWNER/NAME`.

Issues are grouped under a `PHP X.Y` milestone (created on demand; disable with
`--no-milestone`) and labelled `type:chore` and `topic:php-compat`; the body
carries a `<!-- php-src-watch:php-X.Y.Z -->` marker that the dedup search
matches on.
