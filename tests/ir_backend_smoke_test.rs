//! Purpose:
//! Integration smoke tests for the opt-in EIR backend CLI path.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - These tests exercise the binary-level `--ir-backend` path instead of only
//!   testing library helpers.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

/// Returns the path to the cargo-built `elephc` binary.
fn elephc_cli_bin() -> String {
    std::env::var("CARGO_BIN_EXE_elephc").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().expect("failed to resolve current test binary");
        path.pop();
        if path.ends_with("deps") {
            path.pop();
        }
        path.join("elephc").to_string_lossy().into_owned()
    })
}

/// Verifies the IR backend compiles, links, and runs straight-line scalar echo programs.
#[test]
fn ir_backend_echoes_scalar_literals() {
    for (name, source, expected) in [
        ("int", "<?php echo 42;", "42"),
        ("string", "<?php echo \"hi\";", "hi"),
        ("bool_true", "<?php echo true;", "1"),
        ("bool_false", "<?php echo false;", ""),
        ("null", "<?php echo null;", ""),
        ("float", "<?php echo 1.5;", "1.5"),
        ("local_store", "<?php $x = 40; echo $x;", "40"),
        ("argc_load", "<?php echo $argc;", "1"),
        ("iadd", "<?php echo $argc + 2;", "3"),
        ("isub", "<?php echo $argc - 1;", "0"),
        ("imul", "<?php echo $argc * 3;", "3"),
    ] {
        let output = compile_and_run_ir_backend(name, source);
        assert_eq!(output, expected, "unexpected stdout for {name}");
    }
}

/// Verifies integer comparisons and conditional branches on both branch directions.
#[test]
fn ir_backend_branches_on_integer_comparison() {
    let source = "<?php if ($argc > 1) { echo 9; } else { echo 4; }";
    assert_eq!(compile_and_run_ir_backend("if_false", source), "4");
    assert_eq!(
        compile_and_run_ir_backend_with_args("if_true", source, &["extra"]),
        "9"
    );
}

/// Verifies branch back-edges and repeated local slot updates in a while loop.
#[test]
fn ir_backend_runs_simple_while_loop() {
    let source = "<?php $i = 0; while ($i < 3) { echo $i; $i = $i + 1; }";
    assert_eq!(compile_and_run_ir_backend("while_loop", source), "012");
}

/// Verifies scalar EIR opcodes that are emitted for arithmetic, truthiness, and string coercion.
#[test]
fn ir_backend_handles_scalar_ops_and_string_coercions() {
    for (name, source, expected) in [
        ("idiv", "<?php echo 7 / 2;", "3.5"),
        ("imod", "<?php echo 7 % 4;", "3"),
        ("ineg", "<?php echo -$argc;", "-1"),
        ("bitwise", "<?php echo 6 & 3; echo 4 | 1; echo 7 ^ 3;", "254"),
        ("shifts", "<?php echo 1 << 3; echo -8 >> 1;", "8-4"),
        (
            "float_ops",
            "<?php echo 1.5 + 2.0; echo 5.0 / 2.0; echo -1.5;",
            "3.52.5-1.5",
        ),
        (
            "truthy_strings",
            "<?php if (\"0\") { echo 1; } else { echo 0; } if (\"hi\") { echo 2; }",
            "02",
        ),
        ("null_coalesce", "<?php $x = null; echo $x ?? 5;", "5"),
        ("concat_int", "<?php echo $argc . \"x\";", "1x"),
        ("concat_false", "<?php echo false . \"x\";", "x"),
        ("concat_null", "<?php echo null . \"x\";", "x"),
        ("error_suppress_expr", "<?php echo @(\"ok\");", "ok"),
    ] {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }
}

/// Verifies PHP logical xor evaluates both operands and compares canonical truthiness.
#[test]
fn ir_backend_handles_logical_xor() {
    assert_eq!(
        compile_and_run_ir_backend("logical_xor_truthy_ints", "<?php echo ($argc xor 2) ? \"T\" : \"F\"; echo \":\"; echo ($argc xor 0) ? \"T\" : \"F\";"),
        "F:T"
    );
    assert_eq!(
        compile_and_run_ir_backend(
            "logical_xor_evaluates_rhs",
            "<?php function mark() { echo \"rhs\"; return false; } $r = (true xor mark()); echo $r ? \"T\" : \"F\";",
        ),
        "rhsT"
    );
}

/// Verifies scalar equality opcodes generated for loose comparisons, strict comparisons, and match.
#[test]
fn ir_backend_handles_scalar_equality() {
    for (name, source, expected) in [
        ("loose_int_eq", "<?php if ($argc == 1) { echo 1; }", "1"),
        ("loose_int_ne", "<?php if ($argc != 2) { echo 2; }", "2"),
        ("strict_int_eq", "<?php if (1 === 1) { echo 3; }", "3"),
        ("strict_int_ne", "<?php if (1 !== 2) { echo 4; }", "4"),
        ("strict_type_mismatch", "<?php if (1 !== true) { echo 5; }", "5"),
        ("loose_bool_truthy", "<?php if (($argc + 1) == true) { echo 6; }", "6"),
        ("strict_string_eq", "<?php if (\"a\" === \"a\") { echo 7; }", "7"),
        ("strict_string_ne", "<?php if (\"a\" !== \"b\") { echo 8; }", "8"),
        ("loose_string_eq", "<?php if (\"a\" == \"a\") { echo 9; }", "9"),
        ("loose_string_ne", "<?php if (\"a\" != \"b\") { echo 10; }", "10"),
        ("match_int", "<?php echo match ($argc) { 1 => 11, default => 0 };", "11"),
    ] {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }
}

/// Verifies print output and scalar switch dispatch through the EIR backend.
#[test]
fn ir_backend_handles_print_and_switch() {
    assert_eq!(
        compile_and_run_ir_backend("print_expr", "<?php print \"p\"; echo print \"q\";"),
        "pq1"
    );

    let switch_source = "<?php switch ($argc) { case 1: echo 1; break; case 2: echo 2; break; default: echo 9; }";
    assert_eq!(compile_and_run_ir_backend("switch_case_one", switch_source), "1");
    assert_eq!(
        compile_and_run_ir_backend_with_args("switch_case_two", switch_source, &["extra"]),
        "2"
    );
    assert_eq!(
        compile_and_run_ir_backend_with_args(
            "switch_default",
            switch_source,
            &["extra", "another"]
        ),
        "9"
    );
}

/// Verifies direct user-defined function calls with scalar params and returns.
#[test]
fn ir_backend_calls_user_functions() {
    for (name, source, expected) in [
        ("fn_return", "<?php function f() { return 42; } echo f();", "42"),
        (
            "fn_add",
            "<?php function add($a, $b) { return $a + $b; } echo add(2, 3);",
            "5",
        ),
        (
            "fn_void",
            "<?php function twice($x) { echo $x; echo $x; } twice(7);",
            "77",
        ),
        (
            "fn_stack_int_arg",
            "<?php function pick($a, $b, $c, $d, $e, $f, $g, $h, $i) { echo $i; } pick(1, 2, 3, 4, 5, 6, 7, 8, 9);",
            "9",
        ),
        (
            "fn_stack_string_arg",
            "<?php function tail($a, $b, $c, $d, $e, $f, $g, $s) { echo $s; } tail(1, 2, 3, 4, 5, 6, 7, \"tail\");",
            "tail",
        ),
    ] {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }
}

/// Verifies positional calls append omitted optional parameter defaults before EIR call emission.
#[test]
fn ir_backend_handles_positional_default_parameters() {
    let source = r#"<?php
function add_ten(int $value = 10): int {
    return $value + 10;
}
class Box {
    public int $id;
    public function __construct(int $id = 42) {
        $this->id = $id;
    }
    public function add(int $value = 10): int {
        return $value + 1;
    }
    public static function stat(int $value = 20): int {
        return $value + 2;
    }
}
echo add_ten();
echo "|";
echo add_ten(5);
echo "|";
$box = new Box();
echo $box->id;
echo "|";
$other = new Box(7);
echo $other->id;
echo "|";
echo $box->add();
echo "|";
echo $box->add(4);
echo "|";
echo Box::stat();
echo "|";
echo Box::stat(5);
"#;
    assert_eq!(
        compile_and_run_ir_backend("positional_default_parameters", source),
        "20|15|42|7|11|5|22|7"
    );
}

/// Verifies positional variadic calls collect tail arguments into the variadic array parameter.
#[test]
fn ir_backend_handles_positional_variadic_parameters() {
    let source = r#"<?php
function num_args(...$args) {
    return count($args);
}
function head_count($head, ...$rest) {
    echo $head;
    echo ":";
    echo count($rest);
}
function sum_values(...$nums) {
    $total = 0;
    foreach ($nums as $n) {
        $total += $n;
    }
    return $total;
}
class Counter {
    public function inst($head, ...$rest) {
        echo $head;
        echo ":";
        echo count($rest);
    }
    public static function stat($head, ...$rest) {
        echo $head;
        echo ":";
        echo count($rest);
    }
}
echo num_args(10, 20, 30, 40);
echo "|";
echo num_args();
echo "|";
head_count(7, 8, 9);
echo "|";
head_count(7);
echo "|";
$counter = new Counter();
$counter->inst(7, 8, 9);
echo "|";
Counter::stat(4, 5, 6, 7);
echo "|";
echo sum_values(1, 2, 3);
echo "|";
echo sum_values();
"#;
    assert_eq!(
        compile_and_run_ir_backend("positional_variadic_parameters", source),
        "4|0|7:2|7:0|7:2|4:3|6|0"
    );
}

/// Verifies pipe calls with static first-class callable targets lower to direct EIR calls.
#[test]
fn ir_backend_handles_static_pipe_calls() {
    for (name, source, expected) in [
        (
            "pipe_user_function",
            "<?php function double($x) { return $x * 2; } echo 3 |> double(...);",
            "6",
        ),
        (
            "pipe_user_function_default",
            "<?php function suffix($value, $tail = \"!\") { return $value . $tail; } echo \"go\" |> suffix(...);",
            "go!",
        ),
        ("pipe_builtin", "<?php echo \"abc\" |> strlen(...);", "3"),
        (
            "pipe_static_method",
            "<?php class MathBox { public static function inc($x) { return $x + 1; } } echo 3 |> MathBox::inc(...);",
            "4",
        ),
        (
            "pipe_instance_method",
            "<?php class Box { public function add($x) { return $x + 4; } } $b = new Box(); echo 3 |> $b->add(...);",
            "7",
        ),
    ] {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }
}

/// Verifies `global` aliases share storage with top-level variables in the EIR backend.
#[test]
fn ir_backend_handles_global_aliases() {
    for (name, source, expected) in [
        (
            "global_read",
            "<?php $x = 5; function show() { global $x; echo $x; } show();",
            "5",
        ),
        (
            "global_write",
            "<?php $x = 1; function f() { global $x; $x = $x + 2; } f(); echo $x;",
            "3",
        ),
        (
            "global_multiple",
            "<?php $a = 1; $b = 2; function bump() { global $a, $b; $a = $a + 10; $b = $b + 20; } bump(); echo $a; echo \":\"; echo $b;",
            "11:22",
        ),
    ] {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }
}

/// Verifies function static locals initialize once and persist across direct calls.
#[test]
fn ir_backend_handles_function_static_locals() {
    for (name, source, expected) in [
        (
            "function_static_counter",
            "<?php function counter() { static $i = 0; $i = $i + 1; echo $i; } counter(); counter(); counter();",
            "123",
        ),
        (
            "function_static_separate_slots",
            "<?php function a() { static $x = 0; $x = $x + 1; echo $x; } function b() { static $x = 0; $x = $x + 10; echo $x; } a(); b(); a(); b();",
            "110220",
        ),
    ] {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }
}

/// Verifies fatal terminators emitted for implicit `never` returns write the legacy diagnostic.
#[test]
fn ir_backend_handles_fatal_never_implicit_return() {
    let run = compile_ir_backend_and_run(
        "fatal_never_implicit_return",
        "<?php function fail(): never { } fail(); echo \"unreachable\";",
        &[],
    );
    assert!(
        !run.status.success(),
        "IR backend fatal fixture unexpectedly succeeded"
    );
    assert_eq!(
        String::from_utf8(run.stdout).expect("fatal stdout should be utf8"),
        ""
    );
    let stderr = String::from_utf8(run.stderr).expect("fatal stderr should be utf8");
    assert!(
        stderr.contains("Fatal error: A never-returning function must not implicitly return"),
        "unexpected fatal stderr: {stderr}"
    );
}

