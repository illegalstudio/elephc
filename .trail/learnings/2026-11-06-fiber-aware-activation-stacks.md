---
title: Fiber-aware bt parks activation slices at suspend and restores them above the consumer
author: OpenCode
date: 2026-11-06
branch: detached/db62f09226
cost: { minutes: 160 }
touches:
  - src/codegen_support/runtime/data/fixed.rs
  - src/codegen_support/runtime/fibers/api/common.rs
  - src/codegen_support/runtime/fibers/api/arm64.rs
  - src/codegen_support/runtime/fibers/api/x86_64.rs
  - src/codegen_support/runtime/fibers/entry.rs
  - crates/elephc-debug/src/lib.rs
  - tests/codegen/cli.rs
  - docs/compiling/dap.md
---

## Decided

- The debugger keeps two thread-local maps keyed by `_fiber_current`: the
  consumer depth recorded when a coroutine is switched into (`FIBER_BASES`) and
  the frames parked at its last suspend (`FIBER_STACKS`).
- `__rt_fiber_start`/`__rt_fiber_resume` call a nullable enter hook before the
  switch. It records the current consumer depth as the coroutine's base and
  restores that coroutine's parked frames above it. First start has no saved
  frames; the body's normal enter hook lands above the base.
- `__rt_fiber_suspend` calls a nullable suspend hook just before the switch. It
  splits the activation stack at the recorded base, parks that slice under the
  coroutine pointer, and leaves the consumer frames in place. The consumer's
  `bt` therefore stops showing the suspended fiber as its caller.
- The fiber trampoline calls a nullable finished hook before switching back on
  both clean return and escape. It removes the coroutine's maps and truncates
  the stack to the base, so a terminated fiber cannot leave frames behind.
- Generators use the same `__rt_fiber_*` primitives, so `yield`/`next()` get the
  same park/restore behavior without a second mechanism.
- The hooks are nullable slots in the core runtime, like the exception hook. A
  normal `--debug` run without a session pays one load and a not-taken branch on
  each coroutine switch; no Rust symbol is required in a non-debug binary.
- The hooks are installed by `elephc_debug_boot` for every debug session mode
  (tracing, stdin, DAP, call capture), so the activation stack is consistent
  even when only call capture is active.

## Tried and ruled out

- Saving the **entire** activation stack at suspend and clearing it: the
  consumer's own frames would disappear while it runs between start and resume.
  The base-depth split keeps them.
- Deriving the base from the fiber body's function id: recursive/nested fibers
  and generators make the boundary a stack depth, not a function identity.
- Calling Rust on every coroutine switch without a capacity check: the slot is
  null unless a debug session installed it, and the runtime path pays only a
  load/branch.
- Hooking only the suspend side: the coroutine's frames must reappear above the
  consumer at resume, and a finished fiber must drop them; all three switch
  directions need a hook.

## Watch out

- The finished hook is a safety net for normal/escape trampoline switches. A
  longjmp that jumps directly to a caller's catch across a fiber boundary (rather
  than through the fiber trampoline) can still leave parked frames; the DAP stop
  at the throw itself is correct, but a later re-raise on that path should not be
  used to infer the fiber's frames.
- Fiber/generator DAP exposure is still one `threadId`; per-fiber thread-like
  stacks and selection are a separate UI feature.
- The maps are per-thread; a fiber cannot migrate across OS threads.
- Nested fibers compose because each base is the consumer depth at its own
  switch-in; a fiber's parked slice is keyed by its `_fiber_current` pointer.
- `ELEPHC_DEBUG_SITES=1` tracing installs the hooks too, so a trace build pays
  the map bookkeeping on every switch. The slots are still null when no debug
  boot runs.
- Tests: fiber and generator line stops assert the consumer stack excludes the
  suspended body and the resumed body's stack is `body -> consumer -> main`; a
  DAP exception stop inside a resumed fiber asserts `fiberBody -> consumer ->
  main`.
