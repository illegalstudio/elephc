---
id: bug_fix-an-empty-array-function-static-lost-its-data-arr-5ebc4ecb
type: bug_fix
title: "An empty-array function static lost its data: array<never> plus an Opaque receiver"
description: "static q = [] kept array<never>, whose zero-width element slots make array_shift preserve nothing, while a static receiver resolved to Opaque whose write-back is a silent no-op, so growth republished nothing. Whole arrays vanished, not just return values. Fixed with array<mixed> element storage plus a StaticLocal receiver place; a wider Mixed-slot fix was tried and rejected because it destroys sort"
created: 2026-09-21
verified_by: "Six-case fixture plus controls beside php 8.5.10; three one-line mutations; leak slope at 10 vs 1000 iterations against a matched control; shutdown prelude simplified back to array_shift with 11/11 green"
sources:
  - path: src/ir_lower/context.rs
    blob: e4e7d96e8641d28091d77cc3c33fc7fb3dc89cd1
  - path: src/codegen/lower_inst/receiver_place.rs
    blob: 9895adb2ab329dbb987275465f548345fa692209
  - path: tests/static_local_array_tests.rs
    blob: babc025a262d0e71f5102fddab64144df5008572
    lines: 89-113
    snip: b4c0f8b92b33
    anchor: "fn compile_and_run(source: &str, stem: &str) -> String {"
supersedes: "gotcha-array-shift-on-a-function-static-loses-the-value-e76fcc18"
---

# An empty-array function static lost its data: array<never> plus an Opaque receiver

## Fact

RESOLVED, and the damage was far worse than the first two characterizations of it. A function
`static` array declared `static $q = [];` lost data outright: three pushed strings then one
`array_shift` left `ret=NULL left=[]` where php gives `ret='aa' left=["bb","cc"]`; nine ints
pushed with NO shift at all counted `0` instead of `9`; a float came back as its bit pattern read
as an int.

TWO INDEPENDENT DEFECTS, both "storage that outlives the call, typed or published as if it did
not":

1. THE TYPE. `static $q = [];` runs its initializer on the first call only, but the declaration
   re-types the name on EVERY call, so the slot kept `array<never>` — not a narrow array type but
   the claim *this array has no elements*. `Never` element slots are ZERO WIDTH and codegen acts
   on it: the "preserve the removed payload" arm of `array_shift`/`array_pop` is literally
   `mov x11, #0`. Hence NULL. Worse, the `array<never>` compaction loop walks at 8-byte stride
   while `__rt_array_push_str` has already re-stamped the runtime header to 16-byte string slots.
2. THE PLACE. A `static` lives in a `.comm` symbol, not a frame slot, and every backend resolver
   for "somewhere a relocated container can be republished" matched only `LoadLocal`/`LoadRefCell`.
   `ReceiverPlace::resolve` answered `Opaque`, whose write-back is `Ok(())` — a SILENT drop, since
   only the growth paths call `require_writable`. After `__rt_array_grow` reallocated, the symbol
   kept the old, freed pointer.

THE MEASUREMENT TRAP THAT PRODUCED TWO WRONG CHARACTERIZATIONS OF THIS BUG. A fixture pushing
exactly two one-character strings fits the rescaled capacity with no realloc, so defect 2 never
fires and only the NULL return shows. From that fixture I concluded "the mutation is correct,
only the returned value is lost" — wrong, and it hid the bigger half. An earlier report concluded
"only statics whose elements are arrays" — also wrong, scalars fail too. WHEN PROBING A STORAGE
DEFECT, VARY THE ELEMENT COUNT PAST THE INITIAL CAPACITY AND VARY THE ELEMENT TYPE, or the probe
measures the capacity rather than the bug.

THE FIX: give an empty-array static `array<mixed>` storage (boxed ELEMENTS) gated on a body scan,
plus `ReceiverPlace::StaticLocal` and a `LoadStaticLocal` branch in the write-back.

A WIDER FIX WAS TRIED AND REJECTED ON MEASUREMENT: widening the whole SLOT to `Mixed` (as
`global` does) fixed more, including `array &$param` write-back — but it puts a Mixed cell in the
`.comm` symbol, and a `mixed &$param` argument is handed that cell as its reference and replaces
it, so `sort($staticQueue)` went from a stale answer to NULL, i.e. destroyed. Do not redo it; the
gate in `required_static_local_storage_type` records why.

REPAIRED AS A SIDE EFFECT: `array_unshift` (which was a hard COMPILE REFUSAL, the loud half of the
same `Opaque` hole), `array_push`, `array_splice`, `current`/`next`/`end`/`reset`, `array_slice`,
`foreach`, and growth for a seeded static.

STILL WRONG, unchanged by this fix: `sort`/`rsort`/`usort` on a static drop the reordering,
`unset($q[0])` empties it, and a STRING-KEYED static loses writes and reads garbage. The common
cause is that PASSING A STATIC TO A BY-REFERENCE PARAMETER IS UNIMPLEMENTED — the callee gets the
value, not a place; a scalar `static $n = 0;` through `int &$n` silently stays 0 where php gives
10. The fix is to give a static the ref-cell treatment a `global` already gets
(`global_ref_cell`, `GLOBAL_REF_CELL_HEAP_KIND`). That is the next real piece of work here.

`array<never>` IS THE GENERAL SHAPE, not a static-only problem: the same trap is described for a
loop-carried accumulator in `check_list_unpack` (`foreach ($x as [$a,$b])` reading every element
as null), and `src/opcache_prelude/build.rs:570` carries a workaround seeding a dummy entry
because "the EIR backend rejects `static $s = [];`".

## Why

A two-element fixture fits the initial capacity, so it shows only the NULL return and hides the data loss. Two successive characterizations of this bug were wrong for exactly that reason -- vary the element COUNT past capacity and the element TYPE, or the probe measures the capacity.