/// Verifies scalar builtin calls lowered by the EIR backend.
#[test]
fn ir_backend_handles_scalar_builtins() {
    for (name, source, expected) in [
        ("strlen", "<?php echo strlen(\"hello\");", "5"),
        (
            "pi_and_phpversion",
            "<?php echo pi() > 3 ? \"pi\" : \"bad\"; echo \":\"; echo phpversion();",
            concat!("pi:", env!("CARGO_PKG_VERSION")),
        ),
        ("intval_float", "<?php echo intval(3.9);", "3"),
        ("intval_str", "<?php echo intval(\"42xyz\");", "42"),
        (
            "intval_mixed_param",
            "<?php function cast_it(mixed $value) { echo intval($value); } cast_it(\"42xyz\"); echo ':'; cast_it(3.9);",
            "42:3",
        ),
        ("floatval_int", "<?php echo floatval(2) + 0.5;", "2.5"),
        ("floatval_str", "<?php echo floatval(\"2.5x\");", "2.5"),
        ("boolval_false", "<?php echo boolval(\"0\");", ""),
        ("boolval_true", "<?php echo boolval(\"hi\");", "1"),
        (
            "type_predicates",
            "<?php echo is_int(1); echo is_float(1.5); echo is_bool(false); echo is_null(null); echo is_string(\"x\");",
            "11111",
        ),
        (
            "is_numeric_scalars",
            "<?php echo is_numeric(1) ? '1' : '0'; echo is_numeric(1.5) ? '1' : '0'; echo is_numeric(true) ? '1' : '0'; echo is_numeric('42') ? '1' : '0'; echo is_numeric('-1.5') ? '1' : '0'; echo is_numeric('.') ? '1' : '0'; echo is_numeric('x') ? '1' : '0';",
            "1101100",
        ),
        (
            "abs_scalars",
            "<?php echo abs(-42); echo ':'; echo abs(-3.5);",
            "42:3.5",
        ),
        (
            "min_max_ints",
            "<?php echo min(3, 7); echo ':'; echo max(3, 7); echo ':'; echo min(3, 1, 2); echo ':'; echo max(1, 3, 2);",
            "3:7:1:3",
        ),
        (
            "min_max_floats",
            "<?php echo min(1.5, 2.5); echo ':'; echo max(1.5, 2.5);",
            "1.5:2.5",
        ),
        (
            "clamp_ints",
            "<?php echo clamp(5, 0, 10); echo ':'; echo clamp(15, 0, 10); echo ':'; echo clamp(-5, 0, 10); echo ':'; echo clamp(0, 0, 10); echo ':'; echo clamp(10, 0, 10);",
            "5:10:0:0:10",
        ),
        (
            "clamp_floats",
            "<?php echo clamp(2.75, 1.5, 2.5); echo ':'; echo clamp(2, 1.5, 3.5);",
            "2.5:2",
        ),
        (
            "clamp_strings",
            "<?php echo clamp('P', 'A', 'C') . ':' . clamp('P', 'X', 'Z');",
            "C:X",
        ),
        (
            "rounding_math",
            "<?php echo floor(3.7); echo ':'; echo ceil(3.2); echo ':'; echo round(3.5); echo ':'; echo round(3.555, 2);",
            "3:4:4:3.56",
        ),
        (
            "sqrt_math",
            "<?php echo sqrt(16.0); echo ':'; echo sqrt(2.0);",
            "4:1.4142135623731",
        ),
        (
            "binary_numeric_math",
            "<?php echo intdiv(7, 2); echo ':'; echo fdiv(10, 4); echo ':'; echo fmod(10.5, 3.2); echo ':'; echo pow(2.0, 10.0);",
            "3:2.5:0.9:1024",
        ),
        (
            "trig_math",
            "<?php echo round(sin(0.0), 4); echo ':'; echo round(cos(0.0), 4); echo ':'; echo round(tan(0.0), 4);",
            "0:1:0",
        ),
        (
            "inverse_and_hyperbolic_math",
            "<?php echo round(asin(1.0), 4); echo ':'; echo round(acos(0.0), 4); echo ':'; echo round(atan(1.0), 4); echo ':'; echo round(sinh(0.0), 4); echo ':'; echo round(cosh(0.0), 4); echo ':'; echo round(tanh(0.0), 4);",
            "1.5708:1.5708:0.7854:0:1:0",
        ),
        (
            "log_exp_and_distance_math",
            "<?php echo round(log(exp(1.0)), 4); echo ':'; echo log2(8.0); echo ':'; echo log10(1000.0); echo ':'; echo exp(0.0); echo ':'; echo hypot(3.0, 4.0); echo ':'; echo round(atan2(1.0, 0.0), 4);",
            "1:3:3:1:5:1.5708",
        ),
        (
            "angle_and_log_base_math",
            "<?php echo round(deg2rad(180.0), 4); echo ':'; echo round(rad2deg(pi()), 1); echo ':'; echo log(1000, 10);",
            "3.1416:180:3",
        ),
        (
            "random_integer_math",
            "<?php echo rand(1, 1); echo ':'; echo mt_rand(5, 5); echo ':'; echo random_int(42, 42); echo ':'; $r = rand(); echo $r >= 0 ? 'ok' : 'bad';",
            "1:5:42:ok",
        ),
        (
            "number_format_strings",
            "<?php echo number_format(1234567); echo ':'; echo number_format(1234.5678, 2); echo ':'; echo number_format(1234567.89, 2, ',', '.'); echo ':'; echo number_format(1234567.89, 2, '.', '');",
            "1,234,567:1,234.57:1.234.567,89:1234567.89",
        ),
        (
            "string_transforms",
            "<?php echo strtolower('Hello WORLD'); echo ':'; echo strtoupper('Hello World'); echo ':'; echo ucfirst('hello'); echo ':'; echo lcfirst('Hello'); echo ':'; echo strrev('Hello');",
            "hello world:HELLO WORLD:Hello:hello:olleH",
        ),
        (
            "grapheme_strrev_strings",
            r#"<?php echo grapheme_strrev("ABCDE"); echo ':'; echo grapheme_strrev("ab\0cd");"#,
            "EDCBA:dc\0ba",
        ),
        (
            "str_pad_strings",
            r#"<?php echo '[' . str_pad("hi", 5) . ']'; echo ':'; echo '[' . str_pad("hi", 5, "_", 0) . ']'; echo ':'; echo '[' . str_pad("hi", 6, "-", 2) . ']'; echo ':'; echo '[' . str_pad("42", 5, "0", 0) . ']';"#,
            "[hi   ]:[___hi]:[--hi--]:[00042]",
        ),
        (
            "trim_strings",
            "<?php echo trim('  hello  '); echo ':'; echo ltrim('  left'); echo ':'; echo rtrim('right  '); echo ':'; echo chop('tailxx', 'x'); echo ':'; echo trim('xyhelloxy', 'xy'); echo ':'; echo ltrim('..left', '.'); echo ':'; echo rtrim('right..', '.');",
            "hello:left:right:tail:hello:left:right",
        ),
        (
            "string_search_predicates",
            "<?php echo strcmp('abc', 'abc'); echo ':'; echo strcmp('abc', 'abd') < 0 ? 'lt' : 'ge'; echo ':'; echo strcasecmp('ABC', 'abc'); echo ':'; echo str_contains('hello world', 'world') ? '1' : '0'; echo str_contains('hello', 'z') ? '1' : '0'; echo str_starts_with('hello', 'he') ? '1' : '0'; echo str_starts_with('hello', 'zz') ? '1' : '0'; echo str_ends_with('hello', 'lo') ? '1' : '0'; echo str_ends_with('hello', 'zz') ? '1' : '0';",
            "0:lt:0:101010",
        ),
        (
            "string_position_mixed_results",
            "<?php echo '['; echo strpos('Hello World', 'Hello'); echo ']'; echo ':'; echo strpos('Hello World', 'World'); echo ':'; echo '['; echo strpos('Hello', 'xyz'); echo ']'; echo ':'; echo strrpos('abcabc', 'bc'); echo ':'; echo '['; echo strrpos('abcabc', 'zz'); echo ']';",
            "[0]:6:[]:4:[]",
        ),
        (
            "strstr_strings",
            "<?php echo strstr('Hello World', 'World'); echo ':'; echo '['; echo strstr('Hello', 'xyz'); echo ']'; echo ':'; echo strstr('abcabc', 'bc');",
            "World:[]:bcabc",
        ),
        (
            "substr_strings",
            "<?php echo substr('Hello World', 6); echo ':'; echo substr('Hello World', 0, 5); echo ':'; echo substr('Hello World', -5); echo ':'; echo '['; echo substr('Hello', 50); echo ']'; echo ':'; echo '['; echo substr('Hello', 1, -2); echo ']';",
            "World:Hello:World:[]:[]",
        ),
        (
            "substr_replace_strings",
            r#"<?php echo substr_replace("hello world", "PHP", 6, 5); echo ':'; echo substr_replace("hello world", "!", 5);"#,
            "hello PHP:hello!",
        ),
        (
            "str_repeat_strings",
            "<?php echo str_repeat('ab', 3); echo ':'; echo '['; echo str_repeat('x', 0); echo ']'; echo ':'; echo strlen(str_repeat('a', 5));",
            "ababab:[]:5",
        ),
        (
            "replace_strings",
            r#"<?php echo str_replace("World", "PHP", "Hello World"); echo ':'; echo str_replace("o", "0", "Hello World"); echo ':'; echo str_ireplace("WORLD", "PHP", "Hello World");"#,
            "Hello PHP:Hell0 W0rld:Hello PHP",
        ),
        (
            "ucwords_strings",
            "<?php echo ucwords('hello world'); echo ':'; echo ucwords(\"two\\twords\");",
            "Hello World:Two\tWords",
        ),
        (
            "ord_chr_strings",
            "<?php echo ord('A'); echo ':'; echo ord(''); echo ':'; echo chr(65);",
            "65:0:A",
        ),
        (
            "escape_and_hex_strings",
            r#"<?php echo addslashes("He said \"hi\" and it's ok"); echo ':'; echo stripslashes("He said \\\"hi\\\""); echo ':'; echo nl2br("line1\nline2"); echo ':'; echo wordwrap("The quick brown fox", 10, "|"); echo ':'; echo bin2hex("AB"); echo ':'; echo hex2bin("4142");"#,
            r#"He said \"hi\" and it\'s ok:He said "hi":line1<br />
line2:The quick |brown fox:4142:AB"#,
        ),
        (
            "html_entity_strings",
            r#"<?php echo htmlspecialchars("<b>\"Hi\" & 'bye'</b>"); echo ':'; echo htmlentities("<a>"); echo ':'; echo html_entity_decode("&lt;b&gt;hi&lt;/b&gt;"); echo ':'; echo html_entity_decode(htmlspecialchars("<div>\"test\"</div>"));"#,
            r#"&lt;b&gt;&quot;Hi&quot; &amp; &#039;bye&#039;&lt;/b&gt;:&lt;a&gt;:<b>hi</b>:<div>"test"</div>"#,
        ),
        (
            "url_and_base64_strings",
            r#"<?php echo urlencode("hello world&foo=bar"); echo ':'; echo urldecode("hello+world%26foo%3Dbar"); echo ':'; echo rawurlencode("hello world"); echo ':'; echo rawurldecode("hello%20world"); echo ':'; echo base64_encode("Hello"); echo ':'; echo base64_decode("SGVsbG8=");"#,
            "hello+world%26foo%3Dbar:hello world&foo=bar:hello%20world:hello world:SGVsbG8=:Hello",
        ),
        (
            "hash_strings",
            r#"<?php echo md5("Hello"); echo ':'; echo sha1("Hello"); echo ':'; echo hash("md5", "Hello"); echo ':'; echo hash("sha1", "Hello"); echo ':'; echo hash("sha256", "Hello");"#,
            "8b1a9953c4611296a827abf8c47804d7:f7ff9e8b7bb2e09b70935a5d785e0cc5d9d0abf0:8b1a9953c4611296a827abf8c47804d7:f7ff9e8b7bb2e09b70935a5d785e0cc5d9d0abf0:185f8db32271fe25f561a6fc938b2e264306ec304eda518007d1764826381969",
        ),
        (
            "sprintf_strings",
            r#"<?php echo sprintf("Hello %s %d %.2f %%", "age", 30, 3.14159); echo ':'; echo sprintf("%05d", 42);"#,
            "Hello age 30 3.14 %:00042",
        ),
        (
            "printf_strings",
            r#"<?php $n = printf("Hi %s", "Bob"); echo ':'; echo $n;"#,
            "Hi Bob:6",
        ),
        (
            "ctype_strings",
            "<?php echo ctype_alpha('Hello') ? '1' : '0'; echo ctype_alpha('Hello123') ? '1' : '0'; echo ctype_digit('12345') ? '1' : '0'; echo ctype_digit('123abc') ? '1' : '0'; echo ctype_alnum('Hello123') ? '1' : '0'; echo ctype_alnum('Hello 123') ? '1' : '0'; echo ctype_space(\" \\t\\n\") ? '1' : '0'; echo ctype_space('hello') ? '1' : '0';",
            "10101010",
        ),
        (
            "gettype_scalars",
            "<?php echo gettype(42); echo ':'; echo gettype(1.5); echo ':'; echo gettype('hi'); echo ':'; echo gettype(false); echo ':'; echo gettype(null);",
            "integer:double:string:boolean:NULL",
        ),
        (
            "float_type_predicates",
            "<?php echo is_nan(fdiv(0.0, 0.0)) ? '1' : '0'; echo is_nan(0) ? '1' : '0'; echo is_infinite(fdiv(1.0, 0.0)) ? '1' : '0'; echo is_infinite(fdiv(-1.0, 0.0)) ? '1' : '0'; echo is_infinite(42.0) ? '1' : '0'; echo is_finite(42.0) ? '1' : '0'; echo is_finite(fdiv(1.0, 0.0)) ? '1' : '0'; echo is_finite(fdiv(0.0, 0.0)) ? '1' : '0';",
            "10110100",
        ),
    ] {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }
}

/// Verifies direct-call materialization boxes concrete values passed to `mixed` parameters.
#[test]
fn ir_backend_handles_gettype_for_mixed_parameters() {
    let source = r#"<?php
class A {}
class Box {
    public function show(mixed $x): string { return gettype($x); }
    public static function stat(mixed $x): string { return gettype($x); }
}
class Constructed {
    public function __construct(mixed $x) { echo gettype($x); }
}
function describe(mixed $x): string {
    return gettype($x);
}
echo describe(42);
echo "|";
echo describe("s");
echo "|";
echo describe(null);
echo "|";
echo describe(true);
echo "|";
echo describe(1.5);
echo "|";
echo describe([1]);
echo "|";
$a = new A();
echo describe($a);
echo "|";
$b = new Box();
echo $b->show([1]);
echo "|";
echo Box::stat($b);
echo "|";
$c = new Constructed([1]);
"#;
    assert_eq!(
        compile_and_run_ir_backend("gettype_mixed_parameters", source),
        "integer|string|NULL|boolean|double|array|object|array|object|array"
    );
}

