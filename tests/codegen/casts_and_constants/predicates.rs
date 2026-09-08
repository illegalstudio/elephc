//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of casts, constants, and introspection predicates, including boolval true, boolval false, and is boolean true.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Compiles `boolval(42)` and verifies it outputs "1" (non-zero truthy value).
#[test]
fn test_boolval_true() {
    let out = compile_and_run("<?php echo boolval(42);");
    assert_eq!(out, "1");
}

/// Compiles `boolval(0)` and verifies it outputs "" (zero is falsy).
#[test]
fn test_boolval_false() {
    let out = compile_and_run("<?php echo boolval(0);");
    assert_eq!(out, "");
}

/// Compiles `is_bool(true)` and verifies it outputs "1" (true is boolean).
#[test]
fn test_is_bool_true() {
    let out = compile_and_run("<?php echo is_bool(true);");
    assert_eq!(out, "1");
}

/// Compiles `is_bool(1)` and verifies it outputs "" (int 1 is not a boolean).
#[test]
fn test_is_bool_false_for_int() {
    let out = compile_and_run("<?php echo is_bool(1);");
    assert_eq!(out, "");
}

/// Compiles `is_string("hello")` and verifies it outputs "1" (string literal).
#[test]
fn test_is_string_true() {
    let out = compile_and_run("<?php echo is_string(\"hello\");");
    assert_eq!(out, "1");
}

/// Compiles `is_string(42)` and verifies it outputs "" (int is not a string).
#[test]
fn test_is_string_false() {
    let out = compile_and_run("<?php echo is_string(42);");
    assert_eq!(out, "");
}

/// Compiles `is_numeric(42)` and verifies it outputs "1" (integer is numeric).
#[test]
fn test_is_numeric_int() {
    let out = compile_and_run("<?php echo is_numeric(42);");
    assert_eq!(out, "1");
}

/// Compiles `is_numeric(3.14)` and verifies it outputs "1" (float is numeric).
#[test]
fn test_is_numeric_float() {
    let out = compile_and_run("<?php echo is_numeric(3.14);");
    assert_eq!(out, "1");
}

/// Compiles `is_numeric("hello")` and verifies it outputs "" (non-numeric string).
#[test]
fn test_is_numeric_string() {
    let out = compile_and_run("<?php echo is_numeric(\"hello\");");
    assert_eq!(out, "");
}

/// Verifies PHP scalar predicate aliases and container/object predicates compile as builtin calls.
#[test]
fn test_type_predicate_aliases_array_and_object() {
    let out = compile_and_run(
        r#"<?php
class Box {}
$object = new Box();
echo is_integer(1) ? "i" : "_";
echo is_long(1) ? "l" : "_";
echo is_double(1.5) ? "d" : "_";
echo is_real(1.5) ? "r" : "_";
echo is_array([1]) ? "a" : "_";
echo is_array(["x" => 1]) ? "h" : "_";
echo is_object($object) ? "o" : "_";
echo is_object([1]) ? "bad" : "_";
"#,
    );
    assert_eq!(out, "ildraho_");
}

/// Verifies `is_array()` inspects boxed Mixed JSON payload tags for array values.
#[test]
fn test_is_array_recognizes_arrays_inside_mixed_array() {
    let out = compile_and_run(
        r#"<?php
$values = [json_decode("[1]"), json_decode("{\"a\":2}", true), 3];
foreach ($values as $value) {
    echo is_array($value) ? "a" : "_";
}
"#,
    );
    assert_eq!(out, "aa_");
}

/// Verifies `strval()` works directly, as a first-class callable, and through string callable dispatch.
#[test]
fn test_strval_direct_first_class_and_callable_dispatch() {
    let out = compile_and_run(
        r#"<?php
echo strval(12);
echo ":";
$strval = strval(...);
echo $strval(true);
echo ":";
echo call_user_func("strval", 7);
"#,
    );
    assert_eq!(out, "12:1:7");
}

/// Verifies `function_exists()` recognizes PHP predicate aliases and `strval()` case-insensitively.
#[test]
fn test_function_exists_recognizes_scalar_alias_builtins() {
    let out = compile_and_run(
        r#"<?php
echo function_exists("is_integer") ? "1" : "0";
echo function_exists("IS_LONG") ? "1" : "0";
echo function_exists("is_double") ? "1" : "0";
echo function_exists("IS_REAL") ? "1" : "0";
echo function_exists("is_object") ? "1" : "0";
echo function_exists("strval") ? "1" : "0";
"#,
    );
    assert_eq!(out, "111111");
}

