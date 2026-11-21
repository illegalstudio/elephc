---
title: "Refusals php only decides at run time, and the erasure re-run gate"
author: Guillaume
date: 2026-11-21
branch: bead-refusals
touches:
  - src/docblock/erasure.rs
  - src/docblock.rs
  - src/types/checker/functions/returns.rs
  - src/types/checker/stmt_check/control_flow.rs
  - src/types/checker/stmt_check/foreach_bindings.rs
  - src/types/checker/stmt_check/assignments/static_properties.rs
  - src/types/checker/inference/expr/effects.rs
  - src/types/checker/schema/validation.rs
  - src/ir_lower/stmt/return_coercions.rs
---

## Decided
- The 13 refusals that started the Demo's two erasure re-runs were GENERAL checker gaps, not PHPDoc substitution (only the TableEditor one was doc-derived). Each is now compiled and decided at run time like php: a nullable local returned from a non-null declaration (`guard_nullable_scalar_return` raises the TypeError for `string`/`float`/`bool`, the union guard already did for `array|bool`), a `mixed` store into a typed static property, a local a closure writes through `use (&$x)` (checker view becomes `mixed` once the closure exists), `list<T>` in a docblock is `array`, the `__toString` override through an interface that only extends `Stringable` (`method_contract_is_source` follows an interface owner to the interface that really declares the method), and an `if`/`else` whose arms both retype a local no longer joins the entry type (`exclude_target_entry_exit` when there is an `else`; the foreach value binding is an entry proof like a parameter).
- `rerun_erasing` is gated: `implicated_files` ignores a refusal whose member, quoted member, declaration or named class/function no annotation changed (`docblock::erasure::Changed`, recorded by `apply_for_file` as it diffs declarations and methods before/after adoption). `erasure_rerun_skipped` in the `ELEPHC_GENERICS_STATS` line counts those skips.

## Watch out
- The static property TypeError says "Cannot assign int to property of type Exception" (no `L::$e`): the nominal guard immediate carries no owner. Return/parameter nominal guards also omit the function name. Message parity only; the TypeError itself is raised.
- `return null;` from a `: string` function is still a compile-time refusal (php raises it at run time).
- Old error tests asserting "return type expects int, got int|null" for a plain local describe the removed refusal; php raises a TypeError at run time there.
