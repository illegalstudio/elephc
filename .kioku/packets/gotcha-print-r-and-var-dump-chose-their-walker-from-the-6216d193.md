---
id: gotcha-print-r-and-var-dump-chose-their-walker-from-the-6216d193
type: gotcha
title: "print_r and var_dump chose their walker from the static type, and a bare array lies"
description: "A declared bare array can be hash storage at run time; both dumpers called the indexed walker unconditionally and printed invented integer keys"
created: 2026-09-20
sources:
  - path: src/codegen/lower_inst/builtins/debug.rs
    blob: 05eb197d6c3dd0dcd1787cb8306d71e3f504b50f
---

# print_r and var_dump chose their walker from the static type, and a bare array lies

## Fact

A declared bare `array` is a REPRESENTATION contract, not a PHP type: the same value can be
indexed storage or hash storage at run time, and the generic contract exists so a callee does
not have to care. `print_r` and `var_dump` both matched `PhpType::Array(_)` and called the
indexed walker unconditionally, so a hash arriving through that contract was walked as an
indexed array -- silently:

    class Maker { public static function passthrough(array $a): array { return $a; } }
    print_r(Maker::passthrough(['untouched' => 0]));
    // php:    Array ( [untouched] => 0 )
    // elephc: Array ( [0] => 4 )

`var_export`, `json_encode`, `foreach`, `count`, `implode` and `array_keys` were all CORRECT on
the same value. That is what scopes the defect to the two walker selections rather than to the
value, and running all of them side by side is the fastest way to scope the next one of these.

Fixed by asking `__rt_heap_kind` (3 = hash storage) before choosing, the idiom
`lower_inst/arrays.rs` already uses for the same reason. A concrete `AssocArray` still goes
straight to the hash walker and a concrete element-typed `Array` keeps its specialised indexed
walker, so nothing already unambiguous pays for the branch.

Attribution: `print_r`, `var_dump`, `var_export`, `print_r_object`, `var_dump_nested` and
`var_dump_object` are all clean after the fix; `codegen_tests arrays` (30 failures),
`print_r_return_mode_heap` (1) and `var_export_and_strstr_result` (3) fail identically with the
file reverted to HEAD, so they are the standing baseline.

## Why

Silent wrong output on ordinary PHP: an array returned from a function is the common case