// --- Mixed-cell-aware predicates ---
//
// `is_string()` / `is_int()` / `is_bool()` peek at the runtime tag of a
// boxed Mixed value via `__rt_mixed_unbox`. Driven by the
// `class_attribute_args()` use case where attribute literals are stored
// as boxed mixed cells, but applies to any Mixed/Union runtime value.

/// Verifies `is_string()` correctly identifies a string inside a boxed Mixed cell
/// from a class attribute with heterogeneous arguments: "hello", 42, true, null.
/// Expects "s___" (first arg is string, rest are not).
#[test]
fn test_is_string_recognizes_string_inside_mixed_array() {
    let out = compile_and_run(
        r#"<?php
#[Tagged("hello", 42, true, null)]
class C {}
$args = class_attribute_args('C', 'Tagged');
foreach ($args as $arg) {
    echo is_string($arg) ? "s" : "_";
}
"#,
    );
    assert_eq!(out, "s___");
}

/// Verifies `is_int()` correctly identifies an int inside a boxed Mixed cell
/// from a class attribute with heterogeneous arguments: "hello", 42, true, null.
/// Expects "_i__" (second arg is int, rest are not).
#[test]
fn test_is_int_recognizes_int_inside_mixed_array() {
    let out = compile_and_run(
        r#"<?php
#[Tagged("hello", 42, true, null)]
class C {}
$args = class_attribute_args('C', 'Tagged');
foreach ($args as $arg) {
    echo is_int($arg) ? "i" : "_";
}
"#,
    );
    assert_eq!(out, "_i__");
}

/// Verifies `is_bool()` correctly identifies a bool inside a boxed Mixed cell
/// from a class attribute with heterogeneous arguments: "hello", 42, true, null.
/// Expects "__b_" (third arg is bool, rest are not).
#[test]
fn test_is_bool_recognizes_bool_inside_mixed_array() {
    let out = compile_and_run(
        r#"<?php
#[Tagged("hello", 42, true, null)]
class C {}
$args = class_attribute_args('C', 'Tagged');
foreach ($args as $arg) {
    echo is_bool($arg) ? "b" : "_";
}
"#,
    );
    assert_eq!(out, "__b_");
}

/// Verifies `is_array` on statically-known arrays/hashes and non-arrays, matching PHP's
/// `bool` result type (not int). Indexed and associative literals are both arrays.
#[test]
fn test_is_array_static() {
    let out = compile_and_run(
        r#"<?php
var_dump(is_array([1, 2, 3]));
var_dump(is_array(["a" => 1]));
var_dump(is_array("nope"));
var_dump(is_array(5));
"#,
    );
    assert_eq!(out, "bool(true)\nbool(true)\nbool(false)\nbool(false)\n");
}

/// Verifies `is_object` is true only for object values and false for scalars, returning `bool`.
#[test]
fn test_is_object_static() {
    let out = compile_and_run(
        r#"<?php
class Box { public int $v = 1; }
var_dump(is_object(new Box()));
var_dump(is_object("nope"));
var_dump(is_object(42));
"#,
    );
    assert_eq!(out, "bool(true)\nbool(false)\nbool(false)\n");
}

/// Verifies a statically Callable closure remains an object for is_object().
#[test]
fn test_is_object_static_callable_closure() {
    let out = compile_and_run(
        r#"<?php
$closure = fn (): int => 1;
echo is_object($closure) ? "object" : "not-object";
"#,
    );
    assert_eq!(out, "object");
}

/// Verifies `is_scalar` is true for int/float/string/bool and false for null/array/object,
/// matching PHP's classification (resources and null are not scalars).
#[test]
fn test_is_scalar_static() {
    let out = compile_and_run(
        r#"<?php
class Box { public int $v = 1; }
var_dump(is_scalar(5));
var_dump(is_scalar(3.5));
var_dump(is_scalar("hi"));
var_dump(is_scalar(true));
var_dump(is_scalar(null));
var_dump(is_scalar([1]));
var_dump(is_scalar(new Box()));
"#,
    );
    assert_eq!(
        out,
        "bool(true)\nbool(true)\nbool(true)\nbool(true)\nbool(false)\nbool(false)\nbool(false)\n"
    );
}

/// Verifies `is_array`/`is_object`/`is_scalar` dispatch on the runtime tag of a boxed `Mixed`
/// value (read from a heterogeneous associative array), not the static union member.
#[test]
fn test_is_kind_predicates_on_mixed() {
    let out = compile_and_run(
        r#"<?php
$het = ["arr" => [1, 2], "num" => 7, "str" => "x", "flo" => 2.5];
var_dump(is_array($het["arr"]));
var_dump(is_array($het["num"]));
var_dump(is_scalar($het["num"]));
var_dump(is_scalar($het["str"]));
var_dump(is_scalar($het["flo"]));
var_dump(is_scalar($het["arr"]));
var_dump(is_object($het["num"]));
"#,
    );
    assert_eq!(
        out,
        "bool(true)\nbool(false)\nbool(true)\nbool(true)\nbool(true)\nbool(false)\nbool(false)\n"
    );
}

