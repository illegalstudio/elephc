//! Purpose:
//! Checks Number argument, final-class, and unsupported operator diagnostics.
//!
//! Called from:
//! - The error integration test binary with the same prelude injection as compilation.
//!
//! Key details:
//! - Userland classes do not acquire internal operator handlers by declaring similar methods.

use super::*;

/// The constructor requires its numeric operand and methods enforce their parameter counts.
#[test]
fn test_error_bcmath_number_arities() {
    expect_error("<?php new BcMath\\Number();", "argument");
    expect_error("<?php $n = new BcMath\\Number(1); $n->add();", "argument");
    expect_error("<?php $n = new BcMath\\Number(1); $n->sqrt(1, 2);", "argument");
}

/// Number remains final and rejects array method arguments and unrelated object arithmetic.
#[test]
fn test_error_bcmath_number_types_and_finality() {
    expect_error("<?php class Invalid extends BcMath\\Number {}", "final");
    expect_error("<?php $n = new BcMath\\Number(1); $n->add([1]);", "parameter $num expects");
    expect_error("<?php class Other {} $n = new BcMath\\Number(1); echo $n + new Other();", "numeric operands");
}

/// A user method with an internal adapter's spelling never enables arithmetic operators.
#[test]
fn test_error_bcmath_number_operator_contract_is_internal() {
    expect_error("<?php class Other { public static function __elephc_add(mixed $a, mixed $b): int { return 1; } } $n = new Other(); echo $n + 1;", "numeric operands");
}

/// A scalar union has no definite Number result: `+ - * **` are refused, while `/` and `%`
/// yield `mixed`, which the declared `BcMath\Number` return checks at run time as PHP does.
#[test]
fn test_error_bcmath_number_union_arithmetic_has_no_definite_object_result() {
    for op in ["+", "-", "*", "/", "%", "**"] {
        for expression in [format!("$value {op} 1"), format!("1 {op} $value")] {
            let source = format!("<?php function unionNumber(BcMath\\Number|int $value): BcMath\\Number {{ return {expression}; }} unionNumber(5);");
            if matches!(op, "/" | "%") {
                check_source(&source).unwrap_or_else(|error| panic!("{source}: {error}"));
            } else {
                expect_error(&source, "numeric operands");
            }
        }
    }
    expect_error("<?php function unionNumber(BcMath\\Number|int $value): BcMath\\Number { return -$value; } unionNumber(5);", "Cannot negate a non-numeric value");
    for expression in ["++$value", "--$value", "$value++", "$value--"] {
        expect_error(&format!("<?php function unionNumber(BcMath\\Number|int $value) {{ return {expression}; }} unionNumber(5);"), "Cannot increment/decrement");
    }
}
