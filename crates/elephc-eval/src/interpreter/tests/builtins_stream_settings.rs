//! Purpose:
//! Interpreter tests for eval stream descriptor setting builtins.
//!
//! Called from:
//! - `cargo test -p elephc-eval` through Rust's test harness.
//!
//! Key details:
//! - Local file streams expose terminal/blocking probes through host libc.
//! - Timeout support currently returns false for regular files, matching the
//!   socket-only behavior of the main backend.

use super::super::*;
use super::support::*;

/// Verifies eval stream setting builtins work directly and through dynamic calls.
#[test]
fn execute_program_dispatches_stream_setting_builtins() {
    let pid = std::process::id();
    let file = format!("elephc_eval_stream_settings_{pid}.txt");
    let source = format!(
        r#"file_put_contents("{file}", "x");
$h = fopen("{file}", "r+");
echo stream_isatty($h) ? "bad" : "notty"; echo ":";
echo stream_set_blocking($h, false) ? "nonblock" : "bad"; echo ":";
echo stream_set_blocking($h, true) ? "block" : "bad"; echo ":";
echo stream_set_chunk_size($h, 1024) === 8192 ? "chunk1" : "bad"; echo ":";
echo stream_set_chunk_size($h, 2048) === 1024 ? "chunk2" : "bad"; echo ":";
echo stream_set_read_buffer($h, 0) === 0 ? "readbuf" : "bad"; echo ":";
echo stream_set_write_buffer($h, 0) === 0 ? "writebuf" : "bad"; echo ":";
echo stream_set_timeout($h, 1) === false ? "notimeout" : "bad"; echo ":";
echo call_user_func("stream_isatty", $h) === false ? "calltty" : "bad"; echo ":";
echo call_user_func("stream_set_chunk_size", $h, 4096) === 2048 ? "callchunk" : "bad"; echo ":";
fclose($h);
echo unlink("{file}") ? "cleanup" : "bad"; echo ":";
echo function_exists("stream_isatty"); echo function_exists("stream_set_blocking");
echo function_exists("stream_set_chunk_size"); echo function_exists("stream_set_read_buffer");
echo function_exists("stream_set_timeout"); echo function_exists("stream_set_write_buffer");
return true;"#
    );
    let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
    let _ = std::fs::remove_file(&file);
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    let _ = std::fs::remove_file(&file);
    assert_eq!(
        values.output,
        concat!(
            "notty:nonblock:block:chunk1:chunk2:readbuf:writebuf:notimeout:",
            "calltty:callchunk:cleanup:111111"
        )
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
