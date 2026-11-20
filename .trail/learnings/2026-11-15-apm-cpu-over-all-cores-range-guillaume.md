---
title: "APM CPU over all cores, range in the link, doc screenshots"
author: Guillaume
date: 2026-11-15
branch: feat/monitor-apm
cost: { minutes: 63, tokens: 279085 }
touches:
  - docs/beyond-php/apm.md
  - docs/beyond-php/profiling.md
  - docs/images/apm/transactions-full.png
  - docs/images/apm/transactions.png
  - docs/images/profiling/call-graph-dark.png
  - docs/images/profiling/call-graph.png
  - docs/images/profiling/checks.png
  - docs/images/profiling/diff.png
  - docs/images/profiling/flame.png
  - docs/images/profiling/function.png
---

## Decided
- Service CPU under `--apm` is a share of every core the service may use: the probe sends `available_parallelism()` (affinity, cgroup quota) as the 4th figure of the `resources` line; a 3-figure line from an older service reads as 0 cores ("unknown"), and the page then falls back to "% of one core".
- The Transactions view's range rides in the hash as `at` (seconds), beside `a` and `ar`; `src/call_graph.rs`'s writeHash keeps all three.
- `scripts/docs/apm_screenshots.sh` photographs the live APM page from `examples/web-apm` on ports 18080/19411/19000; `scripts/docs/profiling_screenshots.sh` regenerates the profiling images (a launched `monitor` is exact by default; `--instrument` is gone).

## Tried and ruled out
- `--virtual-time-budget` for a live monitor page: the SSE stream keeps the network busy, the budget never ends and headless Chromium hangs; use `--timeout=6000`.
- Regenerating `source-lines.png` locally: it is a sampled per-line view, and a launched run is always exact now; the image is kept, the script skips it.

## Watch out
- A test that spawns a process (the alert hook's `sh`) shares every open file with the child until it execs; an `flock` held through such a copy outlives `drop`. Tests that check a file lock is released take `PROCESS_AND_LOCKS` with the spawning tests (src/monitor/apm/tests.rs).
- Navigating to the same URL with only a different hash does not reload the page: add a query parameter when checking a rebuilt page.
