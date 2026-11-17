---
title: "monitor --apm single page, script runs, terminal views"
author: Guillaume
date: 2026-11-15
branch: feat/monitor-apm
cost: { minutes: 56, tokens: 359680 }
touches:
  - src/monitor/apm/mod.rs
  - src/monitor/apm/view/script.js
  - src/call_graph.rs
  - src/monitor/local.rs
---

## Decided
- One entry point (user, 2026-11-15): the APM view lives inside the monitor call-graph page (src/call_graph.rs TEMPLATE_HTML) through __APM_STYLE__/__APM_PANEL__/__APM_SCRIPT__ placeholders filled from src/monitor/apm/view/ only when --apm; without it they are removed. The view's URL keys a/ar are preserved by the graph's writeHash.
- Script APM = one run per transaction, only through 'elephc monitor <script> --apm' (argv belongs to PHP; no env var). Name 'CLI <script> [<command>]'; only an argument that reads as a command name is kept. Run windows carry epoch 0; monitor returns the script's exit code under --apm.
- Rates: service windows tile watched time (sum of spans); runs do not, so measured_ms() uses first-start..last-end for runs.
- Profile capture from the page asks twice (first ask only arms the sampler), 3 s apart.
- One collector per (store, service): per-target fs2 lock file in the store.
## Watch out
- 'monitor <service> --apm --store <dir>' IS a collector (runs until Ctrl-C); browsing a store is 'monitor --apm --store <dir>' with no service.
- render_html_exact was removed; use render_html_page / render_html_frames.
- The worktree guard rejects big inline python heredocs; write edit scripts to the scratchpad and run them with the worktree path.
