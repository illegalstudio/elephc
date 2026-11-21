---
title: Null static locals narrowed when every write is one class
author: Guillaume
date: 2026-11-15
branch: fix/null-local-narrowing
cost: { minutes: 41, tokens: 170192 }
touches:
  - examples/web-apm/main
  - examples/web-apm/main.key
  - examples/web-apm/main.s
  - src/types/checker/driver/init.rs
  - src/types/checker/loop_storage.rs
  - src/types/checker/mixed_storage_scan.rs
  - src/types/checker/mod.rs
  - src/types/checker/stmt_check/control_flow.rs
  - src/types/checker/stmt_check/narrowing/conditions.rs
  - tests/codegen/types/narrowing.rs
---

## Decided
- A `static $x = null;` lazily initialized on its null arm narrows to the object type after the `if` only when every write the function makes to the static is `new` of one class (`single_class_written_locals` in src/types/checker/loop_storage.rs, recorded per body by the mixed-storage scan; checked by `static_local_keeps_class` in stmt_check/narrowing/conditions.rs). Once set it stays that class, so the `else` path of a later call and a re-entrant call between guard and read are both harmless.
- Statics stay excluded from `local_binding_is_unaliased`: the new predicate reuses its alias checks (`binding_has_no_hidden_writer`) without the static exclusion, and adds `body_contains_eval`.

## Tried and ruled out
- Treating the static's `else` path as dead like a plain local's: wrong, a later call takes it with the object already set.
- Purging the narrowing after calls, as for static properties: not needed once all writes are one class, and too weak otherwise (the reset can sit in any re-entrant call).

## Watch out
- `eval` cannot read or write a static local at all (elephc-bx7t): the eval scope sync inventory (codegen/lower_inst/builtins/eval/sync_inventory.rs) keeps only `LocalKind::PhpLocal`, and statics live in `.comm` symbols.
- `new Other()` elsewhere in the body compiles through another path and runs correctly; it is not a counterexample to test.
