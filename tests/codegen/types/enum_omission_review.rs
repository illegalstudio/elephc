//! Purpose:
//! Distinguishes trailing enum argument shortages from named holes during runtime unpack.
//!
//! Called from:
//! - The codegen types integration suite.
//!
//! Key details:
//! - Dynamic arrays prevent static spread expansion from hiding the selected enum binder.
//! - Later supplied regular parameters require a hole message; trailing omissions require counts.

use crate::support::*;

/// Named prefixes use count diagnostics while later supplied parameters keep named-hole errors.
#[test]
fn test_enum_omission_review_trailing_and_holes() {
    let out = compile_and_run(r#"<?php
interface OmissionContract { public function f(int $a, int $b, int $c = 3): int; }
enum OmissionValue implements OmissionContract {
    case A;
    public function f(int $a, int $b, int $c = 3): int { return $a + $b + $c; }
}
function invokeOmission(OmissionContract $value, array $args): string {
    try { return (string)$value->f(...$args); }
    catch (ArgumentCountError $error) { return $error->getMessage(); }
}
echo invokeOmission(OmissionValue::A, ['a' => 1]), '|';
echo invokeOmission(OmissionValue::A, ['b' => 2]), '|';
echo invokeOmission(OmissionValue::A, ['a' => 1, 'c' => 3]), '|';
echo invokeOmission(OmissionValue::A, [1]), '|';
echo invokeOmission(OmissionValue::A, []);
"#);
    assert_eq!(out, "Too few arguments to function OmissionValue::f(), 1 passed and at least 2 expected|OmissionValue::f(): Argument #1 ($a) not passed|OmissionValue::f(): Argument #2 ($b) not passed|Too few arguments to function OmissionValue::f(), 1 passed and at least 2 expected|Too few arguments to function OmissionValue::f(), 0 passed and at least 2 expected");
}

/// Omission diagnostics retain the same distinction without EIR optimization.
#[test]
fn test_enum_omission_review_trailing_and_holes_no_ir_opt() {
    without_ir_opt(test_enum_omission_review_trailing_and_holes);
}
