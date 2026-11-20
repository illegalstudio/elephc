---
title: APM collector e2e tests and the not-found row
author: Guillaume
date: 2026-11-15
branch: feat/monitor-apm
cost: { minutes: 1, tokens: 1909 }
touches:
  - crates/elephc-probe/src/lib.rs
  - src/monitor/apm/mod.rs
  - tests/web/apm.rs
---

## Decided
- `<not found>` (404/405/410) applies only to a route the probe's table has not seen yet (`elephc_probe_note_transaction` in crates/elephc-probe/src/lib.rs). A known route keeps its own 404s, so a product page answering 404 for a missing id keeps its 4xx rate, and its queries (counted live on its row) stay with its requests. The scanner protection (R1) only needs unseen paths kept out of the table.
- The collector keeps a window row that has DB/network work but no completed request (`has_activity` in src/monitor/apm/mod.rs): a slow request's queries are counted before it ends, and dropping that row loses them for good.

## Tried and ruled out
- A collector e2e test that sends a fast request then immediately the slow one does not catch `has_activity` regressions: the fast request's completion and the slow one's first statements land in the same 1 s poll window, so the row survives on the fast request's count. A 1.5 s pause between them (a poll in between) makes the test fail under mutation (tests/web/apm.rs `web_apm_collector_stores_a_slow_routes_work_across_polls`).

## Watch out
- A route's first request under `--apm` alone holds its I/O as pending until its answer, so a test about live I/O across polls needs the route interned first (one earlier request).
- Prove poll-timing tests by mutation; their timing can make them pass vacuously.
- The worktree's 1.98.1 toolchain has no rustfmt/clippy components.
