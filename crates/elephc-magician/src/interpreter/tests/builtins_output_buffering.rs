//! Purpose:
//! Interpreter tests for the eval output-buffering (`ob_*`) builtins against the
//! fake runtime ops' buffer stack.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - `FakeOps` mirrors the runtime contract: echoes route into the top fake
//!   buffer, flushing routes into the parent buffer or the captured output, and
//!   `ob_start()` rejects non-null handler callbacks with a warning.

use super::super::*;
use super::support::*;

/// Verifies ob_start/ob_get_clean capture eval'd echoes instead of emitting them.
#[test]
fn execute_program_captures_echo_between_ob_start_and_ob_get_clean() {
    let program = parse_fragment(
        br#"ob_start();
echo "hi";
$s = ob_get_clean();
echo strtoupper($s);
echo "|";
echo ob_get_level();
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    assert_eq!(values.output, "HI|0");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies nested buffers: ob_end_flush folds the inner buffer into the outer one.
#[test]
fn execute_program_folds_nested_buffers_through_ob_end_flush() {
    let program = parse_fragment(
        br#"ob_start();
echo "a";
ob_start();
echo "b";
ob_end_flush();
echo "c";
echo ob_get_clean();
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    assert_eq!(values.output, "abc");
}

/// Verifies ob_get_length/ob_get_contents report the buffer and false without one.
#[test]
fn execute_program_reports_ob_length_and_contents() {
    let program = parse_fragment(
        br#"echo ob_get_length() === false ? "nf" : "bad";
ob_start();
echo "1234";
$n = ob_get_length();
$c = ob_get_contents();
ob_end_clean();
echo ":", $n, ":", $c;
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    assert_eq!(values.output, "nf:4:1234");
}

/// Verifies ob_get_status and ob_list_handlers report the default-handler shape.
#[test]
fn execute_program_reports_ob_status_and_handlers() {
    let program = parse_fragment(
        br#"ob_start();
echo "abc";
$st = ob_get_status();
$handlers = ob_list_handlers();
ob_end_clean();
echo $st["name"], ":", $st["level"], ":", $st["buffer_used"], ":", count($handlers);
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    assert_eq!(values.output, "default output handler:0:3:1");
}

/// Verifies ob_start rejects plain-scalar callbacks with PHP's warning lines
/// and registers callable handlers with their display name.
#[test]
fn execute_program_handles_ob_start_callback_shapes() {
    let program = parse_fragment(
        br#"$bad = ob_start(42);
echo $bad === false ? "rejected" : "started";
echo ":", ob_get_level(), ";";
$ok = ob_start("strtoupper");
$name = ob_list_handlers()[0];
ob_end_clean();
echo $ok === true ? "named" : "bad", ":", $name;
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    assert_eq!(
        values.output,
        concat!(
            "Warning: ob_start(): no array or string given\n",
            "Notice: ob_start(): Failed to create buffer\n",
            "rejected:0;named:strtoupper"
        )
    );
}

/// Verifies ob_flush emits the buffered bytes while keeping the buffer active.
#[test]
fn execute_program_ob_flush_emits_and_keeps_buffer() {
    let program = parse_fragment(
        br#"ob_start();
echo "x";
ob_flush();
echo "y";
ob_end_clean();
echo "z";
return ob_implicit_flush(false);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    assert_eq!(values.output, "xz");
    assert_eq!(values.get(result), FakeValue::Bool(true));
    assert!(!values.ob_implicit_flush);
}
