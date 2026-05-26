//! Purpose:
//! Integration or regression tests for diagnostic coverage of misc additional math diagnostics, including compound assignment missing rhs, compound assignment rejects append target, and instanceof missing class name.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Invalid PHP snippets are checked through shared diagnostic helpers for messages, spans, and recovery behavior.

use super::*;

#[test]
// Tests that compound assignment operators (`**=`, `&=`, `|=`, `^=`, `<<=`, `>>=`) produce an error when the RHS is missing.
fn test_error_compound_assignment_missing_rhs() {
    for src in [
        "<?php $x **=;",
        "<?php $x &=;",
        "<?php $x |=;",
        "<?php $x ^=;",
        "<?php $x <<=;",
        "<?php $x >>=;",
    ] {
        expect_error(src, "Unexpected token");
    }
}

#[test]
// Tests that array-append target (`$items[]`) is rejected as an invalid compound assignment target.
fn test_error_compound_assignment_rejects_append_target() {
    expect_error("<?php $items = [1]; $items[] += 2;", "Invalid assignment target");
}

#[test]
// Tests that `instanceof` with a non-class RHS (integer literal) produces the expected error.
fn test_error_instanceof_missing_class_name() {
    expect_error(
        "<?php class A {} $a = new A(); echo $a instanceof 1;",
        "Expected class or interface name after 'instanceof'",
    );
}

// Tests `fdiv()` arity error when called with only 1 argument.
expect_builtin_arity_error!(
    test_error_fdiv_wrong_args,
    "<?php fdiv(1);",
    "fdiv() takes exactly 2 arguments"
);

// Tests `mt_rand()` arity error when called with 1 argument (requires 0 or 2).
expect_builtin_arity_error!(
    test_error_mt_rand_wrong_args,
    "<?php mt_rand(1);",
    "mt_rand() takes 0 or 2 arguments"
);

// Tests `rand()` arity error when called with 1 argument (requires 0 or 2).
expect_builtin_arity_error!(
    test_error_rand_wrong_args,
    "<?php rand(1);",
    "rand() takes 0 or 2 arguments"
);

// Tests `asin()` arity error when called with no arguments.
expect_builtin_arity_error!(
    test_error_asin_wrong_args,
    "<?php asin();",
    "asin() takes exactly 1 argument"
);

// Tests `acos()` arity error when called with no arguments.
expect_builtin_arity_error!(
    test_error_acos_wrong_args,
    "<?php acos();",
    "acos() takes exactly 1 argument"
);

// Tests `tan()` arity error when called with no arguments.
expect_builtin_arity_error!(
    test_error_tan_wrong_args,
    "<?php tan();",
    "tan() takes exactly 1 argument"
);

// Tests `atan()` arity error when called with no arguments.
expect_builtin_arity_error!(
    test_error_atan_wrong_args,
    "<?php atan();",
    "atan() takes exactly 1 argument"
);

// Tests `sinh()` arity error when called with no arguments.
expect_builtin_arity_error!(
    test_error_sinh_wrong_args,
    "<?php sinh();",
    "sinh() takes exactly 1 argument"
);

// Tests `cosh()` arity error when called with no arguments.
expect_builtin_arity_error!(
    test_error_cosh_wrong_args,
    "<?php cosh();",
    "cosh() takes exactly 1 argument"
);

// Tests `tanh()` arity error when called with no arguments.
expect_builtin_arity_error!(
    test_error_tanh_wrong_args,
    "<?php tanh();",
    "tanh() takes exactly 1 argument"
);

// Tests `log2()` arity error when called with no arguments.
expect_builtin_arity_error!(
    test_error_log2_wrong_args,
    "<?php log2();",
    "log2() takes exactly 1 argument"
);

// Tests `log10()` arity error when called with no arguments.
expect_builtin_arity_error!(
    test_error_log10_wrong_args,
    "<?php log10();",
    "log10() takes exactly 1 argument"
);

// Tests `rad2deg()` arity error when called with no arguments.
expect_builtin_arity_error!(
    test_error_rad2deg_wrong_args,
    "<?php rad2deg();",
    "rad2deg() takes exactly 1 argument"
);

#[test]
// Tests that a bare `const` without a name produces an error.
fn test_error_const_missing_name() {
    expect_error("<?php const = 5;", "Expected constant name");
}

#[test]
// Tests that `const MAX;` (without value) produces an error.
fn test_error_const_missing_value() {
    expect_error("<?php const MAX;", "Expected '='");
}

#[test]
// Tests that `static ;` without a variable name produces an error.
fn test_error_static_missing_var() {
    expect_error("<?php static ;", "Expected variable after 'static'");
}

#[test]
// Tests that `static $x;` without an initializer produces an error.
fn test_error_static_missing_init() {
    expect_error("<?php static $x;", "Expected '=' after static variable");
}

// --- Variadic / Spread errors ---

#[test]
// Tests `sin()` arity error when called with no arguments.
fn test_error_sin_no_args() {
    expect_error("<?php sin();", "sin() takes exactly 1 argument");
}

#[test]
// Tests `cos()` arity error when called with no arguments.
fn test_error_cos_no_args() {
    expect_error("<?php cos();", "cos() takes exactly 1 argument");
}

#[test]
// Tests `atan2()` arity error when called with 1 argument (requires exactly 2).
fn test_error_atan2_one_arg() {
    expect_error("<?php atan2(1);", "atan2() takes exactly 2 arguments");
}

#[test]
// Tests `atan2()` arity error when called with 3 arguments (requires exactly 2).
fn test_error_atan2_three_args() {
    expect_error("<?php atan2(1, 2, 3);", "atan2() takes exactly 2 arguments");
}

#[test]
// Tests `log()` arity error when called with no arguments (requires 1 or 2).
fn test_error_log_no_args() {
    expect_error("<?php log();", "log() takes 1 or 2 arguments");
}

#[test]
// Tests `hypot()` arity error when called with 1 argument (requires exactly 2).
fn test_error_hypot_one_arg() {
    expect_error("<?php hypot(1);", "hypot() takes exactly 2 arguments");
}

#[test]
// Tests `exp()` arity error when called with no arguments.
fn test_error_exp_no_args() {
    expect_error("<?php exp();", "exp() takes exactly 1 argument");
}

#[test]
// Tests `pi()` arity error when called with an argument (takes no arguments).
fn test_error_pi_with_arg() {
    expect_error("<?php pi(1);", "pi() takes no arguments");
}

#[test]
// Tests `deg2rad()` arity error when called with no arguments.
fn test_error_deg2rad_no_args() {
    expect_error("<?php deg2rad();", "deg2rad() takes exactly 1 argument");
}
