---
title: "APM review fixes: isolated handler work, overlapping rates, store reads"
author: Guillaume
date: 2026-11-15
branch: feat/monitor-apm
cost: { minutes: 53, tokens: 230977 }
touches:
  - docs/beyond-php/apm.md
  - README.md
  - crates/elephc-instr/src/lib.rs
  - crates/elephc-probe/src/apm.rs
  - crates/elephc-probe/src/apm_runtime_tests.rs
  - src/monitor/apm/alerts.rs
  - src/monitor/apm/collector.rs
  - src/monitor/apm/history.rs
  - src/monitor/apm/tests.rs
  - src/monitor/apm/browse.rs
---

## Decided
- An isolated handler child (`--web-isolation=pool|request`) settles its pending database/network work with `elephc_probe_settle_request(status)` before clearing its route; its web worker then records the request by name. The probe's `pending_route_id` holds the one rule both use: the route's row, or `<not found>` for an unseen route answered 404/405/410.
- Rates over overlapping windows (several services in one store) always divide by `measured_ms` (union of covered time), in the table, the chart points (`dashboard::build_series`) and the terminal trends (`terminal::trend`). The monitor page labels sources (`label_sources`) after loading, like the terminal and the export.
- `RangePlan::load` reads every stored window that STARTED before memory begins, not only those that ended before it; memory joins after the last stored window.
- `Store::newest_ms` reads each day file incrementally (`read_newest`, offset per day, shared by clones) and keeps the max `t`: lines from several collectors are not in time order. A thread-local byte counter (`NEWEST_BYTES_READ`) proves a call reads only what was appended.
- `src/monitor/apm/mod.rs` is a dispatcher: window, summary, history, collector, browse, runs, options modules, re-exported so paths did not change.

## Tried and ruled out
- Reading only the last 64 KiB of the day file for `newest_ms`: a wide window line starting before the chunk is missed when a smaller, older line follows it.
- `perl -i` with `<>` slurped into an array and printed afterwards: in-place output is closed at EOF, so the file is emptied and its content goes to stdout. Write files explicitly.

## Watch out
- AGENTS.md checks reviewers apply: `///` on every fn (tests and helpers too), `//! Purpose:` preamble on every file, slim mod.rs, README flag examples. Check every fn line a branch adds (git diff -U0 against origin/main) for a `///` above it, skipping attribute lines, before opening the PR.
