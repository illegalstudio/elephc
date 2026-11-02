---
title: Rebase onto main 2026-11-02 — semantic merge traps that compiled cleanly
author: Guillaume
date: 2026-11-02
branch: feat/closed-world-symfony-build-speed
touches:
  - src/ir_lower/expr/native_isset.rs
  - src/ir_lower/stmt/declarations.rs
  - src/ir_lower/expr/call_arg_coercion.rs
  - src/codegen/lower_inst/method_dispatch.rs
  - src/codegen/lower_inst/objects/throwable_new.rs
  - crates/elephc-builtin-contract/src/support.rs
  - crates/elephc-magician/src/interpreter/builtins/core/unserialize.rs
---

## Decided
- Each side fixed some ownership or dispatch rule in its own place. Textual merges then applied
  both fixes, and only tests, PHP output and Symfony parity caught it.
- `isset($a[$k])` released its receiver twice: main inside the probe, the branch in
  `lower_native_isset_offset_probe_from_value`. The caller owns that release.
- The branch spells the bare `object` type as `Object("")`. Main's `Class|false` argument guard
  then tested `instanceof ""` and refused every object. The guard now skips an empty class name.
- Main's `static_receiver_declares_method` looked only at the receiver's own method table. It
  missed two cases:
  - an inherited private method (Symfony `CompiledUrlMatcherTrait::doMatch()`, so every 404 became
    a 500);
  - a static method called through `->`.
  It now walks the parents and counts static methods.
- The branch publishes call sites as ONE `php_site` record pointer. Main code that read or wrote
  `_php_diagnostic_line`/`_file` directly needs to do one of two things:
  - load `[[php_site] + 8]`;
  - call `__rt_diag_resolve_site` first, which preserves every register.
- After a function `static`, main set the local fact to the initializer's type (`array<never>`),
  overwriting the branch's hash declaration for a string-keyed `static $m = []`. A nested
  `$m[$k][$i] ??= ...` then emitted `ArrayToHash` on the boxed slot and failed EIR validation
  (Twig's `getAttribute()` property-checker cache). The fact is now the slot's declared type.
- In eval, `serialize`/`unserialize` are `Adapter { runtime_builtin: None }`, not shared-runtime.
  A hybrid adapter still takes the shared helper whenever the arity fits, and that helper cannot
  see eval-declared classes. Legacy `C:` payloads are now parsed in eval: GMP's hidden
  `__elephc_legacy_unserialize` hook is found through the class contract, because
  `method_exists()` hides it.

## Tried and ruled out
- `purge_property_narrowings_after_call` keeping the narrowed-`$this` key: not the cause of the
  Symfony 404 failure, reverted as unproven.
- Building eval's `__PHP_Incomplete_Class` with `new_object`: the checker now declares the name,
  but the object it builds has no property surface. Kept the `stdClass` stand-in, a known
  divergence.

## Watch out
- Builtin-contract count asserts: predict `branch + (main − base)` per counter. A remaining gap
  of 2 meant both sides had made the same two names eval-supported, not a lost binding.
- Regenerate the four doc generators after every contract change. `gen_php_comparison.py` needs
  the `tomllib` shim on this machine.
