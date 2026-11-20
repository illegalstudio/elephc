---
title: monitor --apm transaction monitoring
author: Guillaume
date: 2026-11-15
branch: feat/monitor-apm
cost: { minutes: 1, tokens: 1765 }
touches:
  - crates/elephc-probe/src/apm.rs
  - crates/elephc-probe/src/lib.rs
  - crates/elephc-web/src/probe_route.rs
  - src/monitor/apm/mod.rs
  - src/codegen/frame.rs
---

## Decided
- APM counters live in the probe's shared region (appended after the replay table, TXN_TABLE_OFFSET) so every --web worker and isolated handler child writes one table the master's endpoint reads. Enabled by the produced binary's runtime arg --apm (master, before fork, through the new elephc_probe_apm_fn slot); never by an env var. A binary without the probe refuses --apm with exit 2.
- Counters are cumulative since start; the client differences snapshots. Snapshot epoch = start time; a changed epoch means restart and the new snapshot is the window since restart.
- Histogram: 256 log-linear buckets from 1 us, exact below 16 us, 8 per octave (<=12.5% wide). Percentiles report the bucket's UPPER edge, never interpolate. Prometheus le buckets count only fine buckets whose upper edge <= le (conservative).
- The COUNT word is incremented last with Release and read with Acquire, so a reader seeing a count also sees the route name and other counters.
- Under --apm, set_route interns from boot (the 'not interned until asked' rule is lifted by the operator's choice).
- Store: daily apm-YYYY-MM-DD.jsonl, whole-line appends, torn line skipped, only that exact name pattern is ever deleted.
## Watch out
- Mixing rustc 1.95 (default) and 1.98.1 builds of the bridge staticlibs links two std copies: duplicate _rust_eh_personality. The main crate needs >=1.97 (mago deps); pin with rustup override in the worktree and rebuild the staticlibs.
- Unit tests in elephc-web cannot call functions that read the runtime .comm slots (link fails: the slots exist only in compiled programs). Cover those through tests/web/apm.rs instead.
- Headless screenshots of the dashboard hang with --virtual-time-budget because the SSE stream never finishes loading; chrome-headless-shell with --timeout works.
- intern_route used to allocate a String per scanned slot (read_route_slot); now route_slot_holds compares in place. apm_runtime_tests::recording_a_known_route_allocates_nothing (counting global allocator) fails if an allocation returns to the request path.
- Combined --with-monitoring gives the exact runtime the PDO/curl event slots, so per-route DB/network counts are not in the APM table (bd elephc-ag8z.8).
