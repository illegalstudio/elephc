---
id: bug_fix-a-by-name-allocation-never-gave-an-owned-referen-73e2fbdb
type: bug_fix
title: "A by-name allocation never gave an owned reference property its cell, so the property default stored through null"
description: "A property with a reference taken to it anywhere holds a POINTER to an object-owned 16-byte cell, not a value. __rt_new_by_name zeroed the layout and ran _class_propinit_<id> without allocating those cells, so the first property default stored through null - a SIGSEGV in emitted code with no PHP diagnostic. Dev-only in Symfony because only the dev container constructs DebugHandlersListener. Fixed with _class_ref_prop_* tables plus a cell-allocation loop on both arches; two adjacent defaulted bools and the eval boundary were each measured NOT to be the ingredient"
created: 2026-09-21
sources:
  - path: src/codegen_support/runtime/objects/new_by_name.rs
    blob: 9f2622125c3330ad12666d323dd9d8a22323e79c
    lines: 143-174
    snip: cdbbf2b2b1f4
    anchor: "emitter.instruction(\"ldr x12, [sp, #32]\"); // reload the matched class id for the reference-cell table lookup"
  - path: src/codegen_support/runtime/data/user.rs
    blob: b6d3ce38902ea2ee0eca2d94879a332c604f7f9a
    lines: 2537-2599
    snip: 489a21622b85
    anchor: "fn emit_class_owned_reference_property_tables("
  - path: src/codegen/lower_inst/objects/allocation_clone.rs
    blob: b1b15398d9cee3b0d5c662912dc3e0fe496e1d8a
    lines: 75-94
    snip: ca6ad87bf454
    anchor: "pub(super) fn emit_owned_reference_property_cell("
  - path: src/codegen/lower_inst/objects/property_stores.rs
    blob: 5f80aa0a7212406ae36443c16b1a9a6adb010a9a
    lines: 281-296
    snip: 3dc60b200316
    anchor: "pub(super) fn emit_reference_property_write("
  - path: tests/codegen/objects/classes.rs
    blob: 51240cce37d354e789ad7dab4f082bc3b0761c52
    lines: 155-215
    snip: edabfbdf79f5
    anchor: "fn test_class_dynamic_instantiation_allocates_owned_reference_property_cells() {"
  - path: tests/codegen/eval_constructors.rs
    blob: 56498fa4722e805e8d9d7134ffc377a4522d4b9a
    lines: 373-402
    snip: e58e9206badf
    anchor: "fn test_eval_new_allocates_owned_reference_property_cells() {"
---

# A by-name allocation never gave an owned reference property its cell, so the property default stored through null

## Fact

`__rt_new_by_name` allocated an object whose OBJECT-OWNED REFERENCE property slots were left at
the zero its layout wipe put there, and the very first property default stored THROUGH one of
them. A SIGSEGV inside emitted code: no PHP-level diagnostic, no catchable error, the worker just
dies.

