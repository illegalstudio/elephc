---
title: GMP eval/Magician parity wiring
author: Guillaume
date: 2026-10-27
branch: feat/gmp-extension
cost: { minutes: 95, tokens: 381424 }
touches:
  - ".config/nextest.toml"
  - ".github/workflows/ci.yml"
  - Cargo.lock
  - Cargo.toml
  - README.md
  - crates/elephc-builtin-contract/src/catalog_classes.rs
  - crates/elephc-builtin-contract/src/catalog_constants.rs
  - crates/elephc-builtin-contract/src/catalog_data.rs
  - crates/elephc-builtin-contract/src/class_spec.rs
  - crates/elephc-builtin-contract/src/lib.rs
---

GMP eval is a forwarding layer, not a second implementation. Register each hidden __elephc_gmp_* prelude helper with the eval bridge (function_has_eval_metadata must allow exactly the 51 public suffixes), then a single EvalValuesHook::Gmp prepends the compiler-owned bool $strict=false and calls the helper through bind_evaluated_native_function_args. Eval fragments are weak-mode, so strict is always false.

Three compiler-side traps surfaced:
1. capture_all_frames (eval/debug_backtrace) appends a hidden __elephc_func_args collector to every prelude helper. emit_user_call must append that ABI slot for builtin->prelude calls, and synthetic builtin callable wrappers need the module signature map so the same lowering sees it.
2. The eval native ABI type-spec parser did not recognize the 'false' literal, so GMP|false became Class("false") and gmp_invert() returning false failed RuntimeFatal. Map false/true to Generic(atom).
3. Callable descriptor generation must skip GMP builtins when the prelude is absent; otherwise any eval host that never linked GMP fails codegen with 'call to unknown function __elephc_gmp_abs'.

Magician operator dispatch needed explicit object paths for bitwise/shift (ObjectBitwise/ObjectShift from the class contract), ~ as -x-1, and array_sum/array_product through __elephc_to_number. sort() needs an insertion-sort comparator that calls spaceship for object entries. var_dump/print_r call __debugInfo for native objects; get_object_vars/(array)/JSON/serialize omit the compiler storage property __elephc_num via ClassContract.hidden_properties.

Still blocked: eval serialize()/unserialize() (elephc-qn9), internal adapter methods visible to reflection (elephc-ag4), eval-declared operand class names in diagnostics (elephc-2ic).
