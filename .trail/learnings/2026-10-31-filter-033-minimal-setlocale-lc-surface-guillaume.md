---
title: "filter 033: minimal setlocale/LC_* surface + implicit array creation"
author: Guillaume
date: 2026-10-31
branch: feat/filter-php-src-compliance
touches:
  - crates/elephc-builtin-contract/src/core_constants.rs
  - crates/elephc-builtin-contract/src/catalog_constants.rs
  - crates/elephc-builtin-contract/src/catalog_data.rs
  - src/locale_prelude.rs
  - src/lib.rs
  - src/main.rs
  - src/pipeline.rs
  - tests/codegen/support/compiler.rs
  - tests/error_tests.rs
  - src/builtins/parity_tests.rs
  - src/ir/runtime_call.rs
  - src/codegen/lower_inst/runtime_calls.rs
  - src/ir_lower/stmt/array_write_storage.rs
  - src/ir_lower/stmt/array_write_core.rs
  - src/types/checker/stmt_check/assignments/arrays.rs
  - scripts/filter/port_phpt.py
  - tests/codegen/arrays/autovivification.rs
  - tests/codegen/system.rs
  - tests/error_tests/misc/system_builtins.rs
  - docs/php/arrays.md
  - scripts/docs/elephc_builtins/extract.py
  - scripts/docs/audit_builtins.py
---

Restored 033.phpt to its original source by adding the seven target-aware LC_* constants (Apple and glibc category numbers differ) and a PreludeProvided setlocale(int, mixed, ...mixed) function. The function accepts PHP's C/POSIX locale names, tries variadic candidates left to right, returns the matched name, and reports false for unknown locales or categories outside 0..6. The second half of the fix was PHP's implicit array creation: $undefined[$k] = $v and $undefined[] = $v now start an indexed array (hash for string keys), with a runtime AutoVivifyArray/AutoVivifyHash guard that allocates only for a zero slot and retains an existing container. Publishing happens after key/value evaluation, so a throwing operand leaves the variable undefined, and a loop body no longer resets the array created by an earlier iteration. 033 now compiles and matches its php-src expectations. setlocale does not yet call libc (tracked as elephc-rc0); null-base and zero-iteration isset edges are tracked as elephc-o7v.