/// Verifies the new kind predicates honor PHP case-insensitive and namespace-qualified
/// call forms, and that `function_exists` recognizes them through the catalog.
#[test]
fn test_is_kind_predicates_case_and_namespace() {
    let out = compile_and_run(
        r#"<?php
echo IS_ARRAY([1]) ? "A" : "_";
echo \is_object(new \stdClass()) ? "O" : "_";
echo function_exists("is_scalar") ? "S" : "_";
"#,
    );
    assert_eq!(out, "AOS");
}

// --- Exponentiation operator ** ---

/// Verifies RUNTIME `is_numeric()` implements PHP 8's numeric-string grammar, not the old
/// digits-and-one-dot scan: leading AND trailing whitespace (` `, `\t`, `\n`, `\v`, `\f`,
/// `\r`) are allowed, a leading `+` is allowed, `.5` / `5.` / `1.2E+3` / `+.5e-2` are
/// numeric, and hexadecimal, underscore separators, a bare `"1e"`, an empty string and a
/// lone `"."`/`"-"` are not. The strings come out of a `foreach` so the runtime scanner
/// (shared with the `(float)`/`(int)` casts) is what answers.
#[test]
fn test_is_numeric_follows_php_numeric_string_grammar() {
    let out = compile_and_run(
        r#"<?php
$strings = [" 42 ", "42\t", "\n42", "\r\n 3.5 \v\f", "1e3", ".5", "5.", "+.5e-2", "0x1A", "1_000", "", " ", "1e", "1e+", ".", "-", "1.2E+3", "-0", "00012", "12abc"];
foreach ($strings as $s) { echo is_numeric($s) ? "T" : "F"; }
"#,
    );
    assert_eq!(out, "TTTTTTTTFFFFFFFFTTTF");
}

/// Verifies PHP's int-preserving `**` at RUNTIME: `int ** int` stays an `int` while the
/// value fits (`2 ** 3` is `int(8)`, `10 ** 18` is `int(1000000000000000000)`), promotes
/// to a `float` at the exact multiplication that overflows `i64` (`2 ** 63`), and is
/// always a `float` for a negative exponent. `$argc` arithmetic keeps every operand off
/// the constant-folding path, so this pins the `Op::ICheckedPow` lowering rather than the
/// AST folder.
#[test]
fn test_runtime_int_power_keeps_int_like_php() {
    let out = compile_and_run(
        r#"<?php
$b = $argc + 1;
var_dump($b ** ($argc + 2));
var_dump($b ** ($argc - 1));
var_dump(($argc - 1) ** $b);
var_dump($b ** ($argc + 61));
var_dump($b ** ($argc + 62));
var_dump(($argc * 10) ** ($argc + 17));
var_dump(($argc * 10) ** ($argc + 18));
var_dump($b ** (0 - $argc));
var_dump((0 - $b) ** ($argc + 2));
echo $b ** ($argc + 2), "|", $b ** ($argc + 62), "|";
"#,
    );
    assert_eq!(
        out,
        "int(8)\nint(1)\nint(0)\nint(4611686018427387904)\nfloat(9.223372036854776E+18)\nint(1000000000000000000)\nfloat(1.0E+19)\nfloat(0.5)\nint(-8)\n8|9.2233720368548E+18|"
    );
}

/// Verifies the runtime `**` lowering agrees with the constant folder: the same
/// base/exponent pairs written as literals and reached through `$argc` must print the
/// same `int(...)`/`float(...)` lines, so a literal and a runtime value never disagree.
#[test]
fn test_int_power_literal_and_runtime_paths_agree() {
    let out = compile_and_run(
        r#"<?php
$two = $argc + 1;
$ten = $argc * 10;
var_dump(2 ** 3, 2 ** 62, 2 ** 63, 10 ** 18, 10 ** 19, 2 ** -1, (-2) ** 3, 0 ** 0);
var_dump($two ** ($argc + 2), $two ** ($argc + 61), $two ** ($argc + 62), $ten ** ($argc + 17), $ten ** ($argc + 18), $two ** (0 - $argc), (0 - $two) ** ($argc + 2), ($argc - 1) ** ($argc - 1));
"#,
    );
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 16, "expected 8 folded and 8 runtime lines: {out:?}");
    assert_eq!(&lines[..8], &lines[8..], "folded and runtime `**` disagree");
    assert_eq!(lines[0], "int(8)");
    assert_eq!(lines[2], "float(9.223372036854776E+18)");
}
