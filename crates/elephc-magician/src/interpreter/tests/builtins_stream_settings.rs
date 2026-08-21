//! Purpose:
//! Interpreter tests for eval stream descriptor setting builtins.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
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
    let file = format!("elephc_magician_stream_settings_{pid}.txt");
    let source = format!(
        r#"file_put_contents("{file}", "x");
$h = fopen("{file}", "r+");
echo stream_isatty($h) ? "bad" : "notty"; echo ":";
echo stream_set_blocking($h, false) ? "nonblock" : "bad"; echo ":";
echo stream_set_blocking($h, true) ? "block" : "bad"; echo ":";
echo stream_set_chunk_size($h, 1024) === 8192 ? "chunk1" : "bad"; echo ":";
echo stream_set_chunk_size($h, 2048) === 1024 ? "chunk2" : "bad"; echo ":";
echo stream_set_read_buffer($h, 0) === 0 ? "readbuf" : "bad"; echo ":";
echo stream_set_write_buffer($h, 0) === -1 ? "writebuf" : "bad"; echo ":";
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

/// Verifies eval raises the same catchable `ValueError`s the compiled backend does for
/// out-of-range `stream_*` arguments, instead of dying as an uncatchable runtime fatal.
///
/// Every wording below was MEASURED against `php -n` 8.5.6 before this test was written:
///
/// ```text
/// stream_get_contents($f, -5)   ValueError: stream_get_contents(): Argument #2 ($length) must be greater than or equal to -1
/// stream_get_line($f, -1)       ValueError: stream_get_line(): Argument #2 ($length) must be greater than or equal to 0
/// stream_set_chunk_size($f, 0)  ValueError: stream_set_chunk_size(): Argument #2 ($size) must be greater than 0
/// stream_socket_shutdown($f, 9) ValueError: stream_socket_shutdown(): Argument #2 ($mode) must be one of STREAM_SHUT_RD, STREAM_SHUT_WR, or STREAM_SHUT_RDWR
/// ```
///
/// `-1` is `stream_get_contents()`'s documented "read to EOF" sentinel and stays legal, so the
/// last fragment proves the guard did not swallow the whole negative range.
#[test]
fn execute_program_stream_builtins_raise_php_argument_range_value_errors() {
    let pid = std::process::id();
    let file = format!("elephc_magician_stream_value_errors_{pid}.txt");
    let source = format!(
        r#"file_put_contents("{file}", "payload");
$f = fopen("{file}", "r");
try {{ stream_get_contents($f, -5); echo "no-throw"; }} catch (ValueError $e) {{ echo $e->getMessage(); }}
echo "|";
try {{ stream_get_line($f, -1); echo "no-throw"; }} catch (ValueError $e) {{ echo $e->getMessage(); }}
echo "|";
try {{ stream_set_chunk_size($f, 0); echo "no-throw"; }} catch (ValueError $e) {{ echo $e->getMessage(); }}
echo "|";
try {{ stream_socket_shutdown($f, 9); echo "no-throw"; }} catch (ValueError $e) {{ echo $e->getMessage(); }}
echo "|" . stream_get_contents($f, -1);
fclose($f);
unlink("{file}");
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
            "stream_get_contents(): Argument #2 ($length) must be greater than or equal to -1",
            "|stream_get_line(): Argument #2 ($length) must be greater than or equal to 0",
            "|stream_set_chunk_size(): Argument #2 ($size) must be greater than 0",
            "|stream_socket_shutdown(): Argument #2 ($mode) must be one of STREAM_SHUT_RD, \
             STREAM_SHUT_WR, or STREAM_SHUT_RDWR",
            "|payload",
        )
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies `stream_set_write_buffer()` reports php's REFUSAL for a stream that is not a
/// userspace wrapper, and that the read buffer keeps its distinct answer.
///
/// MEASURED on `php -n` 8.5.6 — the split is not cosmetic: `_php_stream_set_option()` carries a
/// generic fallback for `PHP_STREAM_OPTION_READ_BUFFER` and none for the write buffer, so the
/// write setter reports `EOF` (`-1`) for a plain file, `php://memory` and `php://temp` alike:
///
/// ```text
/// stream_set_write_buffer(fopen($tmp, "r+"), 0)     int(-1)
/// stream_set_write_buffer(fopen("php://memory"), 0) int(-1)
/// stream_set_read_buffer(fopen($tmp, "r+"), 0)      int(0)
/// ```
#[test]
fn execute_program_stream_set_write_buffer_reports_phps_refusal() {
    let pid = std::process::id();
    let file = format!("elephc_magician_stream_write_buffer_{pid}.txt");
    let source = format!(
        r#"file_put_contents("{file}", "x");
$h = fopen("{file}", "r+");
echo stream_set_write_buffer($h, 0), "|";
echo stream_set_write_buffer($h, 8192), "|";
echo stream_set_read_buffer($h, 0), "|";
echo stream_set_read_buffer($h, 4096);
fclose($h);
unlink("{file}");
return true;"#
    );
    let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
    let _ = std::fs::remove_file(&file);
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    let _ = std::fs::remove_file(&file);
    assert_eq!(values.output, "-1|-1|0|0");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
