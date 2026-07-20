//! Purpose:
//! Integration tests for end-to-end codegen coverage of the output-buffering
//! (`ob_*`) builtins: capture, nesting, flush/clean variants, status queries,
//! process-exit flushing, and capture of every stdout-producing builtin.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout.
//! - Buffered writers under test include echo, print, printf, print_r, var_dump,
//!   readfile, and fpassthru — each must route through `__rt_stdout_write` (or a
//!   capture-aware shim) so active buffers see their bytes.

use super::*;

/// Verifies basic capture: echoes between ob_start and ob_get_clean are returned,
/// not printed.
#[test]
fn test_ob_start_get_clean_captures_echo() {
    let out = compile_and_run(
        r#"<?php
ob_start();
echo "hello";
$s = ob_get_clean();
echo strtoupper($s);
"#,
    );
    assert_eq!(out, "HELLO");
}

/// Verifies ob_get_contents reads the buffer without consuming it.
#[test]
fn test_ob_get_contents_is_non_destructive() {
    let out = compile_and_run(
        r#"<?php
ob_start();
echo "abc";
$first = ob_get_contents();
echo "def";
$second = ob_get_clean();
echo $first, "|", $second;
"#,
    );
    assert_eq!(out, "abc|abcdef");
}

/// Verifies nested buffers: ob_end_flush moves the inner contents into the outer
/// buffer, preserving chronological order.
#[test]
fn test_nested_buffers_flush_to_parent() {
    let out = compile_and_run(
        r#"<?php
ob_start();
echo "A";
ob_start();
echo "B";
ob_end_flush();
echo "C";
echo ob_get_clean();
"#,
    );
    assert_eq!(out, "ABC");
}

/// Verifies ob_get_level reports the nesting depth at each point.
#[test]
fn test_ob_get_level_tracks_nesting() {
    let out = compile_and_run(
        r#"<?php
$l0 = ob_get_level();
ob_start();
$l1 = ob_get_level();
ob_start();
$l2 = ob_get_level();
ob_end_clean();
ob_end_clean();
echo $l0, $l1, $l2, ob_get_level();
"#,
    );
    assert_eq!(out, "0120");
}

/// Verifies ob_get_length returns the byte count, and false with no active buffer.
#[test]
fn test_ob_get_length_and_false_case() {
    let out = compile_and_run(
        r#"<?php
var_dump(ob_get_length());
ob_start();
echo "12345";
$len = ob_get_length();
ob_end_clean();
var_dump($len);
"#,
    );
    assert_eq!(out, "bool(false)\nint(5)\n");
}

/// Verifies the no-buffer failure modes return false with PHP's notices
/// (`ob_get_contents`/`ob_get_clean` stay silent, the rest raise E_NOTICE).
#[test]
fn test_no_buffer_operations_return_false() {
    let out = compile_and_run(
        r#"<?php
var_dump(ob_get_contents());
var_dump(ob_get_clean());
var_dump(ob_end_clean());
var_dump(ob_end_flush());
var_dump(ob_flush());
var_dump(ob_clean());
var_dump(ob_get_flush());
"#,
    );
    assert_eq!(
        out,
        concat!(
            "bool(false)\n",
            "bool(false)\n",
            "Notice: ob_end_clean(): Failed to delete buffer. No buffer to delete\n",
            "bool(false)\n",
            "Notice: ob_end_flush(): Failed to delete and flush buffer. No buffer to delete or flush\n",
            "bool(false)\n",
            "Notice: ob_flush(): Failed to flush buffer. No buffer to flush\n",
            "bool(false)\n",
            "Notice: ob_clean(): Failed to delete buffer. No buffer to delete\n",
            "bool(false)\n",
            "Notice: ob_get_flush(): Failed to delete and flush buffer. No buffer to delete or flush\n",
            "bool(false)\n",
        )
    );
}

/// Verifies ob_clean truncates the buffer but keeps it active.
#[test]
fn test_ob_clean_truncates_and_keeps_buffer() {
    let out = compile_and_run(
        r#"<?php
ob_start();
echo "discarded";
ob_clean();
echo "kept";
$s = ob_get_clean();
echo "[", $s, "]";
"#,
    );
    assert_eq!(out, "[kept]");
}

/// Verifies ob_end_clean discards buffered output entirely.
#[test]
fn test_ob_end_clean_discards_output() {
    let out = compile_and_run(
        r#"<?php
echo "before|";
ob_start();
echo "invisible";
ob_end_clean();
echo "after";
"#,
    );
    assert_eq!(out, "before|after");
}

