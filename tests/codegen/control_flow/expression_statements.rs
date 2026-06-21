//! Purpose:
//! Integration tests for end-to-end codegen of bare expression statements whose leading
//! token is a value or unary operator (e.g. `0 > $T && $T += 0x40;`, `new C();`, `-$x;`).
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - PHP allows any expression as a statement. These fixtures exercise the statement
//!   dispatcher's bare-expression fallback for non-variable, non-keyword leading tokens and
//!   assert PHP-equivalent stdout. One fixture uses `$argc` so the construct survives
//!   AST-level constant folding and actually reaches codegen.

use super::*;

/// Verifies the short-circuit `cond && action;` idiom runs the action when the literal-led
/// condition is true: `-5 < 0` holds, so `$T += 0x40` (64) makes `$T == 59`. This is the
/// Symfony intl-normalizer pattern (`0 > $T && $T += 0x40;`).
#[test]
fn test_value_led_short_circuit_runs_action() {
    let out = compile_and_run("<?php $T = -5; 0 > $T && $T += 0x40; echo $T;");
    assert_eq!(out, "59");
}

/// Verifies the same idiom skips the action when the literal-led condition is false:
/// `0 > 5` is false, so `$T += 0x40` never runs and `$T` stays `5`.
#[test]
fn test_value_led_short_circuit_skips_action_when_false() {
    let out = compile_and_run("<?php $T = 5; 0 > $T && $T += 0x40; echo $T;");
    assert_eq!(out, "5");
}

/// Verifies a literal-led short-circuit statement survives constant folding by using a
/// runtime-unknown operand (`$argc`, which is 1 when run with no args): `0 < 1` holds, so
/// `$n += 10` yields `11`.
#[test]
fn test_value_led_short_circuit_with_runtime_unknown() {
    let out = compile_and_run("<?php $n = $argc; 0 < $n && $n += 10; echo $n;");
    assert_eq!(out, "11");
}

/// Verifies the literal-led `cond || action;` form: `0 < $x` is false (with `$x == 0`), so the
/// right side `$x = 42` executes and `$x` becomes `42`.
#[test]
fn test_value_led_or_runs_action() {
    let out = compile_and_run("<?php $x = 0; 0 < $x || $x = 42; echo $x;");
    assert_eq!(out, "42");
}

/// Verifies a bare `new C();` statement (no assignment) executes the constructor for its
/// side effect, like PHP. Previously this errored at statement position.
#[test]
fn test_bare_new_object_statement_runs_constructor() {
    let out = compile_and_run(
        "<?php class C { public function __construct() { echo \"ctor\"; } } new C(); echo \"-done\";",
    );
    assert_eq!(out, "ctor-done");
}

/// Verifies a unary-operator-led statement (`-$x;`) parses and runs as a discarded
/// expression statement, leaving `$x` unchanged.
#[test]
fn test_unary_led_statement_is_discarded() {
    let out = compile_and_run("<?php $x = 7; -$x; echo $x;");
    assert_eq!(out, "7");
}

/// Verifies a call-result negation drives a short-circuit action: `f()` prints `f` and
/// returns false, so `!false` is true and `print("g")` runs, giving `fg`.
#[test]
fn test_negated_call_short_circuit_statement() {
    let out = compile_and_run(
        "<?php function f() { echo \"f\"; return false; } !f() && print(\"g\");",
    );
    assert_eq!(out, "fg");
}
