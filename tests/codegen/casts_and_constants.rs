use crate::support::*;

// --- Type casting ---

#[test]
fn test_cast_int_from_float() {
    let out = compile_and_run("<?php echo (int)3.7;");
    assert_eq!(out, "3");
}

#[test]
fn test_cast_int_from_string() {
    let out = compile_and_run("<?php echo (int)\"42\";");
    assert_eq!(out, "42");
}

#[test]
fn test_cast_int_from_bool() {
    let out = compile_and_run("<?php echo (int)true;");
    assert_eq!(out, "1");
}

#[test]
fn test_cast_float_from_int() {
    let out = compile_and_run("<?php echo (float)42;");
    assert_eq!(out, "42");
}

#[test]
fn test_cast_float_from_string() {
    let out = compile_and_run("<?php echo (float)'3.14';");
    assert_eq!(out, "3.14");
}

#[test]
fn test_cast_float_from_string_integer() {
    let out = compile_and_run("<?php echo (float)'42';");
    assert_eq!(out, "42");
}

#[test]
fn test_cast_float_from_string_non_numeric() {
    let out = compile_and_run("<?php echo (float)'abc';");
    assert_eq!(out, "0");
}

#[test]
fn test_cast_string_from_int() {
    let out = compile_and_run("<?php echo (string)42;");
    assert_eq!(out, "42");
}

#[test]
fn test_cast_string_from_float() {
    let out = compile_and_run("<?php echo (string)3.14;");
    assert_eq!(out, "3.14");
}

#[test]
fn test_cast_string_from_bool_true() {
    let out = compile_and_run("<?php echo (string)true;");
    assert_eq!(out, "1");
}

#[test]
fn test_cast_string_from_bool_false() {
    let out = compile_and_run("<?php echo (string)false;");
    assert_eq!(out, "");
}

#[test]
fn test_cast_bool_from_int_zero() {
    let out = compile_and_run("<?php echo (bool)0;");
    assert_eq!(out, "");
}

#[test]
fn test_cast_bool_from_int_nonzero() {
    let out = compile_and_run("<?php echo (bool)42;");
    assert_eq!(out, "1");
}

#[test]
fn test_cast_bool_from_string_empty() {
    let out = compile_and_run("<?php echo (bool)\"\";");
    assert_eq!(out, "");
}

#[test]
fn test_cast_bool_from_string_nonempty() {
    let out = compile_and_run("<?php echo (bool)\"hello\";");
    assert_eq!(out, "1");
}

#[test]
fn test_cast_mixed_unboxes_payload() {
    let out = compile_and_run(
        r#"<?php
$map = [
    "int" => 42,
    "float" => 3.75,
    "true" => true,
    "false" => false,
    "null" => null,
    "text" => "27",
];
echo (int)$map["float"];
echo "|";
echo (int)$map["text"];
echo "|";
echo (bool)$map["int"] ? "1" : "0";
echo (bool)$map["false"] ? "1" : "0";
echo "|";
echo (string)$map["true"];
echo "|";
echo (string)$map["null"];
echo "|";
echo (string)$map["int"];
"#,
    );
    assert_eq!(out, "3|27|10|1||42");
}

#[test]
fn test_cast_integer_alias() {
    let out = compile_and_run("<?php echo (integer)3.7;");
    assert_eq!(out, "3");
}

#[test]
fn test_cast_double_alias() {
    let out = compile_and_run("<?php echo (double)42;");
    assert_eq!(out, "42");
}

#[test]
fn test_cast_boolean_alias() {
    let out = compile_and_run("<?php echo (boolean)1;");
    assert_eq!(out, "1");
}

// --- gettype ---

#[test]
fn test_gettype_int() {
    let out = compile_and_run("<?php echo gettype(42);");
    assert_eq!(out, "integer");
}

#[test]
fn test_gettype_float() {
    let out = compile_and_run("<?php echo gettype(3.14);");
    assert_eq!(out, "double");
}

#[test]
fn test_gettype_string() {
    let out = compile_and_run("<?php echo gettype(\"hi\");");
    assert_eq!(out, "string");
}

#[test]
fn test_gettype_bool() {
    let out = compile_and_run("<?php echo gettype(true);");
    assert_eq!(out, "boolean");
}

#[test]
fn test_gettype_null() {
    let out = compile_and_run("<?php echo gettype(null);");
    assert_eq!(out, "NULL");
}

