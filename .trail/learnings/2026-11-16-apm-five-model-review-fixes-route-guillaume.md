---
title: "APM five-model review fixes: route budget, macOS proportional memory, incremental store"
author: Guillaume
date: 2026-11-16
branch: feat/monitor-apm
touches:
  - crates/elephc-probe/src/apm.rs
  - crates/elephc-probe/src/apm_runtime_tests.rs
  - crates/elephc-probe/src/lib.rs
  - crates/elephc-probe/src/resources.rs
  - crates/elephc-web/src/isolated_worker.rs
  - crates/elephc-web/src/probe_route.rs
  - crates/elephc-web/src/worker.rs
  - docs/beyond-php/apm.md
  - docs/compiling/cli-reference.md
  - src/call_graph.rs
---

## Decided
- A five-model review (Fable 5.2 plus four Ollama models via dedicated Claude Code instances) found 26 issues; all fixed. The probe interns a route only at its answer under --apm even after a sampled ask (set_route uses the pending path whenever apm is on), never for 408/413 answered before PHP (status flag ANSWERED_BEFORE_PHP, row <rejected>), and no faster than a shared token bucket (64 burst, 1/min, word at ROUTE_BUDGET_OFFSET). Unknown HTTP methods label as OTHER.
- In-flight rows have a third state (answering): note_transaction marks it before counting, and the reap counts only rows still serving, so a death between counting and freeing is not a second 500.
- macOS memory is proportional: per region private_pages + shared_pages / ref_count via proc_pidinfo(PROC_PIDREGIONINFO) on each child (struct declared locally, 96 bytes). phys_footprint charged fork-shared pages to every worker (measured 4 x 201 MB for 200 MB). Memory is read once per process after the CPU walk settled.
- Collected windows are stamped with the collector clock (window.t = now) so day files and pruning agree. Alerts are re-checked on failed polls with a synthetic empty gap window, so rps floors fire on a dead service; alert_windows drops windows that started before the alert span (post-outage giants).
- Store reads are incremental for load/runs_in/newest_ms through one DayRead cache keyed by day, reset on (dev, ino) change; runs fold per minute and transaction, service windows fold per minute once the day is over. Today stays raw so short ranges keep the store resolution.
- dashboard.rs is the server (slots via a Drop guard, head deadline, write timeout, SSE retry + 5 min lifetime); api.rs builds page/series/alerts/metrics and renders outside the history lock. crates/elephc-probe/src/apm_runtime.rs holds the --apm runtime moved out of lib.rs.

## Tried and ruled out
- Folding stored service windows by the minute for every day: short ranges in store-browse mode lose the store resolution, and three store tests asserting per-window reads failed. Only past days fold.
- A new runtime slot for pre-PHP answers: a high bit on the existing note_transaction status avoids touching codegen on both arches.

## Watch out
- available_parallelism already reads cgroup quotas (v1 and v2), rounded down to whole cores; a fractional quota can read over 100%, so the CPU chart caps at 100 only when no point exceeds it.
- --duration is meaningful under --apm with --live and in store browsing (no target); warn only for a one-shot service read or a script.
- The worktree guard refuses heredocs and multi-command lines; append test code with Edit, mutate with single-file sed and restore from a scratchpad copy.
