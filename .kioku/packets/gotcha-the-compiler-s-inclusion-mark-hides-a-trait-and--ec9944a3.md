---
id: gotcha-the-compiler-s-inclusion-mark-hides-a-trait-and--ec9944a3
type: gotcha
title: "The compiler's inclusion mark hides a trait and an interface from the interpreter, one hop apart"
description: "compiler_included_skip is a promise about the compiled world; a trait's members and an interface's contract are not readable from there, so an interpreted class that uses either cannot be declared at all"
created: 2026-09-22
verified_by: "ELEPHC_EVAL_TRACE on the compiled Symfony bin/console, before and after the trait retry"
sources:
  - path: crates/elephc-magician/src/interpreter/include_exec.rs
    blob: c7f1115fb6cccaf8017b9f06834bb4ba01cd3ea8
    lines: 160-210
    snip: feba38dc08da
    anchor: "static REINCLUDE_COMPILER_INCLUDED_SOURCES: std::cell::Cell<bool> ="
  - path: crates/elephc-magician/src/interpreter/statements/trait_declarations.rs
    blob: 044491fb28b946809c7bb871892bd78d6d4c1382
    lines: 344-395
    snip: 44dae992733a
    anchor: "fn ensure_eval_traits_available("
---

# The compiler's inclusion mark hides a trait and an interface from the interpreter, one hop apart

## Fact

`interpreter::include_exec` skips an include whose file the compiler already took
(`phase=compiler_included_skip`). That promise holds for a CLASS — the compiled class is callable —
and fails for every declaration whose BODY the interpreter needs to read:

  TRAIT      members are copied into the using class at declaration time.
  INTERFACE  the contract is validated against the pending class.

MEASURED on the compiled `bin/console`, `about`:

  1. `RoutingControllerPass` (interpreted) uses `PriorityTaggedServiceTrait`.
     `PriorityTaggedServiceTrait.php` traced `compiler_included_skip`, `context.trait_decl()`
     stayed empty, `stage=expand_traits` failed, and the command died with
     `class ... could not be declared`.
  2. With a retry that ignores the mark (`with_compiler_included_sources_reincluded`), the file
     is genuinely included (`phase=input_cached`) and the class in it declares — and the SAME
     command now dies one hop later, on `CompilerPassInterface.php`, also
     `compiler_included_skip`, immediately before the `statement_error`.

THE FLAG MUST BE REQUEST-SCOPED, NOT CONTEXT-SCOPED. A per-`ElephcEvalContext` flag changes
nothing: an autoload callback is evaluated against a DIFFERENT context than the one that asked for
the symbol, so the include the callback performs never sees it. Measured — the retry ran and the
include was skipped anyway. A thread-local fixed it.

RETRY ONLY, NEVER BYPASS UP FRONT. Ignoring the mark for every autoload would re-interpret files
the compiled world already has, and a class declared twice hits the duplicate guard at the top of
`execute_class_decl_stmt` and fatals. The bypass belongs on the SECOND attempt, after the ordinary
autoload has failed to produce the symbol.

The next failure carries NO `class_decl_*` trace line at all, which places it outside every stage
that path traces — `ensure_eval_class_parent_available` is the one untraced step between the
duplicate guard and `validate_parent`.

## Why

It is the remaining blocker for the compiled console and the dev --web environment, and it recurs once per declaration KIND