#[test]
fn test_gettype_mixed_returns_concrete_payload_type() {
    let out = compile_and_run(
        r#"<?php
$map = [
    "i" => 42,
    "s" => "hi",
    "n" => null,
    "a" => [1, 2],
    "b" => true,
];
echo gettype($map["i"]);
echo "|";
echo gettype($map["s"]);
echo "|";
echo gettype($map["n"]);
echo "|";
echo gettype($map["a"]);
echo "|";
echo gettype($map["b"]);
"#,
    );
    assert_eq!(out, "integer|string|NULL|array|boolean");
}

// --- empty ---

#[test]
fn test_empty_zero() {
    let out = compile_and_run("<?php echo empty(0);");
    assert_eq!(out, "1");
}

#[test]
fn test_empty_nonzero() {
    let out = compile_and_run("<?php echo empty(42);");
    assert_eq!(out, "");
}

#[test]
fn test_empty_empty_string() {
    let out = compile_and_run("<?php echo empty(\"\");");
    assert_eq!(out, "1");
}

#[test]
fn test_empty_nonempty_string() {
    let out = compile_and_run("<?php echo empty(\"hi\");");
    assert_eq!(out, "");
}

#[test]
fn test_empty_null() {
    let out = compile_and_run("<?php echo empty(null);");
    assert_eq!(out, "1");
}

#[test]
fn test_empty_false() {
    let out = compile_and_run("<?php echo empty(false);");
    assert_eq!(out, "1");
}

#[test]
fn test_empty_true() {
    let out = compile_and_run("<?php echo empty(true);");
    assert_eq!(out, "");
}

#[test]
fn test_empty_mixed_uses_boxed_payload_semantics() {
    let out = compile_and_run(
        r#"<?php
$map = [
    "zero" => 0,
    "blank" => "",
    "null" => null,
    "arr" => [],
    "one" => 1,
    "text" => "hi",
];
echo empty($map["zero"]) ? "1" : "0";
echo empty($map["blank"]) ? "1" : "0";
echo empty($map["null"]) ? "1" : "0";
echo empty($map["arr"]) ? "1" : "0";
echo empty($map["one"]) ? "1" : "0";
echo empty($map["text"]) ? "1" : "0";
"#,
    );
    assert_eq!(out, "111100");
}

// --- unset ---

#[test]
fn test_unset_variable() {
    let out = compile_and_run(
        r#"<?php
$x = 42;
unset($x);
echo is_null($x);
"#,
    );
    assert_eq!(out, "1");
}

// --- settype ---

