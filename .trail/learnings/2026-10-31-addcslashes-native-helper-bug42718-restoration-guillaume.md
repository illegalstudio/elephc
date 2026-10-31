---
title: addcslashes native helper + bug42718 restoration
author: Guillaume
date: 2026-10-31
branch: feat/filter-php-src-compliance
touches:
  - src/codegen_support/runtime/strings/addcslashes.rs
  - src/codegen_support/runtime/strings/mod.rs
  - src/codegen_support/runtime/emitters.rs
  - src/codegen_support/runtime/data/fixed.rs
  - src/ir/runtime_fn.rs
  - src/builtins/string/addcslashes.rs
  - crates/elephc-builtin-contract/src/catalog_data.rs
  - crates/elephc-magician/src/interpreter/builtins/string/addcslashes.rs
  - crates/elephc-magician/src/interpreter/builtins/hooks/direct.rs
  - crates/elephc-magician/src/interpreter/builtins/hooks/values.rs
  - tests/codegen/strings/encoding.rs
  - tests/error_tests/misc/string_and_type_builtins.rs
  - scripts/filter/port_phpt.py
---

addcslashes is implemented as a target-aware runtime helper (ARM64 and x86_64) that builds a 256-byte escape table on the stack, expands ascending ranges, treats descending ranges as literal bytes plus emits one warning per rejected range, uses PHP's short C escapes for bell/backspace/tab/newline/vtab/formfeed/carriage return, octal for other non-printables, and a backslash for printable bytes. The ARM mask table must store boolean 1 markers, not the character value: storing the byte value made NUL (0) indistinguishable from an unmarked entry. The builtin is a normal registry runtime function with a Magician direct/value binding. bug42718.phpt now uses its original addcslashes source; its XFAIL expectation is recorded as the actual 1\000 output. The ini_get filter.default_flags substitution remains for chv.11.