/// Verifies `unset($local)` writes PHP null into local slots on the EIR backend.
#[test]
fn ir_backend_handles_unset_locals() {
    for (name, source, expected) in [
        (
            "unset_int_local",
            "<?php $x = 42; unset($x); echo is_null($x) ? 'null' : 'value';",
            "null",
        ),
        (
            "unset_multiple_locals",
            "<?php $a = 1; $b = 'owned' . $argc; unset($a, $b); echo is_null($a) ? 'A' : 'a'; echo is_null($b) ? 'B' : 'b';",
            "AB",
        ),
    ] {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }
}

/// Verifies diagnostic output builtins lowered by the EIR backend for concrete values.
#[test]
fn ir_backend_handles_debug_output_builtins() {
    let source = r#"<?php
print_r(42);
echo "|";
print_r("hi");
echo "|";
print_r(true);
echo "|";
print_r(false);
echo "|";
print_r([1, 2]);
echo "---\n";
var_dump(42);
var_dump("hi");
var_dump(true);
var_dump(false);
var_dump(null);
var_dump(3.14);
var_dump([1, 2, 3]);
var_dump(["a" => 1, "b" => 2]);
"#;
    let expected =
        "42|hi|1||Array\n---\nint(42)\nstring(2) \"hi\"\nbool(true)\nbool(false)\nNULL\nfloat(3.14)\narray(3) {\n}\narray(2) {\n}\n";
    assert_eq!(
        compile_and_run_ir_backend("debug_output_builtins", source),
        expected
    );
}

/// Verifies diagnostic output builtins inspect boxed Mixed payloads from lowered helpers.
#[test]
fn ir_backend_handles_debug_output_for_mixed_values() {
    let source = r#"<?php
$ints = array_fill(0, 1, 42);
$floats = array_fill(0, 1, 1.5);
$bools = array_fill(0, 1, true);
$nulls = array_fill(0, 1, null);
$arrays = array_fill(0, 1, [1, 2]);
var_dump($ints[0]);
var_dump(grapheme_strrev("abc"));
var_dump($floats[0]);
var_dump($bools[0]);
var_dump($nulls[0]);
var_dump($arrays[0]);
echo "[";
print_r($ints[0]);
echo "|";
print_r(strpos("abc", "z"));
echo "]";
"#;
    assert_eq!(
        compile_and_run_ir_backend("debug_output_mixed_values", source),
        "int(42)\nstring(3) \"cba\"\nfloat(1.5)\nbool(true)\nNULL\narray(2) {\n}\n[42|]"
    );
}

/// Verifies heterogeneous associative arrays store and read boxed Mixed payloads.
#[test]
fn ir_backend_handles_mixed_associative_array_slots() {
    let source = r#"<?php
$map = [
    "i" => 42,
    "s" => "hello",
    "b" => true,
    "n" => null,
    "a" => [1, 2],
];
var_dump($map["i"]);
var_dump($map["s"]);
var_dump($map["b"]);
var_dump($map["n"]);
var_dump($map["a"]);
echo "[";
print_r($map["s"]);
echo "|";
print_r($map["n"]);
echo "]";
"#;
    assert_eq!(
        compile_and_run_ir_backend("mixed_assoc_array_slots", source),
        "int(42)\nstring(5) \"hello\"\nbool(true)\nNULL\narray(2) {\n}\n[hello|]"
    );
}

/// Verifies mixed numeric add/sub/mul dispatches through boxed Mixed runtime helpers.
#[test]
fn ir_backend_handles_mixed_numeric_binops() {
    let source = r#"<?php
$map = [
    "i" => 40,
    "f" => 1.5,
];
echo $map["i"] + 2;
echo ":";
echo $map["i"] - 5;
echo ":";
echo $map["i"] * 2;
echo ":";
echo $map["f"] + 2.5;
echo ":";
echo $map["f"] * 2;
"#;
    assert_eq!(
        compile_and_run_ir_backend("mixed_numeric_binops", source),
        "42:35:80:4:3"
    );
}

/// Verifies scalar extern calls are materialized through the target C ABI.
#[test]
fn ir_backend_handles_scalar_extern_calls() {
    let source = r#"<?php
extern function abs(int $n): int;
extern function getpid(): int;
echo abs(-42);
echo ":";
echo getpid() > 0 ? "pid" : "bad";
"#;
    assert_eq!(
        compile_and_run_ir_backend("scalar_extern_calls", source),
        "42:pid"
    );
}

/// Verifies extern calls marshal PHP strings to and from C strings.
#[test]
fn ir_backend_handles_string_extern_calls() {
    let source = r#"<?php
extern function atoi(string $s): int;
extern function strcmp(string $left, string $right): int;
extern function getenv(string $name): string;
echo atoi("99");
echo ":";
echo strcmp("aa", "ab") < 0 ? "lt" : "bad";
echo ":";
$path = getenv("PATH");
echo strlen($path) > 0 ? "env" : "empty";
"#;
    assert_eq!(
        compile_and_run_ir_backend("string_extern_calls", source),
        "99:lt:env"
    );
}

/// Verifies pointer extension builtins and pointer extern calls use raw-address ABI values.
#[test]
fn ir_backend_handles_basic_pointer_builtins() {
    let source = r#"<?php
extern function malloc(int $size): ptr;
extern function free(ptr $ptr): void;
$null = ptr_null();
echo ptr_is_null($null) ? "null" : "bad";
echo ":";
echo $null;
echo ":";
echo ptr_is_null(ptr_offset($null, 0)) ? "offset-null" : "bad";
echo ":";
$mem = malloc(1);
echo ptr_is_null($mem) ? "bad" : "allocated";
echo ":";
echo ptr_is_null(ptr_offset($mem, 0)) ? "bad" : "offset";
free($mem);
"#;
    assert_eq!(
        compile_and_run_ir_backend("basic_pointer_builtins", source),
        "null:0x0:offset-null:allocated:offset"
    );
}

/// Verifies raw pointer memory reads and writes through the EIR backend.
#[test]
fn ir_backend_handles_pointer_memory_builtins() {
    let source = r#"<?php
extern function malloc(int $size): ptr;
extern function free(ptr $ptr): void;
$buf = malloc(16);
ptr_set($buf, 123456789);
echo ptr_get($buf);
echo ":";
ptr_write8($buf, 255);
ptr_write8(ptr_offset($buf, 1), 1);
echo ptr_read8($buf);
echo ",";
echo ptr_read8(ptr_offset($buf, 1));
echo ":";
ptr_write16($buf, 0x1234);
echo ptr_read16($buf);
echo ":";
ptr_write32($buf, 305419896);
echo ptr_read32($buf);
free($buf);
"#;
    assert_eq!(
        compile_and_run_ir_backend("pointer_memory_builtins", source),
        "123456789:255,1:4660:305419896"
    );
}

/// Verifies pointer string copy builtins preserve byte counts through raw memory.
#[test]
fn ir_backend_handles_pointer_string_builtins() {
    let source = r#"<?php
extern function malloc(int $size): ptr;
extern function free(ptr $ptr): void;
$buf = malloc(16);
$written = ptr_write_string($buf, "GET /");
$s = ptr_read_string($buf, $written);
echo $written;
echo ":";
echo $s;
echo ":";
echo strlen(ptr_read_string($buf, 0));
free($buf);
"#;
    assert_eq!(
        compile_and_run_ir_backend("pointer_string_builtins", source),
        "5:GET /:0"
    );
}

/// Verifies pointer casts preserve the raw address while changing pointee metadata.
#[test]
fn ir_backend_handles_pointer_casts() {
    let source = r#"<?php
extern function malloc(int $size): ptr;
extern function free(ptr $ptr): void;
$buf = malloc(8);
ptr_set($buf, 77);
$typed = ptr_cast<int>($buf);
echo ptr_get($typed);
free($buf);
"#;
    assert_eq!(compile_and_run_ir_backend("pointer_casts", source), "77");
}

/// Verifies scalar buffer allocation preserves the declared logical length.
#[test]
fn ir_backend_handles_buffer_new_and_len() {
    let source = r#"<?php
buffer<int> $values = buffer_new<int>(7);
echo buffer_len($values);
"#;
    assert_eq!(compile_and_run_ir_backend("buffer_new_len", source), "7");
}

/// Verifies scalar buffer element reads and writes for integer and floating-point elements.
#[test]
fn ir_backend_handles_buffer_scalar_get_set() {
    let int_source = r#"<?php
buffer<int> $values = buffer_new<int>(3);
$values[0] = 4;
$values[1] = 5;
echo $values[0] + $values[1] + buffer_len($values);
"#;
    assert_eq!(compile_and_run_ir_backend("buffer_int_get_set", int_source), "12");

    let float_source = r#"<?php
buffer<float> $values = buffer_new<float>(2);
$values[0] = 1.25;
$values[1] = 2.75;
echo (int) ($values[0] + $values[1]);
"#;
    assert_eq!(compile_and_run_ir_backend("buffer_float_get_set", float_source), "4");
}

/// Verifies packed buffer values survive cross-class property type refinement.
#[test]
fn ir_backend_handles_cross_class_packed_buffer_property_reads() {
    let source = r#"<?php
packed class Point {
    public int $x;
}

class Box {
    public $items;

    public function __construct() {
        $this->items = 0;
    }
}

class Loader {
    public function load(): Box {
        $box = new Box();
        buffer<Point> $items = buffer_new<Point>(1);
        $items[0]->x = 7;
        $box->items = $items;
        return $box;
    }
}

class Game {
    public $box;

    public function __construct() {
        $this->box = 0;
    }

    public function run(): int {
        $loader = new Loader();
        $this->box = $loader->load();
        return $this->box->items[0]->x;
    }
}

$game = new Game();
echo $game->run();
"#;
    assert_eq!(
        compile_and_run_ir_backend("cross_class_packed_buffer_property_reads", source),
        "7"
    );
}

/// Verifies `buffer_free()` releases the buffer and nulls the source local.
#[test]
fn ir_backend_handles_buffer_free() {
    let source = r#"<?php
buffer<int> $values = buffer_new<int>(2);
$values[0] = 9;
buffer_free($values);
echo "ok";
"#;
    assert_eq!(compile_and_run_ir_backend("buffer_free", source), "ok");

    let run = compile_ir_backend_and_run(
        "buffer_free_uaf",
        r#"<?php
buffer<int> $values = buffer_new<int>(1);
buffer_free($values);
echo buffer_len($values);
"#,
        &[],
    );
    assert!(
        !run.status.success(),
        "IR backend buffer use-after-free fixture unexpectedly succeeded"
    );
    let stderr = String::from_utf8(run.stderr).expect("fatal stderr should be utf8");
    assert!(
        stderr.contains("Fatal error: use of buffer after buffer_free()"),
        "unexpected fatal stderr: {stderr}"
    );
}

/// Verifies `ClassName::class` materializes the compile-time class-name string.
#[test]
fn ir_backend_handles_named_class_constant() {
    let source = r#"<?php
class C {}
echo C::class;
"#;
    assert_eq!(compile_and_run_ir_backend("named_class_constant", source), "C");
}

/// Verifies scoped class and interface constants inline their checked values.
#[test]
fn ir_backend_handles_scoped_class_constants() {
    let source = r#"<?php
class Direct { const I = 42; const S = "ok"; }
class Base { const TOKEN = "base"; }
class Child extends Base {}
interface Limits { const MAX = 9; }
class Impl implements Limits {}
echo Direct::I;
echo ":";
echo Direct::S;
echo ":";
echo Child::TOKEN;
echo ":";
echo Limits::MAX;
echo ":";
echo Impl::MAX;
"#;
    assert_eq!(
        compile_and_run_ir_backend("scoped_class_constants", source),
        "42:ok:base:9:9"
    );
}

/// Verifies simple object allocation and named `instanceof` metadata checks.
#[test]
fn ir_backend_handles_simple_object_instanceof() {
    let source = r#"<?php
interface Marker {}
class Base {}
class Child extends Base implements Marker {}
class Other {}
$child = new Child();
$base = new Base();
echo ($child instanceof Child) ? "T" : "F";
echo ($child instanceof Base) ? "T" : "F";
echo ($child instanceof Marker) ? "T" : "F";
echo ($child instanceof Other) ? "T" : "F";
echo ($base instanceof Child) ? "T" : "F";
echo (42 instanceof Base) ? "T" : "F";
echo ($child instanceof Missing) ? "T" : "F";
"#;
    assert_eq!(
        compile_and_run_ir_backend("simple_object_instanceof", source),
        "TTTFFFF"
    );
}

/// Verifies named `instanceof` works when class metadata includes method tables.
#[test]
fn ir_backend_handles_instanceof_on_classes_with_methods() {
    let source = r#"<?php
class Base {
    public function value(): int {
        return 1;
    }
}
class Child extends Base {}
$child = new Child();
echo ($child instanceof Child) ? "T" : "F";
echo ($child instanceof Base) ? "T" : "F";
"#;
    assert_eq!(
        compile_and_run_ir_backend("instanceof_classes_with_methods", source),
        "TT"
    );
}

/// Verifies dynamic `instanceof` targets resolve through EIR runtime metadata.
#[test]
fn ir_backend_handles_dynamic_instanceof_targets() {
    let source = r#"<?php
interface Marker {}
class Base {}
class Child extends Base implements Marker {}
class Other {}
$child = new Child();
$className = "Base";
$interfaceName = "Marker";
$otherName = "Other";
$lowerName = "child";
$absoluteName = "\\Base";
$missing = "Missing";
$targetChild = new Child();
$targetOther = new Other();
echo ($child instanceof $className) ? "T" : "F";
echo ($child instanceof $interfaceName) ? "T" : "F";
echo ($child instanceof $otherName) ? "T" : "F";
echo ($child instanceof $lowerName) ? "T" : "F";
echo ($child instanceof $absoluteName) ? "T" : "F";
echo ($child instanceof $missing) ? "T" : "F";
echo (42 instanceof $className) ? "T" : "F";
echo ($child instanceof $targetChild) ? "T" : "F";
echo ($child instanceof $targetOther) ? "T" : "F";
"#;
    assert_eq!(
        compile_and_run_ir_backend("dynamic_instanceof_targets", source),
        "TTFTTFFTF"
    );
}

