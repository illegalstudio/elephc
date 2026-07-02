//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of I/O printing, including print basic, print integer, and print expression returns one.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies that `print` outputs a plain string literal unchanged.
#[test]
fn test_print_basic() {
    let out = compile_and_run("<?php print \"hello\";");
    assert_eq!(out, "hello");
}

/// Verifies that `print` outputs a bare integer literal as its decimal string representation.
#[test]
fn test_print_int() {
    let out = compile_and_run("<?php print 42;");
    assert_eq!(out, "42");
}

/// Verifies that `print` returns `1` when used in an expression context, matching PHP's value-for-side-effect semantics.
#[test]
fn test_print_expression_returns_one() {
    let out = compile_and_run("<?php $ok = print \"hello\"; echo \"\\n\"; echo $ok;");
    assert_eq!(out, "hello\n1");
}

/// Verifies that `print` returning `1` is correctly absorbed by `echo`, producing `"x1"` not `"x"` or a parse error.
#[test]
fn test_print_expression_can_be_nested_in_echo() {
    let out = compile_and_run("<?php echo print \"x\";");
    assert_eq!(out, "x1");
}

/// Verifies that `print` can accept a short-ternary expression as its operand; `print` binds tighter than `?:`, so `false ?: "fallback"` is evaluated first, then printed, and the resulting `1` return is echoed.
#[test]
fn test_print_expression_operand_accepts_short_ternary() {
    let out = compile_and_run("<?php echo print false ?: \"fallback\";");
    assert_eq!(out, "fallback1");
}

/// Verifies precedence: `print "x" and false` parses as `(print "x") and false` — `print` outputs and returns `1`, which is truthy, so `and false` does not suppress output.
#[test]
fn test_print_expression_binds_tighter_than_word_and() {
    let out = compile_and_run("<?php echo print \"x\" and false;");
    assert_eq!(out, "x");
}

/// Verifies that `print __FILE__` emits the source file path at compile time (magic constant lowering).
#[test]
fn test_print_expression_lowers_magic_constants() {
    let out = compile_and_run("<?php print __FILE__;");
    assert!(out.ends_with("test.php"), "unexpected __FILE__ output: {out}");
}

/// Verifies `var_dump` formats a bare integer as `int(N)` with a trailing newline.
#[test]
fn test_var_dump_int() {
    let out = compile_and_run("<?php var_dump(42);");
    assert_eq!(out, "int(42)\n");
}

