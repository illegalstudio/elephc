---
title: Call values reuse the debug frame reader; coverage reuses the debug site guards
author: OpenCode
date: 2026-11-05
branch: detached/db62f09226
cost: { minutes: 180 }
touches:
  - crates/elephc-debug/src/lib.rs
  - crates/elephc-debug/src/value.rs
  - src/monitor/calls.rs
  - src/monitor/command.rs
  - src/monitor/mod.rs
  - src/test_command.rs
  - src/test_command/args.rs
  - src/test_command/coverage.rs
  - src/codegen/lower_inst/builtins/system.rs
  - tests/codegen/cli.rs
  - tests/codegen/testing_command.rs
  - docs/compiling/cli-reference.md
  - docs/beyond-php/testing.md
  - docs/compiling/debug-metadata.md
---

## Decided

- `monitor --calls` uses the debug runtime's existing function-enter hook, not
  the exact `--instrument` ABI. The hook already has the function id and frame
  pointer; debug metadata already carries parameter offsets and PHP types; the
  read-only value formatter already renders arrays, strings and ref-cells.
  Adding a second call-value ABI to `elephc-instr` would duplicate that work and
  its ownership rules.
- The debug runtime keeps a bounded global ring (10,000 events). Each event is
  one JSON line on stderr: `elephc-debug-call: {"depth":…,"function":…,"args":…}`.
  `monitor --calls` parses those lines, passes every other stderr line through,
  and prints an indented call-values table after the program exits.
- Return values are captured by the debug exit hook. `emit_exit_call` preserves
  the result on the stack, passes `(tag, lo, hi)` as extra ABI arguments, and the
  runtime attaches `{"value","type"}` to the activation's call event before
  popping it. Strings pass pointer+length, tagged scalars pass payload+tag,
  floats pass raw bits, references/pointers use tag 13, and void/never uses the
  tag sentinel `-1` so no return is recorded.
- The activation frame stores the call event index, so return values attach to
  the exact call even under recursion. The event ring stores JSON values with a
  `return` field; `--calls-out FILE` writes the same events one JSON object per
  line.
- `test --coverage` compiles the suite with `--debug`, runs it with
  `ELEPHC_DEBUG_SITES=1`, parses the `elephc-debug-site` lines, and joins them
  with the `.edbg.json` sites table. A line is covered when any site on it ran.
- Coverage forces single-process execution. Site counters are process-local; a
  per-test fork would lose every child's hits. A native fatal therefore ends the
  coverage run, as with `--no-isolation`.
- The test driver ends with `exit($runner->finish())`, so the generated dynamic
  exit path now calls `elephc_debug_dump` for debug builds. Without that, the
  suite would exit before publishing its counters.

## Tried and ruled out

- Extending `elephc_instr_enter`/`_exit` with argument/return descriptors: it
  would require a new static descriptor format, per-target return-register
  marshalling, and a second value formatter in a crate that deliberately does
  not depend on the debugger. The debug hooks already carry the activation
  identity and frame.
- Capturing call values from a `--with-monitoring` binary: that runtime does
  not know parameter frame offsets or PHP types. `--calls` therefore builds with
  `--debug`; it is a value-capture mode, not a timing profile.
- Per-child coverage files: they would solve isolation coverage but need a
  shared append protocol, cleanup, and aggregation. Single-process coverage is
  the honest v1 and is documented.
- Branch coverage: the compiler emits one guard per source line, so branch
  points are not represented. lcov/cobertura and branch coverage stay separate.

## Watch out

- `monitor --calls` text and JSONL now carry arguments and returns. HTML panel,
  call graph/timeline integration and the `none|types|full` mode matrix remain
  (`elephc-lx21`, `elephc-jwio.3`).
- The call event ring loses events past 10,000 and reports the dropped count.
- A function with no named locals still gets a call event (main included); the
  metadata lookup falls back to an empty parameter list.
- Return capture reads the result before the final return-value pop but after
  local cleanup. Void/never passes tag `-1`; by-reference returns use the pointer
  tag and display as an address, not a dereferenced value.
- The enter hook locks the debugger mutex and formats values on every call. This
  is a debug-only capture; a normal `--debug` run pays nothing beyond the
  existing enter hook because `CALLS_ENABLED` is false.
- Coverage only counts compiled functions. Filtered-out tests and unreachable
  declarations are absent from the denominator, not marked uncovered.
- Coverage runs the suite in one process; a test that calls `exit()` directly
  ends the run early (but the dynamic exit path still dumps what ran).
- The test driver's `exit()` now calls the debug dump for every debug build, not
  only coverage. A normal debug test run without `ELEPHC_DEBUG_SITES` makes the
  dump a no-op.
