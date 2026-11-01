---
id: gotcha-a-by-reference-foreach-over-a-hash-yields-the-ce-aca9b70e
type: gotcha
title: "A by-reference foreach over a HASH yields the cell pointers when the value is read"
description: "Pre-existing miscompile found while compiling symfony/var-dumper; mutation works, reading does not"
tags: [ir_lower, arrays]
created: 2026-09-16
verified_by: "scratchpad/assocbyref.php and assocbyref2.php against php -n"
sources:
  - path: src/codegen_support/runtime/arrays/hash_ref_element.rs
    blob: be445e9354e8722ab9fdbf7a5e17c2dad4e00b11
---

# A by-reference foreach over a HASH yields the cell pointers when the value is read

## Fact

    $v = ['a' => 1, 'b' => 2];
    foreach ($v as $k => &$z) { $out[] = $k.'='.$z; }   // a=4303088304,b=4303088352

Every READ of the by-reference value variable answers the reference cell's ADDRESS instead of the
value it holds — `gettype($z)` still says integer, and `var_export`, concatenation and a plain copy
all take the pointer. WRITING through it is fine (`$z *= 10` updates the array), and an INDEXED
array is fine; only a hash source is affected.

Two codegen_tests already pin this and are part of the standing failure set:
`test_regression_642_by_ref_foreach_reference_hash_property_shared_with_another_owner` and its
`untyped_hash` twin. The same defect is behind
`$v = ['a'=>1]; $w = &$v; foreach ($w as $k => &$z)`.

It matters for Symfony: `ReflectionCaster::castFunctionAbstract` reads a by-reference value
(`is_object($v)`) while walking `getStaticVariables()`, which is a hash.
