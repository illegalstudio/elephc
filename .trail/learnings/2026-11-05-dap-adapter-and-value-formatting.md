---
title: DAP belongs in the runtime agent; the CLI is only the socketpair bridge
author: OpenCode
date: 2026-11-05
branch: detached/db62f09226
cost: { minutes: 300 }
touches:
  - crates/elephc-debug/src/dap.rs
  - crates/elephc-debug/src/value.rs
  - crates/elephc-debug/src/lib.rs
  - src/debug_cli.rs
  - tests/codegen/cli.rs
  - docs/compiling/dap.md
  - docs/compiling/cli-reference.md
  - docs/compiling/output-and-diagnostics.md
  - docs/compiling/debug-metadata.md
---

## Decided

- DAP is served by the runtime crate, not by translating the human line
  session. `Debugger` already owns the stop point, activation stack, locals and
  metadata maps; the protocol loop uses those directly.
- `elephc debug --dap` creates a Unix socketpair, puts the child end on fd 3,
  and launches the debug binary with `ELEPHC_DEBUG_DAP=1` and
  `ELEPHC_DEBUG_FD=3`. The CLI only forwards bytes between the IDE stdio and the
  socket. The debuggee's stdout is redirected to the adapter's stderr so PHP
  output and compiler warnings cannot corrupt the DAP stream.
- The first executed site is a configuration barrier. The runtime holds there
  without emitting `stopped` until `configurationDone`; it then resumes to the
  first breakpoint unless `launch.stopOnEntry` is true. This matches the normal
  DAP launch sequence instead of making every launch require an extra continue.
- Value formatting is read-only and lives in `value.rs`. It reads the boxed
  Mixed cell, indexed-array header/element stride, hash header/entry chain, and
  reference-cell payload word directly from the stopped program. No incref,
  decref, COW split, allocation or user-code call runs while inspecting.
- Array parameters can arrive as a boxed Mixed cell even when the declared type
  is `array`, because the caller materializes a temporary. The heap-kind byte
  before the pointer distinguishes raw array/hash storage from a boxed cell.
- Compiler-owned locals whose names contain `#` or start with `__elephc_` are
  filtered from metadata before they reach `locals`/`variables`.
- Attach is a one-shot Unix socket created mode 0600 by the debug binary when
  `ELEPHC_DEBUG_DAP_LISTEN` is set. The runtime accepts the first client at its
  entry stop and removes the path. `elephc debug --attach <socket>` connects and
  proxies DAP on stdio; it does not own the debuggee.
- `setBreakpoints` verifies the on-disk SHA-256 against the metadata hash. A
  stale source returns `verified: false` with a message and is not armed.
- Launch-mode pause is a SIGUSR1 signal from the CLI proxy. The runtime handler
  only stores an atomic flag; the next site hook stops with reason `pause` and
  acknowledges the buffered request.

## Tried and ruled out

- Parsing the human `locals`/`bt` text in the CLI: it cannot express nested
  arrays, variable references, scopes or step events without a second parser;
  DAP in the runtime is smaller and has access to the activation stack.
- Putting DAP on the debuggee's stdout: PHP output shares that stream. The
  socketpair plus stdout-to-stderr redirect is the isolation boundary.
- Calling `var_dump`/`print_r` from the debugger to format values: those helpers
  allocate, mutate output state and can invoke `__debugInfo()`/destructors.
  Borrowed inspection must remain side-effect-free.
- Automatically emitting an entry `stopped` event for every launch: a normal
  client expects to run to its first breakpoint after `configurationDone`.
  Entry stopping is now driven by `stopOnEntry`.

## Watch out

- Requests other than `pause` sent while the program is running stay buffered in
  the socket until the next stop.
- Pause is launch-mode only: the CLI knows the child pid. Attach pause needs a
  pid handshake or an async reader in the debuggee and is not wired.
- The SIGUSR1 handler replaces any user handler and assumes the program does not
  use SIGUSR1. This is a debug-mode limitation.
- The attach socket is one-shot and removed after accept. A stale socket path is
  replaced only when it is a socket; a regular file is refused.
- `disconnect` is covered by a test that polls for process exit; pause and attach
  each have separate end-to-end tests.
- Source paths use suffix matching before hash verification. A missing or
  unreadable source falls back to the metadata hash being unavailable (breakpoint
  verified from the site table only); stale sources are rejected.
- The stale-source test compiles first, mutates the included file, then attaches
  to the prebuilt binary. Do not re-run the compiler before setting the
  breakpoint or the test loses its stale state.
- Objects show `object(class_id=N)` only. Class names, property scopes and
  `__debugInfo()` need the class metadata table.
- Borrowed reference addresses fall back to the declared payload tag when the
  address is not a managed cell. This is correct for the supported shapes but
  does not validate arbitrary borrowed storage.
- The formatter caps arrays/hashes at 1024 children and nested Mixed unboxing at
  depth 8. Pagination (`start`/`count`) is not implemented.
