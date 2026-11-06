---
title: Postmortem is an uncaught-exception flush over the existing call ring, not a signal handler
author: OpenCode
date: 2026-11-06
branch: detached/db62f09226
cost: { minutes: 150 }
touches:
  - crates/elephc-debug/src/lib.rs
  - crates/elephc-debug/src/value.rs
  - src/debug_cli.rs
  - tests/codegen/cli.rs
  - docs/beyond-php/debugging.md
  - docs/compiling/cli-reference.md
  - docs/compiling/dap.md
  - .plans/native-debugger.md
---

## Decided

- `ELEPHC_DEBUG_POSTMORTEM=<file>` arms a launch. `elephc debug ... --postmortem
  FILE` sets it on the launched child for both the terminal and DAP paths;
  `--attach` refuses the flag because the already-running process was not
  configured to capture.
- Postmortem reuses the existing bounded call-event ring. When no
  `ELEPHC_DEBUG_CALLS` mode was requested, `CALL_MODE_NONE` records
  function/depth events without values. Otherwise the requested modes and their
  redaction/bounds apply unchanged.
- `ELEPHC_DEBUG_CALLS_MAX` overrides the 10,000-event ring for bounded
  diagnostics and tests. The document carries `captured` and `dropped`.
- Postmortem-only runs arm all site flags and track each activation's last
  **source** site (branch/path counters are ignored). This gives the exact throw
  line without opening an interactive stop. An entry without a site falls back to
  the function's `line_start` metadata.
- The throw hook (`caught == 0`) writes the document once: exception class,
  message, code, throw site, PHP stack, recent call events, captured/dropped.
  The exception fields are read from the fixed Throwable prefix
  (`class @ 0`, message `@ 8/16`, code `@ 24`) using the debugger's untrusted-read
  helpers.
- `elephc debug --last FILE` is a read-only inspector: exception, throw site,
  stack and the first 32 recent calls, without launching a program.
- A normal exit and a caught exception write nothing. Native crashes and hard
  fatals that bypass the throw hook are explicitly outside the guarantee: no
  allocation or JSON encoding happens in a signal handler.

## Tried and ruled out

- A separate lightweight postmortem ring: the existing event ring already carries
  the bounded call history and dropped counter, and the postmortem needs exactly
  that shape. Reusing it avoids a second capture path and keeps `monitor --calls`
  and postmortem consistent.
- Flushing in `elephc_debug_dump` at process exit: it cannot distinguish a normal
  exit from a fatal, and a fatal may bypass the epilogue. The uncaught branch of
  the throw hook is the one reliable point.
- Flushing from a SIGSEGV handler: allocating/formatting JSON there is unsafe.
  A native crash may leave no file; documented.
- Recording values by default for postmortem-only runs: unnecessary overhead and
  data exposure. Function/depth events are enough to see the recent call path.
- Writing after every caught throw: the ring would be consumed by exceptions the
  program handled normally. Only uncaught (`caught == 0`) writes.

## Watch out

- `--postmortem` is launch-only; attaching cannot retrofit the environment.
- The exception `file`/`line` is the throw site from the activation stack, not a
  separate Throwable trace side table.
- The recent-call list filters to events with a `function` field; dumps and trace
  segment events remain in the JSONL/call-capture path but are not "calls".
- `ELEPHC_DEBUG_CALLS_MAX` is clamped to `1..=10_000`; values above the fixed BSS
  capacity are ignored.
- A postmortem-only run still pays the site-hook cost (flags armed) to keep exact
  stack lines. This is a debug-only cost.
- The inspector prints at most 32 calls; the JSON document retains the full ring.
- The e2e ring test uses a side-effecting `tick()` and
  `ELEPHC_DEBUG_CALLS_MAX=3`; a pure call may be removed before codegen.