/// Verifies ob_flush writes to stdout while keeping the buffer active.
#[test]
fn test_ob_flush_writes_and_keeps_buffer() {
    let out = compile_and_run(
        r#"<?php
ob_start();
echo "first;";
ob_flush();
echo "second;";
$rest = ob_get_clean();
echo "rest=", $rest;
"#,
    );
    assert_eq!(out, "first;rest=second;");
}

/// Verifies ob_get_flush returns the contents and flushes them to stdout.
#[test]
fn test_ob_get_flush_returns_and_flushes() {
    let out = compile_and_run(
        r#"<?php
ob_start();
echo "body;";
$got = ob_get_flush();
echo "got=", $got;
"#,
    );
    assert_eq!(out, "body;got=body;");
}

/// Verifies a still-active buffer is flushed to stdout at normal script end.
#[test]
fn test_active_buffer_flushes_at_script_end() {
    let out = compile_and_run(
        r#"<?php
ob_start();
echo "flushed-at-exit";
"#,
    );
    assert_eq!(out, "flushed-at-exit");
}

/// Verifies nested still-active buffers flush bottom-up in chronological order.
#[test]
fn test_nested_buffers_flush_at_script_end() {
    let out = compile_and_run(
        r#"<?php
echo "a";
ob_start();
echo "b";
ob_start();
echo "c";
"#,
    );
    assert_eq!(out, "abc");
}

/// Verifies exit() drains active buffers before terminating the process.
#[test]
fn test_exit_flushes_active_buffers() {
    let out = compile_and_run(
        r#"<?php
ob_start();
echo "before-exit";
exit(0);
"#,
    );
    assert_eq!(out, "before-exit");
}

/// Verifies print_r's structural array output is captured by an active buffer.
#[test]
fn test_print_r_array_is_captured() {
    let out = compile_and_run(
        r#"<?php
ob_start();
print_r([1, 2]);
$dump = ob_get_clean();
echo "captured:", $dump;
"#,
    );
    assert_eq!(out, "captured:Array\n(\n    [0] => 1\n    [1] => 2\n)\n");
}

/// Verifies printf output is captured and its byte-count result is preserved.
#[test]
fn test_printf_is_captured_and_returns_count() {
    let out = compile_and_run(
        r#"<?php
ob_start();
$n = printf("num=%d!", 42);
$s = ob_get_clean();
echo $s, "|", $n;
"#,
    );
    assert_eq!(out, "num=42!|7");
}

/// Verifies var_dump scalar and array walkers are captured by an active buffer.
#[test]
fn test_var_dump_is_captured() {
    let out = compile_and_run(
        r#"<?php
ob_start();
var_dump([1, "x"]);
var_dump(true);
$dump = ob_get_clean();
echo strlen($dump) > 0 ? "captured" : "lost", ":", $dump;
"#,
    );
    assert_eq!(
        out,
        "captured:array(2) {\n  [0]=>\n  int(1)\n  [1]=>\n  string(1) \"x\"\n}\nbool(true)\n"
    );
}

/// Verifies readfile streaming output is captured by an active buffer.
#[test]
fn test_readfile_is_captured() {
    let out = compile_and_run(
        r#"<?php
file_put_contents("ob_data.txt", "streamed-bytes");
ob_start();
readfile("ob_data.txt");
$got = ob_get_clean();
echo "got:", $got;
"#,
    );
    assert_eq!(out, "got:streamed-bytes");
}

/// Verifies fpassthru streaming output is captured by an active buffer.
#[test]
fn test_fpassthru_is_captured() {
    let out = compile_and_run(
        r#"<?php
file_put_contents("ob_pass.txt", "pass-bytes");
ob_start();
$h = fopen("ob_pass.txt", "r");
fpassthru($h);
fclose($h);
$got = ob_get_clean();
echo "got:", $got;
"#,
    );
    assert_eq!(out, "got:pass-bytes");
}

/// Verifies buffered output can grow far beyond the initial 1 KiB capacity.
#[test]
fn test_buffer_grows_beyond_initial_capacity() {
    let out = compile_and_run(
        r#"<?php
ob_start();
echo str_repeat("ab", 5000);
$s = ob_get_clean();
echo strlen($s), ":", substr($s, 0, 4), ":", substr($s, -4);
"#,
    );
    assert_eq!(out, "10000:abab:abab");
}

