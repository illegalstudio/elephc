//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of operators, including addition, subtraction, and multiplication.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use crate::support::*;

// --- Phase 3: Arithmetic ---

#[test]
// Verifies integer addition with literal operands: 10 + 32 = 42.
fn test_addition() {
    let out = compile_and_run("<?php echo 10 + 32;");
    assert_eq!(out, "42");
}

#[test]
// Verifies integer subtraction with literal operands: 100 - 58 = 42.
fn test_subtraction() {
    let out = compile_and_run("<?php echo 100 - 58;");
    assert_eq!(out, "42");
}

#[test]
// Verifies integer multiplication with literal operands: 6 * 7 = 42.
fn test_multiplication() {
    let out = compile_and_run("<?php echo 6 * 7;");
    assert_eq!(out, "42");
}

#[test]
// Verifies integer division with literal operands: 84 / 2 = 42.
fn test_division() {
    let out = compile_and_run("<?php echo 84 / 2;");
    assert_eq!(out, "42");
}

#[test]
// Verifies arithmetic with variables: loads two integers from memory and adds them.
fn test_arithmetic_with_variables() {
    let out = compile_and_run("<?php $a = 10; $b = 32; echo $a + $b;");
    assert_eq!(out, "42");
}

#[test]
// Verifies operator precedence: multiplication binds tighter than addition, so 2 + 3 * 4 = 14.
fn test_operator_precedence() {
    let out = compile_and_run("<?php echo 2 + 3 * 4;");
    assert_eq!(out, "14");
}

#[test]
// Verifies parenthesized precedence: (2 + 3) * 4 = 20, confirming parentheses override default precedence.
fn test_parenthesized_arithmetic() {
    let out = compile_and_run("<?php echo (2 + 3) * 4;");
    assert_eq!(out, "20");
}

#[test]
// Verifies a complex expression mixing parentheses, addition, multiplication, and subtraction: (10 + 5) * 2 - 7 = 23.
fn test_complex_expression() {
    let out = compile_and_run("<?php echo (10 + 5) * 2 - 7;");
    assert_eq!(out, "23");
}

#[test]
// Verifies assignment of an arithmetic expression result to a variable, then echo: $a + $b → $c → output.
fn test_arithmetic_assign_and_echo() {
    let out = compile_and_run("<?php $a = 10; $b = 32; $c = $a + $b; echo $c;");
    assert_eq!(out, "42");
}

#[test]
// Verifies subtraction producing a negative result: 3 - 10 = -7, confirming signed integer handling.
fn test_subtraction_negative_result() {
    let out = compile_and_run("<?php echo 3 - 10;");
    assert_eq!(out, "-7");
}

#[test]
// Verifies left-associative chaining of addition: 1 + 2 + 3 + 4 = 10.
fn test_nested_arithmetic() {
    let out = compile_and_run("<?php echo 1 + 2 + 3 + 4;");
    assert_eq!(out, "10");
}

#[test]
// Verifies that adding 1 to the maximum 64-bit integer constant overflows to float at compile time.
fn test_constant_int_add_overflow_promotes_to_float() {
    let out = compile_and_run("<?php echo gettype(9223372036854775807 + 1);");
    assert_eq!(out, "double");
}

#[test]
// Verifies that squaring a large integer constant overflows to float at compile time.
fn test_constant_int_multiply_overflow_promotes_to_float() {
    let out = compile_and_run("<?php echo gettype(3037000500 * 3037000500);");
    assert_eq!(out, "double");
}

#[test]
// Verifies that adding 1 to the maximum 64-bit integer at runtime overflows to float.
fn test_runtime_int_add_overflow_promotes_to_float() {
    let out = compile_and_run("<?php $a = 9223372036854775807; $b = 1; echo gettype($a + $b);");
    assert_eq!(out, "double");
}

#[test]
// Verifies that squaring a large integer at runtime overflows to float.
fn test_runtime_int_multiply_overflow_promotes_to_float() {
    let out = compile_and_run("<?php $a = 3037000500; $b = 3037000500; echo gettype($a * $b);");
    assert_eq!(out, "double");
}

#[test]
// Verifies that runtime integer arithmetic without overflow remains integer, not float.
fn test_runtime_int_arithmetic_without_overflow_stays_integer() {
    let out = compile_and_run("<?php $a = 40; $b = 2; echo gettype($a + $b) . ':' . ($a + $b);");
    assert_eq!(out, "integer:42");
}

#[test]
// Verifies that a runtime overflow result (float) participates correctly in subsequent arithmetic.
fn test_runtime_overflow_result_participates_in_later_arithmetic() {
    let out = compile_and_run("<?php $a = 9223372036854775807; $b = 1; $c = $a + $b; echo gettype($c + 1);");
    assert_eq!(out, "double");
}

// --- Phase 3: Concatenation ---

#[test]
// Verifies string literal concatenation: "Hello, " . "World!" = "Hello, World!".
fn test_concat_literals() {
    let out = compile_and_run("<?php echo \"Hello, \" . \"World!\";");
    assert_eq!(out, "Hello, World!");
}

