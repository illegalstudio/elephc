//! Purpose:
//! Interpreter tests for the `set_time_limit()` builtin: the unconditional `true` php returns,
//! the observable `$seconds` coercion refusals, the exact-arity `ArgumentCountError`, and the
//! named/spread/dynamic call forms.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::builtins_set_time_limit`.
//!
//! Key details:
//! - Every expected string is php 8.5.10's own output on the same fragment, re-measured in this
//!   session (`scratchpad/verify_stl_tests.php`).
//! - elephc arms no execution timer, so a POSITIVE `$seconds` is a documented no-op rather than a
//!   limit php would enforce. Nothing here asserts enforcement in either direction; the return
//!   value and the coercion boundary are what these pin. The divergence is recorded on
//!   `src/builtins/system/set_time_limit.rs`.
//! - The COMPILED backend folds the call to `true` without coercing `$seconds` at all, so the two
//!   `TypeError`s below are interpreter-only behavior today. That asymmetry is deliberate and
//!   named; it is not an accident these tests are hiding.

use super::super::*;
use super::support::*;

/// Runs one fragment and returns what it echoed.
fn out(fragment: &[u8]) -> String {
    let program = parse_fragment(fragment).expect("parse set_time_limit() fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute set_time_limit() fragment");
    values.output.clone()
}

/// Verifies php's return value is `true` for every argument, including both integer extremes and
/// the weak-coerced `bool`/numeric-string forms. No argument was found on php 8.5.10 CLI that
/// makes it return `false`, so a `false` anywhere here would be an invention.
///
/// php 8.5.10: `true:true:true:true:true:true:true:true`.
#[test]
fn set_time_limit_returns_true_for_every_argument() {
    assert_eq!(
        out(br#"echo var_export(set_time_limit(0), true), ":";
echo var_export(set_time_limit(30), true), ":";
echo var_export(set_time_limit(-1), true), ":";
echo var_export(set_time_limit(PHP_INT_MAX), true), ":";
echo var_export(set_time_limit(PHP_INT_MIN), true), ":";
echo var_export(set_time_limit(true), true), ":";
echo var_export(set_time_limit(false), true), ":";
echo var_export(set_time_limit("5"), true);"#),
        "true:true:true:true:true:true:true:true",
    );
}

/// Verifies `$seconds` is still COERCED even though the coerced value is thrown away: a
/// non-numeric string and an array are each a catchable `TypeError` naming the parameter, exactly
/// as php words it. Dropping the coercion would turn both into a silent success.
///
/// php 8.5.10: the two `TypeError`s below.
#[test]
fn set_time_limit_rejects_a_non_int_seconds_with_a_catchable_type_error() {
    assert_eq!(
        out(br#"try {
    set_time_limit("x");
} catch (\TypeError $e) {
    echo $e->getMessage(), ":";
}
try {
    set_time_limit([1]);
} catch (\TypeError $e) {
    echo $e->getMessage();
}"#),
        "set_time_limit(): Argument #1 ($seconds) must be of type int, string given:\
set_time_limit(): Argument #1 ($seconds) must be of type int, array given",
    );
}

/// Verifies `$seconds` is REQUIRED and the arity is exact: php words both ends as "expects
/// exactly 1 argument", and both are catchable rather than an uncatchable fatal.
///
/// php 8.5.10: `set_time_limit() expects exactly 1 argument, 0 given` then
/// `set_time_limit() expects exactly 1 argument, 2 given`.
#[test]
fn set_time_limit_rejects_wrong_arity_with_a_catchable_argument_count_error() {
    assert_eq!(
        out(br#"try {
    set_time_limit();
} catch (\ArgumentCountError $e) {
    echo $e->getMessage(), ":";
}
try {
    set_time_limit(1, 2);
} catch (\ArgumentCountError $e) {
    echo $e->getMessage();
}"#),
        "set_time_limit() expects exactly 1 argument, 0 given:\
set_time_limit() expects exactly 1 argument, 2 given",
    );
}

/// Verifies the named-argument, spread, `call_user_func`, case-insensitive and root-namespace
/// call forms all reach the same registry entry, and that the answer is a real `true` under `===`
/// rather than a truthy placeholder.
///
/// php 8.5.10: `true:true:true:true:true:true`.
#[test]
fn set_time_limit_supports_named_spread_and_dynamic_call_forms() {
    assert_eq!(
        out(br#"echo var_export(set_time_limit(seconds: 0), true), ":";
echo var_export(set_time_limit(...[0]), true), ":";
echo var_export(call_user_func("set_time_limit", 0), true), ":";
echo var_export(SET_TIME_LIMIT(0), true), ":";
echo var_export(\set_time_limit(0), true), ":";
echo var_export(set_time_limit(0) === true, true);"#),
        "true:true:true:true:true:true",
    );
}
