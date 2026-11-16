//! Purpose:
//! Regression tests for the call shapes the shared builtin contracts were widened to accept: each
//! one used to compile and then answer something other than php, or to stop codegen.
//!
//! Called from:
//! - `cargo test` through Rust's test harness (`tests/codegen/regressions.rs`).
//!
//! Key details:
//! - Every expected string was captured from `php -n` (8.5.10).

use super::*;

/// `htmlspecialchars()`'s `$double_encode` honoured through a named argument and through a spread.
///
/// The prelude rewrite that keeps valid character references used to fire only for four
/// positional arguments, so the other two spellings of the same call double-encoded.
#[test]
fn test_htmlspecialchars_double_encode_named_and_spread() {
    let out = compile_and_run(
        r#"<?php
echo htmlspecialchars('&amp; &foo;', ENT_QUOTES, 'UTF-8', double_encode: false), "\n";
echo htmlspecialchars('&amp; <b>', double_encode: false), "\n";
$args = ['&amp; &nbsp;', ENT_QUOTES, 'UTF-8', false];
echo htmlspecialchars(...$args), "\n";
echo htmlspecialchars('&amp;', double_encode: true), "\n";
"#,
    );
    assert_eq!(out, "&amp; &amp;foo;\n&amp; &lt;b&gt;\n&amp; &nbsp;\n&amp;amp;\n");
}

/// `fgetcsv()`'s `$length` bounds the first line read, and `0`/`null` mean no bound.
///
/// The long-form helper declared the parameter and never read it.
#[test]
fn test_fgetcsv_length_bounds_the_line() {
    let out = compile_and_run(
        r#"<?php
$f = fopen('php://memory', 'w+');
fwrite($f, "alpha,beta\nx,y\n");
rewind($f);
echo json_encode(@fgetcsv($f, 4, ",", '"')), "\n";
echo json_encode(@fgetcsv($f, 4, ",", '"', "\\")), "\n";
echo json_encode(@fgetcsv($f, 0, ",", '"', "\\")), "\n";
"#,
    );
    assert_eq!(out, "[\"alph\"]\n[\"a\",\"be\"]\n[\"ta\"]\n");
}

/// `array_column()` binds its named arguments to php's parameter names in the three-argument form.
#[test]
fn test_array_column_named_arguments_with_index_key() {
    let out = compile_and_run(
        r#"<?php
$rows = [['id' => 1, 'name' => 'a'], ['id' => 2, 'name' => 'b']];
echo json_encode(array_column($rows, index_key: 'name', column_key: 'id')), "\n";
echo json_encode(array_column($rows, 'id', index_key: 'name')), "\n";
"#,
    );
    assert_eq!(out, "{\"a\":1,\"b\":2}\n{\"a\":1,\"b\":2}\n");
}

/// The user-comparator set functions refuse too few arguments and non-array operands like php.
#[test]
fn test_user_comparator_set_functions_validate_their_arguments() {
    let out = compile_and_run(
        r#"<?php
foreach ([
    fn() => array_diff_ukey(['a' => 1]),
    fn() => array_udiff_uassoc(['a' => 1], ['b' => 2]),
    fn() => array_intersect_uassoc(['k' => 1], 42, 'strcmp'),
    fn() => array_uintersect_uassoc(['k' => 1], ['k' => 1], 3, 'strcmp', 'strcmp'),
] as $call) {
    try {
        $call();
    } catch (Throwable $e) {
        echo get_class($e), ': ', $e->getMessage(), "\n";
    }
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "ArgumentCountError: array_diff_ukey() expects at least 2 arguments, 1 given\n",
            "ArgumentCountError: array_udiff_uassoc() expects at least 3 arguments, 2 given\n",
            "TypeError: array_intersect_uassoc(): Argument #2 must be of type array, int given\n",
            "TypeError: array_uintersect_uassoc(): Argument #3 must be of type array, int given\n",
        )
    );
}

/// `readfile()`'s `$use_include_path` and `$context` compile and leave the read unchanged.
#[test]
fn test_readfile_accepts_include_path_and_context() {
    let out = compile_and_run(
        r#"<?php
$d = sys_get_temp_dir() . '/elx' . getmypid();
mkdir($d);
file_put_contents("$d/a", "hi");
echo readfile("$d/a", false), "\n";
echo readfile("$d/a", false, null), "\n";
unlink("$d/a");
rmdir($d);
"#,
    );
    assert_eq!(out, "hi2\nhi2\n");
}

/// Evaluated `passthru($command, &$result_code)` writes the exit status through the caller's cell.
///
/// The one-argument handler was the only one, so the widened contract turned php's catchable
/// ArgumentCountError into a runtime fatal.
#[test]
fn test_eval_passthru_writes_the_result_code() {
    let out = compile_and_run(
        r#"<?php
$code = -1;
eval('passthru("printf hi", $code); echo "|", $code;');
eval('passthru("exit 3", $code); echo "|", $code;');
"#,
    );
    assert_eq!(out, "hi|0|3");
}

/// Evaluated `stream_socket_server()` writes `$error_code` and `$error_message` like php.
#[test]
fn test_eval_stream_socket_server_writes_its_error_outputs() {
    let out = compile_and_run(
        r#"<?php
$code = -1;
$message = 'x';
eval('$s = stream_socket_server("tcp://127.0.0.1:0", $code, $message); echo is_resource($s) ? "r" : "f", "|", $code, "|", $message, "|";');
eval('$s = @stream_socket_server("tcp://203.0.113.1:80", $code, $message); echo $s === false ? "f" : "r", "|", $code > 0 ? "e" : "z", "|", str_contains($message, "os error") ? "raw" : "plain";');
"#,
    );
    assert_eq!(out, "r|0||f|e|plain");
}
