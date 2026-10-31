---
title: Tokenizer php-src phpt 54/54 and the three compiler fixes it needed
author: Guillaume
date: 2026-10-30
branch: feat/tokenizer-native-engine
cost: { minutes: 163, tokens: 710002 }
touches:
  - src/ir_lower/expr/unset.rs
  - src/codegen/lower_inst/objects/runtime_property_writes.rs
  - src/types/checker/schema/property_default_selectors.rs
  - crates/elephc-builtin-contract/src/mbstring_abi/ini/core.rs
  - src/tokenizer_prelude.rs
  - scripts/ci/run-tokenizer-phpt.py
---

The full php-src ext/tokenizer suite now passes 54/54 through `scripts/ci/run-tokenizer-phpt.py`.

Code fixes behind the last five:
- `unset($mixed->prop)` from a Mixed array element now dispatches through `UnsetPropertyAction::RuntimeClass` and the backend's Mixed runtime-class ladder. The const-string branch in `lower_dynamic_prop_unset_runtime` must keep the runtime ladder when the receiver is Mixed; folding to `lower_named_prop_unset` cannot resolve a runtime class.
- Property defaults are constant-bound for every class in `property_default_selectors::normalize`. A remaining `ConstRef` is an undefined constant, published as `deferred_property_default_error`, so object/static access throws `Error: Undefined constant "X"` at the PHP-observable point instead of silently materializing 0.
- `memory_limit` is now a shared Core INI directive. The tokenizer prelude reads it through the injected `ini_set` wrapper (read old value, restore it) and fatals before materializing an oversized token array.

Runner semantics:
- A trailing `?>` is replaced with `;` because elephc compiles PHP-only sources.
- A compile-time refusal is accepted for an EXPECTF that starts `Fatal error:` when the stable message matches (PHP itself fatals during compilation for cases such as overriding a final constructor).
- Fatal expectations compare the first fatal line only, because PHP fatal stack frames are not tokenizer output.

Verification:
- `python3 scripts/ci/run-tokenizer-phpt.py --tests <php-src>/ext/tokenizer/tests --elephc target/debug/elephc` → 54/54.
- `cargo test -p elephc-tokenizer-engine` → 12/12, including the 54-file PHP 8.5 differential corpus.
- `cargo test --test codegen_tests tokenizer` → 17/17.
- `cargo test --test parser_tests` → 530/530; tokenizer error suite 3/3; `cargo check` zero warnings.
