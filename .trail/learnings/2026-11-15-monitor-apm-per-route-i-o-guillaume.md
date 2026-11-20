---
title: "monitor --apm: per-route I/O, captures, alerts"
author: Guillaume
date: 2026-11-15
branch: feat/monitor-apm
cost: { minutes: 58, tokens: 253940 }
touches:
  - crates/elephc-instr/src/lib.rs
  - crates/elephc-probe/src/lib.rs
  - src/monitor/apm/traces.rs
  - src/monitor/apm/alerts.rs
  - crates/elephc-web/src/worker.rs
---

## Decided
- Per-route DB/network under --apm: the combined build's exact runtime forwards every event (before its ENABLED gate) to the probe through elephc_probe_apm_event_fn; probe-only note functions feed the same counters. elephc_monitor_event_active_fn is now installed in every probe build and answers yes under --apm, which is what makes PDO/curl time their calls. Txn row header grew to 12 words (4 I/O counters), snapshot route line carries them.
- A slice ends with the HTTP status in the upper bits of elephc_instr_request (status << 8); the trace line gets a trailing status= field older readers ignore.
- --capture-every N: collector thread asks WANT_EXACT every N s and keeps per route the 5 slowest and 5 latest failures (TraceBook). Page: /api/traces, /?trace=<id> renders that request's exact graph.
- --alert reuses parse_assert syntax; checked over --alert-window; no traffic keeps state; --alert-exec via sh -c with ELEPHC_ALERT_* env, not waited for. One-shot (no collector) exits 2 when a budget is broken.
## Watch out
- examples/web-apm avoids 'static $x = null' lazy init: elephc rejects $x = null; if ($x === null) { $x = new C(); } $x->p (bd elephc-vp4a).
- A bash trap that cleans up on INT must also exit, or the loop keeps running (examples/web-apm/demo.sh).
