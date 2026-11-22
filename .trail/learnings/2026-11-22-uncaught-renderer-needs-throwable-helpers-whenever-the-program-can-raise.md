---
title: The uncaught renderer needs the Throwable helpers whenever the program can raise, not only when it spells a throwable type
author: Guillaume
date: 2026-11-22
branch: bead-k542
touches:
  - src/throwable_string_prelude.rs
  - src/optimize.rs
  - tests/codegen/exceptions.rs
---

`helpers_needed` (b9a7b231d0) rooted `__elephc_throwable_*` only for a source that spelled a throwable type. But every builtin or operator error (`1 % 0`, `intdiv(1, 0)`, a builtin ValueError) reaches the uncaught-exception renderer, so `$x = 1 % 0;` printed the Fatal line without `Stack trace:` / `thrown in`. Fix: after the spelling checks, `helpers_needed` also returns `optimize::block_may_raise(user statements)` (the effects analysis `block_may_throw`, with no summaries active, so any call counts). `echo 1;` stays at 13 functions; a program with a call, `/`, `%`, a non-literal shift, a clone, an undefined constant read and so on roots them. Probe: `--emit-ir`, count `^  function `.
Separate gaps seen while probing, which also fail with `$u = "Exception";` added: internal-function frames are missing from traces (`#0 f.php(3): intdiv(1, 0)`, bead elephc-9d0), `Call to a member function foo() on null` prints no ` in FILE:LINE` / `thrown in`, and an undefined property read prints no Warning.
The raise test uses the callable summaries of the whole program (`CallableEffectAnalysis::from_program`), not the bare statement effects. The project's own model is conservative (a ref assign, an array push/assign, a typed-parameter call all "may throw"), so module-wide optimizer fixtures with those (release_local_slot already_ref_bound, integer_destinations, stack_numeric tagged_scalar_rhs) now root the helpers and fail their module-wide assertions; with `$u = "Exception";` added, stack_numeric tagged_scalar_rhs already failed before, because rooting changes the asm of the user function itself (a `__rt_decref_mixed` appears).
