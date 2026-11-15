---
title: One run can carry both exact instrumentation and call capture, but its timing is a debug-build timing
author: OpenCode
date: 2026-11-06
branch: detached/db62f09226
cost: { minutes: 110 }
touches:
  - src/monitor/calls.rs
  - src/monitor/command.rs
  - tests/codegen/cli.rs
  - docs/compiling/cli-reference.md
---

## Decided

- `monitor --calls --exact` compiles a `.php` source once with
  `--debug --with-monitoring`. `--debug` is what supplies the embedded metadata,
  the debug runtime and `ELEPHC_DEBUG_CALLS`; `--with-monitoring` supplies the
  exact enter/exit instrumentation. Both ride in the same binary.
- The same child is activated over the control socketpair (for exact profiling)
  and receives `ELEPHC_DEBUG_CALLS=<mode>` (for call capture). One program run
  publishes both streams on stderr.
- A binary target must carry both the `.edbg.json` sidecar (debug metadata) and
  the monitoring marker; the error names the rebuild flags.
- stderr is parsed once:
  - `elephc-debug-call:` events feed the call table/JSONL/HTML;
  - `elephc-instr:` and the other profiler prefixes stay out of the passthrough
    and are fed to `parse_instrument_dump`;
  - `elephc-instr: note:` lines are forwarded as `elephc monitor:` notes;
  - every other line is the program's own diagnostic and is passed through.
- Output order is exact profile first (table + recommendations), then the
  call-value table. `--calls-out` and `--html` still apply to the call events;
  combined `--html` is the call-tree panel, not the profile HTML.
- A combined run fails with 1 if either side is missing: no instrument frames or
  no call events. Call events are checked after the profile is printed, so a
  profile is still visible if call capture is empty.
- `--exact` remains refused in calls mode with the profile exporters it cannot
  honour (`--out`, `--pprof`, `--dot`, `--save`, `--baseline`, `--assert`, …).

## Tried and ruled out

- Compiling calls-only with `--with-monitoring` always: it adds instrumentation
  cost and a monitoring bridge to every call-capture run. Only `--exact` pays.
- Running two processes (one exact, one calls): the task is one run and the two
  profiles must describe the same execution. Two runs can observe different code
  paths and heap state.
- Letting raw `elephc-instr:` lines pass through: `run_instrument` deliberately
  suppresses them; printing the dump as program stderr would flood the call table
  and look like user output.
- Writing the profile `--html` in combined mode: `--html` already means the
  call-tree panel in calls mode. The profile HTML remains available through
  `monitor --exact` without `--calls`.
- Enabling profile exporters (`--out`, `--pprof`, `--dot`) in combined mode
  without designing their interaction with call exports. Kept refused for now.

## Watch out

- **The exact timing is a debug-build timing.** `--debug` disables EIR
  optimization and the AST passes that erase source statements, so it is slower
  and less inlined than a production `--with-monitoring` build. This is the price
  of call capture in the same binary.
- **Call capture adds overhead to the measured calls.** Argument and return
  formatting runs inside the exact enter/exit hooks, so the inclusive/self times
  include capture cost. A pure `monitor --exact` run is the right profile when
  pure timing is the question.
- `wrap`/other small helpers stay visible in the combined profile because
  `--debug` disables the inliner; the call tree needs those frames anyway.
- The profile and call events come from the same child process, but they are not
  correlated event-by-event; the table is per function and the call tree is per
  invocation.
- `--calls=none` still captures function/depth events; the exact profile is
  unaffected.
- The e2e fixture uses `wrap -> work` with a 1000-iteration loop. It asserts the
  `exact profile —` header, `{main}`/`wrap`/`work` rows, `incl`/`self`, and the
  call values `wrap(...) -> int(499501)` and `work(...) -> int(499500)`, plus the
  program output `499501`.
