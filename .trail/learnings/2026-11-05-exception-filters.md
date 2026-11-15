---
title: Exception filters hook the single throw helper through a nullable slot
author: OpenCode
date: 2026-11-05
branch: detached/db62f09226
cost: { minutes: 110 }
touches:
  - src/codegen_support/runtime/exceptions/throw_current.rs
  - src/codegen_support/runtime/data/fixed.rs
  - crates/elephc-debug/src/lib.rs
  - crates/elephc-debug/src/dap.rs
  - tests/codegen/cli.rs
  - docs/compiling/dap.md
---

## Decided

- Every PHP throw funnels through `__rt_throw_current`, so the debugger needs one
  hook there, not per throw site. The helper loads a nullable
  `elephc_debug_exception_fn` slot after the thrown Throwable and handler top are
  established, but before it chooses between the nearest handler and the
  uncaught report.
- The hook receives `(caught: bool, throwable: pointer)`. `caught` is
  `_exc_handler_top != 0`: a non-zero handler top means the exception will be
  received by a `try`/`catch`. That is exactly the caught/uncaught distinction the
  DAP filters need, and it needs no unwinder re-entry.
- `setExceptionBreakpoints` maps `caught`, `uncaught`, `throw` (and `all`) onto an
  `ExceptionFilters` struct and an `EXCEPTION_BREAKS_ACTIVE` atomic. With no filter
  selected, the throw helper pays one load and a branch; the hook is null in a
  normal `--debug` run.
- The stop is emitted as `reason: "exception"` with the Throwable class as
  `text` and a `caught`/`uncaught` description plus the last source site. The
  hook runs before handler dispatch, so the stack still shows the thrower.
- The throwable class comes from the object's class id through the existing class
  table. Internal Throwable payloads already display by class name only, which is
  the right granularity for the stopped event.
- The slot is a linker-visible `comm` symbol in the core runtime data, like the
  profiler's throw slot, so a binary without `--debug` still links and simply
  leaves it zero.

## Tried and ruled out

- Hooking each `throw` statement in codegen: builtin errors, guard-generated
  TypeErrors, callable invocations and the unwinder's own uncaught path all throw
  without passing through a source `throw`. The helper is the single chokepoint.
- Emitting a `stopped` event from the exception hook without a DAP session: the
  slot is null unless `ELEPHC_DEBUG_DAP` boot installed the hook, so a normal debug
  run never reaches Rust.
- Reporting the exception after handler selection: by then the unwinder may have
  released frames and the Throwable may already be owned by the catch variable.
- Treating `caught` as "a catch clause exists in source": the handler top is the
  runtime truth, including nested and rethrown exceptions.

## Watch out

- The hook runs while the thrower's registers are saved but before the unwinder
  releases frames. Do not incref/decref or allocate from the hook; the formatter
  is borrowed-only.
- An uncaught stop that is continued prints PHP's normal fatal report and exits
  with `255`; the DAP socket closes before a `terminated` event can be written, so
  clients must treat EOF as process end.
- Fibers and generators switch stacks outside the activation push/pop pairs. A
  throw crossing a suspended stack can report the wrong innermost frame until the
  fiber-aware stack lands.
- `setExceptionBreakpoints` accepts no `filterOptions` yet; `supportsExceptionFilterOptions`
  is false. Conditions on exception filters are not implemented.
- The hook is installed only by the DAP boot path. `elephc debug` (stdin) and
  `monitor --calls` do not stop on exceptions.
- The e2e fixture throws once inside a catch and once uncaught; the test stops on
  both, checks `RuntimeException` and `caught`/`uncaught`, continues, and expects
  the uncaught exit status 255.