THE FALSE ASSUMPTION. A property that has a reference taken to it ANYWHERE in the program
(`$r = &$this->prop`, or a by-reference return) is promoted by `apply_reference_property_promotions`
into `owned_reference_properties`. Its slot then stops holding a VALUE and holds a POINTER to a
16-byte cell the object owns. Every access dereferences it — `emit_reference_property_write` emits

    ldr  x10, [x9, #<slot>]        ; the slot IS the cell pointer
    str  xzr, [x9, #<slot>+8]
    str  x0,  [x10]                ; the value goes in the CELL

The DIRECT allocator allocates one cell per such slot in `emit_object_allocation`, right after its
zero-fill. The BY-NAME allocator picks its layout from a class id known only at RUN time, so it
cannot see that list at its call sites: it zeroed the property region, restored the
typed-uninitialized markers from `_class_uninit_prop_*`, and then called `_class_propinit_<id>` —
whose body is exactly `$this->prop = <default>;` — with the cell pointer still zero.

Everything the direct allocator does after ITS zero-fill has to be reproduced by the by-name one
from a table. The uninit markers already were. The reference cells never were, and the exclusion
of owned-reference slots from `class_uninitialized_property_marker_offsets` shows the author knew
the slot was special and still left it empty.

## Fix

`_class_ref_prop_counts` / `_class_ref_prop_offset_ptrs` / `_class_ref_prop_offsets_<id>`, emitted
beside the existing `_class_uninit_prop_*` tables, plus a loop in `__rt_new_by_name` (both arches)
that allocates one 16-byte ZEROED cell per offset and stores the pointer in the slot, before the
propinit call. Zeroing is half the contract, not a nicety: a refcounted default runs
`release_previous_referenced_value` first, which loads whatever the cell holds and hands it to
`__rt_heap_free_safe`.

## Reach

Three allocators land on `__rt_new_by_name` and all three crashed, measured:
`new $cls()` falling through to the runtime fallback, an `eval()`-side `new` (the interpreter has
no layout of its own — this is the framework-container shape), and `unserialize()`. The fix also
repairs a reference property with NO default, whose first ordinary read would have dereferenced
the same null.

## The ingredient nobody guessed

Two adjacent defaulted bools are NOT enough: a six-variant probe and a matched control both print
beside php. The eval boundary alone is NOT enough either: the same two-bool class constructed
through `eval()` is clean. It takes the reference promotion AND a by-name allocation.

In Symfony the two meet in `DebugHandlersListener`: `private bool $hasTerminatedWithException =
false;` is promoted because `configure()` writes `$hasRun = &$this->hasTerminatedWithException;`,
and only the DEV container constructs the class, from an interpreted factory. Prod never
constructs it — which is why a whole-binary crash looked like an environment bug and drew two
wrong attributions to unrelated same-day changes.

## Also fixed in passing

The x86_64 half of the same helper read `_class_uninit_prop_counts` and
`_class_uninit_prop_offset_ptrs` with `emit_load_symbol_to_reg` — a `mov`, i.e. the table's FIRST
ENTRY — and then indexed that register as a table base. AArch64 uses `emit_symbol_address` at the
same two sites. Unverified on that target (the helper is not even emitted for a cross-target
`--emit-asm` probe); the read is unambiguous and the two arches now agree.

## Hazards for the next person

- `--emit-asm` writes the USER assembly only. The runtime helpers are assembled separately and
  cached as `runtime-v<ver>-...-heap<n>.o` under `$XDG_CACHE_HOME/elephc`, so grepping the emitted
  `.s` for `__rt_new_by_name` finds nothing and proves nothing.
- The by-name and direct allocators are two spellings of one contract with no shared code and no
  lint tying them together. `owned_reference_property_offsets` (codegen) and
  `class_owned_reference_property_offsets` (runtime data) are the same rule written twice because
  they read different halves of the compiler; the same is already true of the uninit markers.

## Why

3 new codegen tests green; mutation (cbz -> unconditional b) turns all three back into SIGSEGV; --lib 29 failures with comm empty in BOTH directions against the recorded baseline; the 25 objects/references sweep failures are IDENTICAL with the change fully neutralised

## The --web reduction, which reproduces the reported symptom exactly

A 25-line `--web` entry — the same two-bool class with the reference taken in a method,
constructed per request from `eval()` — is a framework-free stand-in for the whole Symfony
failure. Matched pair on one script, one server, 3 requests each:

    before (cbz -> unconditional b):  3/3 replies 0 octets, `elephc-web: worker N terminated by signal 11`
    after:                            3/3 `HTTP/1.1 200 OK`, 114 octets, `before=TF after=TT`, worker alive

php prints `before=TF after=TT`. Use this shape, not the Symfony app, to bisect the next
by-name-allocation crash: it costs one compile instead of two minutes and 1.4 GB of assembly.
