---
id: gotcha-php-evaluates-every-dim-operand-of-isset-read-ch-dc2db5b2
type: gotcha
title: "PHP evaluates every dim operand of isset/read chains even past a null level"
description: "nullable receiver lowering skipped the index expression, so isset($a[$x][$c = f()]) never assigned $c when $a[$x] was missing"
created: 2026-09-26
sources:
  - path: src/ir_lower/expr/array_access.rs
    blob: 7cbe98d1659c4e6c54705044bce0338c8f1ba0a2
  - path: src/ir_lower/expr/native_isset.rs
    blob: 9e8628ca416d2fc64502a4d25e3c8c5e7817e05a
---

# PHP evaluates every dim operand of isset/read chains even past a null level

## Fact

`lower_nullable_array_access` and `lower_nullable_native_isset_offset_probe` branched on the null receiver BEFORE evaluating the index, and the null edge never lowered it. PHP evaluates all dim operands first (FETCH_DIM_IS on null yields null), so side effects in the index -- assignments, calls -- always happen. Fix: `lower_skipped_index_for_effects` lowers a side-effecting index (by `optimize::expr_has_side_effects`) on the null edge and discards it with the expression-statement rule (release only an owning temporary: an assignment's value is the local's own storage, and releasing it freed `$class`).
Separate, still open: `isset($a[$e='k'], $a[$g='m'])` then `isset($g)` answers true for a variable only assigned in a short-circuited operand.

## Why

PhpDumper::generateProxyClasses reads $class right after isset($alreadyGenerated[$g][$class = ...][...]); cache:clear crashed