/// Verifies ob_get_status() simple mode reports PHP's default-handler shape.
#[test]
fn test_ob_get_status_simple_mode() {
    let out = compile_and_run(
        r#"<?php
ob_start();
echo "12345";
$st = ob_get_status();
ob_end_clean();
echo $st["name"], "|", $st["type"], "|", $st["flags"], "|", $st["level"], "|";
echo $st["chunk_size"], "|", $st["buffer_size"], "|", $st["buffer_used"];
"#,
    );
    assert_eq!(out, "default output handler|0|112|0|0|16384|5");
}

/// Verifies ob_get_status(true) returns one entry per nesting level.
#[test]
fn test_ob_get_status_full_mode() {
    let out = compile_and_run(
        r#"<?php
ob_start();
echo "xx";
ob_start();
echo "yyy";
$st = ob_get_status(true);
ob_end_clean();
ob_end_clean();
echo count($st), "|";
echo $st[0]["level"], ":", $st[0]["buffer_used"], "|";
echo $st[1]["level"], ":", $st[1]["buffer_used"];
"#,
    );
    assert_eq!(out, "2|0:2|1:3");
}

/// Verifies ob_get_status() with no active buffer returns an empty array.
#[test]
fn test_ob_get_status_empty_without_buffer() {
    let out = compile_and_run(
        r#"<?php
$st = ob_get_status();
echo count($st);
"#,
    );
    assert_eq!(out, "0");
}

/// Verifies ob_list_handlers reports the default handler once per level.
#[test]
fn test_ob_list_handlers_reports_default_handler() {
    let out = compile_and_run(
        r#"<?php
echo count(ob_list_handlers()), "|";
ob_start();
ob_start();
$handlers = ob_list_handlers();
ob_end_clean();
ob_end_clean();
echo count($handlers), "|", $handlers[0], "|", $handlers[1];
"#,
    );
    assert_eq!(out, "0|2|default output handler|default output handler");
}

/// Verifies ob_implicit_flush accepts a flag and returns true like PHP 8.
#[test]
fn test_ob_implicit_flush_returns_true() {
    let out = compile_and_run(
        r#"<?php
var_dump(ob_implicit_flush(false));
var_dump(ob_implicit_flush(true));
var_dump(ob_implicit_flush());
"#,
    );
    assert_eq!(out, "bool(true)\nbool(true)\nbool(true)\n");
}

/// Verifies ob_start with an explicit null callback succeeds.
#[test]
fn test_ob_start_null_callback_and_options() {
    let out = compile_and_run(
        r#"<?php
$started = ob_start(null, 4096, 112);
echo "captured";
$s = ob_get_clean();
var_dump($started);
echo "[", $s, "]";
"#,
    );
    assert_eq!(out, "bool(true)\n[captured]");
}

/// Verifies ob_* builtins resolve case-insensitively like every PHP builtin.
#[test]
fn test_ob_builtins_case_insensitive() {
    let out = compile_and_run(
        r#"<?php
OB_START();
echo "ci";
echo Ob_Get_Level();
echo strtoupper(OB_GET_CLEAN());
"#,
    );
    assert_eq!(out, "CI1");
}

/// Verifies a buffer started inside eval() captures static echoes, and static
/// buffers capture eval'd echoes: one shared runtime buffer stack.
#[test]
fn test_ob_state_is_shared_between_static_and_eval() {
    let out = compile_and_run(
        r#"<?php
eval('ob_start(); echo "e1;";');
echo "s1;";
eval('$x = ob_get_clean(); echo strtoupper($x);');
echo "|";
ob_start();
eval('echo "e2";');
$inner = ob_get_clean();
echo "|", $inner;
"#,
    );
    assert_eq!(out, "E1;S1;||e2");
}

