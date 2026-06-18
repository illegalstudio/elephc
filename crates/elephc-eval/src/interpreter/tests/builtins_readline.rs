//! Purpose:
//! Interpreter tests for eval's `readline()` builtin.
//!
//! Called from:
//! - `cargo test -p elephc-eval` through Rust's test harness.
//!
//! Key details:
//! - The test harness runs with stdin at EOF, so `readline()` returns false
//!   without blocking for terminal input.

use super::super::*;
use super::support::*;

/// Verifies `readline()` reports EOF and participates in callable dispatch.
#[test]
fn execute_program_dispatches_readline_builtin() {
    let program = parse_fragment(
        br#"echo readline() === false ? "eof" : "bad"; echo ":";
echo call_user_func("readline") === false ? "call" : "bad"; echo ":";
echo function_exists("readline"); echo is_callable("readline");
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "eof:call:11");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
