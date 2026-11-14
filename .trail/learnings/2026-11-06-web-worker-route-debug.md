---
title: Web route debugging uses a per-worker advertised socket, not a shared DAP fd
author: OpenCode
date: 2026-11-06
cost: { minutes: 180 }
branch: detached/db62f09226
touches:
  - crates/elephc-debug/src/lib.rs
  - crates/elephc-debug/src/dap.rs
  - crates/elephc-web/src/debug_bridge.rs
  - crates/elephc-web/src/worker.rs
  - src/codegen/debug_sites.rs
  - src/codegen/frame.rs
  - src/codegen_support/runtime/data/fixed.rs
  - src/debug_cli.rs
  - tests/web/debug.rs
  - tests/web_tests.rs
  - docs/beyond-php/debugging.md
  - docs/compiling/cli-reference.md
  - docs/compiling/dap.md
  - .plans/native-debugger.md
---

## Decided

- `--debug --web` boots once in the **master** (`emit_web_entry_stub`), before
  `elephc_web_run` forks. The metadata, route filters and the worker hook slots
  are inherited by every worker. The per-request handler boot is a no-op.
- The master does **not** arm site flags. A matching request arms them for its
  own handler and the end hook disarms them, so unrelated requests pay neither
  the wrapper nor the debugger lock.
- A shared inherited DAP fd is unusable: whichever sibling reads first would
  steal the session. Instead, the selected worker creates a mode-0600 Unix
  socket after `fork()`, writes a mode-0600 `{pid, method, path, socket}`
  announcement into the broker's 0700 directory, and polls `accept` for at most
  30s. The broker connects to exactly that worker.
- `elephc debug --web --listen HOST:PORT` compiles `--debug --web`, launches the
  prefork server with `--workers`, and waits for the first matching request:
  - `--route` is `*`, exact, or a trailing-`*` prefix;
  - `--method` is case-insensitive;
  - `--header 'Name: value'` matches one case-insensitive header name and exact
    value.
- The broker writes `server.pid` in its session directory and leaves the server
  running after detaching. `disconnect` resumes the selected request; it must
  not `process::exit(0)` as a normal launch does.
- DAP disconnect/EOF handling is web-aware: `web_session` makes the request
  loop close the transport and return to the handler. Break sites, conditions,
  data watches and variable references are cleared by `web_request_end`.
- The DAP fiber stack already works per worker; the test proves a breakpoint in
  `fiberWork` after `Fiber::resume()` reports the fiber thread and the chain
  `fiberWork -> selected -> main`.
- The concurrent test pins two **keep-alive** connections to distinct workers by
  reading `/pid`, then sends `/selected` and `/other` on those connections. A
  fresh connection could be rerouted to the blocked worker by SO_REUSEPORT and
  made the test flaky.

## Tried and ruled out

- A shared socketpair on fd 3 for every worker: all siblings would read the same
  DAP frames, and the first reader would own the session. The inherited fd
  cannot identify the selected request.
- A loopback listener in the master: the web server forks before tokio and again
  per request; a listener created before `fork()` is inherited but does not
  select a route, and a worker-side listener races its siblings. The advertised
  per-request socket is the only endpoint that knows both the worker and the
  request.
- Booting the debugger in the worker's handler prologue only: the route hook
  must run before the handler and must install metadata/hook slots; booting in
  each worker would race DAP setup and lose the master-inherited route filters.
- Calling `process::exit(0)` on web disconnect: it kills the worker that must
  serve every later request. The web-session path resumes instead.
- Testing concurrency with a fresh `/other` connection: SO_REUSEPORT can route
  it to the blocked worker; the test pins a second keep-alive connection.

## Watch out

- A broker that never connects leaves the selected worker in the 30s accept
  poll. Other workers keep serving; the request is delayed, not failed.
- The first matching request wins; a second matching request in another worker
  waits out its own accept timeout and then proceeds normally.
- `--web-isolation=pool|request` is not wired: those handlers run behind the
  worker/broker IPC boundary and need the DAP stream passed across it.
- The broker's session directory is 0700, sockets/announcements 0600, and the
  `server.pid` file 0600. The server is left running by design; kill its master
  from `server.pid`.
- The server's stdout is redirected to stderr so only the broker's stdio is the
  DAP channel.
- The e2e test compiles a real `--debug --web` fixture and drives the broker over
  DAP; it does not use a fake adapter.
