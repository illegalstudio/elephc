---
id: bug_fix-a-by-reference-capture-whose-closure-writes-it-i-b2f8f873
type: bug_fix
title: "A by-reference CAPTURE whose closure writes it is already boxed, so it may be retyped"
description: "local_binding_is_widenable refused every ref_bound local, including the one shape expr::closures has already widened to Mixed, costing a diagnostic on Symfony RedisTrait's preg_match-then-assign"
created: 2026-09-17
verified_by: "Reduction prints the closure write, the matches capture and the final string in sequence byte-identical to php -n, with =& and by-ref foreach controls still bound; error_tests at its 99-failure baseline, identical names"
sources:
  - path: src/types/checker/mod.rs
    blob: 1970f01aa81b6d51314f13863a8ebe73032e2a43
  - path: src/types/checker/inference/ops.rs
    blob: 91de9e4ffa126388f90c4ad1b3165ecb4fcd124a
  - path: src/ir_lower/expr/closures.rs
    blob: 8d3836d616cbe357631227ea0321ada5889f8c7e
supersedes: "gotcha-a-local-written-through-a-by-ref-argument-cannot-ab92b480"
---

# A by-reference CAPTURE whose closure writes it is already boxed, so it may be retyped

## Fact

`local_binding_is_widenable` refuses every name in `ref_bound_locals`, because re-representing a
slot another name reaches would break the alias. Right for `$a =& $b` and for a by-reference
`foreach`; wrong for a `use (&$x)` capture whose closure body WRITES `$x`, because
`expr::closures` reacts to exactly that pair with

    } else if by_ref && body_writes_local(body, capture) {
        ctx.set_local_type(capture, PhpType::Mixed);

— a PHP reference has no type, so the shared cell must already hold anything the closure stores.
The cell is boxed before the checker ever refuses.

Symfony's `RedisTrait` line 332: an error handler captures `$error` by reference, `preg_match`'s
by-reference third argument writes the matches array into the same local, the ternary reads
`$error[1]`, and the assignment stores a string — `cannot reassign $error from array<mixed> to
string`.

`Checker::by_ref_capture_boxed_locals` records the subset, using the SAME predicate
(`ir_lower::body_writes_local`, widened to `pub(crate)`) on the same body and name so the two
cannot drift. Excluded: the recursive self-binding `$f = function () use (&$f) {…}`, whose capture
lowering takes `ref_cell_capture_storage_type` and does not box; and any name a `=&` or a
by-reference `foreach` also binds — those sites remove it, so the stricter binding wins.

## Why

The checker was stricter than the storage it was protecting