#[test]
// Verifies string concatenation with variables: loads two strings from memory and concatenates.
fn test_concat_variables() {
    let out = compile_and_run("<?php $a = \"Hello, \"; $b = \"World!\"; echo $a . $b;");
    assert_eq!(out, "Hello, World!");
}

#[test]
// Verifies left-associative chaining of string concatenation: "a" . "b" . "c" = "abc".
fn test_concat_chain() {
    let out = compile_and_run("<?php echo \"a\" . \"b\" . \"c\";");
    assert_eq!(out, "abc");
}

#[test]
// Verifies concatenation assignment: $msg = "foo" . "bar"; echo $msg; = "foobar".
fn test_concat_assign() {
    let out = compile_and_run("<?php $msg = \"foo\" . \"bar\"; echo $msg;");
    assert_eq!(out, "foobar");
}

#[test]
// Verifies concatenation with embedded newline escape: "hello" . "\n" outputs "hello\n".
fn test_concat_with_newline() {
    let out = compile_and_run("<?php echo \"hello\" . \"\\n\";");
    assert_eq!(out, "hello\n");
}

// --- Phase 3: Mixed-type concatenation ---

#[test]
// Verifies concatenation of string literal and integer literal: "Value: " . 42 = "Value: 42".
fn test_concat_string_and_int() {
    let out = compile_and_run("<?php echo \"Value: \" . 42;");
    assert_eq!(out, "Value: 42");
}

#[test]
// Verifies concatenation of integer literal and string literal: 42 . " is the answer" = "42 is the answer".
fn test_concat_int_and_string() {
    let out = compile_and_run("<?php echo 42 . \" is the answer\";");
    assert_eq!(out, "42 is the answer");
}

#[test]
// Verifies concatenation of two integer literals coerces to string: 1 . 2 = "12".
fn test_concat_int_and_int() {
    let out = compile_and_run("<?php echo 1 . 2;");
    assert_eq!(out, "12");
}

#[test]
// Verifies concatenation of a string literal and a parenthesized expression result: "Result: " . ($a + $b) = "Result: 42".
fn test_concat_expr_result() {
    let out = compile_and_run("<?php $a = 10; $b = 32; echo \"Result: \" . ($a + $b);");
    assert_eq!(out, "Result: 42");
}

#[test]
// Verifies mixed-type concatenation chaining left-to-right: "x=" . 5 . " y=" . 10 = "x=5 y=10".
fn test_concat_chain_mixed() {
    let out = compile_and_run("<?php echo \"x=\" . 5 . \" y=\" . 10;");
    assert_eq!(out, "x=5 y=10");
}

#[test]
// Verifies concatenation with a negative integer: "num: " . -7 = "num: -7".
fn test_concat_negative_int() {
    let out = compile_and_run("<?php echo \"num: \" . -7;");
    assert_eq!(out, "num: -7");
}

// --- Modulo ---

#[test]
// Verifies integer modulo: 10 % 3 = 1.
fn test_modulo() {
    let out = compile_and_run("<?php echo 10 % 3;");
    assert_eq!(out, "1");
}

#[test]
// Verifies modulo with zero remainder: 15 % 5 = 0.
fn test_modulo_zero_remainder() {
    let out = compile_and_run("<?php echo 15 % 5;");
    assert_eq!(out, "0");
}

// --- Comparison operators ---

#[test]
// Verifies loose equality comparison returning true: 1 == 1 outputs "1".
fn test_equal_true() {
    let out = compile_and_run("<?php echo 1 == 1;");
    assert_eq!(out, "1");
}

#[test]
// Verifies loose equality comparison returning false: 1 == 2 outputs empty string (echo false prints nothing in PHP).
fn test_equal_false() {
    let out = compile_and_run("<?php echo 1 == 2;");
    assert_eq!(out, ""); // echo false prints nothing in PHP
}

#[test]
// Verifies loose inequality returning true: 1 != 2 outputs "1".
fn test_not_equal() {
    let out = compile_and_run("<?php echo 1 != 2;");
    assert_eq!(out, "1");
}

// --- Loose comparison across types ---

#[test]
// Verifies loose equality at compile time: empty string equals false, var_dump shows bool(true).
fn test_loose_eq_empty_string_false() {
    let out = compile_and_run("<?php var_dump(\"\" == false);");
    assert_eq!(out, "bool(true)\n");
}

#[test]
// Verifies loose equality at compile time: integer 0 equals false, var_dump shows bool(true).
fn test_loose_eq_zero_false() {
    let out = compile_and_run("<?php var_dump(0 == false);");
    assert_eq!(out, "bool(true)\n");
}

#[test]
// Verifies loose equality at compile time: integer 1 equals true, var_dump shows bool(true).
fn test_loose_eq_one_true() {
    let out = compile_and_run("<?php var_dump(1 == true);");
    assert_eq!(out, "bool(true)\n");
}

#[test]
// Verifies loose equality at compile time: string "0" equals false (string zero is falsy), var_dump shows bool(true).
fn test_loose_eq_string_vs_int() {
    let out = compile_and_run("<?php var_dump(\"0\" == false);");
    assert_eq!(out, "bool(true)\n");
}

