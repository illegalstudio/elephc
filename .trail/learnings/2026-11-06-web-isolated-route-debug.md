---
title: Isolated web route debugging runs the hook in the handler child, not across IPC
author: OpenCode
date: 2026-11-06
cost: { minutes: 150 }
branch: detached/db62f09226
touches:
  - crates/elephc-web/src/handler_broker/process/handler.rs
  - src/debug_cli.rs
  - tests/web/debug.rs
  - docs/beyond-php/debugging.md
  - docs/compiling/cli-reference.md
  - docs/compiling/dap.md
  - .plans/native-debugger.md
---

## Decided

- In `--web-isolation=pool|request`, PHP executes in a **handler child**, not in
  the worker. The route hook is therefore called in
  `execute_handler_request()`:
  - `debug_bridge::request_begin(&method, &path, &headers)` before
    `request_state::set_request` moves the snapshot;
  - `debug_bridge::request_end()` immediately after `handler()` returns.
- The handler child creates and accepts its own DAP socket and advertises it in
  the broker directory. No descriptor crosses the worker/handler IPC boundary:
  the child is the process that owns the PHP stack, so it is also the process
  that owns the DAP session. The debug CLI broker connects to the advertised
  socket exactly as it does for worker isolation.
- The master boot state is inherited through the fork chain
  master -> worker -> broker -> handler child: metadata, route filters, hook
  slots, the site-flag pointer and the debugger metadata. The master leaves
  `INTERACTIVE` false and the site flags unarmed; the matching handler child
  arms them for its request and disarms them at end.
- The worker and broker stay asynchronous while the selected handler child is
  stopped. With `--handler-concurrency 2`, a second handler child serves
  `/other`; with the default `1`, that request queues behind the stopped child.
- `--web-isolation=worker|pool|request` is a broker option for source targets.
  A prebuilt binary carries the mode it was compiled with; the flag is ignored
  (with a warning) for a binary target.
- Pool children are persistent: `request_end` must clear `INTERACTIVE`,
  disarmed sites, breakpoints, watches and variable refs before the next
  request, or the pool child would believe it still owns a DAP client.
- The fiber stack, entry stop, breakpoints, stepping and web-aware disconnect
  all work identically in the handler child; the tests assert the same
  `fiberWork -> selected -> main` chain in request and pool modes.

## Tried and ruled out

- Passing an accepted DAP fd from the worker to the handler child over
  `handler_ipc`: the protocol would need SCM_RIGHTS, an adoption API in the
  debug runtime and per-request fd ownership tracking. The handler child can
  create the socket itself after the request snapshot arrives, so there is no
  boundary to cross and the worker never blocks.
- Creating the session in the broker: the broker inherits the metadata but is
  not the process running PHP; adopting the stream in the handler child would
  still be required. The handler-child hook is the same code as worker
  isolation, reused.
- Using worker-isolation keep-alive pinning for request/pool: the worker is not
  blocked in isolated mode, so a fresh `/other` connection is served by the same
  asynchronous worker and a second handler child. The isolated test uses one
  worker to prove that.
- Arming sites in the master for isolated modes: unrelated requests would pay
  the hook. The per-request hook arms only the selected child.

## Watch out

- `--handler-concurrency 1` (the default) serializes a stopped selected request
  with every other request in that worker's broker. Use `2+` for diagnostics.
- `request_begin` runs before the `--max-execution-time` alarm is armed, so a
  debug attach wait does not count as PHP execution time. The alarm still guards
  the actual handler.
- In request mode `/pid` returns the disposable handler-child PID, not the
  worker PID; it is not used for connection pinning there.
- The handler child's session files are mode 0600 in the broker's 0700
  directory; the child removes the announcement/socket after accept and the
  key/address/pid files after a TCP attach.
- A handler child killed while stopped closes the DAP stream; the worker returns
  its failure response and the pool/request supervisor replaces the child.
- The master is never part of the debug session; the tests keep serving `/other`
  while the selected child is stopped and after the session.
