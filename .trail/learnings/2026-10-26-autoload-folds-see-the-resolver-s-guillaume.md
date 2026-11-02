---
title: "Autoload folds see the resolver's if, not the ternary include"
author: Guillaume
date: 2026-10-26
branch: reconcile/dirname-symfony
touches:
  - src/resolver/engine.rs
---

## Decided
- `require COND ? 'a.php' : 'b.php';` is rewritten by the resolver (`ternary_include_as_if`, src/resolver/engine.rs) into `if (COND) { require a } else { require b }` with both files spliced BEFORE `fold_file_scope_existence_conditions` runs. A fold over the include must match that `If` shape too (Carbon Translator's `$m->hasReturnType()` choice: `fold_reflection_require_ternaries`).
- Existence probes in autoload folds are kind-aware: `class_exists()` of an interface/trait is false in php. `existence_condition` takes a `kind_ok(function, class)` answered by `autoloaded_declaration_kind` from the class file; unknown kind falls back to availability.
- `$n = &static::$x` for a LOCAL is supported (borrowed alias of the static's storage via `lower_ref_assign_static_property`). Property/element targets are refused by name.

## Tried and ruled out
- Binding an object property to a static via `BindPropRefCell(LoadStaticPropertyRefCell)`: the static "cell" is the symbol address, not a heap ref cell; layouts are not guaranteed equal. Needs statics stored as heap cells (bead elephc-a03.1).

## Watch out
- A fixture where `class_exists(SomeInterface::class)` picks the branch is wrong: php says false. Value-check fixtures with php before trusting them.
