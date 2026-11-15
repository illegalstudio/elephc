---
title: Trace segments are a debug-session console feature even when call capture is off
author: OpenCode
date: 2026-11-06
branch: detached/db62f09226
cost: { minutes: 75 }
touches:
  - src/debug_prelude.rs
  - crates/elephc-debug/src/lib.rs
  - tests/codegen/cli.rs
  - docs/beyond-php/debugging.md
  - docs/beyond-php/profiling.md
  - .plans/native-debugger.md
---

## Decided

- The trace-segment stack is no longer gated on `CALLS_ENABLED`. It is active
  when a `monitor --calls` capture is on **or** an interactive/DAP session is
  attached.
- With call capture on, segments still emit `segment_start`/`segment_end`/
  `segment_warning` events and feed JSONL/HTML.
- With no call capture but an interactive/DAP session, `trace_start` prints
  `elephc-trace: start <path>`, `trace_end` prints
  `elephc-trace: end <name>`, and `elephc_dump()` writes
  `[path] elephc_dump[label]: value`. This is visible in the debug console
  (stderr), which is the terminal client's console and the adapter's inherited
  stderr in DAP.
- A segment still open when the interactive session ends is reported as
  `elephc-trace: segment '<name>' closed incomplete` from the main epilogue.
- Empty segment names are validated in the PHP prelude and raise `ValueError`
  before the native event call. The runtime's empty-name warning remains
  defensive.
- Outside a call capture and outside an interactive/DAP session, traces remain
  no-ops.

## Tried and ruled out

- Reusing `CALLS_ENABLED` for debug-session traces: it would make `elephc_dump(
  ..., 'auto')` record into an event ring nobody displays instead of printing to
  the debug console, and it would pay argument capture for every call.
- Adding a DAP `elephc/traces` request: the acceptance only requires the segment
  to be visible in the debug console outside monitor capture; a custom request
  would add adapter surface without a viewer.
- Raising `ValueError` from the native event call: the C ABI has no cheap
  Throwable path. The PHP wrapper validates first, like `elephc_dump($to)`.

## Watch out

- Debug-console trace lines are stderr output; a program that redirects stderr
  loses them, just like other debug diagnostics.
- The segment stack is thread-local; fibers/threads have independent stacks.
- Mismatched ends still warn and do not pop; the mismatched name is not entered.
- Postmortem-only runs have `CALLS_ENABLED` true for the ring, so traces there
  follow the event path, not the console path.
- The dump scope prefix uses the current segment path at dump time; after the
  matching `trace_end` the prefix disappears.