/// Verifies dynamic `instanceof` metadata includes classes whose method symbols are emitted by EIR.
#[test]
fn ir_backend_handles_dynamic_instanceof_on_classes_with_methods() {
    let source = r#"<?php
class MethodBase {
    public function baseValue(): int {
        return 1;
    }
}
class MethodChild extends MethodBase {
    public function childValue(): int {
        return 2;
    }
}
$child = new MethodChild();
$baseName = "MethodBase";
$childName = "MethodChild";
echo ($child instanceof $baseName) ? "T" : "F";
echo ($child instanceof $childName) ? "T" : "F";
"#;
    assert_eq!(
        compile_and_run_ir_backend("dynamic_instanceof_classes_with_methods", source),
        "TT"
    );
}

/// Verifies enum case scoped constants load initialized singleton objects.
#[test]
fn ir_backend_handles_enum_case_singletons() {
    let unit_source = r#"<?php
enum Color {
    case Red;
    case Blue;
}
$case = Color::Red;
echo ($case instanceof Color) ? "T" : "F";
"#;
    assert_eq!(
        compile_and_run_ir_backend("enum_case_singletons", unit_source),
        "T"
    );

    let backed_source = r#"<?php
enum Code: int {
    case Ok = 7;
}
echo Code::Ok->value;
"#;
    assert_eq!(
        compile_and_run_ir_backend("backed_enum_case_singletons", backed_source),
        "7"
    );
}

/// Verifies invalid dynamic `instanceof` targets use the runtime fatal path.
#[test]
fn ir_backend_fatals_on_invalid_dynamic_instanceof_target() {
    let run = compile_ir_backend_and_run(
        "invalid_dynamic_instanceof_target",
        r#"<?php
class User {}
$user = new User();
$target = 42;
echo ($user instanceof $target) ? "T" : "F";
"#,
        &[],
    );
    assert!(
        !run.status.success(),
        "IR backend invalid dynamic instanceof fixture unexpectedly succeeded"
    );
    assert_eq!(
        String::from_utf8(run.stdout).expect("fatal stdout should be utf8"),
        ""
    );
    let stderr = String::from_utf8(run.stderr).expect("fatal stderr should be utf8");
    assert!(
        stderr.contains("Fatal error: Class name must be a valid object or a string"),
        "unexpected fatal stderr: {stderr}"
    );
}

/// Verifies simple typed static properties round-trip through EIR symbol storage.
#[test]
fn ir_backend_handles_simple_static_properties() {
    let source = r#"<?php
class Counter {
    public static int $i;
    public static string $s;
    public static float $f;
    public static bool $b;
}
Counter::$i = 7;
Counter::$s = "ok";
Counter::$f = 1.5;
Counter::$b = true;
echo Counter::$i;
echo ":";
echo Counter::$s;
echo ":";
echo Counter::$f;
echo ":";
if (Counter::$b) { echo "T"; } else { echo "F"; }
"#;
    assert_eq!(
        compile_and_run_ir_backend("simple_static_properties", source),
        "7:ok:1.5:T"
    );
}

/// Verifies static properties on classes with method metadata link through emitted method symbols.
#[test]
fn ir_backend_handles_static_properties_on_classes_with_methods() {
    let source = r#"<?php
class Counter {
    public static int $i;

    public function value(): int {
        return 1;
    }

    public static function marker(): int {
        return 2;
    }
}
Counter::$i = 7;
echo Counter::$i;
"#;
    assert_eq!(
        compile_and_run_ir_backend("static_properties_with_methods", source),
        "7"
    );
}

/// Verifies lexical `self::` and `parent::` static-property receivers lower in class methods.
#[test]
fn ir_backend_handles_lexical_static_property_receivers() {
    let source = r#"<?php
class BaseCounter {
    public static int $i;
}
class Counter extends BaseCounter {
    public static int $j;

    public static function setBoth(): void {
        self::$j = 4;
        parent::$i = 6;
    }

    public static function total(): int {
        return self::$j + parent::$i;
    }
}
Counter::setBoth();
echo Counter::total();
"#;
    assert_eq!(
        compile_and_run_ir_backend("lexical_static_property_receivers", source),
        "10"
    );
}

/// Verifies supported literal static-property defaults are initialized before main user code.
#[test]
fn ir_backend_handles_literal_static_property_defaults() {
    let source = r#"<?php
class BaseDefaults {
    public static int $base = -3;
}
class Defaults extends BaseDefaults {
    public static int $i = 7;
    public static string $s = "ok";
    public static float $f = -2.5;
    public static bool $b = true;
}
echo Defaults::$base;
echo ":";
echo Defaults::$i;
echo ":";
echo Defaults::$s;
echo ":";
echo Defaults::$f;
echo ":";
if (Defaults::$b) { echo "T"; } else { echo "F"; }
"#;
    assert_eq!(
        compile_and_run_ir_backend("literal_static_property_defaults", source),
        "-3:7:ok:-2.5:T"
    );
}

/// Verifies typed static properties still fatal when read before initialization.
#[test]
fn ir_backend_fatals_on_uninitialized_typed_static_property() {
    let run = compile_ir_backend_and_run(
        "uninitialized_typed_static_property",
        r#"<?php
class Counter { public static int $i; }
echo Counter::$i;
"#,
        &[],
    );
    assert!(
        !run.status.success(),
        "IR backend uninitialized typed static property fixture unexpectedly succeeded"
    );
    assert_eq!(
        String::from_utf8(run.stdout).expect("fatal stdout should be utf8"),
        ""
    );
    let stderr = String::from_utf8(run.stderr).expect("fatal stderr should be utf8");
    assert!(
        stderr.contains("Fatal error: Typed static property Counter::$i must not be accessed before initialization"),
        "unexpected fatal stderr: {stderr}"
    );
}

/// Verifies simple declared object properties round-trip through EIR object slots.
#[test]
fn ir_backend_handles_simple_object_properties() {
    let source = r#"<?php
class Box {
    public int $i;
    public string $s;
    public float $f;
    public bool $b;
}
$box = new Box();
$box->i = 7;
$box->s = "ok";
$box->f = 1.5;
$box->b = true;
echo $box->i;
echo ":";
echo $box->s;
echo ":";
echo $box->f;
echo ":";
if ($box->b) { echo "T"; } else { echo "F"; }
"#;
    assert_eq!(
        compile_and_run_ir_backend("simple_object_properties", source),
        "7:ok:1.5:T"
    );
}

/// Verifies dynamic property reads with literal names use declared object slots.
#[test]
fn ir_backend_handles_literal_dynamic_object_property_reads() {
    let source = r#"<?php
class Box {
    public int $i = 7;
}
$box = new Box();
echo $box->{"i"};
"#;
    assert_eq!(
        compile_and_run_ir_backend("literal_dynamic_object_property_read", source),
        "7"
    );
}

/// Verifies dynamic property reads dispatch runtime string names to declared slots.
#[test]
fn ir_backend_handles_runtime_dynamic_object_property_reads() {
    let source = r#"<?php
class Box {
    public int $i = 7;
}
$name = "i";
$box = new Box();
echo $box->{$name};
"#;
    assert_eq!(
        compile_and_run_ir_backend("runtime_dynamic_object_property_read", source),
        "7"
    );
}

/// Verifies nullsafe property reads short-circuit null receivers and box non-null values.
#[test]
fn ir_backend_handles_nullsafe_object_properties() {
    let source = r#"<?php
class Box {
    public int $i;
}
function maybe_box(bool $flag): ?Box {
    if ($flag) {
        $box = new Box();
        $box->i = 9;
        return $box;
    }
    return null;
}
$missing = maybe_box(false)?->i;
if (is_null($missing)) {
    echo "null";
} else {
    echo "bad";
}
echo ":";
echo maybe_box(true)?->i;
"#;
    assert_eq!(
        compile_and_run_ir_backend("nullsafe_object_properties", source),
        "null:9"
    );
}

/// Verifies nullsafe method calls skip arguments for null receivers and call normally otherwise.
#[test]
fn ir_backend_handles_nullsafe_method_calls() {
    let source = r#"<?php
function side(): string {
    echo "bad";
    return "side";
}
class Box {
    public function label(string $value): string {
        return $value;
    }
}
function maybe_box(bool $flag): ?Box {
    if ($flag) {
        return new Box();
    }
    return null;
}
echo maybe_box(false)?->label(side()) ?? "none";
echo ":";
echo maybe_box(true)?->label("ok");
"#;
    assert_eq!(
        compile_and_run_ir_backend("nullsafe_method_calls", source),
        "none:ok"
    );
}

/// Verifies supported scalar object-property defaults are copied into new instances.
#[test]
fn ir_backend_handles_literal_object_property_defaults() {
    let source = r#"<?php
class Defaults {
    public int $i = -3;
    public string $s = "ok";
    public float $f = 1.5;
    public bool $b = true;
}
$box = new Defaults();
echo $box->i;
echo ":";
echo $box->s;
echo ":";
echo $box->f;
echo ":";
if ($box->b) { echo "T"; } else { echo "F"; }
"#;
    assert_eq!(
        compile_and_run_ir_backend("literal_object_property_defaults", source),
        "-3:ok:1.5:T"
    );
}

/// Verifies object allocation works for classes whose metadata includes method tables.
#[test]
fn ir_backend_handles_object_properties_on_classes_with_methods() {
    let source = r#"<?php
class Box {
    public int $i;

    public function value(): int {
        return 1;
    }

    public static function marker(): int {
        return 2;
    }
}
$box = new Box();
$box->i = 7;
echo $box->i;
"#;
    assert_eq!(
        compile_and_run_ir_backend("object_properties_with_methods", source),
        "7"
    );
}

/// Verifies direct instance-method calls pass `$this` through the EIR method ABI.
#[test]
fn ir_backend_calls_simple_instance_method() {
    let source = r#"<?php
class Box {
    public int $i;

    public function value(): int {
        return $this->i;
    }
}
$box = new Box();
$box->i = 7;
echo $box->value();
"#;
    assert_eq!(
        compile_and_run_ir_backend("simple_instance_method_call", source),
        "7"
    );
}

/// Verifies direct static-method calls lower through the EIR method-symbol ABI.
#[test]
fn ir_backend_calls_simple_static_method() {
    let source = r#"<?php
class MathBox {
    public static function add(int $a, int $b): int {
        return $a + $b;
    }
}
echo MathBox::add(2, 3);
"#;
    assert_eq!(
        compile_and_run_ir_backend("simple_static_method_call", source),
        "5"
    );
}

/// Verifies static method return metadata survives object property stores.
#[test]
fn ir_backend_uses_static_method_object_return_type() {
    let source = r#"<?php
class Box {
    public int $value;

    public function __construct(int $value) {
        $this->value = $value;
    }
}

class Factory {
    public static function make(): Box {
        return new Box(7);
    }
}

class Holder {
    public $box;

    public function __construct() {
        $this->box = 0;
    }

    public function load(): void {
        $this->box = Factory::make();
    }

    public function value(): int {
        return $this->box->value;
    }
}

$holder = new Holder();
$holder->load();
echo $holder->value();
"#;
    assert_eq!(
        compile_and_run_ir_backend("static_method_object_return_type", source),
        "7"
    );
}

/// Verifies lexical `self::` static method return metadata survives object property stores.
#[test]
fn ir_backend_uses_self_static_method_object_return_type() {
    let source = r#"<?php
class Box {
    public int $value;

    public function __construct(int $value) {
        $this->value = $value;
    }
}

class Holder {
    public $box;

    public function __construct() {
        $this->box = 0;
    }

    public static function make(): Box {
        return new Box(11);
    }

    public function load(): void {
        $this->box = self::make();
    }

    public function value(): int {
        return $this->box->value;
    }
}

$holder = new Holder();
$holder->load();
echo $holder->value();
"#;
    assert_eq!(
        compile_and_run_ir_backend("self_static_method_object_return_type", source),
        "11"
    );
}

/// Verifies lexical `parent::` static method return metadata survives object property stores.
#[test]
fn ir_backend_uses_parent_static_method_object_return_type() {
    let source = r#"<?php
class Box {
    public int $value;

    public function __construct(int $value) {
        $this->value = $value;
    }
}

class BaseHolder {
    public static function make(): Box {
        return new Box(13);
    }
}

class Holder extends BaseHolder {
    public $box;

    public function __construct() {
        $this->box = 0;
    }

    public function load(): void {
        $this->box = parent::make();
    }

    public function value(): int {
        return $this->box->value;
    }
}

$holder = new Holder();
$holder->load();
echo $holder->value();
"#;
    assert_eq!(
        compile_and_run_ir_backend("parent_static_method_object_return_type", source),
        "13"
    );
}

/// Verifies lexical `self::` and `parent::` static-method receivers lower in class methods.
#[test]
fn ir_backend_calls_lexical_static_method_receivers() {
    let source = r#"<?php
class BaseMath {
    public static function add(int $a, int $b): int {
        return $a + $b;
    }
}
class MathBox extends BaseMath {
    public static function selfAdd(): int {
        return self::add(2, 3);
    }

    public static function parentAdd(): int {
        return parent::add(4, 5);
    }
}
echo MathBox::selfAdd();
echo ":";
echo MathBox::parentAdd();
"#;
    assert_eq!(
        compile_and_run_ir_backend("lexical_static_method_receivers", source),
        "5:9"
    );
}

