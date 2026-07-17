//! Purpose:
//! Interpreter tests for userspace stream-wrapper option dispatch.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - `stream_set_blocking()` and `stream_set_timeout()` map to
//!   `stream_set_option($option, $arg1, $arg2)` on wrapper streams.

use super::super::*;
use super::support::*;

/// Verifies stream setting builtins dispatch to wrapper `stream_set_option()`.
#[test]
fn execute_program_dispatches_user_stream_wrapper_options() {
    let program = parse_fragment(
        br#"class EvalOptionWrapperW {
    public function stream_open($path, $mode, $options, &$opened_path): bool {
        return true;
    }
    public function stream_set_option($option, $arg1, $arg2): bool {
        echo "O(" . $option . "," . $arg1 . "," . $arg2 . ")";
        if ($option === 1) {
            return $arg1 === 1;
        }
        if ($option === 4) {
            return $arg2 === 7;
        }
        return false;
    }
}
stream_wrapper_register("optw", "EvalOptionWrapperW");
$h = fopen("optw://one", "r");
echo stream_set_blocking($h, true) ? "block" : "bad"; echo ":";
echo stream_set_blocking($h, false) === false ? "nonblockfalse" : "bad"; echo ":";
echo stream_set_timeout($h, 3, 7) ? "timeout" : "bad"; echo ":";
echo call_user_func("stream_set_timeout", $h, 5) === false ? "calltimeoutfalse" : "bad";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "O(1,1,0)block:O(1,0,0)nonblockfalse:O(4,3,7)timeout:O(4,5,0)calltimeoutfalse"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
