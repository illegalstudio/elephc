---
title: DAP threads enumerate live fibers and frame ids encode thread plus depth
author: OpenCode
date: 2026-11-06
branch: detached/db62f09226
cost: { minutes: 110 }
touches:
  - crates/elephc-debug/src/lib.rs
  - crates/elephc-debug/src/dap.rs
  - tests/codegen/cli.rs
  - docs/compiling/dap.md
---

## Decided

- The runtime keeps an active coroutine chain in addition to the parked-frame
  maps. Enter pushes the coroutine, suspend pops it, and the trampoline finish
  hook removes it. The last chain entry is the currently executing fiber, or the
  chain is empty for the main consumer.
- `threads` returns `main` (id `1`) plus a stable thread-like id for every entry
  in `FIBER_BASES`/`FIBER_STACKS`. Ids start at `2` and are assigned once per
  adapter session. The client can select a suspended fiber without resuming it.
- A stopped event's `threadId` is the current coroutine's thread-like id, or `1`
  for main. Function and exception stops use the same rule.
- `stackTrace` reads the requested `threadId`: main shows the consumer frames
  below the outermost active coroutine; a suspended fiber shows the caller chain
  plus its parked slice; the innermost active fiber shows the whole live stack.
  Missing `threadId` means the current thread, preserving existing clients.
- Frame ids encode `(thread_id, depth)`. Main keeps the historical plain depth
  (`0`, `1`, …) so existing frame ids stay valid; fiber frames use the thread id
  in the high 16 bits. `scopes`, `variables`, `evaluate` and `dataBreakpointInfo`
  decode the frame id and inspect that thread's frame without changing the
  debuggee.
- `thread()`/`thread_stack()` never allocate in the debuggee; they only read the
  thread-local activation maps and the stopped frame memory.

## Tried and ruled out

- One flat `frameId` list across threads: two fibers can have the same depth and
  collide. Encoding the thread id keeps ids unambiguous.
- Making main always show the full live stack: it would duplicate the active
  fiber's frames and make the main thread look like it called the fiber. Main
  shows only the frames below the outermost active coroutine.
- Auto-resuming a suspended fiber to inspect its stack: that would run PHP code
  and change scheduler state. The parked slice is read directly.
- Enumerating fibers only from `FIBER_STACKS`: the current/active fiber has its
  frames live in the activation stack and no parked slice, so it must be included
  from `FIBER_BASES`.

## Watch out

- Thread ids are stable only for one adapter session; a reconnect (attach) starts
  a new mapping.
- Nested fibers are best-effort: an inner suspended fiber's caller chain is the
  current live stack, which may not contain an outer fiber that is itself parked.
  The client still sees both fibers as threads.
- Thread-like stacks are exposed only while stopped; `threads` is a DAP request in
  the stop loop, not a running-program sampler.
- `frame_id` uses the low 16 bits for depth; a single thread with >65,535
  activations would wrap. That is far beyond the runtime's activation limits.
- The e2e test stops in the consumer after `$fiber->start()`, selects the suspended
  fiber thread, asserts `fiberBody -> consumer -> main`, inspects `$inside` at the
  parked frame, then continues to output `done`.
