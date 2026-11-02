---
id: bug_fix-an-eval-return-of-a-static-property-or-class-con-b01c6cfa
type: bug_fix
title: "An eval return of a static property or class constant handed the caller the class's own cell"
description: "EvalStmt::Return treated every non-LoadVar result as owned; a StaticPropertyGet/ClassConstantFetch is the cell the class still holds, so the caller's reassignment freed it"
created: 2026-09-26
sources:
  - path: crates/elephc-magician/src/interpreter/statements/dispatch.rs
    blob: 9511de05abaeb2faa882f8d81570d6ba1d11a1fc
  - path: crates/elephc-magician/src/interpreter/expressions.rs
    blob: dae145f3da8984be6aac2e2458fb498365423a42
---

# An eval return of a static property or class constant handed the caller the class's own cell

## Fact

Shape: `public static function get() { return self::$enc; }` then `$e = self::get(); $e = "x";`.
The static read returns the stored cell unretained (eval_static_property_value_or_default hands
out context.static_property()). Return then passed it on as owned, and the local's reassignment
released it: the static read back as another allocation's bytes ("0", a pointer-sized int, or a
tag the return-type check spells "mixed").

In Symfony: Mbstring::mb_str_split does `$encoding = mb_internal_encoding();` then
`$encoding = self::getEncoding($encoding)`; the next mb_internal_encoding() returned "0" and
mb_strlen() threw `must be a valid encoding, "0" given`.

Fix: Return retains when every value the expression can produce is a class-held member cell
(expr_returns_shared_member_cell); mixed ?: / ?? are left alone on purpose (retaining a fresh
side leaks, a local side is transferred by the scope release).
Pin: tests/codegen/eval.rs test_eval_returned_static_property_survives_the_callers_reassignment.
Diagnostic that found it: a state value changing with NO setter call in ELEPHC_EVAL_TRACE
(static_method_dispatch lines) means a freed cell, not a write.

## Why

It silently corrupted symfony/polyfill-mbstring's internal encoding to "0" and killed debug:container --parameters; the symptom appears far from the cause