/// Verifies fixed-class object construction calls `__construct` through the EIR method ABI.
#[test]
fn ir_backend_calls_simple_constructor() {
    let source = r#"<?php
class Box {
    public int $i;

    public function __construct(int $i) {
        $this->i = $i;
    }
}
$box = new Box(9);
echo $box->i;
"#;
    assert_eq!(
        compile_and_run_ir_backend("simple_constructor_call", source),
        "9"
    );
}

/// Verifies typed declared properties still fatal when read before initialization.
#[test]
fn ir_backend_fatals_on_uninitialized_typed_object_property() {
    let run = compile_ir_backend_and_run(
        "uninitialized_typed_object_property",
        r#"<?php
class Box { public int $i; }
$box = new Box();
echo $box->i;
"#,
        &[],
    );
    assert!(
        !run.status.success(),
        "IR backend uninitialized typed property fixture unexpectedly succeeded"
    );
    assert_eq!(
        String::from_utf8(run.stdout).expect("fatal stdout should be utf8"),
        ""
    );
    let stderr = String::from_utf8(run.stderr).expect("fatal stderr should be utf8");
    assert!(
        stderr.contains("Fatal error: Typed property Box::$i must not be accessed before initialization"),
        "unexpected fatal stderr: {stderr}"
    );
}

/// Verifies selected type predicates inspect boxed Mixed payloads in the EIR backend.
#[test]
fn ir_backend_handles_mixed_type_predicates() {
    for (name, source, expected) in [
        (
            "is_null_mixed_array_fill",
            "<?php $a = array_fill(0, 1, null); echo is_null($a[0]) ? 'null' : 'value';",
            "null",
        ),
        (
            "null_coalesce_reads_mixed_null",
            "<?php $a = array_fill(0, 1, null); echo $a[0] ?? 5;",
            "5",
        ),
        (
            "is_int_bool_string_mixed_array_fill",
            "<?php $ints = array_fill(0, 1, 7); $bools = array_fill(0, 1, true); echo is_int($ints[0]) ? 'i' : '_'; echo is_bool($bools[0]) ? 'b' : '_'; echo is_string($ints[0]) ? 'bad' : 'ok';",
            "ibok",
        ),
    ] {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }
}

/// Verifies `empty()` lowering for scalar, array, hash, and iterable operands.
#[test]
fn ir_backend_handles_empty_builtin() {
    assert_eq!(
        compile_and_run_ir_backend(
            "empty_scalar_values",
            "<?php echo empty(0) ? 'T' : 'F'; echo empty(42) ? 'T' : 'F'; echo empty('') ? 'T' : 'F'; echo empty('hi') ? 'T' : 'F'; echo empty(null) ? 'T' : 'F'; echo empty(false) ? 'T' : 'F'; echo empty(true) ? 'T' : 'F'; echo empty(0.0) ? 'T' : 'F'; echo empty(1.5) ? 'T' : 'F';",
        ),
        "TFTFTTFTF"
    );
    assert_eq!(
        compile_and_run_ir_backend(
            "empty_array_values",
            "<?php $empty = []; $full = [1]; $hash = ['a' => 1]; echo empty($empty) ? 'E' : 'N'; echo ':'; echo empty($full) ? 'E' : 'N'; echo ':'; echo empty($hash) ? 'E' : 'N';",
        ),
        "E:N:N"
    );
    let iterable_source = "<?php function describe(iterable $items): string { return empty($items) ? 'empty' : 'not'; } echo describe([]); echo '|'; echo describe([1]); echo '|'; echo describe(['a' => 1]);";
    assert_eq!(
        compile_and_run_ir_backend("empty_iterable_values", iterable_source),
        "empty|not|not"
    );
}

/// Verifies `isset()` lowering for scalar values and already-evaluated array offsets.
#[test]
fn ir_backend_handles_isset_builtin() {
    assert_eq!(
        compile_and_run_ir_backend(
            "isset_scalar_values",
            "<?php $x = 42; $n = null; echo isset($x) ? 'Y' : 'N'; echo isset($n) ? 'Y' : 'N'; echo isset($x, $n) ? 'Y' : 'N';",
        ),
        "YNN"
    );
    assert_eq!(
        compile_and_run_ir_backend(
            "isset_present_array_offset",
            "<?php $items = [1]; echo isset($items[0]) ? 'Y' : 'N';",
        ),
        "Y"
    );
}

/// Verifies `intdiv()` division-by-zero follows the legacy fatal diagnostic.
#[test]
fn ir_backend_handles_intdiv_division_by_zero() {
    let run = compile_ir_backend_and_run("intdiv_zero", "<?php echo intdiv(1, 0);", &[]);
    assert!(
        !run.status.success(),
        "IR backend intdiv zero fixture unexpectedly succeeded"
    );
    assert_eq!(
        String::from_utf8(run.stdout).expect("intdiv stdout should be utf8"),
        ""
    );
    let stderr = String::from_utf8(run.stderr).expect("intdiv stderr should be utf8");
    assert!(
        stderr.contains("Fatal error: division by zero"),
        "unexpected intdiv stderr: {stderr}"
    );
}

/// Verifies scalar casts and string indexing lowered by the EIR backend.
#[test]
fn ir_backend_handles_scalar_casts_and_string_indexing() {
    for (name, source, expected) in [
        (
            "string_casts_to_numbers",
            "<?php echo (int)\"42xyz\"; echo \":\"; echo (float)\"2.5x\";",
            "42:2.5",
        ),
        (
            "scalar_casts_to_string",
            "<?php echo (string)7; echo \":\"; echo (string)1.5; echo \":\"; echo (string)false;",
            "7:1.5:",
        ),
        (
            "scalar_casts_to_bool",
            "<?php echo (bool)\"0\"; echo \":\"; echo (bool)\"hi\";",
            ":1",
        ),
        (
            "string_indexing",
            "<?php echo \"hello\"[1]; echo \":\"; echo \"hello\"[-1]; echo \":\"; echo \"hi\"[9];",
            "e:o:",
        ),
        (
            "string_switch_subject",
            "<?php switch (\"2\") { case 2: echo \"hit\"; }",
            "hit",
        ),
    ] {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }
}

/// Verifies dynamic scalar power and spaceship operators lowered by the EIR backend.
#[test]
fn ir_backend_handles_power_and_spaceship() {
    let source = "<?php echo $argc ** 3; echo \":\"; echo ($argc + 0.5) ** 2.0; echo \":\"; echo $argc <=> 2; echo \":\"; echo 2 <=> $argc;";
    assert_eq!(compile_and_run_ir_backend("pow_spaceship_argc_one", source), "1:2.25:-1:1");
    assert_eq!(
        compile_and_run_ir_backend_with_args("pow_spaceship_argc_two", source, &["extra"]),
        "8:6.25:0:0"
    );
}

/// Verifies explicit ownership ops emitted around string local slots.
#[test]
fn ir_backend_handles_string_ownership_ops() {
    for (name, source, expected) in [
        ("literal_string_acquire", "<?php $s = \"hello\"; echo $s;", "hello"),
        ("concat_string_acquire", "<?php $x = \"a\" . $argc; echo $x;", "a1"),
        (
            "string_copy_survives_source_release",
            "<?php $x = \"a\" . $argc; $y = $x; $x = \"b\" . $argc; echo $y;",
            "a1",
        ),
        (
            "string_release_on_overwrite",
            "<?php $x = \"a\" . $argc; $x = \"b\" . $argc; echo $x;",
            "b1",
        ),
        ("empty_string_release", "<?php $x = (string)false; $x = \"z\"; echo $x;", "z"),
    ] {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }
}

/// Verifies basic indexed-array allocation, append growth, and count lowering.
#[test]
fn ir_backend_handles_basic_indexed_arrays() {
    for (name, source, expected) in [
        ("array_count_ints", "<?php $a = [1, 2, 3]; echo count($a);", "3"),
        ("array_get_int", "<?php $a = [10, 20]; echo $a[1];", "20"),
        ("array_get_float", "<?php $a = [1.5, 2.5]; echo $a[1];", "2.5"),
        ("array_get_string", "<?php $a = [\"a\", \"b\"]; echo $a[1];", "b"),
        ("array_get_oob_null", "<?php $a = [10]; echo $a[9];", ""),
        ("array_get_negative_null", "<?php $a = [10]; echo $a[-1];", ""),
        ("array_count_strings", "<?php $a = [\"a\", \"b\"]; echo count($a);", "2"),
        (
            "array_push_grows_local",
            "<?php $a = []; $a[] = 1; $a[] = 2; $a[] = 3; $a[] = 4; $a[] = 5; echo count($a);",
            "5",
        ),
        ("array_set_int", "<?php $a = [10, 20]; $a[1] = 99; echo $a[1];", "99"),
        ("array_set_float", "<?php $a = [1.5, 2.5]; $a[0] = 3.5; echo $a[0];", "3.5"),
        ("array_set_string", "<?php $a = [\"a\", \"b\"]; $a[1] = \"z\"; echo $a[1];", "z"),
        ("array_set_extends_int", "<?php $a = [1]; $a[3] = 9; echo count($a); echo \":\"; echo $a[0];", "4:1"),
        ("array_set_extends_string", "<?php $a = [\"a\"]; $a[2] = \"z\"; echo count($a); echo \":\"; echo $a[2];", "3:z"),
        ("array_set_empty_count", "<?php $a = []; $a[2] = 7; echo count($a);", "3"),
        (
            "array_push_builtin_mutates_local",
            "<?php $a = [10]; array_push($a, 20); echo count($a); echo ' '; echo $a[1];",
            "2 20",
        ),
        (
            "array_push_builtin_return_is_legacy_null",
            "<?php $a = [10]; $n = array_push($a, 20); echo $n; echo ':'; echo $a[1];",
            ":20",
        ),
    ] {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }

    let dynamic_source = "<?php $a = [10, 20, 30]; echo $a[$argc];";
    assert_eq!(compile_and_run_ir_backend("array_get_dynamic_one", dynamic_source), "20");
    assert_eq!(
        compile_and_run_ir_backend_with_args("array_get_dynamic_two", dynamic_source, &["extra"]),
        "30"
    );
}

/// Verifies PHP indexed-array `+` preserves left keys and appends missing right suffixes.
#[test]
fn ir_backend_handles_indexed_array_union() {
    for (name, source, expected) in [
        (
            "array_union_keeps_left_numeric_keys",
            "<?php $left = [10, 20]; $right = [99, 88, 77]; $result = $left + $right; echo count($result); echo ':'; echo $result[0]; echo ','; echo $result[1]; echo ','; echo $result[2];",
            "3:10,20,77",
        ),
        (
            "array_union_string_suffix",
            "<?php $left = ['left']; $right = ['ignored', 'added']; $result = $left + $right; echo count($result); echo ':'; echo $result[0]; echo ','; echo $result[1];",
            "2:left,added",
        ),
        (
            "array_union_empty_left",
            "<?php $result = [] + ['first', 'second']; echo count($result); echo ':'; echo $result[0]; echo ','; echo $result[1];",
            "2:first,second",
        ),
    ] {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }
}

/// Verifies `array_pop()` mutates indexed arrays and returns PHP `mixed` values.
#[test]
fn ir_backend_handles_indexed_array_pop() {
    for (name, source, expected) in [
        (
            "array_pop_int_mutates_count",
            "<?php $a = [1, 2, 3]; $v = array_pop($a); echo $v; echo ' '; echo count($a);",
            "3 2",
        ),
        (
            "array_pop_string_value",
            "<?php $a = ['a', 'b']; $v = array_pop($a); echo $v; echo ':'; echo count($a);",
            "b:1",
        ),
        (
            "array_pop_empty_null",
            "<?php $a = [1]; array_pop($a); $v = array_pop($a); echo is_null($v) ? 'null' : 'value';",
            "null",
        ),
    ] {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }
}

/// Verifies `array_shift()` mutates indexed arrays and compacts remaining elements.
#[test]
fn ir_backend_handles_indexed_array_shift() {
    for (name, source, expected) in [
        (
            "array_shift_int_mutates_count",
            "<?php $a = [10, 20, 30]; $v = array_shift($a); echo $v; echo ' '; echo count($a);",
            "10 2",
        ),
        (
            "array_shift_int_compacts_slots",
            "<?php $a = [10, 20, 30]; array_shift($a); echo $a[0]; echo ':'; echo $a[1];",
            "20:30",
        ),
        (
            "array_shift_string_value",
            "<?php $a = ['a', 'b', 'c']; $v = array_shift($a); echo $v; echo ':'; echo $a[0]; echo ':'; echo count($a);",
            "a:b:2",
        ),
        (
            "array_shift_empty_null",
            "<?php $a = [1]; array_shift($a); $v = array_shift($a); echo is_null($v) ? 'null' : 'value';",
            "null",
        ),
    ] {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }
}

/// Verifies `array_unshift()` prepends indexed values and returns the new length.
#[test]
fn ir_backend_handles_indexed_array_unshift() {
    for (name, source, expected) in [
        (
            "array_unshift_int_returns_count",
            "<?php $a = [2, 3]; $n = array_unshift($a, 1); echo $n; echo ':'; echo $a[0]; echo ':'; echo $a[1]; echo ':'; echo $a[2];",
            "3:1:2:3",
        ),
        (
            "array_unshift_bool_payload",
            "<?php $a = [false]; $n = array_unshift($a, true); echo $n; echo ':'; echo $a[0] ? 'T' : 'F'; echo ':'; echo $a[1] ? 'T' : 'F';",
            "2:T:F",
        ),
    ] {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }
}