#[test]
// Verifies loose inequality at compile time: empty string is not equal to true, var_dump shows bool(true).
fn test_loose_neq_empty_string_true() {
    let out = compile_and_run("<?php var_dump(\"\" != true);");
    assert_eq!(out, "bool(true)\n");
}

#[test]
// Verifies loose equality at compile time: null equals false (null is falsy), var_dump shows bool(true).
fn test_loose_eq_null_false() {
    let out = compile_and_run("<?php var_dump(null == false);");
    assert_eq!(out, "bool(true)\n");
}

#[test]
// Verifies compile-time loose equality of two non-numeric strings compares by byte sequence, not lexicographically.
fn test_constant_loose_eq_non_numeric_strings_compare_by_bytes() {
    let out = compile_and_run("<?php var_dump(\"abc\" == \"def\");");
    assert_eq!(out, "bool(false)\n");
}

#[test]
// Verifies compile-time loose equality of numeric strings ("0" == "00") compares numerically as equal.
fn test_constant_loose_eq_numeric_strings_compare_numerically() {
    let out = compile_and_run("<?php var_dump(\"0\" == \"00\");");
    assert_eq!(out, "bool(true)\n");
}

#[test]
// Verifies compile-time loose equality of number and non-numeric string is false: 0 == "abc" is bool(false).
fn test_constant_loose_eq_number_and_non_numeric_string_is_false() {
    let out = compile_and_run("<?php var_dump(0 == \"abc\");");
    assert_eq!(out, "bool(false)\n");
}

#[test]
// Verifies compile-time loose equality of number and numeric string is true: 10 == "1e1" both evaluate to 10.0.
fn test_constant_loose_eq_number_and_numeric_string_is_true() {
    let out = compile_and_run("<?php var_dump(10 == \"1e1\");");
    assert_eq!(out, "bool(true)\n");
}

#[test]
// Verifies runtime loose equality of two non-numeric strings compares by byte sequence.
fn test_runtime_loose_eq_non_numeric_strings_compare_by_bytes() {
    let out = compile_and_run("<?php $a = \"abc\"; $b = \"def\"; var_dump($a == $b);");
    assert_eq!(out, "bool(false)\n");
}

#[test]
// Verifies runtime loose equality of numeric strings "0" == "00" compares numerically as equal.
fn test_runtime_loose_eq_numeric_strings_compare_numerically() {
    let out = compile_and_run("<?php $a = \"0\"; $b = \"00\"; var_dump($a == $b);");
    assert_eq!(out, "bool(true)\n");
}

#[test]
// Verifies runtime loose equality of number and non-numeric string is false: $n=0, $s="abc" → bool(false).
fn test_runtime_loose_eq_number_and_non_numeric_string_is_false() {
    let out = compile_and_run("<?php $n = 0; $s = \"abc\"; var_dump($n == $s);");
    assert_eq!(out, "bool(false)\n");
}

#[test]
// Verifies runtime loose equality of number and numeric string is true: $n=10, $s="1e1" → bool(true).
fn test_runtime_loose_eq_number_and_numeric_string_is_true() {
    let out = compile_and_run("<?php $n = 10; $s = \"1e1\"; var_dump($n == $s);");
    assert_eq!(out, "bool(true)\n");
}

#[test]
// Verifies runtime loose equality of bool and string uses truthiness: true=="abc" is true (truthy), false=="abc" is false.
fn test_runtime_loose_eq_bool_and_string_uses_truthiness() {
    let out = compile_and_run("<?php $s = \"abc\"; var_dump(true == $s); var_dump(false == $s);");
    assert_eq!(out, "bool(true)\nbool(false)\n");
}

#[test]
// Verifies runtime loose equality of null and string uses empty-string rule: null=="" is true, null=="0" is false.
fn test_runtime_loose_eq_null_and_string_uses_empty_string_rule() {
    let out = compile_and_run("<?php $empty = \"\"; $zero = \"0\"; var_dump(null == $empty); var_dump(null == $zero);");
    assert_eq!(out, "bool(true)\nbool(false)\n");
}

#[test]
// Verifies integer less-than comparison: 1 < 2 outputs "1".
fn test_less_than() {
    let out = compile_and_run("<?php echo 1 < 2;");
    assert_eq!(out, "1");
}

#[test]
// Verifies integer greater-than comparison: 2 > 1 outputs "1".
fn test_greater_than() {
    let out = compile_and_run("<?php echo 2 > 1;");
    assert_eq!(out, "1");
}

#[test]
// Verifies integer less-than-or-equal comparison: 2 <= 2 outputs "1".
fn test_less_equal() {
    let out = compile_and_run("<?php echo 2 <= 2;");
    assert_eq!(out, "1");
}

#[test]
// Verifies integer greater-than-or-equal comparison: 1 >= 2 outputs empty string (false).
fn test_greater_equal() {
    let out = compile_and_run("<?php echo 1 >= 2;");
    assert_eq!(out, "");
}
