---
title: Dumps and named trace segments ride the call-capture event ring as first-class kinds
author: OpenCode
date: 2026-11-06
branch: detached/db62f09226
cost: { minutes: 180 }
touches:
  - crates/elephc-builtin-contract/src/spec.rs
  - crates/elephc-builtin-contract/src/catalog_data.rs
  - crates/elephc-builtin-contract/src/registry.rs
  - crates/elephc-builtin-contract/src/support.rs
  - src/builtins/debug/mod.rs
  - src/builtins/debug/event.rs
  - src/builtins/mod.rs
  - src/debug_prelude.rs
  - src/pipeline.rs
  - src/main.rs
  - src/ir/runtime_fn.rs
  - src/codegen/lower_inst/builtins/debug_events.rs
  - src/codegen/lower_inst/builtins.rs
  - src/codegen/lower_inst/runtime_functions/group_08.rs
  - crates/elephc-debug/src/lib.rs
  - crates/elephc-debug/src/value.rs
  - src/monitor/capture_events.rs
  - src/monitor/calls.rs
  - src/monitor/call_html.rs
  - tests/codegen/cli.rs
  - docs/beyond-php/profiling.md
---

## Decided

- The public helpers are a debug-only PHP prelude (`elephc_dump`,
  `elephc_trace_start`, `elephc_trace_end`) calling one internal builtin,
  `__elephc_debug_event(kind, mixed value, ?string label, string to)`.
  `--strict-php` skips the prelude, so the names are undefined there; this keeps
  the helpers extension-only without adding three public catalog entries.
- `__elephc_debug_event` is declared in the shared contract as `internal: true`,
  implemented by a typed runtime target. Its custom lowering boxes the mixed
  value with the same value-boxing helper as ordinary mixed calls, stages it on
  the temporary stack, materializes the label/destination string pairs, and
  calls the debug bridge's `elephc_debug_event` C ABI. A boxed non-mixed value is
  released after the event; a borrowed Mixed value is not.
- The debug crate's event ring now carries mixed event kinds:
  - `{"kind":"call","depth","function","segments","args","return"}`;
  - `{"kind":"dump","label","value","type","segments","to"}`;
  - `{"kind":"segment_start","name","segments"}`;
  - `{"kind":"segment_end","name","complete","segments"}`;
  - `{"kind":"segment_warning","message"}`.
  Call returns still attach by the event index stored on the activation frame;
  dump/segment events may interleave without disturbing that index.
- `elephc_dump` uses `auto` to record when a capture is active and print to
  stderr otherwise. `stderr` always prints and still records; `terminal` prints
  to stderr (there is no separate terminal console in this native build) and
  records; `html` records or falls back to stderr; an unknown destination raises
  `ValueError` from the PHP prelude before the native recorder is called.
  An explicit dump is recorded even under `--calls=none`, since that mode only
  suppresses automatic arguments/returns.
- Segments are a thread-local stack. `trace_end` must match the top or warns and
  leaves the stack untouched. Open segments are closed as `complete:false` at
  capture exit. Call and dump events carry the current segment path.
- The three helper functions and the internal event builtin are filtered out of
  the call tree by name in `record_call`; the text and HTML renderers additionally
  filter by `function` presence so dump/segment events never appear as
  `<unknown>()` calls.
- `monitor --calls-out` writes the full mixed event stream; existing consumers
  find call events by their `function` field. The HTML add Dumps and Trace
  segments panels, tag call/dump lines with their segment path, and count calls
  and dumps between each segment start/end.
- Text output gains `dumps (N):` and `trace segments (N):` sections after the
  call table. Segment rows report complete/incomplete plus call/dump counts.

## Tried and ruled out

- Declaring three public builtins in the shared catalog: the helpers are
  debug-only and `--strict-php` hides them; a prelude plus one internal builtin
  keeps the public catalog stable and reuses the existing prelude-injection
  path.
- Passing a raw PHP value through an `extern` function: extern parameters are
  C ABI scalars/pointers, with no way to take a PHP value's boxed cell address
  from PHP source. The internal builtin owns that boxing.
- Treating dump/segment events as calls with a fake function name: the call tree
  would show `<unknown>()` and the call-count header would be wrong. Filtering
  by the `function` field is the contract.
- Making `--calls=none` suppress explicit dumps: the plan distinguishes
  automatic argument capture from an explicit `elephc_dump()` request.
- Using a process-global segment stack: fibers/threads would interleave
  unrelated names. The stack is thread-local; fiber-aware segment parking is
  still a later refinement.
- Throwing a PHP `ValueError` from the native event call: that call cannot cheaply
  construct a Throwable. The PHP prelude now validates `$to` and raises before the
  native recorder runs, so the runtime warning remains defensive only. (The
  segment-name checks still warn on stderr.)

## Watch out

- `--strict-php` must be checked before prelude injection; the e2e/manual test
  confirms `--debug --strict-php` reports the three helpers as undefined.
- The debug prelude is injected for every `--debug` build (unless strict), not
  only when referenced; unused functions are left to reachability pruning. It
  adds three tiny declarations to debug builds.
- `elephc_dump` always renders nested values with `render_nested`, independent of
  `--calls=none|types|scalars`; only automatic args/returns follow the mode.
- A segment opened in a fiber is stored on the OS thread; a suspend/resume can
  leave it visible while the consumer runs. Fiber-aware segment parking is
  future work.
- `trace_end` with a mismatched or absent top produces a warning event; it does
  not pop. The JSONL keeps the warning, and the event ring's `segment_warning`
  kind is rendered nowhere yet (text/HTML currently show the warning on stderr).
- The event ring remains bounded by event count; dump/segment events share the
  same overflow counter as calls.
- Call capture is debug-only; using the helpers in a non-debug build is not
  possible because the prelude is not injected and the internal builtin is
  internal.
- The e2e fixture has `parse` open a segment, call `inner`, dump an array, and
  close the segment; it asserts the text Dumps/Trace sections, JSONL event kinds
  and fields, HTML panels/tags, and that helper names never appear as calls.