/// Verifies mutating indexed-array sort builtins call the legacy integer sort helpers.
#[test]
fn ir_backend_handles_indexed_array_sorting() {
    for (name, source, expected) in [
        (
            "sort_indexed_ints",
            "<?php $a = [3, 1, 2]; sort($a); echo $a[0]; echo $a[1]; echo $a[2];",
            "123",
        ),
        (
            "rsort_indexed_ints",
            "<?php $a = [1, 3, 2]; rsort($a); echo $a[0]; echo $a[1]; echo $a[2];",
            "321",
        ),
        (
            "asort_indexed_ints",
            "<?php $a = [3, 1, 2]; asort($a); echo $a[0]; echo $a[1]; echo $a[2];",
            "123",
        ),
        (
            "arsort_indexed_ints",
            "<?php $a = [1, 3, 2]; arsort($a); echo $a[0]; echo $a[1]; echo $a[2];",
            "321",
        ),
        (
            "ksort_indexed_ints_preserves_slots",
            "<?php $a = [3, 1, 2]; ksort($a); echo $a[0]; echo $a[1]; echo $a[2];",
            "312",
        ),
        (
            "krsort_indexed_ints_preserves_slots",
            "<?php $a = [1, 2, 3]; krsort($a); echo $a[0]; echo $a[1]; echo $a[2];",
            "123",
        ),
        (
            "natsort_indexed_ints",
            "<?php $a = [3, 1, 2]; natsort($a); echo $a[0]; echo $a[1]; echo $a[2];",
            "123",
        ),
        (
            "natcasesort_indexed_ints",
            "<?php $a = [3, 1, 2]; natcasesort($a); echo $a[0]; echo $a[1]; echo $a[2];",
            "123",
        ),
        (
            "shuffle_indexed_ints",
            "<?php $a = [1, 2, 3]; shuffle($a); echo count($a); echo ':'; echo array_sum($a); echo ':'; echo (in_array(1, $a) ? '1' : '0'); echo (in_array(2, $a) ? '2' : '0'); echo (in_array(3, $a) ? '3' : '0');",
            "3:6:123",
        ),
    ] {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }
}

/// Verifies indexed arrays can read pointer-sized nested array elements.
#[test]
fn ir_backend_handles_nested_indexed_array_reads() {
    for (name, source, expected) in [
        (
            "nested_indexed_literal_reads",
            "<?php $a = [[1], [2]]; echo $a[0][0] . ':' . $a[1][0];",
            "1:2",
        ),
        (
            "nested_indexed_local_read",
            "<?php $a = [[1], [2]]; $b = $a[1]; echo $b[0];",
            "2",
        ),
        (
            "nested_indexed_after_append",
            "<?php $a = []; $a[] = [7]; echo $a[0][0];",
            "7",
        ),
    ] {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }
}

/// Verifies simple positional list destructuring through indexed-array reads.
#[test]
fn ir_backend_handles_simple_list_unpack() {
    for (name, source, expected) in [
        (
            "list_unpack_int_literal",
            "<?php [$a, $b, $c] = [10, 20, 30]; echo $a . ' ' . $b . ' ' . $c;",
            "10 20 30",
        ),
        (
            "list_unpack_string_literal",
            "<?php [$x, $y] = ['hello', 'world']; echo $x . ' ' . $y;",
            "hello world",
        ),
        (
            "list_unpack_from_variable",
            "<?php $arr = [1, 2, 3]; [$a, $b, $c] = $arr; echo $a . ' ' . $b . ' ' . $c;",
            "1 2 3",
        ),
        (
            "list_unpack_after_int_append",
            "<?php $items = []; $items[] = 7; [$a] = $items; echo $a;",
            "7",
        ),
        (
            "list_unpack_after_string_append",
            "<?php $items = []; $items[] = 'z'; [$a] = $items; echo $a;",
            "z",
        ),
    ] {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }
}

/// Verifies AArch64 far-slot materialization does not clobber indexed-array receiver registers.
#[test]
fn ir_backend_keeps_indexed_array_receiver_across_far_slot_loads() {
    let mut get_source = String::from("<?php $a = [10, 20, 30]; $x = $argc;");
    for _ in 0..40 {
        get_source.push_str(" $x = $x + 1;");
    }
    get_source.push_str(" echo $a[2]; echo ':'; echo $x;");
    assert_eq!(
        compile_and_run_ir_backend("array_get_far_slot_receiver", &get_source),
        "30:41"
    );

    let mut push_source = String::from("<?php $a = [0]; $x = $argc;");
    for _ in 0..40 {
        push_source.push_str(" $x = $x + 1;");
    }
    push_source.push_str(" $a[] = $x; echo $a[1];");
    assert_eq!(
        compile_and_run_ir_backend("array_push_far_slot_receiver", &push_source),
        "41"
    );
}

/// Verifies indexed-array aggregate builtins that delegate to runtime helpers.
#[test]
fn ir_backend_handles_indexed_array_aggregates() {
    for (name, source, expected) in [
        ("array_sum_ints", "<?php $a = [10, 20, 30]; echo array_sum($a);", "60"),
        ("array_sum_empty", "<?php $a = []; echo array_sum($a);", "0"),
        (
            "array_product_ints",
            "<?php $a = [2, 3, 4]; echo array_product($a);",
            "24",
        ),
        ("array_product_empty", "<?php $a = []; echo array_product($a);", "1"),
    ] {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }
}

/// Verifies `array_fill()` produces Mixed-boxed indexed arrays for scalar values.
#[test]
fn ir_backend_handles_indexed_array_fill() {
    for (name, source, expected) in [
        (
            "array_fill_int",
            "<?php $a = array_fill(0, 3, 7); echo count($a); echo ':'; echo $a[0]; echo $a[1]; echo $a[2];",
            "3:777",
        ),
        (
            "array_fill_float",
            "<?php $a = array_fill(0, 2, 1.5); echo count($a); echo ':'; echo $a[0]; echo '|'; echo $a[1];",
            "2:1.5|1.5",
        ),
        (
            "array_fill_bool",
            "<?php $a = array_fill(0, 2, true); echo count($a); echo ':'; echo $a[0]; echo $a[1];",
            "2:11",
        ),
        (
            "array_fill_null",
            "<?php $a = array_fill(0, 1, null); echo count($a); echo ':'; echo $a[0]; echo 'done';",
            "1:done",
        ),
    ] {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }
}

/// Verifies `array_fill_keys()` builds associative arrays from string-key indexed arrays.
#[test]
fn ir_backend_handles_indexed_array_fill_keys() {
    for (name, source, expected) in [
        (
            "array_fill_keys_int_lookup",
            "<?php $keys = ['x', 'y']; $m = array_fill_keys($keys, 7); echo count($m); echo ':'; echo $m['y'];",
            "2:7",
        ),
        (
            "array_fill_keys_numeric_key_normalization",
            "<?php $m = array_fill_keys(['1', '02'], 8); echo $m[1]; echo ':'; echo $m['02'];",
            "8:8",
        ),
        (
            "array_fill_keys_float_lookup",
            "<?php $m = array_fill_keys(['x'], 1.5); echo $m['x'];",
            "1.5",
        ),
        (
            "array_fill_keys_refcounted_array_values",
            "<?php $inner = [14]; $m = array_fill_keys(['a', 'b'], $inner); $v = array_values($m); echo count($m); echo ':'; echo count($v[0]); echo ':'; echo $v[1][0];",
            "2:1:14",
        ),
    ] {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }
}

/// Verifies `array_chunk()` returns nested indexed arrays with the expected chunk boundaries.
#[test]
fn ir_backend_handles_indexed_array_chunk() {
    for (name, source, expected) in [
        (
            "array_chunk_count",
            "<?php $a = [1, 2, 3, 4, 5]; $c = array_chunk($a, 2); echo count($c);",
            "3",
        ),
        (
            "array_chunk_inner_values",
            "<?php $a = [1, 2, 3, 4, 5]; $c = array_chunk($a, 2); echo $c[0][1]; echo ':'; echo $c[2][0];",
            "2:5",
        ),
        (
            "array_chunk_preserves_source",
            "<?php $a = [1, 2, 3]; $c = array_chunk($a, 2); echo count($a); echo ':'; echo $a[0] . $a[1] . $a[2];",
            "3:123",
        ),
        (
            "array_chunk_nested_array_counts",
            "<?php $inner = [5]; $rows = [$inner, [9]]; $chunks = array_chunk($rows, 1); echo count($chunks); echo ':'; echo count($chunks[0]); echo ':'; echo count($chunks[1]);",
            "2:1:1",
        ),
    ] {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }
}

/// Verifies `array_pad()` copies indexed arrays and pads on either side.
#[test]
fn ir_backend_handles_indexed_array_pad() {
    for (name, source, expected) in [
        (
            "array_pad_right_int",
            "<?php $a = [1, 2]; $b = array_pad($a, 5, 0); echo count($b); echo ':'; echo $b[0] . $b[1] . $b[2] . $b[4];",
            "5:1200",
        ),
        (
            "array_pad_left_int",
            "<?php $a = [1, 2]; $b = array_pad($a, -4, 9); echo count($b); echo ':'; echo $b[0] . $b[1] . $b[2] . $b[3];",
            "4:9912",
        ),
        (
            "array_pad_no_growth_copies_source",
            "<?php $a = [7, 8]; $b = array_pad($a, 1, 0); echo count($b); echo ':'; echo $b[0] . $b[1];",
            "2:78",
        ),
        (
            "array_pad_nested_array_counts",
            "<?php $inner = [5]; $rows = [[1]]; $padded = array_pad($rows, 3, $inner); echo count($padded); echo ':'; echo count($padded[0]); echo ':'; echo count($padded[2]);",
            "3:1:1",
        ),
    ] {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }
}

/// Verifies `array_combine()` builds associative arrays from string-key indexed arrays.
#[test]
fn ir_backend_handles_indexed_array_combine() {
    for (name, source, expected) in [
        (
            "array_combine_int_lookup",
            "<?php $keys = ['a', 'b']; $vals = [1, 2]; $m = array_combine($keys, $vals); echo count($m); echo ':'; echo $m['b'];",
            "2:2",
        ),
        (
            "array_combine_numeric_key_normalization",
            "<?php $m = array_combine(['1', '02'], [7, 8]); echo $m[1]; echo ':'; echo $m['02'];",
            "7:8",
        ),
        (
            "array_combine_float_lookup",
            "<?php $m = array_combine(['x'], [1.5]); echo $m['x'];",
            "1.5",
        ),
        (
            "array_combine_refcounted_array_values",
            "<?php $m = array_combine(['row'], [[5]]); $v = array_values($m); echo count($m); echo ':'; echo count($v[0]); echo ':'; echo $v[0][0];",
            "1:1:5",
        ),
    ] {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }
}

/// Verifies `array_flip()` builds associative arrays from indexed integer and string values.
#[test]
fn ir_backend_handles_indexed_array_flip() {
    for (name, source, expected) in [
        (
            "array_flip_count",
            "<?php $a = [10, 20, 30]; $f = array_flip($a); echo count($f);",
            "3",
        ),
        (
            "array_flip_integer_values_are_integer_keys",
            "<?php $a = [10, 20]; $f = array_flip($a); echo $f[10]; echo '|'; echo $f['20'];",
            "0|1",
        ),
        (
            "array_flip_string_values_normalize_numeric_keys",
            "<?php $a = ['1', '02', '2']; $f = array_flip($a); echo count($f); echo '|'; echo $f[1]; echo '|'; echo $f['02']; echo '|'; echo $f['2'];",
            "3|0|1|2",
        ),
    ] {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }
}

/// Verifies indexed-array reversal returns a reversed copy without mutating the source.
#[test]
fn ir_backend_handles_indexed_array_reverse() {
    let source = "<?php $a = [3, 1, 2]; $b = array_reverse($a); echo $b[0] . $b[1] . $b[2]; echo ':'; echo $a[0] . $a[1] . $a[2];";
    assert_eq!(compile_and_run_ir_backend("array_reverse_indexed", source), "213:312");
}

/// Verifies indexed-array deduplication returns first occurrences without mutating the source.
#[test]
fn ir_backend_handles_indexed_array_unique() {
    let source = "<?php $a = [1, 2, 1, 3, 2]; $b = array_unique($a); echo count($b); echo ':'; echo $b[0] . $b[1] . $b[2]; echo ':'; echo count($a); echo ':'; echo $a[0] . $a[1] . $a[2] . $a[3] . $a[4];";
    assert_eq!(
        compile_and_run_ir_backend("array_unique_indexed", source),
        "3:123:5:12132"
    );
}

/// Verifies indexed-array merge concatenates operands without mutating either source.
#[test]
fn ir_backend_handles_indexed_array_merge() {
    let source = "<?php $a = [1, 2]; $b = [3, 4]; $c = array_merge($a, $b); echo count($c); echo ':'; echo $c[0] . $c[1] . $c[2] . $c[3]; echo ':'; echo count($a); echo ':'; echo $a[0] . $a[1]; echo ':'; echo $b[0] . $b[1];";
    assert_eq!(
        compile_and_run_ir_backend("array_merge_indexed", source),
        "4:1234:2:12:34"
    );
}

/// Verifies indexed-array merge keeps the right element type when the left side is empty.
#[test]
fn ir_backend_handles_indexed_array_merge_empty_left() {
    let source = "<?php $a = []; $b = [3, 4]; $c = array_merge($a, $b); echo count($c); echo ':'; echo $c[0] . $c[1];";
    assert_eq!(
        compile_and_run_ir_backend("array_merge_indexed_empty_left", source),
        "2:34"
    );
}

