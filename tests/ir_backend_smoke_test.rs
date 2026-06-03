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
