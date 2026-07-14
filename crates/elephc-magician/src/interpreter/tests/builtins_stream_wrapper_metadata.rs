//! Purpose:
//! Interpreter tests for path metadata operations on eval userspace stream wrappers.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - These cases verify `stream_metadata()` option/value mapping for path-based
//!   filesystem mutation builtins.

use super::super::*;
use super::support::*;

/// Verifies path metadata builtins dispatch to wrapper `stream_metadata()`.
#[test]
fn execute_program_dispatches_user_stream_wrapper_metadata() {
    let program = parse_fragment(
        br##"class EvalMetadataWrapperW {
    public function stream_metadata($path, $option, $value): bool {
        echo $path . "#" . $option . "#";
        if ($option === 1) {
            echo $value[0] . "/" . $value[1];
            return true;
        }
        echo $value;
        return true;
    }
}
stream_wrapper_register("metaw", "EvalMetadataWrapperW");
echo chmod("metaw://file", 384) ? "chmod" : "bad"; echo ":";
echo touch("metaw://file", 100, 200) ? "touch" : "bad"; echo ":";
echo chown("metaw://file", 501) ? "chown" : "bad"; echo ":";
echo chgrp("metaw://file", "staff") ? "chgrp" : "bad"; echo ":";
echo lchown("metaw://file", 501) === false ? "lskip" : "bad";
return true;"##,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "metaw://file#6#384chmod:metaw://file#1#100/200touch:metaw://file#3#501chown:metaw://file#4#staffchgrp:lskip"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
