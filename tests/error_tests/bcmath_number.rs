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