/// Verifies `var_dump` formats a string as `string(N) "..."` including length, quotes, and a trailing newline.
#[test]
fn test_var_dump_string() {
    let out = compile_and_run(r#"<?php var_dump("hello");"#);
    assert_eq!(out, "string(5) \"hello\"\n");
}

/// Verifies `var_dump` formats boolean `true` as `bool(true)` with a trailing newline.
#[test]
fn test_var_dump_bool_true() {
    let out = compile_and_run("<?php var_dump(true);");
    assert_eq!(out, "bool(true)\n");
}

/// Verifies `var_dump` formats boolean `false` as `bool(false)` with a trailing newline.
#[test]
fn test_var_dump_bool_false() {
    let out = compile_and_run("<?php var_dump(false);");
    assert_eq!(out, "bool(false)\n");
}

/// Verifies `var_dump` formats `null` as `NULL` (uppercase, no parentheses) with a trailing newline.
#[test]
fn test_var_dump_null() {
    let out = compile_and_run("<?php var_dump(null);");
    assert_eq!(out, "NULL\n");
}

/// Verifies `var_dump` formats a float as `float(VALUE)` with full precision and a trailing newline.
#[test]
fn test_var_dump_float() {
    let out = compile_and_run("<?php var_dump(3.14);");
    assert_eq!(out, "float(3.14)\n");
}

/// Verifies `var_dump` emits the correct concrete type tag and value for each heterogeneous assoc-array slot: int, string, bool, null, array, and object.
#[test]
fn test_var_dump_mixed_prints_concrete_payload() {
    let out = compile_and_run(
        r#"<?php
class Box {}

$map = [
    "i" => 42,
    "s" => "hello",
    "b" => true,
    "n" => null,
    "a" => [1, 2],
    "o" => new Box(),
];

var_dump($map["i"]);
var_dump($map["s"]);
var_dump($map["b"]);
var_dump($map["n"]);
var_dump($map["a"]);
var_dump($map["o"]);
"#,
    );
    assert_eq!(
        out,
        "int(42)\nstring(5) \"hello\"\nbool(true)\nNULL\narray(2) {\n}\nobject(Box)\n"
    );
}

/// Verifies `print_r` outputs a bare integer as its decimal string representation (no type label), no trailing newline.
#[test]
fn test_print_r_int() {
    let out = compile_and_run("<?php print_r(42);");
    assert_eq!(out, "42");
}

/// Verifies `print_r` outputs a string unchanged, no type label, no trailing newline.
#[test]
fn test_print_r_string() {
    let out = compile_and_run(r#"<?php print_r("hello");"#);
    assert_eq!(out, "hello");
}

/// Verifies `print_r` outputs `1` for boolean `true`, no type label, no trailing newline.
#[test]
fn test_print_r_bool_true() {
    let out = compile_and_run("<?php print_r(true);");
    assert_eq!(out, "1");
}

/// Verifies `print_r` outputs an empty string for boolean `false`.
#[test]
fn test_print_r_bool_false() {
    let out = compile_and_run("<?php print_r(false);");
    assert_eq!(out, "");
}

/// Verifies `print_r` renders an indexed array with PHP's recursive
/// `Array\n(\n    [N] => value\n)\n` body and numeric keys.
#[test]
fn test_print_r_array() {
    let out = compile_and_run("<?php print_r([1, 2, 3]);");
    assert_eq!(out, "Array\n(\n    [0] => 1\n    [1] => 2\n    [2] => 3\n)\n");
}

/// Verifies `print_r` renders an indexed string array, with raw (unquoted) values.
#[test]
fn test_print_r_string_array() {
    let out = compile_and_run(r#"<?php print_r(["a", "b", "c"]);"#);
    assert_eq!(out, "Array\n(\n    [0] => a\n    [1] => b\n    [2] => c\n)\n");
}

/// Verifies `print_r` renders a bool array with PHP's `1`/empty rendering for true/false.
#[test]
fn test_print_r_bool_array() {
    let out = compile_and_run("<?php print_r([true, false, true]);");
    assert_eq!(out, "Array\n(\n    [0] => 1\n    [1] => \n    [2] => 1\n)\n");
}

/// Verifies `print_r` renders a float array using PHP's float text.
#[test]
fn test_print_r_float_array() {
    let out = compile_and_run("<?php print_r([1.5, 2.25]);");
    assert_eq!(out, "Array\n(\n    [0] => 1.5\n    [1] => 2.25\n)\n");
}

/// Verifies `print_r` renders an associative array with unquoted string keys.
#[test]
fn test_print_r_assoc_array() {
    let out = compile_and_run(r#"<?php print_r(["name" => "bob", "age" => 30]);"#);
    assert_eq!(out, "Array\n(\n    [name] => bob\n    [age] => 30\n)\n");
}

/// Verifies `print_r` renders an empty array as the bare `Array\n(\n)\n` shell.
#[test]
fn test_print_r_empty_array() {
    let out = compile_and_run("<?php print_r([]);");
    assert_eq!(out, "Array\n(\n)\n");
}

/// Verifies `print_r` renders a hash with a heterogeneous (Mixed) value set,
/// matching PHP's per-type rendering (string raw, bool `1`, null empty).
#[test]
fn test_print_r_mixed_value_hash() {
    let out = compile_and_run(r#"<?php print_r(["s" => "x", "b" => true, "n" => null]);"#);
    assert_eq!(out, "Array\n(\n    [s] => x\n    [b] => 1\n    [n] => \n)\n");
}

/// Verifies `print_r` recurses into a nested array inside a hash, indenting the
/// nested body by 8 spaces per level and emitting the trailing blank line that
/// PHP writes after a nested array's closing paren.
#[test]
fn test_print_r_nested_array_in_hash() {
    let out = compile_and_run(r#"<?php print_r(["x" => [1, 2], "y" => 3]);"#);
    assert_eq!(
        out,
        "Array\n(\n    [x] => Array\n        (\n            [0] => 1\n            [1] => 2\n        )\n\n    [y] => 3\n)\n"
    );
}

/// Verifies `print_r` recurses into an array of arrays (indexed nesting), which
/// relies on the runtime value_type stamp to dispatch the nested element type.
#[test]
fn test_print_r_nested_indexed_arrays() {
    let out = compile_and_run("<?php print_r([[1, 2], [3, 4]]);");
    assert_eq!(
        out,
        "Array\n(\n    [0] => Array\n        (\n            [0] => 1\n            [1] => 2\n        )\n\n    [1] => Array\n        (\n            [0] => 3\n            [1] => 4\n        )\n\n)\n"
    );
}

/// Verifies `print_r` renders a deeply nested structure with the correct
/// cumulative indentation at each level.
#[test]
fn test_print_r_deep_nesting() {
    let out = compile_and_run(r#"<?php print_r([1 => ["a" => ["z" => 9]]]);"#);
    assert_eq!(
        out,
        "Array\n(\n    [1] => Array\n        (\n            [a] => Array\n                (\n                    [z] => 9\n                )\n\n        )\n\n)\n"
    );
}

/// Verifies `print_r` renders a single boxed Mixed scalar (an element read from
/// a heterogeneous array) with no type wrapper, matching PHP.
#[test]
fn test_print_r_mixed_scalar_element() {
    let out = compile_and_run(
        r#"<?php $a = [1, "two", 3.5, true, null]; print_r($a[1]); echo "|"; print_r($a[3]);"#,
    );
    assert_eq!(out, "two|1");
}

/// Verifies `print_r($value, true)` returns the rendered int as a string instead
/// of writing to stdout.
#[test]
fn test_print_r_return_int() {
    let out = compile_and_run(r#"<?php echo print_r(42, true);"#);
    assert_eq!(out, "42");
}

/// Verifies `print_r($value, true)` returns the rendered string unchanged.
#[test]
fn test_print_r_return_string() {
    let out = compile_and_run(r#"<?php echo print_r("hello", true);"#);
    assert_eq!(out, "hello");
}

/// Verifies `print_r($value, true)` returns `1` for boolean true (no type label).
#[test]
fn test_print_r_return_bool_true() {
    let out = compile_and_run(r#"<?php echo print_r(true, true);"#);
    assert_eq!(out, "1");
}

/// Verifies `print_r($value, true)` returns the empty string for boolean false.
#[test]
fn test_print_r_return_bool_false() {
    let out = compile_and_run(r#"<?php $s = print_r(false, true); echo strlen($s);"#);
    assert_eq!(out, "0");
}

/// Verifies `print_r($value, true)` returns the full array body as a string.
#[test]
fn test_print_r_return_array() {
    let out = compile_and_run(r#"<?php echo print_r([1, 2, 3], true);"#);
    assert_eq!(out, "Array\n(\n    [0] => 1\n    [1] => 2\n    [2] => 3\n)\n");
}

/// Verifies `print_r($value, true)` returns the associative-array body as a string.
#[test]
fn test_print_r_return_assoc_array() {
    let out = compile_and_run(r#"<?php echo print_r(["a" => 1], true);"#);
    assert_eq!(out, "Array\n(\n    [a] => 1\n)\n");
}

/// Verifies `print_r($value, true)` captures nested array output recursively.
#[test]
fn test_print_r_return_nested_array() {
    let out = compile_and_run(r#"<?php echo print_r([[1, 2], [3, 4]], true);"#);
    assert_eq!(
        out,
        "Array\n(\n    [0] => Array\n        (\n            [0] => 1\n            [1] => 2\n        )\n\n    [1] => Array\n        (\n            [0] => 3\n            [1] => 4\n        )\n\n)\n"
    );
}

/// Verifies `print_r($value)` without `$return` still writes to stdout (backward compat).
#[test]
fn test_print_r_no_return_still_echoes() {
    let out = compile_and_run(r#"<?php $r = print_r(42); echo "|$r";"#);
    assert_eq!(out, "42|1");
}

/// Verifies `print_r($value, false)` keeps echo mode (writes to stdout, returns true).
#[test]
fn test_print_r_return_false_echoes() {
    let out = compile_and_run(r#"<?php $r = print_r(42, false); echo "|$r";"#);
    assert_eq!(out, "42|1");
}

/// Verifies the string returned by `print_r($value, true)` has the correct length.
#[test]
fn test_print_r_return_length() {
    let out = compile_and_run(r#"<?php echo strlen(print_r(["a" => 1], true));"#);
    assert_eq!(out, "23");
}

/// Verifies `var_dump` formats each argument independently with correct type tags and a trailing newline per call, in source order.
#[test]
fn test_var_dump_multiple() {
    let out = compile_and_run(
        r#"<?php
var_dump(1);
var_dump("hi");
var_dump(true);
"#,
    );
    assert_eq!(out, "int(1)\nstring(2) \"hi\"\nbool(true)\n");
}

/// Regression: `var_dump` of a heterogeneous (Mixed) indexed array must emit one typed line per
/// element, not an empty body. The Mixed-array walker previously masked the value_type stamp with
/// `0xff`, leaving the COW bit set so the `== Mixed` check failed and skipped the whole body.
#[test]
fn test_var_dump_mixed_indexed_array() {
    let out = compile_and_run(
        r#"<?php
var_dump([1, "x", 2.5]);
"#,
    );
    assert_eq!(
        out,
        "array(3) {\n  [0]=>\n  int(1)\n  [1]=>\n  string(1) \"x\"\n  [2]=>\n  float(2.5)\n}\n"
    );
}

/// `var_export` renders scalars the way PHP does: bare integers, `'…'`-quoted strings with
/// `\\`/`\'` escaping, `true`/`false`, `NULL`, and an integer-valued float gaining a `.0`.
#[test]
fn test_var_export_scalars() {
    let out = compile_and_run(
        r#"<?php
var_export(42); echo "|";
var_export(-5); echo "|";
var_export(3.5); echo "|";
var_export(1.0); echo "|";
var_export(true); echo "|";
var_export(false); echo "|";
var_export(null); echo "|";
var_export("it's a \\test");
"#,
    );
    assert_eq!(out, r"42|-5|3.5|1.0|true|false|NULL|'it\'s a \\test'");
}

/// `var_export` renders floats with PHP's `serialize_precision = -1` semantics: the
/// shortest decimal that round-trips (so `1/3` keeps 16 significant digits, not 14),
/// scientific notation with a `.0` mantissa and minimal exponent (`1.0E+17`, `1.0E-6`),
/// and `-0.0` preserved. This is distinct from the default `(string)`/`echo` precision.
#[test]
fn test_var_export_float_precision() {
    let out = compile_and_run(
        r#"<?php
var_export(0.1); echo "|";
var_export(1.0 / 3.0); echo "|";
var_export(1.5e300); echo "|";
var_export(1e17); echo "|";
var_export(1e16); echo "|";
var_export(0.000001); echo "|";
var_export(1234567890123456.0); echo "|";
var_export(-0.0); echo "|";
var_export(-123.456);
"#,
    );
    assert_eq!(
        out,
        "0.1|0.3333333333333333|1.5E+300|1.0E+17|10000000000000000.0|1.0E-6|1234567890123456.0|-0.0|-123.456"
    );
}

/// `var_export` renders arrays in PHP's parsable `array ( … )` layout: 2-space-per-level indent,
/// `key => value,` entries, integer keys bare and string keys quoted, and a nested array placed on
/// its own line. Covers the empty array and a nested associative array.
#[test]
fn test_var_export_arrays() {
    let out = compile_and_run(
        "<?php var_export([]); echo \"\\n---\\n\"; \
         var_export([1, 'two', ['a' => 1, 'b' => [10, 20]]]);",
    );
    assert_eq!(
        out,
        "array (\n)\n---\narray (\n  0 => 1,\n  1 => 'two',\n  2 => \n  array (\n    'a' => 1,\n    'b' => \n    array (\n      0 => 10,\n      1 => 20,\n    ),\n  ),\n)"
    );
}

/// `var_export($value, true)` returns the rendered string instead of printing it, and
/// `function_exists('var_export')` sees the injected function. The unused-on-echo return is null.
#[test]
fn test_var_export_return_mode_and_function_exists() {
    let out = compile_and_run(
        r#"<?php
echo function_exists("var_export") ? "Y" : "N";
echo "|";
$s = var_export([1, 2], true);
echo $s;
"#,
    );
    assert_eq!(out, "Y|array (\n  0 => 1,\n  1 => 2,\n)");
}

/// A user-defined `var_export` wins over the injected prelude (the prelude must detect the
/// declaration and skip injection, so there is no redeclaration error).
#[test]
fn test_var_export_user_definition_wins() {
    let out = compile_and_run(
        r#"<?php
function var_export($value, $return = false) { return "custom"; }
echo var_export(123, true);
"#,
    );
    assert_eq!(out, "custom");
}

// --- File I/O: CSV, timestamps, directory listing, temp files, seek/rewind/eof ---
