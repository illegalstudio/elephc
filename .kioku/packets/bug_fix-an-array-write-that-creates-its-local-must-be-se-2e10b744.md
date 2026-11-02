---
id: bug_fix-an-array-write-that-creates-its-local-must-be-se-2e10b744
type: bug_fix
title: "An array write that CREATES its local must be seeded at scope entry, on both sides"
description: "Why $keys[] = $k against an unassigned name compiled to a null local, and why the seed cannot live at the write"
tags: [checker, ir_lower, symfony]
created: 2026-09-16
verified_by: "scratchpad/vivwide.php, appendviv, viv2..viv7 byte-identical against php -n; symfony var-dumper StubCaster now checks clean"
sources:
  - path: src/append_vivify.rs
    blob: 9085397528a17249d310a223fb18a8dfdbd40f79
  - path: src/ir_lower/function.rs
    blob: 113878ac2dfbf17ed7e8f9428abe51482f94e825
  - path: src/types/checker/stmt_check/assignments.rs
    blob: 80a5d3086c99ec8b3fdeb5bb06d996e3e05aa308
---

# An array write that CREATES its local must be seeded at scope entry, on both sides

## Fact

PHP auto-vivifies on `$keys[] = $k` / `$rows['id'] = $v` when nothing assigned the name yet: the
write CREATES the array. elephc got this wrong in two directions at once.

The checker refused it — `Undefined variable: $keys` — EXCEPT when a suppression path
(`can_suppress_stale_undefined_variable_errors`) happened to swallow the diagnostic. Where it was
swallowed the program compiled and EIR lowering never created the storage, so the local stayed
NULL: `implode(',', $keys)` died with "must be of type ?array, null given". A silent miscompile
hiding behind an error-suppression rule.

The fix is ONE scan, `crate::append_vivify::vivified_array_locals`, used by both sides:
- `Checker::seed_vivified_array_locals` binds each name to `Array(Never)` in the scope's entry
  environment (function signature resolution, method pass, closure body, top level).
- `ir_lower::function::seed_vivified_array_locals` stores an empty array into the same names in
  the still-open entry block.
Passing the environment's own keys as the "already bound" set is what keeps them in step: params,
superglobals and a closure's enclosing locals are all in there already.

Two things this had to get right:
- The seed is at scope ENTRY, not at the write. The write is normally inside a loop, and seeding
  there resets the array every iteration. It also has to be at entry for a SECOND reason:
  `stabilize_loop_storage` only records a loop-carried storage contract for names its entry
  environment already holds, and without that contract the slot is read at its pre-widening
  representation on iteration 2 — a segfault, not a wrong value.
- Only a name whose FIRST binding is such a write qualifies. `$a = f(); … $a[] = 1;` already has
  storage and an entry store of `[]` would retype it.
