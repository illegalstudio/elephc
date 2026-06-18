# Benchmark time-series

This folder keeps a **committed historical record** of the benchmark suite, so
performance trends survive even after GitHub expires the underlying CI
artifacts (~90 days).

## What's here

| File | Purpose |
|---|---|
| `series.json` | The time-series data: one sample per day, branch `main`. |
| `fetch_series.py` | Regenerates `series.json` from CI artifacts. |
| `dashboard.py` | Local web dashboard to visualize the series. |
| `dashboard.sh` | One-line launcher for the dashboard. |
| `README.md` | This file. |

## Viewing the dashboard

The simplest way to browse the data is a small local dashboard (Python stdlib
only — no `pip install`). Use the launcher:

```bash
./.benchmarks/dashboard.sh
```

or call the script directly (identical behaviour; extra args are forwarded):

```bash
python3 .benchmarks/dashboard.py
```

It reads `series.json`, embeds it into an HTML page, serves it on
`http://127.0.0.1:8000/`, and opens your browser. Charts use Chart.js from a CDN
(needs internet); the data is inlined so no `fetch`/CORS dance is required.
Stop it with `Ctrl-C`.

```bash
python3 .benchmarks/dashboard.py --port 9000   # different port
python3 .benchmarks/dashboard.py --no-open     # serve without opening a browser
```

The dashboard shows per-case stat cards (latest, delta vs first, min/max), an
absolute median-time line chart, and an "× slower than C" ratio chart.

## Where the data comes from

The numbers are **not** produced locally. They come from the CI benchmark job:

- Workflow: `.github/workflows/ci.yml`, job **"Benchmark Suite"** (runs on the
  `macos-14` Apple Silicon runner).
- Generator: `scripts/benchmark_suite.py`, invoked as
  `--iterations 3 --warmup 1`, which compiles each case in
  `benchmarks/cases/` with the release `elephc` binary, runs it, and records the
  **median wall-clock time in milliseconds**.
- Each CI run uploads a `benchmark-results` artifact containing
  `benchmark-results.json`. `fetch_series.py` collects those artifacts and folds
  them into the single `series.json` stored here.

### Cases measured

The cases live in `benchmarks/cases/<name>/` (`main.php`, `main.c`,
`expected.txt`):

- `sum_loop`
- `array_sum`
- `string_concat`

Each record carries `elephc_ms`, `c_ms`, and `php_ms`:

- `elephc_ms` — the compiled elephc binary (the metric we track).
- `c_ms` — an `-O2` C baseline compiled on the same runner, for reference.
- `php_ms` — the PHP interpreter. **Usually `null`**: PHP is not installed on
  the GitHub runners, so the suite skips it. It is kept in the schema so local
  runs (where `php` exists) populate it.

### Sampling policy

`series.json` contains **one sample per calendar day** (the last CI run of that
day) and **only from `main`**. This keeps a clean product-evolution line without
per-PR noise. Every record records the source `commit` and CI `run_id` so any
point can be traced back to the exact run.

> Note: each point is a single CI median on a shared cloud runner, so absolute
> values carry runner noise. Read **trends and step-changes**, not sub-millisecond
> differences between adjacent days.

## How to update

Re-run the fetch script from anywhere inside the repo (requires an authenticated
[`gh`](https://cli.github.com/) and `python3`):

```bash
python3 .benchmarks/fetch_series.py
```

**The update is incremental and non-destructive.** The script reads the existing
`series.json`, keeps every point already recorded, and only downloads dates it
doesn't have yet. Points whose CI artifacts have since expired stay in the file —
**history is never lost by re-running.** (As a side benefit, refreshes are fast:
already-recorded days are not re-downloaded.)

Commit the refreshed file:

```bash
git add .benchmarks/series.json
git commit -m "chore: refresh benchmark series"
```

Useful flags:

```bash
python3 .benchmarks/fetch_series.py --branch some-branch   # sample another branch
python3 .benchmarks/fetch_series.py --all-runs             # keep every run, not 1/day
python3 .benchmarks/fetch_series.py --out /tmp/series.json # write elsewhere
python3 .benchmarks/fetch_series.py --rebuild              # DESTRUCTIVE: rebuild from
                                                           # scratch, drops expired history
```

Because artifacts expire after ~90 days, refresh periodically so recent points
are captured here before they vanish upstream. Avoid `--rebuild` unless you
specifically want to discard the committed history and start over.

## Reading the data

`series.json` is a flat list — trivial to load in Python, `jq`, or a notebook.
Quick `jq` example, the `sum_loop` elephc time per day:

```bash
jq -r '.series[] | "\(.date)\t\(.cases.sum_loop.elephc_ms)"' .benchmarks/series.json
```
