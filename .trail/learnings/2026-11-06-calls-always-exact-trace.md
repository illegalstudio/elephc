---
title: Call values are the debug dimension of the exact monitor; --exact is now implied
author: OpenCode
date: 2026-11-06
branch: detached/db62f09226
cost: { minutes: 60 }
touches:
  - src/monitor/calls.rs
  - src/monitor/command.rs
  - tests/codegen/cli.rs
  - docs/beyond-php/debugging.md
  - docs/compiling/cli-reference.md
  - .plans/native-debugger.md
---

## Decided

- Every `monitor --calls` run now builds `--debug --with-monitoring`, activates the
  exact control channel, and prints the exact per-function profile before the
  call values. `--exact` remains accepted but is implied.
- A binary target therefore needs both the `.edbg.json` sidecar and the
  monitoring marker for any calls mode, not just `--exact`/`--html`.
- `--trace FILE` is accepted in calls mode and sets `ELEPHC_INSTR_TRACE` on the
  same child. The runtime writes the Chrome/Perfetto trace at exit while call
  events are captured; the two exports describe one run.
- The unified `--html` page still implies the exact capture; it is now a
  consequence of calls mode rather than a special case.
- The text table, JSONL, redaction, truncation and postmortem event ring are
  unchanged; only the surrounding build/run policy moved.

## Tried and ruled out

- Keeping `--exact` opt-in: the issue's commands capture exact performance plus
  values without spelling `--exact`, and the unified page's Profile tab needs the
  profile. An opt-in flag would make the default report a calls-only artifact.
- Letting `--calls --trace` stay refused: the trace is a different exporter over
  the same exact hooks, not an overload of the call-value stream. Refusing it
  forced two runs to get both.
- Writing the trace from the call events: the runtime trace captures spans with
  timestamps and nesting from the instrument hooks; reconstructing it from the
  debug event ring would lose instrumentation timing.

## Watch out

- `--calls` is now heavier: it compiles/links the monitoring capability and pays
  the exact hooks. This is the documented debug-capture cost.
- The exact table is a debug-build timing and includes call-capture overhead.
- A binary built with only `--debug` is refused for `--calls`; rebuild with
  `--debug --with-monitoring`.
- `--trace` is written by the child; a missing file means the exact capture did
  not reach the trace flush, not that the calls failed.
- The e2e `test_cli_monitor_calls_exact_profile_and_trace` checks the exact header,
  call values and a `{"traceEvents":[` Chrome trace from one command.
