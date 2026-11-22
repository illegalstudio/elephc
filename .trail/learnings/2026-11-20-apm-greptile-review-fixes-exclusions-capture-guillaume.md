---
title: "APM Greptile review fixes: exclusions, capture ids and source, shared store locks, combined-build traces"
author: Guillaume
date: 2026-11-20
branch: feat/apm-v2
touches:
  - crates/elephc-instr/src/lib.rs
  - crates/elephc-probe/src/apm_runtime.rs
  - crates/elephc-probe/src/apm_runtime_tests.rs
  - docs/README.md
  - docs/beyond-php/apm.md
  - docs/internals/apm.md
  - src/call_graph.rs
  - src/codegen/frame.rs
  - src/codegen_support/runtime/data/fixed.rs
  - src/monitor/apm/api.rs
---

## Decided
- Excluded `--apm-transaction` boundaries nest with their own depth (`EXCLUDED_DEPTH`) and the state before the outermost one (`EXCLUDED_BEFORE`); `NESTED_DEPTH` stays for counted transactions nested in a counted one.
- Combined `--with-monitoring` builds: elephc-instr owns the bridge's trace slot, so it forwards each request's traceparent to the probe through the `elephc_probe_trace_fn` `.comm` slot before its dormancy check (same pattern as `elephc_probe_apm_event_fn`).
- Capture ids stay `t_ms * 1000 + seq`: `next_capture_id` uses a per-process offset plus a counter, and `append_capture` hard-links the graph to its name (fails when taken) and moves to the next free id.
- Shared stores: claims state `<retention> <full days> <minute days>`; roll-ups take the finest any live claim asks for; prune/roll-up runs under `.maintenance.lock` (try-lock, skip when busy); deploys under `.deploys.lock`.
- Captures carry `source`; a live page filters them with `History::source_filter()`, the same filter as its windows; `kind == "run"` belongs on every page.
- Scripts row totals and p95 are computed server-side over every run before the 240-point cut.

## Tried and ruled out
- Widening capture ids to `t_ms * 1_000_000 + random`: ids go to the page as JSON numbers and pass 2^53, so JS would round them.

## Watch out
- perl -0pi substitutions on JS: a replacement holding `$(` (jQuery-style `$("id")`) is interpolated as perl's `$(` (group ids) and writes numbers into the file. Escape it as `\$(` or use the Edit tool; check with `node -e "new Function(require('fs').readFileSync(f,'utf8'))"`.
- Web APM integration tests fail under full parallelism on this machine (timing); run them with `--test-threads=4`.
- Two rustc toolchains mixed in one build (default 1.95 linker path + 1.98 RUSTC) gave `duplicate symbol _rust_eh_personality`; put the 1.98 toolchain first in PATH too.
- Adding a field to a struct and filling literals with a mass substitution: `src/monitor/local.rs` has its own `Window` (display, source) beside `apm::Window`, and one of its literals sits in a `#[cfg(target_os = "linux")]` block macOS never compiles. The macOS build passed, Linux CI broke (E0560). No Linux std is installed locally, so grep the literals in cfg-gated blocks by hand.
- Store lock tests (claims, `.maintenance.lock`): a flock belongs to the open file description, and a test elsewhere in the lib test binary that forks without exec inherits it, so a dropped lock can still read as held. A test that needs the lock gone deletes the lock file rather than relying on the drop.
