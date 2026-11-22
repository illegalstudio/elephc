---
title: The uncaught renderer needs the Throwable helpers whenever the program can raise, not only when it spells a throwable type
author: Guillaume
date: 2026-11-22
branch: bead-k542
touches:
  - src/throwable_string_prelude.rs
  - src/optimize.rs
  - tests/codegen/exceptions.rs
  - tests/codegen/optimizer.rs
---

`helpers_needed` (b9a7b231d0) rooted `__elephc_throwable_*` only for a source that spelled a throwable type. But every builtin or operator error (`1 % 0`, `intdiv(1, 0)`, a builtin ValueError) reaches the uncaught-exception renderer, so `$x = 1 % 0;` printed the Fatal line without `Stack trace:` / `thrown in`. Fix: after the spelling checks, `helpers_needed` also returns `optimize::block_may_raise(program, user statements)`: `effects::block_may_throw` over the non-prelude statements with the callable summaries of the whole program installed (`CallableEffectAnalysis::from_program`). `echo 1;` stays at 13 functions. The project's model is conservative (a call, `/`, `%`, a ref assign, an array push all "may throw"), which is the bar the catch-pruning pass already uses. Probe: `--emit-ir`, count `^  function `.

Rooting the helpers does NOT leak into user functions through a shared fact. `module_uses_backtrace` (codegen/frame.rs) is module-wide on purpose: a module that reads a trace (the helpers call `getTrace`) or throws publishes `__rt_php_frame_publish_bt` in every function and emits a `<fn>__bt_reader` after `@endfn`. That reader boxes the arguments (`__rt_mixed_from_value`, `__rt_array_push_refcounted`, `__rt_decref_mixed`), and shared-outlining (`_eir_shared_mixed_to_string`) changes with more copies of a ladder. So a raising program's asm differs from a non-raising one by design; any "frame-only owner retires without decref" test must stop at `@endfn`.

Module-wide optimizer assertions therefore need the helpers out of the way: `tests/codegen/optimizer.rs::without_throwable_machinery` drops `__elephc_throwable_*`, `Error::*`, `Exception::*` (EIR `  function NAME(` or asm `@fn name=NAME` segments). Used by release_local_slot (already_ref_bound, scalar slot) and integer_destinations (supported_target_emitters); stack_numeric tagged_scalar_rhs cuts the function at `@endfn`.

Separate gaps seen while probing (filed as bd bugs): uncaught `Call to a member function foo() on null` prints no ` in FILE:LINE` / `thrown in`; an undefined property read prints no Warning. Internal-function frames are missing from traces (`#0 f.php(3): intdiv(1, 0)`, bead elephc-9d0).
