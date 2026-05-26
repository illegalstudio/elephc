//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of callables constants and system, including const integer, const string, and const float.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures compile to native binaries while malformed or fatal cases assert captured failures.

use crate::support::*;

// Compiles PHP `source` to a native binary, expects it to fail at runtime,
// and returns the captured stderr. The temporary directory is cleaned up regardless
// of success or failure.
/// Provides the Compile and run expect runtime error helper used by the constants and system module.
fn compile_and_run_expect_runtime_error(source: &str) -> String {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let (user_asm, _runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    let stderr = assemble_and_run_expect_failure(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    let _ = fs::remove_dir_all(&dir);
    stderr
}

// --- Constants (const / define) ---

// Tests `const MAX = 100; echo MAX;` compiles and outputs "100".
/// Verifies that const integer.
#[test]
fn test_const_int() {
    let out = compile_and_run("<?php\nconst MAX = 100;\necho MAX;\n");
    assert_eq!(out, "100");
}

// Tests `const GREETING = "hello"; echo GREETING;` compiles and outputs "hello".
/// Verifies that const string.
#[test]
fn test_const_string() {
    let out = compile_and_run("<?php\nconst GREETING = \"hello\";\necho GREETING;\n");
    assert_eq!(out, "hello");
}

// Tests `const PI = 3.14; echo PI;` compiles and outputs "3.14".
/// Verifies that const float.
#[test]
fn test_const_float() {
    let out = compile_and_run("<?php\nconst PI = 3.14;\necho PI;\n");
    assert_eq!(out, "3.14");
}

// Tests `const DEBUG = true; echo DEBUG;` compiles and outputs "1" (PHP boolean true coercion).
/// Verifies that const boolean.
#[test]
fn test_const_bool() {
    let out = compile_and_run("<?php\nconst DEBUG = true;\necho DEBUG;\n");
    assert_eq!(out, "1");
}

// Tests `define("MAX_SIZE", 256); echo MAX_SIZE;` compiles and outputs "256".
/// Verifies that define integer.
#[test]
fn test_define_int() {
    let out = compile_and_run("<?php\ndefine(\"MAX_SIZE\", 256);\necho MAX_SIZE;\n");
    assert_eq!(out, "256");
}

// Tests `define("APP_NAME", "elephc"); echo APP_NAME;` compiles and outputs "elephc".
/// Verifies that define string.
#[test]
fn test_define_string() {
    let out = compile_and_run("<?php\ndefine(\"APP_NAME\", \"elephc\");\necho APP_NAME;\n");
    assert_eq!(out, "elephc");
}

// Tests `define()` returns `true` and the constant is usable: `echo define(...)` outputs "1",
// and echoing the constant name outputs its value.
/// Verifies that define returns true.
#[test]
fn test_define_returns_true() {
    let out = compile_and_run("<?php\necho define(\"FEATURE_ON\", true);\necho FEATURE_ON;\n");
    assert_eq!(out, "11");
}

// Tests that `@define(...)` suppresses the duplicate-constant warning.
// `DUPLICATE_CONST` is defined twice, the second call is wrapped with `@`,
// and the program must not emit a Warning to stderr.
/// Verifies that error control suppresses duplicate define warning.
#[test]
fn test_error_control_suppresses_duplicate_define_warning() {
    let out = compile_and_run_capture(
        "<?php\ndefine(\"DUPLICATE_CONST\", 1);\necho @define(\"DUPLICATE_CONST\", 2) ? \"bad\" : \"ok\";\necho DUPLICATE_CONST;\n",
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "ok1");
    assert_eq!(out.stderr, "");
}

// Tests that a duplicate `define()` call without `@` emits a PHP warning at runtime.
// The second `define("DUPLICATE_WARN", 2)` returns `false`, so "ok" is echoed,
// and stderr must contain "Warning: define()".
/// Verifies that duplicate define emits runtime warning.
#[test]
fn test_duplicate_define_emits_runtime_warning() {
    let out = compile_and_run_capture(
        "<?php\ndefine(\"DUPLICATE_WARN\", 1);\necho define(\"DUPLICATE_WARN\", 2) ? \"bad\" : \"ok\";\necho DUPLICATE_WARN;\n",
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "ok1");
    assert!(
        out.stderr.contains("Warning: define()"),
        "expected duplicate define warning, got stderr={}",
        out.stderr
    );
}

// Tests that `define()` checks for duplicate names at runtime (not compile time).
// `once()` is called twice; the first call defines `RUNTIME_DUPLICATE` and returns `true`,
// the second call (suppressed with `@`) returns `false`. No warning is emitted.
/// Verifies that define duplicate is checked at runtime.
#[test]
fn test_define_duplicate_is_checked_at_runtime() {
    let out = compile_and_run_capture(
        "<?php\nfunction once() { return define(\"RUNTIME_DUPLICATE\", 1); }\necho once() ? \"T\" : \"F\";\necho @once() ? \"T\" : \"F\";\necho RUNTIME_DUPLICATE;\n",
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "TF1");
    assert_eq!(out.stderr, "");
}

// Tests that two `const` declarations can be used together in an expression:
// `const X = 10; const Y = 20; echo X + Y;` outputs "30".
/// Verifies that const in expression.
#[test]
fn test_const_in_expression() {
    let out = compile_and_run("<?php\nconst X = 10;\nconst Y = 20;\necho X + Y;\n");
    assert_eq!(out, "30");
}

// Tests that a `const` declared at the top level is visible inside a function:
// `const LIMIT = 42; function test() { echo LIMIT; } test();` outputs "42".
/// Verifies that const in function.
#[test]
fn test_const_in_function() {
    let out =
        compile_and_run("<?php\nconst LIMIT = 42;\nfunction test() { echo LIMIT; }\ntest();\n");
    assert_eq!(out, "42");
}

// Tests that a `define()` call at the top level is visible inside a function:
// `define("RATE", 100); function show() { echo RATE; } show();` outputs "100".
/// Verifies that define in function.
#[test]
fn test_define_in_function() {
    let out =
        compile_and_run("<?php\ndefine(\"RATE\", 100);\nfunction show() { echo RATE; }\nshow();\n");
    assert_eq!(out, "100");
}

// Tests that a `const` string can be used with the concatenation operator:
// `const PREFIX = "hello"; echo PREFIX . " world";` outputs "hello world".
/// Verifies that const concat.
#[test]
fn test_const_concat() {
    let out = compile_and_run("<?php\nconst PREFIX = \"hello\";\necho PREFIX . \" world\";\n");
    assert_eq!(out, "hello world");
}

// --- List unpacking ---

// Tests `[$a, $b, $c] = [10, 20, 30]; echo $a . " " . $b . " " . $c;` outputs "10 20 30".
/// Verifies that list unpack integer.
#[test]
fn test_list_unpack_int() {
    let out = compile_and_run(
        "<?php\n[$a, $b, $c] = [10, 20, 30];\necho $a . \" \" . $b . \" \" . $c;\n",
    );
    assert_eq!(out, "10 20 30");
}

// Tests `[$x, $y] = ["hello", "world"]; echo $x . " " . $y;` outputs "hello world".
/// Verifies that list unpack string.
#[test]
fn test_list_unpack_string() {
    let out = compile_and_run("<?php\n[$x, $y] = [\"hello\", \"world\"];\necho $x . \" \" . $y;\n");
    assert_eq!(out, "hello world");
}

// Tests unpacking from a variable array: `$arr = [1, 2, 3]; [$a, $b, $c] = $arr;`
// outputs "1 2 3".
/// Verifies that list unpack from variable.
#[test]
fn test_list_unpack_from_variable() {
    let out = compile_and_run(
        "<?php\n$arr = [1, 2, 3];\n[$a, $b, $c] = $arr;\necho $a . \" \" . $b . \" \" . $c;\n",
    );
    assert_eq!(out, "1 2 3");
}

// Tests `[$first, $second] = [42, 99]; echo $first + $second;` outputs "141".
/// Verifies that list unpack two vars.
#[test]
fn test_list_unpack_two_vars() {
    let out = compile_and_run("<?php\n[$first, $second] = [42, 99];\necho $first + $second;\n");
    assert_eq!(out, "141");
}

// Tests skipped entries in list unpacking: `[$first, , $third,] = [10, 20, 30];`
// outputs "10 30". Commas without a variable name discard that element.
/// Verifies that list unpack skipped entries.
#[test]
fn test_list_unpack_skipped_entries() {
    let out = compile_and_run(
        "<?php\n[$first, , $third,] = [10, 20, 30];\necho $first . \" \" . $third;\n",
    );
    assert_eq!(out, "10 30");
}

// Tests nested list patterns: `[[$a, $b], [$c, $d]] = [[1, 2], [3, 4]];`
// outputs "1234".
/// Verifies that list unpack nested patterns.
#[test]
fn test_list_unpack_nested_patterns() {
    let out = compile_and_run(
        "<?php\n[[$a, $b], [$c, $d]] = [[1, 2], [3, 4]];\necho $a . $b . $c . $d;\n",
    );
    assert_eq!(out, "1234");
}

// Tests nested list with a heterogeneous inner array:
// `[[$a, $b], $c] = [[10, 20], 30];` outputs "10:20:30\n".
/// Verifies that list unpack nested pattern from heterogeneous array.
#[test]
fn test_list_unpack_nested_pattern_from_heterogeneous_array() {
    let out = compile_and_run(
        "<?php\n[[$a, $b], $c] = [[10, 20], 30];\necho $a . \":\" . $b . \":\" . $c;\necho \"\\n\";\n",
    );
    assert_eq!(out, "10:20:30\n");
}

// Tests list unpacking with associative keys:
// `["name" => $name, "id" => $id] = ["id" => 7, "name" => "Ada"];` outputs "7:Ada".
/// Verifies that list unpack associative keys.
#[test]
fn test_list_unpack_associative_keys() {
    let out = compile_and_run(
        "<?php\n[\"name\" => $name, \"id\" => $id] = [\"id\" => 7, \"name\" => \"Ada\"];\necho $id . \":\" . $name;\n",
    );
    assert_eq!(out, "7:Ada");
}

// Tests list unpacking with an associative key and a trailing comma is accepted:
// `["id" => $id,] = ["id" => 7];` outputs "7".
/// Verifies that list unpack associative keys allow trailing comma.
#[test]
fn test_list_unpack_associative_keys_allow_trailing_comma() {
    let out = compile_and_run("<?php\n[\"id\" => $id,] = [\"id\" => 7];\necho $id;\n");
    assert_eq!(out, "7");
}

// Tests that a variable key expression can be used as an associative key in list unpacking:
// `$key = "id"; [$key => $id] = ["id" => 7];` outputs "7".
/// Verifies that list unpack dynamic associative key.
#[test]
fn test_list_unpack_dynamic_associative_key() {
    let out = compile_and_run(
        "<?php\n$key = \"id\";\n[$key => $id] = [\"id\" => 7];\necho $id;\n",
    );
    assert_eq!(out, "7");
}

// Tests the legacy `list()` syntax with skipped entries:
// `list($a, , $c) = [1, 2, 3];` outputs "13".
/// Verifies that list construct unpack with skipped entries.
#[test]
fn test_list_construct_unpack_with_skipped_entries() {
    let out = compile_and_run("<?php\nlist($a, , $c) = [1, 2, 3];\necho $a . $c;\n");
    assert_eq!(out, "13");
}

// Tests that list unpacking can target array-index mutations:
// `$items = [0]; [$items[0], $items[]] = [5, 6];` outputs "5 6".
/// Verifies that list unpack array append target.
#[test]
fn test_list_unpack_array_append_target() {
    let out = compile_and_run(
        "<?php\n$items = [0];\n[$items[0], $items[]] = [5, 6];\necho $items[0] . \" \" . $items[1];\n",
    );
    assert_eq!(out, "5 6");
}

// Tests that list unpacking can target an object property:
// `[$box->x] = [42];` where `$box = new Box();` outputs "42".
/// Verifies that list unpack object property target.
#[test]
fn test_list_unpack_object_property_target() {
    let out = compile_and_run(
        "<?php\nclass Box { public int $x = 0; }\n$box = new Box();\n[$box->x] = [42];\necho $box->x;\n",
    );
    assert_eq!(out, "42");
}

// Tests that list unpacking can target static properties with index access and append:
// `class Bag { public static array $items = [0]; }`
// `[Bag::$items[0], Bag::$items[]] = [7, 8];` outputs "7 8".
/// Verifies that list unpack static property targets.
#[test]
fn test_list_unpack_static_property_targets() {
    let out = compile_and_run(
        "<?php\nclass Bag { public static array $items = [0]; }\n[Bag::$items[0], Bag::$items[]] = [7, 8];\necho Bag::$items[0] . \" \" . Bag::$items[1];\n",
    );
    assert_eq!(out, "7 8");
}

// --- call_user_func_array ---

// Tests basic `call_user_func_array("add", [3, 4])` where `add($a, $b) { return $a + $b; }`
// outputs "7".
/// Verifies that call user func array basic.
#[test]
fn test_call_user_func_array_basic() {
    let out = compile_and_run("<?php\nfunction add($a, $b) { return $a + $b; }\necho call_user_func_array(\"add\", [3, 4]);\n");
    assert_eq!(out, "7");
}

// Tests `call_user_func_array("double", [21])` where `double($n) { return $n * 2; }`
// outputs "42".
/// Verifies that call user func array single arg.
#[test]
fn test_call_user_func_array_single_arg() {
    let out = compile_and_run("<?php\nfunction double($n) { return $n * 2; }\necho call_user_func_array(\"double\", [21]);\n");
    assert_eq!(out, "42");
}

// Tests that `call_user_func_array("greet", ["World"])` returns a string:
// `function greet($name) { return "Hello " . $name; }` outputs "Hello World".
/// Verifies that call user func array string return.
#[test]
fn test_call_user_func_array_string_return() {
    let out = compile_and_run("<?php\nfunction greet($name) { return \"Hello \" . $name; }\necho call_user_func_array(\"greet\", [\"World\"]);\n");
    assert_eq!(out, "Hello World");
}

// Tests that `call_user_func_array` can invoke a builtin function by its string name:
// `call_user_func_array("STRLEN", ["hello"])` outputs "5".
/// Verifies that call user func array string builtin callback.
#[test]
fn test_call_user_func_array_string_builtin_callback() {
    let out = compile_and_run(r#"<?php echo call_user_func_array("STRLEN", ["hello"]);"#);
    assert_eq!(out, "5");
}

// Tests `call_user_func_array(summarize(...), [7, 8, 9])` with a variadic callback:
// `summarize($head = 1, ...$rest)` outputs "7:2" (head=7, rest has 2 elements).
/// Verifies that call user func by ref callable parameter uses descriptor entry.
#[test]
fn test_call_user_func_by_ref_callable_parameter_uses_descriptor_entry() {
    let out = compile_and_run(
        r#"<?php
function run(callable &$cb): void {
    echo call_user_func($cb, 6);
}
$cb = function($n) { return $n * 2; };
run($cb);
"#,
    );
    assert_eq!(out, "12");
}

/// Verifies that call user func dynamic string user callback.
#[test]
fn test_call_user_func_dynamic_string_user_callback() {
    let out = compile_and_run(
        r#"<?php
function add_pair($left, $right): int {
    return $left + $right;
}
$callback = "ADD_PAIR";
echo call_user_func($callback, 2, 5);
"#,
    );
    assert_eq!(out, "7");
}

/// Verifies that call user func dynamic string boxes string return.
#[test]
fn test_call_user_func_dynamic_string_boxes_string_return() {
    let out = compile_and_run(
        r#"<?php
function greet_dynamic(string $name): string {
    return "hi " . $name;
}
$callback = "greet_dynamic";
echo call_user_func($callback, "Ada");
"#,
    );
    assert_eq!(out, "hi Ada");
}

/// Verifies that call user func dynamic string builtin callback.
#[test]
fn test_call_user_func_dynamic_string_builtin_callback() {
    let out = compile_and_run(
        r#"<?php
$callback = "STRLEN";
echo call_user_func($callback, "hello");
"#,
    );
    assert_eq!(out, "5");
}

/// Verifies that call user func dynamic string static method callback.
#[test]
fn test_call_user_func_dynamic_string_static_method_callback() {
    let out = compile_and_run(
        r#"<?php
class Formatter {
    public static function wrap(string $value): string {
        return "[" . $value . "]";
    }
}

$callback = "Formatter::wrap";
echo call_user_func($callback, "ok");
"#,
    );
    assert_eq!(out, "[ok]");
}

/// Verifies that call user func array dynamic string assoc callback.
#[test]
fn test_call_user_func_array_dynamic_string_assoc_callback() {
    let out = compile_and_run(
        r#"<?php
function stamp_named(string $prefix, int $value): string {
    return $prefix . ":" . $value;
}
$callback = "stamp_named";
$args = ["value" => 7, "prefix" => "id"];
echo call_user_func_array($callback, $args);
"#,
    );
    assert_eq!(out, "id:7");
}

/// Verifies that call user func array dynamic string builtin assoc callback.
#[test]
fn test_call_user_func_array_dynamic_string_builtin_assoc_callback() {
    let out = compile_and_run(
        r#"<?php
$callback = "strlen";
$args = ["string" => "hello"];
echo call_user_func_array($callback, $args);
"#,
    );
    assert_eq!(out, "5");
}

/// Verifies that call user func array dynamic string static method assoc callback.
#[test]
fn test_call_user_func_array_dynamic_string_static_method_assoc_callback() {
    let out = compile_and_run(
        r#"<?php
class Formatter {
    public static function join(string $prefix, int $value): string {
        return $prefix . ":" . $value;
    }
}

$callback = "Formatter::join";
$args = ["value" => 7, "prefix" => "id"];
echo call_user_func_array($callback, $args);
"#,
    );
    assert_eq!(out, "id:7");
}

/// Verifies that call user func invokable object callback.
#[test]
fn test_call_user_func_invokable_object_callback() {
    let out = compile_and_run(
        r#"<?php
class Twice {
    public function __invoke(int $value): int {
        return $value * 2;
    }
}

echo call_user_func(new Twice(), 9);
"#,
    );
    assert_eq!(out, "18");
}

/// Verifies that call user func array instance method array callback.
#[test]
fn test_call_user_func_array_instance_method_array_callback() {
    let out = compile_and_run(
        r#"<?php
class Formatter {
    public function join(string $prefix, int $value): string {
        return $prefix . ":" . $value;
    }
}

$formatter = new Formatter();
$args = ["value" => 7, "prefix" => "id"];
echo call_user_func_array([$formatter, "join"], $args);
"#,
    );
    assert_eq!(out, "id:7");
}

/// Verifies that call user func static method array callback.
#[test]
fn test_call_user_func_static_method_array_callback() {
    let out = compile_and_run(
        r#"<?php
class Formatter {
    public static function wrap(string $value): string {
        return "[" . $value . "]";
    }
}

echo call_user_func(["Formatter", "wrap"], "ok");
"#,
    );
    assert_eq!(out, "[ok]");
}

/// Verifies that call user func variable instance method array callback.
#[test]
fn test_call_user_func_variable_instance_method_array_callback() {
    let out = compile_and_run(
        r#"<?php
class Formatter {
    public function join(string $prefix, int $value): string {
        return $prefix . ":" . $value;
    }
}

$formatter = new Formatter();
$callback = [$formatter, "join"];
echo call_user_func($callback, "id", 7);
"#,
    );
    assert_eq!(out, "id:7");
}

/// Verifies that call user func array variable static method array callback.
#[test]
fn test_call_user_func_array_variable_static_method_array_callback() {
    let out = compile_and_run(
        r#"<?php
class Formatter {
    public static function wrap(string $value): string {
        return "[" . $value . "]";
    }
}

$callback = [Formatter::class, "wrap"];
$args = ["value" => "ok"];
echo call_user_func_array($callback, $args);
"#,
    );
    assert_eq!(out, "[ok]");
}

/// Verifies that call user func array dynamic args for callable without known signature.
#[test]
fn test_call_user_func_array_dynamic_args_for_callable_without_known_signature() {
    let out = compile_and_run(
        r#"<?php
function make_callback(): callable {
    return function(string $prefix): int {
        echo $prefix;
        return 7;
    };
}
$args = ["abc"];
echo call_user_func_array(make_callback(), $args);
"#,
    );
    assert_eq!(out, "abc7");
}

/// Verifies that call user func array unknown signature dynamic args overflow stack.
#[test]
fn test_call_user_func_array_unknown_signature_dynamic_args_overflow_stack() {
    let out = compile_and_run(
        r#"<?php
function make_callback(): callable {
    return function(
        $a1, $a2, $a3, $a4, $a5,
        $a6, $a7, $a8, $a9, $a10,
        $a11, $a12, $a13, $a14, $a15,
        $a16, $a17, $a18, $a19, $a20
    ): int {
        return $a1 + $a2 + $a3 + $a4 + $a5
            + $a6 + $a7 + $a8 + $a9 + $a10
            + $a11 + $a12 + $a13 + $a14 + $a15
            + $a16 + $a17 + $a18 + $a19 + $a20;
    };
}

$args = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10,
         11, 12, 13, 14, 15, 16, 17, 18, 19, 20];
echo call_user_func_array(make_callback(), $args);
"#,
    );
    assert_eq!(out, "210");
}

/// Verifies that call user func array unknown signature captured callback dynamic args overflow stack.
#[test]
fn test_call_user_func_array_unknown_signature_captured_callback_dynamic_args_overflow_stack() {
    let out = compile_and_run(
        r#"<?php
$base = 10;
$callbacks = [
    function(
        $a1, $a2, $a3, $a4, $a5,
        $a6, $a7, $a8, $a9, $a10,
        $a11, $a12, $a13, $a14, $a15,
        $a16, $a17, $a18, $a19, $a20
    ) use ($base): int {
        return $base + $a1 + $a2 + $a3 + $a4 + $a5
            + $a6 + $a7 + $a8 + $a9 + $a10
            + $a11 + $a12 + $a13 + $a14 + $a15
            + $a16 + $a17 + $a18 + $a19 + $a20;
    }
];
$cb = $callbacks[0];
$args = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10,
         11, 12, 13, 14, 15, 16, 17, 18, 19, 20];
echo call_user_func_array($cb, $args);
"#,
    );
    assert_eq!(out, "220");
}

/// Verifies that call user func array unknown signature dynamic string args overflow stack.
#[test]
fn test_call_user_func_array_unknown_signature_dynamic_string_args_overflow_stack() {
    let out = compile_and_run(
        r#"<?php
function make_callback(): callable {
    return function(string $a, string $b, string $c, string $d, string $e, string $f): int {
        echo $a . $b . $c . $d . $e . $f;
        return 1;
    };
}

$args = ["a", "b", "c", "d", "e", "f"];
echo call_user_func_array(make_callback(), $args);
"#,
    );
    assert_eq!(out, "abcdef1");
}

/// Verifies that call user func array dynamic assoc args for known signature.
#[test]
fn test_call_user_func_array_dynamic_assoc_args_for_known_signature() {
    let out = compile_and_run(
        r#"<?php
function stamp($prefix, $value): int {
    echo $prefix;
    echo ":";
    echo $value;
    return 9;
}
$args = ["value" => 7, "prefix" => "id"];
echo call_user_func_array("stamp", $args);
"#,
    );
    assert_eq!(out, "id:79");
}

/// Verifies that call user func array variadic callback.
#[test]
fn test_call_user_func_array_variadic_callback() {
    let out = compile_and_run(
        "<?php
        function summarize($head = 1, ...$rest) {
            echo $head;
            echo \":\";
            echo count($rest);
        }
        call_user_func_array(summarize(...), [7, 8, 9]);
        ",
    );
    assert_eq!(out, "7:2");
}

// Tests that `call_user_func_array(count_parts(...), [1.5, 2.5])` correctly counts
// a variadic parameter containing float values: outputs "2".
/// Verifies that call user func array dynamic assoc args for variadic callback.
#[test]
fn test_call_user_func_array_dynamic_assoc_args_for_variadic_callback() {
    let out = compile_and_run(
        r#"<?php
function summarize($head, ...$rest) {
    echo $head;
    echo ":";
    foreach ($rest as $key => $value) {
        echo $key . "=" . $value . ";";
    }
}
$args = ["head" => 1, "x" => 2, "y" => 3];
call_user_func_array("summarize", $args);
"#,
    );
    assert_eq!(out, "1:x=2;y=3;");
}

/// Verifies that call user func array first class dynamic assoc args for variadic callback.
#[test]
fn test_call_user_func_array_first_class_dynamic_assoc_args_for_variadic_callback() {
    let out = compile_and_run(
        r#"<?php
function summarize($head, ...$rest) {
    echo $head;
    echo ":";
    foreach ($rest as $key => $value) {
        echo $key . "=" . $value . ";";
    }
}
$args = ["head" => 1, "x" => 2];
$cb = summarize(...);
call_user_func_array($cb, $args);
echo "|";
call_user_func_array(summarize(...), $args);
"#,
    );
    assert_eq!(out, "1:x=2;|1:x=2;");
}

/// Verifies that call user func array dynamic assoc args for returned callable signature.
#[test]
fn test_call_user_func_array_dynamic_assoc_args_for_returned_callable_signature() {
    let out = compile_and_run(
        r#"<?php
function make_callback(): callable {
    return function(string $prefix): int {
        echo $prefix;
        return 7;
    };
}

$args = ["prefix" => "abc"];
echo call_user_func_array(make_callback(), $args);
"#,
    );
    assert_eq!(out, "abc7");
}

/// Verifies that call user func array dynamic assoc args for returned untyped callable signature.
#[test]
fn test_call_user_func_array_dynamic_assoc_args_for_returned_untyped_callable_signature() {
    let out = compile_and_run(
        r#"<?php
function make_callback(): callable {
    return function($left, $right): int {
        return ($left * 10) + $right;
    };
}

$args = ["right" => 2, "left" => 1];
echo call_user_func_array(make_callback(), $args);
"#,
    );
    assert_eq!(out, "12");
}

/// Verifies that call user func array dynamic assoc args for callable without static signature.
#[test]
fn test_call_user_func_array_dynamic_assoc_args_for_callable_without_static_signature() {
    let out = compile_and_run(
        r#"<?php
$callbacks = [
    function($left, $right): int {
        return ($left * 10) + $right;
    },
    function($right, $left): int {
        return ($right * 100) + $left;
    }
];
$idx = 0;
$cb = $callbacks[$idx];
$args = ["right" => 2, "left" => 1];
echo call_user_func_array($cb, $args);
"#,
    );
    assert_eq!(out, "12");
}

/// Verifies that call user func array dynamic assoc unknown signature boxes string return.
#[test]
fn test_call_user_func_array_dynamic_assoc_unknown_signature_boxes_string_return() {
    let out = compile_and_run(
        r#"<?php
$callbacks = [
    function($left, $right): string {
        return "sum:" . ($left + $right);
    },
    function($right, $left): string {
        return "alt:" . ($right + $left);
    }
];
$idx = 0;
$cb = $callbacks[$idx];
$args = ["right" => 2, "left" => 1];
echo call_user_func_array($cb, $args);
"#,
    );
    assert_eq!(out, "sum:3");
}

/// Verifies that call user func array dynamic indexed unknown signature boxes string return.
#[test]
fn test_call_user_func_array_dynamic_indexed_unknown_signature_boxes_string_return() {
    let out = compile_and_run(
        r#"<?php
$callbacks = [
    function($value): string {
        return "v:" . $value;
    },
];
$idx = 0;
$cb = $callbacks[$idx];
$args = [7];
echo call_user_func_array($cb, $args);
"#,
    );
    assert_eq!(out, "v:7");
}

/// Verifies that call user func array variadic float tail count.
#[test]
fn test_call_user_func_array_variadic_float_tail_count() {
    let out = compile_and_run(
        "<?php
        function count_parts(...$parts) {
            echo count($parts);
        }
        call_user_func_array(count_parts(...), [1.5, 2.5]);
        ",
    );
    assert_eq!(out, "2");
}

// Tests that a first-class callable (`$f = bump(...)`) preserves by-ref parameter
// semantics when invoked via `call_user_func_array($f, [$value])`: `$value` is mutated to 6.
/// Verifies that call user func array first class callable preserves by ref params.
#[test]
fn test_call_user_func_array_first_class_callable_preserves_by_ref_params() {
    let out = compile_and_run(
        r#"<?php
function bump(&$n) {
    $n = $n + 1;
}

$f = bump(...);
$value = 5;
call_user_func_array($f, [$value]);
echo $value;
"#,
    );
    assert_eq!(out, "6");
}

// Tests that a string callback `"bump"` preserves by-ref parameter semantics when invoked
// via `call_user_func_array("bump", [$value])`: `$value` is mutated to 6.
/// Verifies that call user func array string callback preserves by ref params.
#[test]
fn test_call_user_func_array_string_callback_preserves_by_ref_params() {
    let out = compile_and_run(
        r#"<?php
function bump(&$n) {
    $n = $n + 1;
}

$value = 5;
call_user_func_array("bump", [$value]);
echo $value;
"#,
    );
    assert_eq!(out, "6");
}

// Tests that a method callable (`$f = $counter->bump(...)`) preserves by-ref parameter
// semantics and captures `$counter` correctly when invoked via
// `call_user_func_array($f, [$value])`: `$value` is mutated to 7.
/// Verifies that call user func array method callable preserves by ref params and capture.
#[test]
fn test_call_user_func_array_method_callable_preserves_by_ref_params_and_capture() {
    let out = compile_and_run(
        r#"<?php
class Counter {
    public function bump(&$n) {
        $n = $n + 2;
    }
}

$counter = new Counter();
$f = $counter->bump(...);
$value = 5;
call_user_func_array($f, [$value]);
echo $value;
"#,
    );
    assert_eq!(out, "7");
}

/// Verifies that call user func array dynamic args for by ref callback use temp cells.
#[test]
fn test_call_user_func_array_dynamic_args_for_by_ref_callback_use_temp_cells() {
    let out = compile_and_run(
        r#"<?php
function bump(&$n) {
    $n = $n + 1;
}

$value = 5;
$args = [$value];
call_user_func_array("bump", $args);
echo $value;
echo ":";
echo $args[0];
"#,
    );
    assert_eq!(out, "5:5");
}

// -- v0.8 constants --

// Tests `echo "a" . PHP_EOL . "b";` outputs "a\nb" (platform newline).
/// Verifies that PHP eol.
#[test]
fn test_php_eol() {
    let out = compile_and_run("<?php echo \"a\" . PHP_EOL . \"b\";");
    assert_eq!(out, "a\nb");
}

// Tests `echo PHP_OS;` outputs the platform-specific OS name (e.g. "Darwin" on macOS).
// The expected value is retrieved from `target().platform.php_os_name()`.
/// Verifies that PHP os.
#[test]
fn test_php_os() {
    let out = compile_and_run("<?php echo PHP_OS;");
    assert_eq!(out, target().platform.php_os_name());
}

// Tests `echo DIRECTORY_SEPARATOR;` outputs "/" (Unix path separator). PHP on Unix
// uses "/" as the directory separator; Windows uses "\\".
/// Verifies that directory separator.
#[test]
fn test_directory_separator() {
    let out = compile_and_run("<?php echo DIRECTORY_SEPARATOR;");
    assert_eq!(out, "/");
}

// -- v0.8 time / microtime --

// Tests `time()` returns a Unix timestamp greater than 1 billion (valid date after ~2001).
/// Verifies that time.
#[test]
fn test_time() {
    let out = compile_and_run("<?php $t = time(); if ($t > 1000000000) { echo \"ok\"; }");
    assert_eq!(out, "ok");
}

// Tests `microtime(true)` returns a float timestamp greater than 1 billion.
/// Verifies that microtime.
#[test]
fn test_microtime() {
    let out = compile_and_run("<?php $t = microtime(true); if ($t > 1000000000) { echo \"ok\"; }");
    assert_eq!(out, "ok");
}

// -- v0.8 sleep / usleep --

// Tests `sleep(0)` succeeds (no-op sleep) and outputs "ok".
/// Verifies that sleep zero.
#[test]
fn test_sleep_zero() {
    let out = compile_and_run("<?php sleep(0); echo \"ok\";");
    assert_eq!(out, "ok");
}

// Tests `usleep(0)` succeeds (no-op microsecond sleep) and outputs "ok".
/// Verifies that usleep zero.
#[test]
fn test_usleep_zero() {
    let out = compile_and_run("<?php usleep(0); echo \"ok\";");
    assert_eq!(out, "ok");
}

// -- v0.8 getenv --

// Tests `getenv("HOME")` returns a non-empty string on the current platform.
/// Verifies that getenv home.
#[test]
fn test_getenv_home() {
    let out =
        compile_and_run("<?php $home = getenv(\"HOME\"); if (strlen($home) > 0) { echo \"ok\"; }");
    assert_eq!(out, "ok");
}

// Tests `getenv("ELEPHC_NONEXISTENT_VAR_XYZ")` returns an empty string (strlen=0)
// for a non-existent environment variable.
/// Verifies that getenv nonexistent.
#[test]
fn test_getenv_nonexistent() {
    let out = compile_and_run(
        "<?php $missing = getenv(\"ELEPHC_NONEXISTENT_VAR_XYZ\"); echo strlen($missing);",
    );
    assert_eq!(out, "0");
}

// Tests `putenv("ELEPHC_TEST_VAR=hello")` followed by `getenv("ELEPHC_TEST_VAR")`
// returns "hello". Verifies environment variable set/get round-trip.
/// Verifies that putenv.
#[test]
fn test_putenv() {
    let out = compile_and_run(
        r#"<?php
putenv("ELEPHC_TEST_VAR=hello");
echo getenv("ELEPHC_TEST_VAR");
"#,
    );
    assert_eq!(out, "hello");
}

// -- v0.8 phpversion / php_uname --

// Tests `phpversion()` returns the compiler version string (`CARGO_PKG_VERSION`).
/// Verifies that phpversion.
#[test]
fn test_phpversion() {
    let out = compile_and_run("<?php echo phpversion();");
    assert_eq!(out, env!("CARGO_PKG_VERSION"));
}

// Tests `php_uname()` and `php_uname("a")` return identical strings (default "a" mode).
/// Verifies that PHP uname.
#[test]
fn test_php_uname() {
    let out = compile_and_run(
        r#"<?php
$default = php_uname();
$explicit = php_uname("a");
echo $default === $explicit ? "same" : "different";
"#,
    );
    assert_eq!(out, "same");
}

// Tests `php_uname()` modes "s", "n", "r", "v", "m", and "a" all return non-empty strings,
// and mode "a" contains the values from all other modes. Verifies each component is present
// in the full "a" output.
/// Verifies that PHP uname modes.
#[test]
fn test_php_uname_modes() {
    let out = compile_and_run(
        r#"<?php
$sys = php_uname("s");
$node = php_uname("n");
$release = php_uname("r");
$version = php_uname("v");
$machine = php_uname("m");
$all = php_uname("a");
echo $sys . "\n";
if (
    strlen($node) > 0 &&
    strlen($release) > 0 &&
    strlen($version) > 0 &&
    strlen($machine) > 0 &&
    str_contains($all, $sys) &&
    str_contains($all, $node) &&
    str_contains($all, $release) &&
    str_contains($all, $version) &&
    str_contains($all, $machine)
) {
    echo "ok";
} else {
    echo "bad";
}
"#,
    );
    assert_eq!(out, format!("{}\nok", target().platform.php_os_name()));
}

// Tests that `php_uname("sn")` (2-character mode string) fails at runtime with a
// "must be a single character" error.
/// Verifies that PHP uname rejects invalid mode length at runtime.
#[test]
fn test_php_uname_rejects_invalid_mode_length_at_runtime() {
    let err = compile_and_run_expect_runtime_error(r#"<?php $mode = "sn"; echo php_uname($mode);"#);
    assert!(err.contains("php_uname(): Argument #1 ($mode) must be a single character"));
}

// Tests that `php_uname("x")` (valid length but invalid mode character) fails at runtime
// with a "must be one of" error.
/// Verifies that PHP uname rejects invalid mode value at runtime.
#[test]
fn test_php_uname_rejects_invalid_mode_value_at_runtime() {
    let err = compile_and_run_expect_runtime_error(r#"<?php $mode = "x"; echo php_uname($mode);"#);
    assert!(err.contains("php_uname(): Argument #1 ($mode) must be one of"));
}

// -- v0.8 exec / shell_exec / system / passthru --

// Tests `shell_exec("echo hello")` returns "hello" (trimmed).
/// Verifies that shell exec.
#[test]
fn test_shell_exec() {
    let out = compile_and_run("<?php $out = shell_exec(\"echo hello\"); echo trim($out);");
    assert_eq!(out, "hello");
}

// Tests `exec("echo test")` returns only the last line "test" (trimmed).
/// Verifies that exec.
#[test]
fn test_exec() {
    let out = compile_and_run("<?php $out = exec(\"echo test\"); echo trim($out);");
    assert_eq!(out, "test");
}

// Tests `system("echo hi")` outputs "hi\n" (writes directly to stdout).
/// Verifies that system.
#[test]
fn test_system() {
    let out = compile_and_run("<?php system(\"echo hi\");");
    assert_eq!(out, "hi\n");
}

// Tests `passthru("echo bye")` outputs "bye\n" (writes directly to stdout, like system).
/// Verifies that passthru.
#[test]
fn test_passthru() {
    let out = compile_and_run("<?php passthru(\"echo bye\");");
    assert_eq!(out, "bye\n");
}
