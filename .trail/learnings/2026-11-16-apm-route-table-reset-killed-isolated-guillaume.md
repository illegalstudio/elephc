---
title: "APM route-table reset, killed isolated web workers, collector clock"
author: Guillaume
date: 2026-11-16
branch: feat/monitor-apm
touches:
  - crates/elephc-probe/src/apm_runtime.rs
  - crates/elephc-probe/src/apm_runtime_tests.rs
  - crates/elephc-probe/src/endpoint.rs
  - crates/elephc-probe/src/lib.rs
  - crates/elephc-web/src/isolated_worker.rs
  - crates/elephc-web/src/probe_route.rs
  - docs/beyond-php/apm.md
  - docs/compiling/cli-reference.md
  - docs/images/apm/transactions-full.png
  - docs/images/apm/transactions.png
---

## Decided
- Isolated web workers (`--web-isolation=pool|request`) hold one in-flight row per request, not one per worker: `elephc_probe_request_begin`/`_end` are runtime slots (`elephc_probe_begin_fn`/`_end_fn`, filled in `emit_probe_init`), wrapped by `probe_route::InFlight` (Drop frees the row, `answering()` marks it before the answer is recorded). The reap scans every row of the dead pid and counts only SERVING ones as 5xx.
- The route table is emptied only on demand: `elephc monitor <target> --apm --reset-routes` (key-authenticated `WANT_RESET_ROUTES` 'R') or the page's two-click button (same-origin POST `/api/reset-routes`). The reset converts in-flight route ids to labels, stamps the ring head as the reset ticket (older samples lose their route), zeroes counters and budget, bumps the generation and sets a new APM_EPOCH so the collector reads it as a restart. Workers rebase lazily via `CURRENT_LABEL` / `GENERATION_SEEN`.
- Windows are stamped with the collector clock (`collected_window`), tested with a service clock ±10 days off.

## Tried and ruled out
- Returning from the reap after the first row of a pid: an isolated worker serving several requests at death lost all but one.

## Watch out
- scripts/docs/apm_screenshots.sh must stop demo.sh with TERM: a background job of a non-interactive bash starts with SIGINT ignored and cannot trap it, so INT left the script hanging after the shots.
- In-flight tracking is capped at 1 024 concurrent requests per service.