#[test]
fn test_settype_to_string() {
    let out = compile_and_run(
        r#"<?php
$x = 42;
settype($x, "string");
echo $x;
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_settype_to_int() {
    let out = compile_and_run(
        r#"<?php
$x = 3.7;
settype($x, "integer");
echo $x;
"#,
    );
    assert_eq!(out, "3");
}

// --- Missing type function tests ---

#[test]
fn test_boolval_true() {
    let out = compile_and_run("<?php echo boolval(42);");
    assert_eq!(out, "1");
}

#[test]
fn test_boolval_false() {
    let out = compile_and_run("<?php echo boolval(0);");
    assert_eq!(out, "");
}

#[test]
fn test_is_bool_true() {
    let out = compile_and_run("<?php echo is_bool(true);");
    assert_eq!(out, "1");
}

#[test]
fn test_is_bool_false_for_int() {
    let out = compile_and_run("<?php echo is_bool(1);");
    assert_eq!(out, "");
}

#[test]
fn test_is_string_true() {
    let out = compile_and_run("<?php echo is_string(\"hello\");");
    assert_eq!(out, "1");
}

#[test]
fn test_is_string_false() {
    let out = compile_and_run("<?php echo is_string(42);");
    assert_eq!(out, "");
}

#[test]
fn test_is_numeric_int() {
    let out = compile_and_run("<?php echo is_numeric(42);");
    assert_eq!(out, "1");
}

#[test]
fn test_is_numeric_float() {
    let out = compile_and_run("<?php echo is_numeric(3.14);");
    assert_eq!(out, "1");
}

#[test]
fn test_is_numeric_string() {
    let out = compile_and_run("<?php echo is_numeric(\"hello\");");
    assert_eq!(out, "");
}

// --- Exponentiation operator ** ---

#[test]
fn test_pow_operator() {
    let out = compile_and_run("<?php echo 2 ** 10;");
    assert_eq!(out, "1024");
}

#[test]
fn test_pow_operator_float() {
    let out = compile_and_run("<?php echo 2.0 ** 0.5;");
    assert_eq!(out, "1.4142135623731");
}

#[test]
fn test_pow_right_associative() {
    // 2 ** 3 ** 2 = 2 ** 9 = 512
    let out = compile_and_run("<?php echo 2 ** 3 ** 2;");
    assert_eq!(out, "512");
}

#[test]
fn test_pow_higher_than_unary() {
    // -2 ** 2 = -(2**2) = -4
    let out = compile_and_run("<?php echo -2 ** 2;");
    assert_eq!(out, "-4");
}

#[test]
fn test_pow_higher_than_multiply() {
    // 3 * 2 ** 3 = 3 * 8 = 24
    let out = compile_and_run("<?php echo 3 * 2 ** 3;");
    assert_eq!(out, "24");
}

// --- fmod, fdiv ---

#[test]
fn test_fmod() {
    let out = compile_and_run("<?php echo fmod(10.5, 3.2);");
    assert_eq!(out, "0.9");
}

#[test]
fn test_fdiv() {
    let out = compile_and_run("<?php echo fdiv(10, 3);");
    assert_eq!(out, "3.3333333333333");
}

#[test]
fn test_fdiv_by_zero() {
    let out = compile_and_run("<?php echo fdiv(1, 0);");
    assert_eq!(out, "INF");
}

// --- rand, mt_rand, random_int ---

#[test]
fn test_rand_range() {
    // rand(1, 1) always returns 1
    let out = compile_and_run("<?php echo rand(1, 1);");
    assert_eq!(out, "1");
}

#[test]
fn test_mt_rand_range() {
    let out = compile_and_run("<?php echo mt_rand(5, 5);");
    assert_eq!(out, "5");
}

#[test]
fn test_random_int_range() {
    let out = compile_and_run("<?php echo random_int(42, 42);");
    assert_eq!(out, "42");
}

#[test]
fn test_rand_no_args() {
    // Just verify it doesn't crash and returns a non-negative number
    let out = compile_and_run("<?php $r = rand(); echo ($r >= 0 ? \"ok\" : \"bad\");");
    assert_eq!(out, "ok");
}

// --- number_format ---

#[test]
fn test_number_format_no_decimals() {
    let out = compile_and_run("<?php echo number_format(1234567);");
    assert_eq!(out, "1,234,567");
}

#[test]
fn test_number_format_with_decimals() {
    let out = compile_and_run("<?php echo number_format(1234.5678, 2);");
    assert_eq!(out, "1,234.57");
}

#[test]
fn test_number_format_small() {
    let out = compile_and_run("<?php echo number_format(42, 2);");
    assert_eq!(out, "42.00");
}

#[test]
fn test_number_format_negative() {
    let out = compile_and_run("<?php echo number_format(-1234.5, 1);");
    assert_eq!(out, "-1,234.5");
}

#[test]
fn test_number_format_custom_separators() {
    // European style: comma for decimal, dot for thousands
    let out = compile_and_run(r#"<?php echo number_format(1234567.89, 2, ",", ".");"#);
    assert_eq!(out, "1.234.567,89");
}

#[test]
fn test_number_format_no_thousands() {
    // Empty string = no thousands separator
    let out = compile_and_run(r#"<?php echo number_format(1234567.89, 2, ".", "");"#);
    assert_eq!(out, "1234567.89");
}

#[test]
fn test_number_format_space_thousands() {
    let out = compile_and_run(r#"<?php echo number_format(1234567, 0, ".", " ");"#);
    assert_eq!(out, "1 234 567");
}

// --- Constants ---

#[test]
fn test_php_int_max() {
    let out = compile_and_run("<?php echo PHP_INT_MAX;");
    assert_eq!(out, "9223372036854775807");
}

#[test]
fn test_php_int_min() {
    let out = compile_and_run("<?php echo PHP_INT_MIN;");
    assert_eq!(out, "-9223372036854775808");
}

#[test]
fn test_m_pi() {
    let out = compile_and_run("<?php echo M_PI;");
    assert_eq!(out, "3.1415926535898");
}

#[test]
fn test_php_float_max() {
    // Just verify it compiles and echoes without crash
    let out = compile_and_run("<?php echo is_float(PHP_FLOAT_MAX);");
    assert_eq!(out, "1");
}
