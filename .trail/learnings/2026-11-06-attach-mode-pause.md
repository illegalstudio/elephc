---
title: Attach pause needs a pid file written before the listen socket, not after accept
author: OpenCode
date: 2026-11-06
branch: detached/db62f09226
cost: { minutes: 70 }
touches:
  - crates/elephc-debug/src/dap.rs
  - src/debug_cli.rs
  - tests/codegen/cli.rs
  - docs/compiling/dap.md
  - docs/compiling/cli-reference.md
---

## Decided

- `listen_for_client` writes `<socket>.pid` (mode 0600, `create_new`) **before**
  binding the listen socket. A client that can connect can therefore read the pid
  without racing the `accept` cleanup; the CLI reads it before `connect`.
- The pid file contains `std::process::id()` and is removed together with the
  socket after the first client is accepted. A stale pid file from a crashed
  debuggee is removed before the new one is created (the socket path already had
  the same stale-socket policy).
- `elephc debug --attach` reads the pid before connecting, passes it to the
  existing `PauseInterceptor`, and the interceptor mirrors each DAP `pause` frame
  to `SIGUSR1`. Launch mode keeps using the child pid the adapter owns.
- If the pid file is missing (an older debug binary, or a manually removed file),
  attach still works, prints one warning, and pause is unavailable. The socket
  remains the attach credential; the pid file is a companion, not a second
  authorization channel.
- The runtime's `SIGUSR1` handler is unchanged: it only stores `PAUSE_REQUESTED`,
  and the next site stops with `reason: "pause"`. The buffered pause request is
  acknowledged after the stop.

## Tried and ruled out

- Reading the pid after `connect`: the accept path removes the pid file and socket
  immediately, so the CLI can lose the race. Reading before `connect` is safe
  because the pid file is written before bind.
- Putting the pid on the socket stream as a greeting: it would mix metadata with
  the DAP framing or need a handshake before the DAP `initialize`; a side file is
  simpler and matches the existing socket credential.
- Trusting the pid file for authentication: possession of the mode-0600 socket is
  still the credential. The pid file is mode 0600 only to avoid leaking process
  ids to other users; the adapter does not verify the pid before signaling.
- Waiting for `accept` before writing the pid: same race, worse; the client can
  connect as soon as `bind` succeeds.

## Watch out

- A pid file can outlive a crashed debuggee. The next listener removes it before
  writing; a CLI attaching to a stale socket without a listener fails at connect
  before signaling.
- The CLI does not verify the pid is alive or matches the socket peer. A stale pid
  that was reused by another process could receive `SIGUSR1`; the socket connect
  is the real attach gate, and this is a local debug mode.
- The `SIGUSR1` handler replaces any user handler and assumes the program does not
  use it (existing launch-mode limitation, now shared with attach).
- The e2e starts the debuggee itself with `ELEPHC_DEBUG_DAP=1
  ELEPHC_DEBUG_DAP_LISTEN=<socket>`, asserts the pid file names the child pid,
  attaches, waits past the entry stop, sends `pause`, receives a `stopped` event
  with reason `pause`, disconnects, and checks both processes exit successfully.
