//! Purpose:
//! Interpreter tests for `sprintf()` zero padding combined with a precision.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::builtins_sprintf_padding`.
//!
//! Key details:
//! - The expected string is `php -n` 8.5.10's own output on the same format list.

use super::super::*;
use super::support::*;

/// Verifies the `0` flag and the custom `'0` padding survive a precision.
///
/// The precision shapes the digits first; the padding then fills the width after the sign.
#[test]
fn sprintf_zero_pads_a_number_that_has_a_precision() {
    let program = parse_fragment(
        br#"echo sprintf("[%'010.3f]", 1.5), sprintf("[%05.3d]", 7), sprintf("[%08.3f]", -1.5),
    sprintf("[%'*10.2f]", 2.5), sprintf("[%+08.2f]", 2.5);
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "[000001.500][00007][-001.500][******2.50][+0002.50]"
    );
}

/// Verifies an argument number that overflows is php's catchable ValueError, not a fatal.
///
/// `php -n` 8.5.10 prints the message below for `%99999999999999999999$s`.
#[test]
fn sprintf_oversized_argument_number_is_a_catchable_value_error() {
    let program = parse_fragment(
        br#"try { sprintf("%99999999999999999999\$s", "x"); } catch (\ValueError $e) { echo $e->getMessage(); }
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "Argument number specifier must be greater than zero and less than 2147483647"
    );
}
