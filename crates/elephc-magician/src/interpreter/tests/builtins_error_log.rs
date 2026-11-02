//! Purpose:
//! Interpreter tests for the `error_log()` builtin: the default (stderr) channel's message,
//! trailing newline, and NUL-truncation shape, the file-append channel's different (no
//! truncation, no added newline) shape, argument coercion, and the catchable `ArgumentCountError`
//! for the wrong arity.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::builtins_error_log`.
//!
//! Key details:
//! - Every expected byte string is `php -n` 8.5.6's own output on the same fragment, re-measured
//!   in this session (`scratchpad/errlog_verify*.php`) rather than copied from an unverified spec.
//! - The default channel's actual destination (fd 2 / stderr) is NOT a `RuntimeValueOps`
//!   assertion an interpreter unit test can observe directly through `values.output`; the fixture
//!   overrides `error_log_write_stderr()` to capture the bytes instead of writing real fd 2 (see
//!   `support::runtime_ops::numeric_string`), which is what these tests assert against.

use super::super::*;
use super::support::*;

/// Runs one fragment and returns what `error_log()` wrote to its captured stderr channel, one
/// entry per call, plus the fragment's own return value.
fn run(fragment: &[u8]) -> (RuntimeCellHandle, Vec<Vec<u8>>, FakeOps) {
    let program = parse_fragment(fragment).expect("parse error_log() fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values).expect("execute fragment");
    let writes = values.error_log_stderr_writes.clone();
    (result, writes, values)
}

/// Verifies the default channel: the exact message plus ONE trailing newline, and a `true`
/// return -- the shape Symfony's `Logger.php:101` (a plain one-argument call) depends on.
///
/// `php -n` 8.5.6: writes `one-arg-msg\n` to fd 2, returns `bool(true)`.
#[test]
fn error_log_writes_the_message_plus_a_trailing_newline_and_returns_true() {
    let (result, writes, values) = run(br#"return error_log("one-arg-msg");"#);
    assert_eq!(writes, vec![b"one-arg-msg\n".to_vec()]);
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies the default channel truncates the message at its first embedded NUL byte -- a real
/// SAPI-logger/syslog behavior, measured directly (`error_log("msg\0with-nul")` writes only
/// `msg\n` to stderr) -- while an empty message still writes a bare newline.
///
/// `php -n` 8.5.6: `msg\n` (NUL and everything after it dropped) then `\n` (empty message).
#[test]
fn error_log_truncates_the_default_channel_at_an_embedded_nul() {
    let (_, writes, _) = run(b"error_log(\"msg\\0with-nul\");\nerror_log(\"\");");
    assert_eq!(writes, vec![b"msg\n".to_vec(), b"\n".to_vec()]);
}

/// Verifies an unrecognized `$message_type` (and, by the same fallback, the unimplemented email
/// type 1) still reaches the default stderr channel and returns `true` -- measured:
/// `error_log("x", 99)` and `error_log("x", 4)` (SAPI, no SAPI here) both write to stderr.
///
/// `php -n` 8.5.6: both write `x\n` to fd 2, both return `bool(true)`.
#[test]
fn error_log_falls_back_to_the_default_channel_for_an_unhandled_message_type() {
    let (result, writes, values) = run(br#"$a = error_log("x", 99);
$b = error_log("x", 4);
return $a && $b;"#);
    assert_eq!(writes, vec![b"x\n".to_vec(), b"x\n".to_vec()]);
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies scalar coercion into the message: int, float, and bool all cast to their normal PHP
/// string form before being written.
///
/// `php -n` 8.5.6: `123\n1.5\n1\n` on stderr (three separate calls), all returning `true`.
#[test]
fn error_log_coerces_scalar_message_arguments() {
    let (_, writes, _) = run(br#"error_log(123);
error_log(1.5);
error_log(true);"#);
    assert_eq!(
        writes,
        vec![b"123\n".to_vec(), b"1.5\n".to_vec(), b"1\n".to_vec()],
    );
}

/// Verifies message_type 3 (append to file): the RAW bytes are written with NO added newline and
/// NO NUL truncation (measured: `error_log("a\0b", 3, $path)` writes the full 3 bytes), an empty
/// destination is a catchable `ValueError` worded exactly as php words it (with no
/// `error_log():` prefix -- confirmed by direct measurement), and two calls append rather than
/// overwrite.
///
/// `php -n` 8.5.6: the file ends up holding `file-msg-1file-msg-2` (no separators); `ValueError:
/// Path must not be empty` for an empty destination.
#[test]
fn error_log_message_type_3_appends_raw_bytes_to_a_file() {
    let path = std::env::temp_dir().join(format!(
        "elephc_error_log_test_{}_{}.log",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let path_str = path.to_string_lossy().replace('\\', "\\\\");
    let fragment = format!(
        r#"error_log("file-msg-1", 3, "{path_str}");
error_log("file-msg-2", 3, "{path_str}");
error_log("a\0b", 3, "{path_str}");
"#
    );
    let program = parse_fragment(fragment.as_bytes()).expect("parse fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute fragment");
    let contents = std::fs::read(&path).expect("read the file error_log wrote to");
    assert_eq!(contents, b"file-msg-1file-msg-2a\0b");
    std::fs::remove_file(&path).ok();

    let out = out_of(br#"try {
    error_log("no-dest", 3, "");
} catch (\ValueError $e) {
    echo $e->getMessage();
}"#);
    assert_eq!(out, "Path must not be empty");
}

/// Runs one fragment and returns what it echoed (message_type-3 test's own small helper, kept
/// local since the file's primary `run()` returns stderr writes instead).
fn out_of(fragment: &[u8]) -> String {
    let program = parse_fragment(fragment).expect("parse fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute fragment");
    values.output.clone()
}

/// Verifies the catchable `ArgumentCountError` for the wrong arity.
///
/// `php -n` 8.5.6: `error_log() expects at least 1 argument, 0 given` then
/// `error_log() expects at most 4 arguments, 5 given`.
#[test]
fn error_log_rejects_wrong_arity_with_a_catchable_argument_count_error() {
    let out = out_of(br#"try {
    error_log();
} catch (\ArgumentCountError $e) {
    echo $e->getMessage(), ":";
}
try {
    error_log("m", 0, "", "", "extra");
} catch (\ArgumentCountError $e) {
    echo $e->getMessage();
}"#);
    assert_eq!(
        out,
        "error_log() expects at least 1 argument, 0 given:\
error_log() expects at most 4 arguments, 5 given",
    );
}

/// Verifies `function_exists("error_log")` answers true now that the builtin is registered.
///
/// `php -n` 8.5.6: `bool(true)`.
#[test]
fn error_log_is_visible_to_function_exists() {
    let program =
        parse_fragment(br#"return function_exists("error_log");"#).expect("parse fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values).expect("execute fragment");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
