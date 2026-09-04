//! Purpose:
//! Interpreter regression tests for Core argument introspection and `zend_version()`.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Cases cover defaults, named arguments, positional surplus, current fixed values, and variadics.

use super::super::*;
use super::support::*;

/// Verifies eval `func_*` calls expose current fixed values and actual call positions.
#[test]
fn execute_program_dispatches_func_args_with_named_defaults_and_surplus() {
    let program = parse_fragment(
        br#"function named($a = 10, $b = 20, ...$rest) {
    $a = 99;
    return func_num_args() . ":" . implode(",", func_get_args()) . ":" . func_get_arg(1);
}
function surplus($a) {
    $a = 7;
    return func_num_args() . ":" . implode(",", func_get_args()) . ":" . func_get_arg(2);
}
echo named(b: 2), "|", surplus(1, 2, 3);
return function_exists("func_get_args");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "2:99,2:2|3:7,2,3:3");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies a source variadic may change locally without rewriting `func_get_args()` history.
#[test]
fn execute_program_func_get_args_preserves_original_variadic_values() {
    let program = parse_fragment(
        br#"function snapshot(...$rest) {
    $rest[0] = 9;
    return implode(",", func_get_args());
}
return snapshot(1, 2);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("1,2".to_string()));
}

/// Verifies invalid positions and global-scope calls use catchable PHP throwable classes.
#[test]
fn execute_program_func_args_throw_php_errors() {
    let program = parse_fragment(
        br#"function bad() {
    try { func_get_arg(-1); } catch (ValueError $e) { echo "negative|"; }
    try { func_get_arg(1); } catch (ValueError $e) { echo "range|"; }
}
bad();
try { func_get_args(); } catch (Error $e) { echo $e->getMessage(); }
return 1;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "negative|range|func_get_args() cannot be called from the global scope"
    );
    assert_eq!(values.get(result), FakeValue::Int(1));
}

/// Verifies eval rejects argument introspection through PHP dynamic-call surfaces.
#[test]
fn execute_program_func_args_reject_dynamic_invocation() {
    let program = parse_fragment(
        br#"function probe_dynamic_args() {
    try {
        $callback = func_num_args(...);
        $callback();
    } catch (Error $error) {
        echo $error->getMessage() . "|";
    }
    try {
        call_user_func("func_get_args");
    } catch (Error $error) {
        echo $error->getMessage() . "|";
    }
    try {
        call_user_func("func_get_arg", 0);
    } catch (Error $error) {
        echo $error->getMessage();
    }
}
probe_dynamic_args(7);
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "Cannot call func_num_args() dynamically|Cannot call func_get_args() dynamically|Cannot call func_get_arg() dynamically"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `zend_version()` follows the active PHP profile and registry visibility.
#[test]
fn execute_program_zend_version_tracks_profile() {
    let _guard = crate::eval_php_profile::scoped_profile(80_400);
    let program = parse_fragment(
        br#"echo zend_version(), "|", call_user_func("zend_version");
return function_exists("zend_version");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "4.4.0|4.4.0");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
