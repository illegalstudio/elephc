//! Purpose:
//! Interpreter tests for userspace stream-wrapper `stream_cast()` dispatch.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Magician keeps `stream_select()` conservative, but still invokes
//!   `stream_cast(STREAM_CAST_FOR_SELECT)` for PHP-observable wrapper effects.

use super::super::*;
use super::support::*;

/// Verifies `stream_select()` invokes wrapper `stream_cast()` for each stream array.
#[test]
fn execute_program_dispatches_user_stream_wrapper_cast_for_select() {
    let program = parse_fragment(
        br#"class EvalCastWrapperW {
    public function stream_open($path, $mode, $options, &$opened_path): bool {
        return true;
    }
    public function stream_cast($cast_as) {
        echo "cast(" . $cast_as . ")";
        return false;
    }
}
stream_wrapper_register("castw", "EvalCastWrapperW");
$h = fopen("castw://one", "r");
$read = [$h]; $write = []; $except = [];
echo stream_select($read, $write, $except, 0) === 0 ? "select" : "bad"; echo ":";
$read = []; $write = [$h]; $except = [$h];
echo stream_select($read, $write, $except, 0, 0) === 0 ? "select2" : "bad";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "cast(3)select:cast(3)cast(3)select2"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