/// Verifies indexed-array value set operations return the expected subset counts and values.
#[test]
fn ir_backend_handles_indexed_array_set_operations() {
    for (name, source, expected) in [
        (
            "array_diff_indexed_ints",
            "<?php $a = [1, 2, 3, 4]; $b = [2, 4]; $c = array_diff($a, $b); echo count($c); echo ':'; echo $c[0]; echo ':'; echo $c[1];",
            "2:1:3",
        ),
        (
            "array_intersect_indexed_ints",
            "<?php $a = [1, 2, 3, 4]; $b = [2, 4, 6]; $c = array_intersect($a, $b); echo count($c); echo ':'; echo $c[0]; echo ':'; echo $c[1];",
            "2:2:4",
        ),
    ] {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }
}

/// Verifies indexed-array `array_values()` returns an alias that still observes COW on writes.
#[test]
fn ir_backend_handles_indexed_array_values() {
    let source = "<?php $a = [10, 20]; $b = array_values($a); echo count($b); echo ':'; echo $b[0] . $b[1]; $b[0] = 99; echo ':'; echo $a[0] . ':' . $b[0];";
    assert_eq!(
        compile_and_run_ir_backend("array_values_indexed", source),
        "2:1020:10:99"
    );
}

/// Verifies associative `array_values()` returns a new insertion-ordered indexed array.
#[test]
fn ir_backend_handles_assoc_array_values() {
    for (name, source, expected) in [
        (
            "array_values_assoc_int",
            "<?php $m = ['a' => 10, 'b' => 20, 'c' => 30]; $v = array_values($m); echo count($v); echo ':'; echo $v[0] + $v[1] + $v[2]; $v[0] = 99; echo ':'; echo $m['a']; echo ':'; echo $v[0];",
            "3:60:10:99",
        ),
        (
            "array_values_assoc_string",
            "<?php $m = ['a' => 'one', 'b' => 'two']; $v = array_values($m); echo count($v); echo ':'; echo $v[0] . ' ' . $v[1];",
            "2:one two",
        ),
    ] {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }
}

/// Verifies indexed-array key existence delegates to the runtime bounds helper.
#[test]
fn ir_backend_handles_indexed_array_key_exists() {
    for (name, source, expected) in [
        (
            "array_key_exists_present",
            "<?php $a = [10, 20, 30]; echo array_key_exists(1, $a) ? \"yes\" : \"no\";",
            "yes",
        ),
        (
            "array_key_exists_oob",
            "<?php $a = [10, 20, 30]; echo array_key_exists(5, $a) ? \"yes\" : \"no\";",
            "no",
        ),
        (
            "array_key_exists_negative",
            "<?php $a = [10, 20, 30]; echo array_key_exists(-1, $a) ? \"yes\" : \"no\";",
            "no",
        ),
        (
            "array_key_exists_bool_key",
            "<?php $a = [10, 20, 30]; echo array_key_exists(false, $a) ? \"yes\" : \"no\";",
            "yes",
        ),
    ] {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }
}

/// Verifies associative-array key existence probes hash keys without reading values.
#[test]
fn ir_backend_handles_assoc_array_key_exists() {
    for (name, source, expected) in [
        (
            "assoc_array_key_exists_string",
            "<?php $m = ['name' => 'Alice', 'age' => '30']; echo array_key_exists('name', $m) ? 'yes' : 'no'; echo ':'; echo array_key_exists('missing', $m) ? 'bad' : 'ok';",
            "yes:ok",
        ),
        (
            "assoc_array_key_exists_int",
            "<?php $m = [1 => 'one', '02' => 'two']; echo array_key_exists(1, $m) ? 'yes' : 'no'; echo ':'; echo array_key_exists('02', $m) ? 'yes' : 'no'; echo ':'; echo array_key_exists(2, $m) ? 'bad' : 'ok';",
            "yes:yes:ok",
        ),
    ] {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }
}

/// Verifies `array_keys()` returns Mixed-boxed keys in insertion order for indexed and assoc arrays.
#[test]
fn ir_backend_handles_array_keys() {
    for (name, source, expected) in [
        (
            "array_keys_indexed",
            "<?php $a = [10, 20, 30]; $k = array_keys($a); echo count($k); echo ':'; echo $k[0]; echo $k[1]; echo $k[2];",
            "3:012",
        ),
        (
            "array_keys_assoc_string",
            "<?php $m = ['x' => 1, 'y' => 2]; $keys = array_keys($m); echo count($keys); echo ':'; echo $keys[0]; echo ' '; echo $keys[1];",
            "2:x y",
        ),
        (
            "array_keys_assoc_mixed",
            "<?php $m = [1 => 'one', '02' => 'two']; $keys = array_keys($m); echo $keys[0]; echo '|'; echo $keys[1];",
            "1|02",
        ),
    ] {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }
}

/// Verifies indexed-array `array_slice()` copies scalar and Mixed payloads with PHP slice bounds.
#[test]
fn ir_backend_handles_indexed_array_slice() {
    for (name, source, expected) in [
        (
            "array_slice_int_explicit_length",
            "<?php $a = [10, 20, 30, 40, 50]; $b = array_slice($a, 1, 3); echo count($b); echo ':'; echo $b[0]; echo ' '; echo $b[1]; echo ' '; echo $b[2];",
            "3:20 30 40",
        ),
        (
            "array_slice_int_omitted_length",
            "<?php $a = [10, 20, 30, 40]; $b = array_slice($a, 2); echo count($b); echo ':'; echo $b[0]; echo ' '; echo $b[1];",
            "2:30 40",
        ),
        (
            "array_slice_int_null_length",
            "<?php $a = [5, 6, 7]; $b = array_slice($a, 1, null); echo count($b); echo ':'; echo $b[0]; echo $b[1];",
            "2:67",
        ),
        (
            "array_slice_int_negative_offset",
            "<?php $a = [10, 20, 30, 40]; $b = array_slice($a, -2, 1); echo count($b); echo ':'; echo $b[0];",
            "1:30",
        ),
        (
            "array_slice_mixed_payloads",
            "<?php $a = [1, true, 3]; $b = array_slice($a, 0, 2); echo count($b); echo ':'; echo $b[0]; echo ':'; echo $b[1];",
            "2:1:1",
        ),
    ] {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }
}

/// Verifies indexed-array membership for scalar and string payloads.
#[test]
fn ir_backend_handles_indexed_in_array() {
    for (name, source, expected) in [
        (
            "in_array_int_found",
            "<?php $a = [10, 20, 30]; echo in_array(20, $a);",
            "1",
        ),
        (
            "in_array_int_missing",
            "<?php $a = [10, 20, 30]; echo in_array(99, $a);",
            "",
        ),
        (
            "in_array_string_found",
            "<?php $a = [\"a\", \"b\", \"c\"]; echo in_array(\"b\", $a);",
            "1",
        ),
        (
            "in_array_string_missing",
            "<?php $a = [\"a\", \"b\", \"c\"]; echo in_array(\"x\", $a);",
            "",
        ),
        (
            "in_array_empty",
            "<?php $a = []; echo in_array(1, $a) ? \"bad\" : \"ok\";",
            "ok",
        ),
    ] {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }
}

/// Verifies indexed-array search returns `int|false` as boxed Mixed.
#[test]
fn ir_backend_handles_indexed_array_search() {
    for (name, source, expected) in [
        (
            "array_search_found",
            "<?php $a = [10, 20, 30]; echo array_search(20, $a);",
            "1",
        ),
        (
            "array_search_missing_strict_false",
            "<?php $a = [10, 20, 30]; echo array_search(99, $a) === false ? \"miss\" : \"hit\";",
            "miss",
        ),
        (
            "array_search_assigned_missing_strict_false",
            "<?php $a = [10, 20, 30]; $r = array_search(99, $a); echo $r === false ? \"miss\" : \"hit\";",
            "miss",
        ),
        (
            "array_search_zero_index_is_not_false",
            "<?php $a = [10, 20, 30]; echo array_search(10, $a) === false ? \"miss\" : \"zero\";",
            "zero",
        ),
        (
            "array_search_empty",
            "<?php $a = []; echo array_search(1, $a) === false ? \"miss\" : \"hit\";",
            "miss",
        ),
    ] {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }
}

/// Verifies indexed-array random key selection returns an in-bounds integer key.
#[test]
fn ir_backend_handles_indexed_array_rand() {
    let source = r#"<?php
$a = [10, 20, 30];
$i = array_rand($a);
echo array_key_exists($i, $a) ? "ok" : "bad";
echo ":";
echo $i >= 0 && $i < count($a) ? "range" : "bad";
"#;
    assert_eq!(compile_and_run_ir_backend("array_rand_indexed", source), "ok:range");
}

/// Verifies that the IR backend lowers `range()` for ascending, descending, and singleton integer spans.
#[test]
fn ir_backend_handles_range_builtin() {
    let cases = [
        (
            "range_ascending",
            "<?php $a = range(1, 5); echo count($a) . ':' . $a[0] . ':' . $a[4];",
            "5:1:5",
        ),
        (
            "range_descending",
            "<?php $a = range(5, 1); echo count($a) . ':' . $a[0] . ':' . $a[4];",
            "5:5:1",
        ),
        (
            "range_singleton",
            "<?php $a = range(3, 3); echo count($a) . ':' . $a[0];",
            "1:3",
        ),
    ];
    for (name, source, expected) in cases {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }
}

/// Verifies indexed-array foreach lowering over a `range()` result.
#[test]
fn ir_backend_handles_indexed_range_foreach() {
    for (name, source, expected) in [
        (
            "range_foreach_values",
            "<?php foreach (range(1, 3) as $value) { echo $value; }",
            "123",
        ),
        (
            "range_foreach_keys",
            "<?php foreach (range(2, 4) as $key => $value) { echo $key; echo ':'; echo $value; echo ';'; }",
            "0:2;1:3;2:4;",
        ),
        (
            "empty_foreach",
            "<?php foreach ([] as $value) { echo $value; } echo 'done';",
            "done",
        ),
        (
            "foreach_sum_mixed_values",
            "<?php function sum_arr($nums) { $total = 0; foreach ($nums as $n) { $total += $n; } return $total; } echo sum_arr([1, 2, 3]);",
            "6",
        ),
    ] {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }
}

/// Verifies associative-array foreach lowering preserves insertion-order keys and values.
#[test]
fn ir_backend_handles_assoc_array_foreach() {
    for (name, source, expected) in [
        (
            "assoc_foreach_string_keys",
            "<?php foreach (['a' => 1, 'b' => 2] as $key => $value) { echo $key; echo ':'; echo $value; echo ';'; }",
            "a:1;b:2;",
        ),
        (
            "assoc_foreach_int_and_string_keys",
            "<?php foreach ([2 => 'x', 'name' => 'y'] as $key => $value) { echo $key; echo '='; echo $value; echo ';'; }",
            "2=x;name=y;",
        ),
        (
            "assoc_foreach_values_only",
            "<?php foreach (['first' => 'A', 'second' => 'B'] as $value) { echo $value; }",
            "AB",
        ),
    ] {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }
}

/// Verifies array truthiness follows PHP length rules for empty and non-empty containers.
#[test]
fn ir_backend_handles_array_truthiness() {
    for (name, source, expected) in [
        (
            "empty_indexed_array_truthiness",
            "<?php $a = []; echo $a ? \"T\" : \"F\"; echo \":\"; echo !$a ? \"T\" : \"F\";",
            "F:T",
        ),
        (
            "non_empty_indexed_array_truthiness",
            "<?php $a = [1]; if ($a) { echo \"T\"; } echo \":\"; echo !$a ? \"T\" : \"F\";",
            "T:F",
        ),
        (
            "non_empty_assoc_array_truthiness",
            "<?php $h = [\"a\" => 1]; echo $h ? \"T\" : \"F\"; echo \":\"; echo !$h ? \"T\" : \"F\";",
            "T:F",
        ),
        (
            "array_boolval",
            "<?php $empty = []; $full = [1]; echo boolval($empty) ? \"T\" : \"F\"; echo \":\"; echo boolval($full) ? \"T\" : \"F\";",
            "F:T",
        ),
        (
            "iterable_numeric_casts",
            "<?php function as_int(iterable $items): int { return (int)$items; } function as_float(iterable $items): float { return (float)$items; } echo as_int([]); echo '|'; echo as_int([10, 20]); echo '|'; echo as_int(['a' => 1]); echo '|'; echo as_float([]); echo '|'; echo as_float([10, 20]);",
            "0|1|1|0|1",
        ),
    ] {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }
}

/// Verifies iterable echo uses the legacy array literal output while concrete array echo is silent.
#[test]
fn ir_backend_handles_iterable_echo() {
    let source = r#"<?php
function show(iterable $items): void {
    echo $items;
}
show(["a" => 1, "b" => 2]);
echo "|";
show([10, 20, 30]);
echo "|";
$direct = [1];
echo $direct;
echo "done";
"#;
    assert_eq!(
        compile_and_run_ir_backend("iterable_echo", source),
        "Array|Array|done"
    );
}

/// Verifies `gettype()` reports iterable array/hash payloads as PHP arrays.
#[test]
fn ir_backend_handles_iterable_gettype() {
    let source = r#"<?php
function describe(iterable $items): string {
    return gettype($items);
}
echo describe(["a" => 1]);
echo "|";
echo describe([1, 2, 3]);
"#;
    assert_eq!(
        compile_and_run_ir_backend("iterable_gettype", source),
        "array|array"
    );
}