/// Verifies eval'd ob queries observe eval'd buffered writes (level, length,
/// status, handlers) through the shared bridge.
#[test]
fn test_eval_ob_queries_through_bridge() {
    let out = compile_and_run(
        r#"<?php
eval('
    ob_start();
    echo "abcd";
    $st = ob_get_status();
    $n = ob_get_length();
    $lvl = ob_get_level();
    $handlers = ob_list_handlers();
    ob_end_clean();
    echo $n, "|", $lvl, "|", $st["buffer_used"], "|", count($handlers), "|", $handlers[0];
');
"#,
    );
    assert_eq!(out, "4|1|4|1|default output handler");
}

/// Verifies a closure output handler transforms the flushed contents with
/// PHP's phase bits (START|FINAL on a single flush).
#[test]
fn test_ob_start_closure_handler_transforms_output() {
    let out = compile_and_run(
        r#"<?php
ob_start(function (string $b, int $p): string { return "<" . $b . ":" . $p . ">"; });
echo "body";
ob_end_flush();
"#,
    );
    assert_eq!(out, "<body:9>");
}

/// Verifies a function-name string handler resolves and transforms output.
#[test]
fn test_ob_start_named_handler_transforms_output() {
    let out = compile_and_run(
        r#"<?php
function shout(string $buffer, int $phase): string {
    return strtoupper($buffer);
}
ob_start('shout');
echo "loud";
ob_end_flush();
"#,
    );
    assert_eq!(out, "LOUD");
}

/// Verifies a first-class-callable handler works. elephc materializes the
/// first-class callable of a named function as that function's descriptor, so
/// the reported handler name is the function name (PHP reports
/// "Closure::__invoke" here); anonymous closures report Closure::__invoke.
#[test]
fn test_ob_start_first_class_callable_handler() {
    let out = compile_and_run(
        r#"<?php
function wrap(string $b, int $p): string { return "[" . $b . "]"; }
ob_start(wrap(...));
echo "fcc";
$name = ob_list_handlers()[0];
ob_end_flush();
echo "|", $name, "|";
ob_start(function ($b, $p) { return $b; });
echo ob_list_handlers()[0];
$anon = ob_get_clean();
echo $anon;
"#,
    );
    assert_eq!(out, "[fcc]|wrap|Closure::__invoke");
}

/// Verifies a handler returning false passes the raw contents through, and a
/// handler returning a non-string is cast to a string like PHP.
#[test]
fn test_ob_handler_return_coercion() {
    let out = compile_and_run(
        r#"<?php
ob_start(function ($b, $p) { return false; });
echo "raw";
ob_end_flush();
echo "|";
ob_start(function ($b, $p) { return 42; });
echo "gone";
ob_end_flush();
"#,
    );
    assert_eq!(out, "raw|42");
}

/// Verifies ob_get_clean returns the RAW contents while the handler result is
/// discarded (CLEAN|FINAL phases), matching PHP.
#[test]
fn test_ob_get_clean_returns_raw_with_handler() {
    let out = compile_and_run(
        r#"<?php
ob_start(function ($b, $p) { return "SHOULD-NOT-APPEAR"; });
echo "raw-contents";
var_dump(ob_get_clean());
"#,
    );
    assert_eq!(out, "string(12) \"raw-contents\"\n");
}

/// Verifies ob_get_flush flushes the TRANSFORMED bytes but returns the RAW
/// contents, matching PHP.
#[test]
fn test_ob_get_flush_flushes_transformed_returns_raw() {
    let out = compile_and_run(
        r#"<?php
ob_start(function ($b, $p) { return strtoupper($b); });
echo "mixed";
$raw = ob_get_flush();
echo "|", $raw;
"#,
    );
    assert_eq!(out, "MIXED|mixed");
}

/// Verifies the chunk-size threshold auto-flushes with the WRITE phase and the
/// remainder flushes on end with FINAL, exactly like PHP.
#[test]
fn test_chunk_size_auto_flush_with_handler() {
    let out = compile_and_run(
        r#"<?php
ob_start(function ($b, $p) { return "{" . $b . ":" . $p . "}"; }, 5);
echo "123";
echo "456";
echo "78";
ob_end_flush();
"#,
    );
    assert_eq!(out, "{123456:1}{78:8}");
}

/// Verifies chunked default-handler buffers auto-flush raw output at the
/// threshold and report PHP's page-aligned buffer_size.
#[test]
fn test_chunk_size_default_handler_and_buffer_size() {
    let out = compile_and_run(
        r#"<?php
ob_start(null, 5);
echo "123456";
echo "[mid]";
$size = ob_get_status()["buffer_size"];
var_dump(ob_get_clean());
echo $size;
"#,
    );
    assert_eq!(out, "123456[mid]string(0) \"\"\n4096");
}

/// Verifies PHP's flags gating: a flags=0 buffer refuses clean/flush/end with
/// the named notices, and everything still flushes at script end.
#[test]
fn test_flags_gate_operations_with_notices() {
    let out = compile_and_run(
        r#"<?php
ob_start(null, 0, 0);
echo "locked;";
var_dump(ob_clean());
var_dump(ob_end_flush());
"#,
    );
    assert_eq!(
        out,
        concat!(
            "locked;",
            "Notice: ob_clean(): Failed to delete buffer of default output handler (0)\n",
            "bool(false)\n",
            "Notice: ob_end_flush(): Failed to send buffer of default output handler (0)\n",
            "bool(false)\n",
        )
    );
}

/// Verifies partial flags: CLEANABLE admits ob_clean, REMOVABLE alone admits
/// every end/get variant, matching PHP.
#[test]
fn test_partial_flags_admit_matching_operations() {
    let out = compile_and_run(
        r#"<?php
ob_start(null, 0, 16);
echo "a";
var_dump(ob_clean());
ob_start(null, 0, 64);
echo "b";
var_dump(ob_get_clean());
"#,
    );
    assert_eq!(out, "bool(true)\nstring(1) \"b\"\n");
}

/// Verifies an unknown function-name callback raises PHP's warning + notice and
/// leaves the stack unchanged.
#[test]
fn test_ob_start_unknown_function_name_rejected() {
    let out = compile_and_run(
        r#"<?php
$r = ob_start('no_such_handler_fn');
echo $r === false ? "rejected" : "started";
echo "|", ob_get_level();
"#,
    );
    assert_eq!(
        out,
        concat!(
            "Warning: ob_start(): function \"no_such_handler_fn\" not found or invalid function name\n",
            "Notice: ob_start(): Failed to create buffer\n",
            "rejected|0",
        )
    );
}

/// Verifies output produced inside a handler is discarded like PHP.
#[test]
fn test_output_inside_handler_is_discarded() {
    let out = compile_and_run(
        r#"<?php
ob_start();
ob_start(function ($b, $p) { echo "[leak]"; return "{" . $b . "}"; });
echo "x";
ob_end_flush();
var_dump(ob_get_clean());
"#,
    );
    assert_eq!(out, "string(3) \"{x}\"\n");
}

/// Verifies ob_get_status reports the user handler's name, type, and flags
/// (user bit before the first run; started/processed bits after).
#[test]
fn test_ob_get_status_user_handler_fields() {
    let out = compile_and_run(
        r#"<?php
function h(string $b, int $p): string { return $b; }
ob_start('h');
echo "xy";
$before = ob_get_status();
ob_flush();
$after = ob_get_status();
ob_end_clean();
echo $before["name"], "|", $before["type"], "|", $before["flags"], "|", $after["flags"];
"#,
    );
    assert_eq!(out, "xyh|1|113|20593");
}

/// Verifies handlers cascade across nested buffers: the inner transformed
/// output folds into the outer buffer and is transformed again.
#[test]
fn test_nested_handlers_cascade() {
    let out = compile_and_run(
        r#"<?php
function outer($b, $p) { return "<" . $b . ">"; }
function inner($b, $p) { return "[" . $b . "]"; }
ob_start('outer');
echo "a";
ob_start('inner');
echo "b";
ob_end_flush();
ob_end_flush();
"#,
    );
    assert_eq!(out, "<a[b]>");
}

/// Verifies a still-active handler buffer is transformed at script end.
#[test]
fn test_handler_applies_at_script_end() {
    let out = compile_and_run(
        r#"<?php
ob_start(function ($b, $p) { return "END<" . $b . ":" . $p . ">"; });
echo "tail";
"#,
    );
    assert_eq!(out, "END<tail:9>");
}

/// Verifies eval-registered handlers run from eval flushes, static flushes of
/// eval-started buffers, and the script-end drain: one shared handler registry.
#[test]
fn test_eval_handlers_across_boundaries() {
    let out = compile_and_run(
        r#"<?php
eval('ob_start(function ($b, $p) { return "<" . $b . ":" . $p . ">"; }); echo "ev"; ob_end_flush();');
echo "|";
eval('function xh($b, $p) { return strtoupper($b); } ob_start("xh"); echo "cross";');
echo "-static";
ob_end_flush();
echo "|";
eval('ob_start(function ($b, $p) { return "END[" . $b . "]"; }); echo "tail";');
"#,
    );
    assert_eq!(out, "<ev:9>|CROSS-STATIC|END[tail]");
}
