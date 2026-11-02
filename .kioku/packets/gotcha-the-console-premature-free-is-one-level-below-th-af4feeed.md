---
id: gotcha-the-console-premature-free-is-one-level-below-th-af4feeed
type: gotcha
title: "The console premature free is one level below the object walker, and the release trace names it in one run"
description: "ELEPHC_EVAL_RELEASE_TRACE shows every release owns its cell; only the last object reaches zero and deep-frees, and the fatal is a child inside a property Mixed that decref_mixed tail-branched into freeing while still shared"
created: 2026-09-22
verified_by: "ELEPHC_EVAL_RELEASE_TRACE and an lldb backtrace on the compiled Symfony bin/console"
sources:
  - path: crates/elephc-magician/src/interpreter/statements/array_updates.rs
    blob: 53dcc9405fa78e016c6cd2324983c7d8c5e09260
    lines: 52-83
    snip: d282867de0b6
    anchor: "pub(in crate::interpreter) fn eval_release_value("
  - path: src/codegen_support/runtime/arrays/object_free_deep.rs
    blob: 241747593bc5d8c3b27b06cc3113ceb65c7b3c2d
    lines: 70-260
    snip: e5f604e20b56
    anchor: "emitter.instruction(\"ldr x10, [x0]\"); // x10 = receiver class_id"
---

# The console premature free is one level below the object walker, and the release trace names it in one run

## Fact

`ELEPHC_EVAL_RELEASE_TRACE=1` on the compiled Symfony console prints, for every object release,
the class and BOTH refcounts. It is the fastest way into this bug and it already exists
(`interpreter::statements::array_updates::eval_release_value`). Seventeen releases precede the
fatal; the last six read:

    stdClass                                   cell_refs=1 object_refs=3
    DependencyInjection\ContainerBuilder       cell_refs=1 object_refs=3
    Loader\Configurator\ServicesConfigurator   cell_refs=1 object_refs=2
    Loader\Configurator\ServiceConfigurator    cell_refs=1 object_refs=2
    Loader\Configurator\ServiceConfigurator    cell_refs=1 object_refs=2
    Loader\Configurator\ServiceConfigurator    cell_refs=1 object_refs=1   <- then the fatal

Every release releases a CELL that it does own (`cell_refs=1`). The ones whose object still has
other owners are harmless: the cell decrements and the object survives. The LAST one is the only
release whose object reaches zero, so it is the only one that runs a deep free -- and the deep
free is what trips `heap debug detected free of a still-referenced block`.

THE OBJECT WALKER IS NOT THE BUG. `__rt_object_free_deep`
(`codegen_support/runtime/arrays/object_free_deep.rs`) releases each property through
`__rt_decref_any` / `__rt_decref_mixed`, never through a deep free -- it decrements, correctly.

The fatal is ONE LEVEL DOWN. The backtrace reads `_rt_object_free_deep + 1104` ->
`_rt_mixed_free_deep + 104` -> fail, with no `decref_mixed` frame between them, because
`__rt_decref_mixed` TAIL-BRANCHES into `__rt_mixed_free_deep` once a refcount reaches zero. So a
property's Mixed box legitimately hit zero, and the deep free of ITS contents reached a child
block whose refcount is still above one. Either that child was retained by someone the parent
never accounted for, or it was stored into the parent without an incref.

`__rt_mixed_free_deep` is a DEEP free by contract -- "recursively release owned child storage" --
so it is entitled to assume exclusive ownership. The defect is therefore in whoever built or
retained that child, not in the free helper.

NEXT STEP: name the child. The heap-debug validator already loads the refcount it is refusing to
free; printing the block's heap KIND and refcount there, or breaking on
`heap_debug_fail` and reading `x9`/`x0`, identifies what kind of storage is over-retained without
another round of guessing at the PHP level.

## Why

It clears the object walker, which looks like the obvious suspect and is correct, and names the one measurement that skips three rounds of guessing