/// Verifies string builtins that produce or consume indexed arrays through runtime helpers.
#[test]
fn ir_backend_handles_string_array_builtins() {
    for (name, source, expected) in [
        (
            "explode_strings",
            r#"<?php $parts = explode(",", "a,b,c"); echo count($parts); echo ":"; echo $parts[0] . "," . $parts[1] . "," . $parts[2];"#,
            "3:a,b,c",
        ),
        (
            "str_split_chunks",
            r#"<?php $parts = str_split("Hello", 2); echo count($parts); echo ":"; echo $parts[0] . "," . $parts[1] . "," . $parts[2];"#,
            "3:He,ll,o",
        ),
        (
            "str_split_default",
            r#"<?php $parts = str_split("abc"); echo $parts[0] . "," . $parts[1] . "," . $parts[2];"#,
            "a,b,c",
        ),
        (
            "implode_string_array",
            r#"<?php echo implode(" ", ["Hello", "World"]);"#,
            "Hello World",
        ),
        (
            "implode_int_array",
            r#"<?php echo implode(", ", [1, 2, 3]);"#,
            "1, 2, 3",
        ),
        (
            "explode_implode_roundtrip",
            r#"<?php $parts = explode("-", "one-two-three"); echo implode(", ", $parts);"#,
            "one, two, three",
        ),
        (
            "sscanf_strings",
            r#"<?php $result = sscanf("John 30", "%s %d"); echo $result[0] . ":" . $result[1];"#,
            "John:30",
        ),
    ] {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }
}

/// Verifies `is_iterable()` static decisions for concrete arrays, hashes, and scalars.
#[test]
fn ir_backend_handles_is_iterable_predicates() {
    let source = r#"<?php
$indexed = [1, 2, 3];
$hash = ["a" => 1];
echo is_iterable($indexed) ? "y" : "n";
echo is_iterable($hash) ? "y" : "n";
echo is_iterable(42) ? "y" : "n";
"#;
    assert_eq!(
        compile_and_run_ir_backend("is_iterable_static_predicates", source),
        "yyn"
    );
}

/// Verifies basic associative-array allocation, lookup, update, and count lowering.
#[test]
fn ir_backend_handles_basic_associative_arrays() {
    for (name, source, expected) in [
        ("hash_count", "<?php $h = [\"a\" => 1, \"b\" => 2]; echo count($h);", "2"),
        ("hash_get_int", "<?php $h = [\"a\" => 1]; echo $h[\"a\"];", "1"),
        ("hash_get_string", "<?php $h = [\"a\" => \"z\"]; echo $h[\"a\"];", "z"),
        ("hash_get_float", "<?php $h = [\"a\" => 1.5]; echo $h[\"a\"];", "1.5"),
        ("hash_get_miss", "<?php $h = [\"a\" => 1]; echo $h[\"missing\"];", ""),
        ("hash_int_key", "<?php $h = [1 => \"one\"]; echo $h[1];", "one"),
        ("hash_set_updates_local", "<?php $h = [\"a\" => 1]; $h[\"a\"] = 7; echo $h[\"a\"];", "7"),
        ("hash_set_string_value", "<?php $h = [\"a\" => \"x\"]; $h[\"a\"] = \"y\"; echo $h[\"a\"];", "y"),
    ] {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }
}

/// Verifies include-once guard lowering skips an already loaded include body.
#[test]
fn ir_backend_handles_include_once_guard() {
    let out = compile_and_run_ir_backend_files(
        "include_once_guard",
        &[
            (
                "main.php",
                "<?php include_once 'piece.php'; include_once 'piece.php';",
            ),
            ("piece.php", "<?php echo \"piece\";"),
        ],
        "main.php",
        &[],
    );
    assert_eq!(out, "piece");
}

/// Verifies require-once activates an include-loaded function variant before dispatch.
#[test]
fn ir_backend_handles_require_once_function_variant() {
    let out = compile_and_run_ir_backend_files(
        "require_once_function_variant",
        &[
            (
                "main.php",
                "<?php require_once 'lib.php'; require_once 'lib.php'; echo double(5);",
            ),
            ("lib.php", "<?php function double($n) { return $n * 2; }"),
        ],
        "main.php",
        &[],
    );
    assert_eq!(out, "10");
}

/// Verifies function_exists() sees include-loaded variants only after activation.
#[test]
fn ir_backend_tracks_function_exists_for_include_variants() {
    let files = &[
        (
            "main.php",
            r#"<?php
if ($argc > 1) {
    include 'lib.php';
}
if (function_exists('optional_exists')) {
    echo optional_exists();
} else {
    echo 'missing';
}
"#,
        ),
        ("lib.php", "<?php function optional_exists() { return 'loaded'; }"),
    ];
    assert_eq!(
        compile_and_run_ir_backend_files(
            "function_exists_include_variant_unloaded",
            files,
            "main.php",
            &[],
        ),
        "missing"
    );
    assert_eq!(
        compile_and_run_ir_backend_files(
            "function_exists_include_variant_loaded",
            files,
            "main.php",
            &["extra"],
        ),
        "loaded"
    );
}

/// Verifies function_exists() returns static booleans for builtins and unknown names.
#[test]
fn ir_backend_handles_static_function_exists_checks() {
    let source = "<?php echo function_exists('strlen') ? 'yes' : 'no'; echo ':'; echo function_exists('definitely_missing') ? 'yes' : 'no';";
    assert_eq!(
        compile_and_run_ir_backend("function_exists_static_names", source),
        "yes:no"
    );
}

/// Verifies global constant declarations, references, and `defined()` lowering.
#[test]
fn ir_backend_handles_global_constants_and_defined() {
    for (name, source, expected) in [
        (
            "constant_int",
            "<?php const ANSWER = 42; echo ANSWER;",
            "42",
        ),
        (
            "constant_string",
            "<?php const NAME = \"elephc\"; echo NAME;",
            "elephc",
        ),
        (
            "constant_bool_null",
            "<?php const FLAG = true; const NOTHING = null; echo FLAG; echo ':'; echo NOTHING;",
            "1:",
        ),
        (
            "defined_user_constant",
            "<?php const FOO = 1; echo defined('FOO') ? 'yes' : 'no'; echo ':'; echo defined('MISSING') ? 'yes' : 'no';",
            "yes:no",
        ),
    ] {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }

    let php_os = if cfg!(target_os = "macos") { "Darwin" } else { "Linux" };
    let source = "<?php echo PHP_OS; echo ':'; echo PATHINFO_DIRNAME; echo ':'; echo defined('PHP_OS') ? 'yes' : 'no';";
    assert_eq!(
        compile_and_run_ir_backend("predefined_constants", source),
        format!("{php_os}:1:yes")
    );
}

/// Verifies `define()` return values, source-order constant use, and duplicate guards.
#[test]
fn ir_backend_handles_define_builtin() {
    for (name, source, expected) in [
        (
            "define_string",
            "<?php define(\"APP_NAME\", \"elephc\"); echo APP_NAME;",
            "elephc",
        ),
        (
            "define_returns_true",
            "<?php echo define(\"FEATURE_ON\", true); echo FEATURE_ON;",
            "11",
        ),
        (
            "define_duplicate_suppressed",
            "<?php define(\"DUPLICATE_CONST\", 1); echo @define(\"DUPLICATE_CONST\", 2) ? \"bad\" : \"ok\"; echo DUPLICATE_CONST;",
            "ok1",
        ),
        (
            "define_duplicate_runtime_function",
            "<?php function once() { return define(\"RUNTIME_DUPLICATE\", 1); } echo once() ? \"T\" : \"F\"; echo @once() ? \"T\" : \"F\"; echo RUNTIME_DUPLICATE;",
            "TF1",
        ),
    ] {
        assert_eq!(compile_and_run_ir_backend(name, source), expected);
    }

    let duplicate = compile_ir_backend_and_run(
        "define_duplicate_warning",
        "<?php define(\"DUPLICATE_WARN\", 1); echo define(\"DUPLICATE_WARN\", 2) ? \"bad\" : \"ok\"; echo DUPLICATE_WARN;",
        &[],
    );
    assert!(
        duplicate.status.success(),
        "IR backend duplicate define fixture failed"
    );
    assert_eq!(
        String::from_utf8(duplicate.stdout).expect("stdout should be utf8"),
        "ok1"
    );
    let stderr = String::from_utf8(duplicate.stderr).expect("stderr should be utf8");
    assert!(
        stderr.contains("Warning: define()"),
        "expected duplicate define warning, got stderr={stderr}"
    );
}

/// Verifies is_callable() static string and scalar decisions match the legacy backend.
#[test]
fn ir_backend_handles_static_is_callable_checks() {
    let source = "<?php function f() { return 1; } echo is_callable('f') ? 'yes' : 'no'; echo ':'; echo is_callable('strlen') ? 'yes' : 'no'; echo ':'; echo is_callable('missing') ? 'yes' : 'no'; echo ':'; echo is_callable(42) ? 'yes' : 'no';";
    assert_eq!(
        compile_and_run_ir_backend("is_callable_static_names", source),
        "yes:yes:no:no"
    );
}

/// Verifies is_callable() treats include-discovered string names as known callables.
#[test]
fn ir_backend_handles_is_callable_for_include_variants() {
    let files = &[
        (
            "main.php",
            r#"<?php
if ($argc > 1) {
    include 'lib.php';
}
echo is_callable('optional_callable') ? 'yes' : 'no';
"#,
        ),
        ("lib.php", "<?php function optional_callable() { return 'loaded'; }"),
    ];
    assert_eq!(
        compile_and_run_ir_backend_files(
            "is_callable_include_variant_unloaded",
            files,
            "main.php",
            &[],
        ),
        "yes"
    );
    assert_eq!(
        compile_and_run_ir_backend_files(
            "is_callable_include_variant_loaded",
            files,
            "main.php",
            &["extra"],
        ),
        "yes"
    );
}

/// Verifies function-variant dispatch fails until the include path activates the variant.
#[test]
fn ir_backend_requires_include_before_function_variant_dispatch() {
    let run = compile_ir_backend_files_and_run(
        "require_once_function_variant_unloaded",
        &[
            (
                "main.php",
                "<?php echo double(5); require_once 'lib.php';",
            ),
            ("lib.php", "<?php function double($n) { return $n * 2; }"),
        ],
        "main.php",
        &[],
    );
    assert!(
        !run.status.success(),
        "IR backend unloaded function-variant fixture unexpectedly succeeded"
    );
    assert_eq!(
        String::from_utf8(run.stdout).expect("fatal stdout should be utf8"),
        ""
    );
    let stderr = String::from_utf8(run.stderr).expect("fatal stderr should be utf8");
    assert!(
        stderr.contains("Fatal error: Call to undefined function double()"),
        "unexpected fatal stderr: {stderr}"
    );
}

/// Compiles `source` with `--ir-backend`, runs the output binary, and returns stdout.
fn compile_and_run_ir_backend(name: &str, source: &str) -> String {
    compile_and_run_ir_backend_with_args(name, source, &[])
}

/// Compiles `source`, runs the output binary with extra args, and returns stdout.
fn compile_and_run_ir_backend_with_args(name: &str, source: &str, args: &[&str]) -> String {
    let run = compile_ir_backend_and_run(name, source, args);
    assert!(run.status.success(), "IR backend binary failed for {name}");
    String::from_utf8(run.stdout).unwrap()
}

/// Compiles `source` with `--ir-backend`, runs the binary, and returns raw process output.
fn compile_ir_backend_and_run(name: &str, source: &str, args: &[&str]) -> Output {
    let dir = std::env::temp_dir().join(format!(
        "elephc_ir_backend_{}_{}_{}",
        name,
        std::process::id(),
        unique_test_id()
    ));
    fs::create_dir_all(&dir).expect("failed to create IR backend hello directory");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("failed to write IR backend PHP fixture");

    let compile = Command::new(elephc_cli_bin())
        .env("XDG_CACHE_HOME", dir.join("cache-root"))
        .current_dir(&dir)
        .arg("--ir-backend")
        .arg(&php_path)
        .output()
        .expect("failed to run elephc CLI with --ir-backend");
    assert!(
        compile.status.success(),
        "elephc --ir-backend failed for {name}: stderr={}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let run = Command::new(dir.join("main"))
        .current_dir(&dir)
        .args(args)
        .output()
        .expect("failed to run IR backend binary");

    let _ = fs::remove_dir_all(&dir);
    run
}

/// Compiles multiple PHP files with `--ir-backend`, runs the entry binary, and returns stdout.
fn compile_and_run_ir_backend_files(
    name: &str,
    files: &[(&str, &str)],
    entry: &str,
    args: &[&str],
) -> String {
    let run = compile_ir_backend_files_and_run(name, files, entry, args);
    assert!(run.status.success(), "IR backend binary failed for {name}");
    String::from_utf8(run.stdout).unwrap()
}

/// Compiles a multi-file `--ir-backend` fixture and returns raw process output.
fn compile_ir_backend_files_and_run(
    name: &str,
    files: &[(&str, &str)],
    entry: &str,
    args: &[&str],
) -> Output {
    let dir = std::env::temp_dir().join(format!(
        "elephc_ir_backend_{}_{}_{}",
        name,
        std::process::id(),
        unique_test_id()
    ));
    fs::create_dir_all(&dir).expect("failed to create IR backend files directory");
    for (path, contents) in files {
        let path = dir.join(path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("failed to create IR backend fixture parent");
        }
        fs::write(path, contents).expect("failed to write IR backend PHP fixture");
    }
    let entry_path = dir.join(entry);

    let compile = Command::new(elephc_cli_bin())
        .env("XDG_CACHE_HOME", dir.join("cache-root"))
        .current_dir(&dir)
        .arg("--ir-backend")
        .arg(&entry_path)
        .output()
        .expect("failed to run elephc CLI with --ir-backend");
    assert!(
        compile.status.success(),
        "elephc --ir-backend failed for {name}: stderr={}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let binary_path = entry_binary_path(&entry_path);
    let run = Command::new(binary_path)
        .current_dir(&dir)
        .args(args)
        .output()
        .expect("failed to run IR backend binary");

    let _ = fs::remove_dir_all(&dir);
    run
}

/// Returns the binary path produced next to a PHP entry file.
fn entry_binary_path(entry_path: &Path) -> std::path::PathBuf {
    entry_path.with_extension("")
}

/// Returns a coarse unique suffix for temporary test directories.
fn unique_test_id() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos()
}
