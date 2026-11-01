//! Purpose:
//! Interpreter tests pinning php's `Warning: Undefined variable $x` -- and the three readers that
//! must stay silent.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::undefined_variable_warning`.
//!
//! Key details:
//! - An unset variable is a WARNING and evaluates to null. It is never an error, and 18 cases in
//!   the php-src sweep printed the right value with no diagnostic at all.
//! - `@`, `??` and `isset()` are the readers php keeps quiet. `@` already went through the
//!   context's suppression depth; `??` now suppresses around its own left operand, because the
//!   rule belongs to `??` rather than to the variable read.
//! - Every expected string is `php -n` 8.5.6's output on the same fragment.

use super::super::*;
use super::support::*;

/// Runs one fragment and returns what it echoed plus the warnings it raised.
fn out_with_warnings(fragment: &[u8]) -> (String, Vec<String>) {
    let program = parse_fragment(fragment).expect("parse undefined variable fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values)
        .expect("execute undefined variable fragment");
    (values.output.clone(), values.warnings.clone())
}

/// Verifies reading an unset variable warns and still yields null.
///
/// `php -n` 8.5.6 prints `a::b` and raises `Warning: Undefined variable $undefined`. Both halves
/// matter: a fix that threw, or that returned anything but null, would be a worse answer than the
/// silence it replaced.
#[test]
fn reading_an_unset_variable_warns_and_yields_null() {
    let (output, warnings) = out_with_warnings(br#"echo "a:", $undefined, ":b";"#);
    assert_eq!(output, "a::b");
    assert_eq!(warnings, vec!["Undefined variable $undefined".to_string()]);
}

/// Verifies the three readers php keeps quiet raise nothing.
///
/// `php -n` 8.5.6 prints `coalesced::no` and raises NO warning for any of them. `??` is the one
/// that had to change: it evaluates its left operand like any other expression, so without
/// suppressing around it every `$config['key'] ?? $default` in existence would have started
/// warning -- which is how a correct-looking warning becomes a worse regression than the missing
/// one.
#[test]
fn coalesce_suppression_and_isset_stay_quiet() {
    let (output, warnings) = out_with_warnings(
        br#"echo $missing ?? "coalesced";
echo @$suppressed;
echo ":", isset($never) ? "yes" : "no";"#,
    );
    assert_eq!(output, "coalesced:no");
    assert_eq!(warnings, Vec::<String>::new());
}

/// Verifies a variable that IS set raises nothing, and one unset after `unset()` warns again.
///
/// `php -n` 8.5.6 prints `set:` and raises exactly one warning, for the read after `unset()`.
/// This is what says the warning tracks the variable's live state rather than firing on a name
/// the parser has seen.
#[test]
fn the_warning_follows_the_variables_live_state() {
    let (output, warnings) = out_with_warnings(
        br#"$here = "set";
echo $here, ":";
unset($here);
echo $here;"#,
    );
    assert_eq!(output, "set:");
    assert_eq!(warnings, vec!["Undefined variable $here".to_string()]);
}
