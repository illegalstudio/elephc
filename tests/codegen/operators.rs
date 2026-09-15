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
/// Verifies integer addition with literal operands: 10 + 32 = 42.
#[test]
fn test_addition() {
    let out = compile_and_run("<?php echo 10 + 32;");
    assert_eq!(out, "42");
}


/// Verifies integer subtraction with literal operands: 100 - 58 = 42.
#[test]
fn test_subtraction() {
    let out = compile_and_run("<?php echo 100 - 58;");
    assert_eq!(out, "42");
}


/// Verifies integer multiplication with literal operands: 6 * 7 = 42.
#[test]
fn test_multiplication() {
    let out = compile_and_run("<?php echo 6 * 7;");
    assert_eq!(out, "42");
}


/// Verifies integer division with literal operands: 84 / 2 = 42.
#[test]
fn test_division() {
    let out = compile_and_run("<?php echo 84 / 2;");
    assert_eq!(out, "42");
}


/// Verifies arithmetic with variables: loads two integers from memory and adds them.
#[test]
fn test_arithmetic_with_variables() {
    let out = compile_and_run("<?php $a = 10; $b = 32; echo $a + $b;");
    assert_eq!(out, "42");
}


/// Verifies operator precedence: multiplication binds tighter than addition, so 2 + 3 * 4 = 14.
#[test]
fn test_operator_precedence() {
    let out = compile_and_run("<?php echo 2 + 3 * 4;");
    assert_eq!(out, "14");
}


/// Verifies parenthesized precedence: (2 + 3) * 4 = 20, confirming parentheses override default precedence.
#[test]
fn test_parenthesized_arithmetic() {
    let out = compile_and_run("<?php echo (2 + 3) * 4;");
    assert_eq!(out, "20");
}


/// Verifies a complex expression mixing parentheses, addition, multiplication, and subtraction: (10 + 5) * 2 - 7 = 23.
#[test]
fn test_complex_expression() {
    let out = compile_and_run("<?php echo (10 + 5) * 2 - 7;");
    assert_eq!(out, "23");
}


/// Verifies assignment of an arithmetic expression result to a variable, then echo: $a + $b → $c → output.
#[test]
fn test_arithmetic_assign_and_echo() {
    let out = compile_and_run("<?php $a = 10; $b = 32; $c = $a + $b; echo $c;");
    assert_eq!(out, "42");
}


/// Verifies subtraction producing a negative result: 3 - 10 = -7, confirming signed integer handling.
#[test]
fn test_subtraction_negative_result() {
    let out = compile_and_run("<?php echo 3 - 10;");
    assert_eq!(out, "-7");
}


/// Verifies left-associative chaining of addition: 1 + 2 + 3 + 4 = 10.
#[test]
fn test_nested_arithmetic() {
    let out = compile_and_run("<?php echo 1 + 2 + 3 + 4;");
    assert_eq!(out, "10");
}


/// Verifies that adding 1 to the maximum 64-bit integer constant overflows to float at compile time.
#[test]
fn test_constant_int_add_overflow_promotes_to_float() {
    let out = compile_and_run("<?php echo gettype(9223372036854775807 + 1);");
    assert_eq!(out, "double");
}


/// Verifies that squaring a large integer constant overflows to float at compile time.
#[test]
fn test_constant_int_multiply_overflow_promotes_to_float() {
    let out = compile_and_run("<?php echo gettype(3037000500 * 3037000500);");
    assert_eq!(out, "double");
}


/// Verifies that adding 1 to the maximum 64-bit integer at runtime overflows to float.
#[test]
fn test_runtime_int_add_overflow_promotes_to_float() {
    let out = compile_and_run("<?php function add_one(int $x) { return $x + 1; } echo gettype(add_one(9223372036854775807));");
    assert_eq!(out, "double");
}


/// Verifies that subtracting past the minimum 64-bit integer at runtime overflows to float.
#[test]
fn test_runtime_int_sub_overflow_promotes_to_float() {
    let out = compile_and_run("<?php function sub_two(int $x) { return $x - 2; } echo gettype(sub_two(-9223372036854775807));");
    assert_eq!(out, "double");
}


/// Verifies that squaring a large integer at runtime overflows to float.
#[test]
fn test_runtime_int_multiply_overflow_promotes_to_float() {
    let out = compile_and_run("<?php function mul_big(int $x) { return $x * 3037000500; } echo gettype(mul_big(3037000500));");
    assert_eq!(out, "double");
}


/// Verifies that runtime integer arithmetic without overflow remains integer, not float.
#[test]
fn test_runtime_int_arithmetic_without_overflow_stays_integer() {
    let out = compile_and_run("<?php function add_small(int $x) { return $x + 2; } $v = add_small(40); echo gettype($v) . ':' . $v;");
    assert_eq!(out, "integer:42");
}


/// Verifies that a runtime overflow result (float) participates correctly in subsequent arithmetic.
#[test]
fn test_runtime_overflow_result_participates_in_later_arithmetic() {
    let out = compile_and_run("<?php function add_one(int $x) { return $x + 1; } $c = add_one(9223372036854775807); echo gettype($c + 1);");
    assert_eq!(out, "double");
}


/// Verifies that pre-increment promotes an overflowing int local and returns the promoted value.
#[test]
fn test_runtime_pre_increment_overflow_promotes_local_to_float() {
    let out = compile_and_run("<?php function pre_inc(int $x) { $y = ++$x; echo gettype($y) . ':' . gettype($x); } pre_inc(9223372036854775807);");
    assert_eq!(out, "double:double");
}


/// Verifies that post-increment returns the old int while promoting the local for later reads.
#[test]
fn test_runtime_post_increment_overflow_returns_old_int_and_promotes_local() {
    let out = compile_and_run("<?php function post_inc(int $x) { $y = $x++; echo gettype($y) . ':' . gettype($x); } post_inc(9223372036854775807);");
    assert_eq!(out, "integer:double");
}


/// Verifies string literal concatenation: "Hello, " . "World!" = "Hello, World!".
#[test]
fn test_concat_literals() {
    let out = compile_and_run("<?php echo \"Hello, \" . \"World!\";");
    assert_eq!(out, "Hello, World!");
}


/// Verifies string concatenation with variables: loads two strings from memory and concatenates.
#[test]
fn test_concat_variables() {
    let out = compile_and_run("<?php $a = \"Hello, \"; $b = \"World!\"; echo $a . $b;");
    assert_eq!(out, "Hello, World!");
}


/// Verifies left-associative chaining of string concatenation: "a" . "b" . "c" = "abc".
#[test]
fn test_concat_chain() {
    let out = compile_and_run("<?php echo \"a\" . \"b\" . \"c\";");
    assert_eq!(out, "abc");
}


/// Verifies concatenation assignment: $msg = "foo" . "bar"; echo $msg; = "foobar".
#[test]
fn test_concat_assign() {
    let out = compile_and_run("<?php $msg = \"foo\" . \"bar\"; echo $msg;");
    assert_eq!(out, "foobar");
}


/// Verifies concatenation with embedded newline escape: "hello" . "\n" outputs "hello\n".
#[test]
fn test_concat_with_newline() {
    let out = compile_and_run("<?php echo \"hello\" . \"\\n\";");
    assert_eq!(out, "hello\n");
}


/// Verifies that concatenating an array onto a string stringifies the array to the literal
/// "Array" (matching PHP's array-to-string conversion) for both an array literal and an
/// array-typed function result, instead of crashing by treating the array pointer as a string.
#[test]
fn test_concat_array_stringifies_to_array_literal() {
    let out = compile_and_run(
        r#"<?php
function makeArr() { return [1, 2, 3]; }
echo "a" . [4, 5];
echo "|";
echo "prefix" . makeArr();
"#,
    );
    assert_eq!(out, "aArray|prefixArray");
}


/// Verifies that echoing an array stringifies to the literal "Array" (matching PHP), routing
/// through the same string-coercion path as concatenation.
#[test]
fn test_echo_array_stringifies_to_array_literal() {
    let out = compile_and_run("<?php $a = [1, 2, 3]; echo $a;");
    assert_eq!(out, "Array");
}


/// Verifies that interpolating an array into a double-quoted string stringifies it to the
/// literal "Array" (matching PHP) for both simple `$a` and complex `{$a}` interpolation.
#[test]
fn test_interpolated_array_stringifies_to_array_literal() {
    let out = compile_and_run("<?php $a = [1, 2, 3]; echo \"v=$a|w={$a}\";");
    assert_eq!(out, "v=Array|w=Array");
}


/// Verifies concatenation of string literal and integer literal: "Value: " . 42 = "Value: 42".
#[test]
fn test_concat_string_and_int() {
    let out = compile_and_run("<?php echo \"Value: \" . 42;");
    assert_eq!(out, "Value: 42");
}


/// Verifies concatenation of integer literal and string literal: 42 . " is the answer" = "42 is the answer".
#[test]
fn test_concat_int_and_string() {
    let out = compile_and_run("<?php echo 42 . \" is the answer\";");
    assert_eq!(out, "42 is the answer");
}


/// Verifies concatenation of two integer literals coerces to string: 1 . 2 = "12".
#[test]
fn test_concat_int_and_int() {
    let out = compile_and_run("<?php echo 1 . 2;");
    assert_eq!(out, "12");
}


/// Verifies concatenation of a string literal and a parenthesized expression result: "Result: " . ($a + $b) = "Result: 42".
#[test]
fn test_concat_expr_result() {
    let out = compile_and_run("<?php $a = 10; $b = 32; echo \"Result: \" . ($a + $b);");
    assert_eq!(out, "Result: 42");
}


/// Verifies mixed-type concatenation chaining left-to-right: "x=" . 5 . " y=" . 10 = "x=5 y=10".
#[test]
fn test_concat_chain_mixed() {
    let out = compile_and_run("<?php echo \"x=\" . 5 . \" y=\" . 10;");
    assert_eq!(out, "x=5 y=10");
}


/// Verifies concatenation with a negative integer: "num: " . -7 = "num: -7".
#[test]
fn test_concat_negative_int() {
    let out = compile_and_run("<?php echo \"num: \" . -7;");
    assert_eq!(out, "num: -7");
}


/// Verifies integer modulo: 10 % 3 = 1.
#[test]
fn test_modulo() {
    let out = compile_and_run("<?php echo 10 % 3;");
    assert_eq!(out, "1");
}


/// Verifies modulo with zero remainder: 15 % 5 = 0.
#[test]
fn test_modulo_zero_remainder() {
    let out = compile_and_run("<?php echo 15 % 5;");
    assert_eq!(out, "0");
}


/// Verifies loose equality comparison returning true: 1 == 1 outputs "1".
#[test]
fn test_equal_true() {
    let out = compile_and_run("<?php echo 1 == 1;");
    assert_eq!(out, "1");
}


/// Verifies loose equality comparison returning false: 1 == 2 outputs empty string (echo false prints nothing in PHP).
#[test]
fn test_equal_false() {
    let out = compile_and_run("<?php echo 1 == 2;");
    assert_eq!(out, ""); // echo false prints nothing in PHP
}


/// Verifies loose inequality returning true: 1 != 2 outputs "1".
#[test]
fn test_not_equal() {
    let out = compile_and_run("<?php echo 1 != 2;");
    assert_eq!(out, "1");
}


/// Verifies loose equality at compile time: empty string equals false, var_dump shows bool(true).
#[test]
fn test_loose_eq_empty_string_false() {
    let out = compile_and_run("<?php var_dump(\"\" == false);");
    assert_eq!(out, "bool(true)\n");
}


/// Verifies loose equality at compile time: integer 0 equals false, var_dump shows bool(true).
#[test]
fn test_loose_eq_zero_false() {
    let out = compile_and_run("<?php var_dump(0 == false);");
    assert_eq!(out, "bool(true)\n");
}


/// Verifies loose equality at compile time: integer 1 equals true, var_dump shows bool(true).
#[test]
fn test_loose_eq_one_true() {
    let out = compile_and_run("<?php var_dump(1 == true);");
    assert_eq!(out, "bool(true)\n");
}


/// Verifies loose equality at compile time: string "0" equals false (string zero is falsy), var_dump shows bool(true).
#[test]
fn test_loose_eq_string_vs_int() {
    let out = compile_and_run("<?php var_dump(\"0\" == false);");
    assert_eq!(out, "bool(true)\n");
}


/// Verifies loose inequality at compile time: empty string is not equal to true, var_dump shows bool(true).
#[test]
fn test_loose_neq_empty_string_true() {
    let out = compile_and_run("<?php var_dump(\"\" != true);");
    assert_eq!(out, "bool(true)\n");
}


/// Verifies loose equality at compile time: null equals false (null is falsy), var_dump shows bool(true).
#[test]
fn test_loose_eq_null_false() {
    let out = compile_and_run("<?php var_dump(null == false);");
    assert_eq!(out, "bool(true)\n");
}


/// Verifies compile-time loose equality of two non-numeric strings compares by byte sequence, not lexicographically.
#[test]
fn test_constant_loose_eq_non_numeric_strings_compare_by_bytes() {
    let out = compile_and_run("<?php var_dump(\"abc\" == \"def\");");
    assert_eq!(out, "bool(false)\n");
}


/// Verifies compile-time loose equality of numeric strings ("0" == "00") compares numerically as equal.
#[test]
fn test_constant_loose_eq_numeric_strings_compare_numerically() {
    let out = compile_and_run("<?php var_dump(\"0\" == \"00\");");
    assert_eq!(out, "bool(true)\n");
}


/// Verifies compile-time loose equality of number and non-numeric string is false: 0 == "abc" is bool(false).
#[test]
fn test_constant_loose_eq_number_and_non_numeric_string_is_false() {
    let out = compile_and_run("<?php var_dump(0 == \"abc\");");
    assert_eq!(out, "bool(false)\n");
}


/// Verifies compile-time loose equality of number and numeric string is true: 10 == "1e1" both evaluate to 10.0.
#[test]
fn test_constant_loose_eq_number_and_numeric_string_is_true() {
    let out = compile_and_run("<?php var_dump(10 == \"1e1\");");
    assert_eq!(out, "bool(true)\n");
}


/// Verifies runtime float comparisons against NaN match PHP: NaN is uncomparable, so `<`, `<=`,
/// `>`, `>=`, `==` are all false and `!=` is true, while `<=>` yields 1 in every direction
/// (including NaN<=>NaN). Operands come from `float`-returning calls so the optimizer cannot
/// constant-fold them, exercising the runtime comparison codegen rather than the folder.
#[test]
fn test_runtime_nan_comparisons() {
    let out = compile_and_run(
        r#"<?php
function nan_val(): float { return NAN; }
function one_val(): float { return 1.0; }
$nan = nan_val();
$one = one_val();
var_dump($nan < $one);
var_dump($nan <= $one);
var_dump($nan > $one);
var_dump($nan >= $one);
var_dump($nan == $one);
var_dump($nan != $one);
echo ($nan <=> $one), ($one <=> $nan), ($nan <=> $nan);
"#,
    );
    assert_eq!(
        out,
        "bool(false)\nbool(false)\nbool(false)\nbool(false)\nbool(false)\nbool(true)\n111"
    );
}


/// Regression: ordered comparisons on `float|false` must inspect the boxed runtime tag.
/// Fractional floats keep their fraction, while a `false` arm compares to numbers by PHP
/// truthiness rather than by treating the boolean payload as numeric zero.
#[test]
fn test_float_or_false_union_uses_runtime_ordering_rules() {
    let out = compile_and_run(
        r#"<?php
function fractional_or_false(bool $ok): float|false {
    return $ok ? 0.5 : false;
}

$value = fractional_or_false(true);
var_dump($value > 0);
var_dump($value >= 0.5);
var_dump($value < 1);
var_dump($value <= 0.5);
var_dump($value <=> 0);
var_dump(0 <=> $value);

$failure = fractional_or_false(false);
var_dump($failure > -1);
var_dump($failure < -1);
var_dump($failure <=> -1);
var_dump(-1 <=> $failure);
var_dump($failure >= 0);
"#,
    );
    assert_eq!(
        out,
        "bool(true)\nbool(true)\nbool(true)\nbool(true)\nint(1)\nint(-1)\n\
         bool(false)\nbool(true)\nint(-1)\nint(1)\nbool(true)\n"
    );
}


/// Regression: boxed float unions preserve PHP's unordered NaN rules for numeric and string
/// operands, including the always-positive spaceship result in either operand order.
#[test]
fn test_float_or_false_union_relational_nan_is_unordered() {
    let out = compile_and_run(
        r#"<?php
function nan_or_false(bool $ok): float|false {
    return $ok ? NAN : false;
}

function string_or_float(string $value): float|string {
    return $value;
}

$nan = nan_or_false(true);
var_dump($nan < 0);
var_dump($nan <= 0);
var_dump($nan > 0);
var_dump($nan >= 0);
var_dump($nan <=> 0);
foreach (["1", "a"] as $raw) {
    $string = string_or_float($raw);
    var_dump($string <=> $nan);
    var_dump($nan <=> $string);
    var_dump($string < $nan);
    var_dump($string > $nan);
    var_dump($nan < $string);
    var_dump($nan > $string);
}
"#,
    );
    assert_eq!(
        out,
        "bool(false)\nbool(false)\nbool(false)\nbool(false)\nint(1)\n\
         int(1)\nint(1)\nbool(false)\nbool(false)\nbool(false)\nbool(false)\n\
         int(1)\nint(1)\nbool(false)\nbool(false)\nbool(false)\nbool(false)\n"
    );
}


/// Verifies runtime loose equality of two non-numeric strings compares by byte sequence.
#[test]
fn test_runtime_loose_eq_non_numeric_strings_compare_by_bytes() {
    let out = compile_and_run("<?php $a = \"abc\"; $b = \"def\"; var_dump($a == $b);");
    assert_eq!(out, "bool(false)\n");
}


/// Verifies runtime loose equality of numeric strings "0" == "00" compares numerically as equal.
#[test]
fn test_runtime_loose_eq_numeric_strings_compare_numerically() {
    let out = compile_and_run("<?php $a = \"0\"; $b = \"00\"; var_dump($a == $b);");
    assert_eq!(out, "bool(true)\n");
}


/// Verifies runtime loose equality of number and non-numeric string is false: $n=0, $s="abc" → bool(false).
#[test]
fn test_runtime_loose_eq_number_and_non_numeric_string_is_false() {
    let out = compile_and_run("<?php $n = 0; $s = \"abc\"; var_dump($n == $s);");
    assert_eq!(out, "bool(false)\n");
}


/// Verifies runtime loose equality of number and numeric string is true: $n=10, $s="1e1" → bool(true).
#[test]
fn test_runtime_loose_eq_number_and_numeric_string_is_true() {
    let out = compile_and_run("<?php $n = 10; $s = \"1e1\"; var_dump($n == $s);");
    assert_eq!(out, "bool(true)\n");
}


/// Verifies runtime loose equality of bool and string uses truthiness: true=="abc" is true (truthy), false=="abc" is false.
#[test]
fn test_runtime_loose_eq_bool_and_string_uses_truthiness() {
    let out = compile_and_run("<?php $s = \"abc\"; var_dump(true == $s); var_dump(false == $s);");
    assert_eq!(out, "bool(true)\nbool(false)\n");
}


/// Verifies runtime loose equality of null and string uses empty-string rule: null=="" is true, null=="0" is false.
#[test]
fn test_runtime_loose_eq_null_and_string_uses_empty_string_rule() {
    let out = compile_and_run("<?php $empty = \"\"; $zero = \"0\"; var_dump(null == $empty); var_dump(null == $zero);");
    assert_eq!(out, "bool(true)\nbool(false)\n");
}


/// Verifies integer less-than comparison: 1 < 2 outputs "1".
#[test]
fn test_less_than() {
    let out = compile_and_run("<?php echo 1 < 2;");
    assert_eq!(out, "1");
}


/// Verifies integer greater-than comparison: 2 > 1 outputs "1".
#[test]
fn test_greater_than() {
    let out = compile_and_run("<?php echo 2 > 1;");
    assert_eq!(out, "1");
}


/// Verifies integer less-than-or-equal comparison: 2 <= 2 outputs "1".
#[test]
fn test_less_equal() {
    let out = compile_and_run("<?php echo 2 <= 2;");
    assert_eq!(out, "1");
}


/// Verifies integer greater-than-or-equal comparison: 1 >= 2 outputs empty string (false).
#[test]
fn test_greater_equal() {
    let out = compile_and_run("<?php echo 1 >= 2;");
    assert_eq!(out, "");
}


/// Regression: a loose `==` between a plain integer and a boxed `Mixed` integer must hold in both
/// operand orders. Loading a Mixed operand unboxes it through a runtime call that clobbers the
/// scratch registers; without saving the already-loaded left operand, `Int == Mixed` lost its left
/// value and compared wrong, while `Mixed == Int` happened to work. The Mixed here comes from a
/// heterogeneous associative array element.
#[test]
fn test_loose_eq_int_and_mixed_both_orders() {
    let out = compile_and_run(
        r#"<?php
$h = ["n" => 100, "s" => "x"];
$m = $h["n"];
$i = 100;
echo ($i == $m ? "y" : "n"), ($m == $i ? "y" : "n"), ($i == $h["n"] ? "y" : "n"),
     ($i == 101 ? "y" : "n");
"#,
    );
    assert_eq!(out, "yyyn");
}


/// Regression for #397: loose equality with a Mixed operand holding a float
/// must not truncate the float to int before comparison. `1.5 == 1` must be
/// false, `1.5 == 1.5` must be true.
#[test]
fn test_loose_eq_mixed_float_vs_int() {
    let out = compile_and_run(
        r#"<?php
function check($m) {
    var_dump($m == 1);
    var_dump($m == 1.5);
    var_dump($m == 2);
}
check(1.5);
"#,
    );
    assert_eq!(out, "bool(false)\nbool(true)\nbool(false)\n");
}


/// Regression for #397: switch with a Mixed subject holding a float must use
/// loose equality, not integer truncation. `switch(1.5) { case 1: ...; case
/// 1.5: ... }` must match `case 1.5`.
#[test]
fn test_switch_mixed_float_subject() {
    let out = compile_and_run(
        r#"<?php
function classify($x) {
    switch ($x) {
        case 1:   return "int-one";
        case 1.5: return "onefive";
        default:  return "other";
    }
}
echo classify(1.5), "\n";
"#,
    );
    assert_eq!(out, "onefive\n");
}


/// Regression for #397: switch with a Mixed subject holding an int must still
/// match int cases correctly (no regression from the Mixed routing change).
#[test]
fn test_switch_mixed_int_subject() {
    let out = compile_and_run(
        r#"<?php
function classify($x) {
    switch ($x) {
        case 1:   return "int-one";
        case 1.5: return "onefive";
        default:  return "other";
    }
}
echo classify(1), "\n";
echo classify(2), "\n";
"#,
    );
    assert_eq!(out, "int-one\nother\n");
}


/// Issue #548, the relational sibling of the #397 family: `>`/`>=`/`<`/`<=`/`<=>` on a Mixed
/// box holding a float must compare numerically, never through an int cast.
///
/// The int cast is architecture-divergent exactly where it matters. A float past the int64
/// range converts with `fcvtzs` on ARM64, which SATURATES — so `1.84e19` became `INT64_MAX`
/// and `> 0` was accidentally right — and with `cvttsd2si` on x86_64, which produces the
/// "integer indefinite" `INT64_MIN`, so the same program answered `false` there. Both diverge
/// from PHP, which compares as float.
///
/// `PHP_INT_MAX * 2` is the overflow shape from the report (it promotes to float). The
/// `PHP_INT_MAX` comparisons are the cases ARM64 got wrong too, so this fails on either target
/// if the cast comes back, not just on the one that reported it. `1.5` keeps an in-range float
/// in the same fixture: truncating THAT answers `1.5 > 1` correctly by accident but
/// `1.5 <=> PHP_INT_MAX` wrongly, so the spaceship rows carry it.
///
/// Each of the four relational operators is asserted on its own, because each maps to its own
/// `CmpPredicate` (`Sgt`/`Sge`/`Slt`/`Sle`) after the shared Mixed dispatch — a regression
/// isolated to one predicate would slip past a fixture that only exercises its sibling. The
/// rows are chosen so every operator has at least one discriminating case: `<= PHP_INT_MAX` is
/// the one ARM64's saturating `fcvtzs` gets wrong on the positive overflow (`INT64_MAX <=
/// INT64_MAX` is true where `1.84e19 <= INT64_MAX` is false), `< PHP_INT_MAX` is the one
/// x86_64's `INT64_MIN` indefinite gets wrong on the same value, and `<= 1` separates float
/// from truncated-int for the in-range `1.5` (`1.5 <= 1` is false; `1 <= 1` is not).
///
/// Every expectation is the host PHP 8.5.10 output for the same fixture.
#[test]
fn test_relational_mixed_float_compares_numerically_across_the_int64_boundary() {
    let out = compile_and_run(
        r#"<?php
function probe($m) {
    var_dump($m > 0);
    var_dump($m > PHP_INT_MAX);
    var_dump($m >= PHP_INT_MAX);
    var_dump($m < 0);
    var_dump($m < PHP_INT_MAX);
    var_dump($m <= PHP_INT_MAX);
    var_dump($m <= 1);
    var_dump($m <=> 0);
    var_dump($m <=> PHP_INT_MAX);
}
$a = $argc > 0 ? PHP_INT_MAX : 0;
probe($a * 2);
probe($a * -2);
probe(1.5);
"#,
    );
    assert_eq!(
        out,
        concat!(
            // 1.8446744073709552E+19: above everything, and above INT64_MAX in particular.
            "bool(true)\nbool(true)\nbool(true)\nbool(false)\n",
            "bool(false)\nbool(false)\nbool(false)\nint(1)\nint(1)\n",
            // -1.8446744073709552E+19: below everything, the saturation case in reverse.
            "bool(false)\nbool(false)\nbool(false)\nbool(true)\n",
            "bool(true)\nbool(true)\nbool(true)\nint(-1)\nint(-1)\n",
            // 1.5: in range, so the PHP_INT_MAX rows and `<= 1` separate float from
            // truncated-int.
            "bool(true)\nbool(false)\nbool(false)\nbool(false)\n",
            "bool(true)\nbool(true)\nbool(false)\nint(1)\nint(-1)\n",
        )
    );
}


/// Regression for #397: `!=` (LooseNotEq) with a Mixed float operand must
/// also avoid truncation. `1.5 != 1` must be true.
#[test]
fn test_loose_neq_mixed_float_vs_int() {
    let out = compile_and_run(
        r#"<?php
function check($m) {
    var_dump($m != 1);
    var_dump($m != 1.5);
}
check(1.5);
"#,
    );
    assert_eq!(out, "bool(true)\nbool(false)\n");
}


/// Regression: loose equality with a Mixed NaN payload must preserve PHP's
/// unordered-float rule. `NAN == 1` is false and `NAN != 1` is true, including
/// on x86_64 where unordered `ucomisd` comparisons set ZF.
#[test]
fn test_loose_eq_mixed_nan_vs_int() {
    let out = compile_and_run(
        r#"<?php
function check($m) {
    var_dump($m == 1);
    var_dump($m != 1);
}
check(NAN);
"#,
    );
    assert_eq!(out, "bool(false)\nbool(true)\n");
}


/// Regression: loose equality with a Mixed string payload must use PHP
/// numeric-string rules instead of `atof`-style casts. Non-numeric strings are
/// not equal to numbers, while numeric strings compare by parsed numeric value.
#[test]
fn test_loose_eq_mixed_string_vs_number_uses_numeric_string_rules() {
    let out = compile_and_run(
        r#"<?php
function check($m) {
    var_dump($m == 0);
    var_dump($m == 0.0);
    var_dump($m != 0);
    var_dump($m == 1.5);
}
check("abc");
check("1.5");
"#,
    );
    assert_eq!(
        out,
        "bool(false)\nbool(false)\nbool(true)\nbool(false)\nbool(false)\nbool(false)\nbool(true)\nbool(true)\n"
    );
}


/// Regression: loose equality between a Mixed boolean and a number compares by
/// PHP truthiness, not by comparing `true` as `1.0`.
#[test]
fn test_loose_eq_mixed_bool_vs_number_uses_truthiness() {
    let out = compile_and_run(
        r#"<?php
function check_true($m) {
    var_dump($m == 2);
    var_dump($m == 0.5);
    var_dump($m == 0);
}
function check_false($m) {
    var_dump($m == 0.0);
    var_dump($m == 1);
}
check_true(true);
check_false(false);
"#,
    );
    assert_eq!(
        out,
        "bool(true)\nbool(true)\nbool(false)\nbool(true)\nbool(false)\n"
    );
}


/// Regression: Mixed array payloads are not loosely equal to numeric operands.
#[test]
fn test_loose_eq_mixed_array_vs_number_is_false() {
    let out = compile_and_run(
        r#"<?php
function check($m) {
    var_dump($m == 0);
    var_dump($m != 1.0);
}
check([]);
check([1]);
"#,
    );
    assert_eq!(out, "bool(false)\nbool(true)\nbool(false)\nbool(true)\n");
}


/// Regression: two empty array literals are strictly equal (deep structural `===`, not pointer
/// identity). This is the base case of the `__rt_array_strict_eq` runtime helper.
#[test]
fn test_strict_eq_empty_arrays() {
    let out = compile_and_run(
        r#"<?php
var_dump([] === []);
var_dump([] === [1]);
"#,
    );
    assert_eq!(out, "bool(true)\nbool(false)\n");
}


/// Regression: a value typed `array|false` (a runtime-Mixed union) compared against an array
/// literal must deep-compare, not compare heap pointers. Previously `$x === []` was always false.
#[test]
fn test_strict_eq_union_array_against_literal() {
    let out = compile_and_run(
        r#"<?php
function make(bool $b): array|false { return $b ? [1, 2] : false; }
$empty = make(true);
$empty = [];
var_dump($empty === []);
$xs = make(true);
var_dump($xs === [1, 2]);
var_dump($xs === [1, 3]);
var_dump($xs === [1, 2, 3]);
$no = make(false);
var_dump($no === []);
"#,
    );
    assert_eq!(
        out,
        "bool(true)\nbool(true)\nbool(false)\nbool(false)\nbool(false)\n"
    );
}


/// Regression: indexed integer arrays compare element-by-element with length sensitivity.
#[test]
fn test_strict_eq_indexed_int_arrays() {
    let out = compile_and_run(
        r#"<?php
var_dump([1, 2, 3] === [1, 2, 3]);
var_dump([1, 2, 3] === [1, 2, 4]);
var_dump([1, 2, 3] === [1, 2]);
var_dump([1, 2] === [1, 2, 3]);
"#,
    );
    assert_eq!(out, "bool(true)\nbool(false)\nbool(false)\nbool(false)\n");
}


/// Regression: string-element arrays compare by string contents, not pointer identity.
#[test]
fn test_strict_eq_string_element_arrays() {
    let out = compile_and_run(
        r#"<?php
var_dump(["a", "b"] === ["a", "b"]);
var_dump(["a", "b"] === ["a", "c"]);
var_dump(["a", "b"] === ["a"]);
"#,
    );
    assert_eq!(out, "bool(true)\nbool(false)\nbool(false)\n");
}


/// Regression: associative arrays require the same key => value pairs in the same insertion order.
#[test]
fn test_strict_eq_assoc_arrays_order_sensitive() {
    let out = compile_and_run(
        r#"<?php
var_dump(["x" => 1, "y" => 2] === ["x" => 1, "y" => 2]);
var_dump(["x" => 1, "y" => 2] === ["x" => 1, "y" => 3]);
var_dump(["x" => 1, "y" => 2] === ["x" => 1, "z" => 2]);
var_dump(["x" => 1, "y" => 2] === ["y" => 2, "x" => 1]);
"#,
    );
    assert_eq!(out, "bool(true)\nbool(false)\nbool(false)\nbool(false)\n");
}


/// Regression: nested arrays compare recursively through `__rt_mixed_strict_eq` re-entering
/// `__rt_array_strict_eq`.
#[test]
fn test_strict_eq_nested_arrays() {
    let out = compile_and_run(
        r#"<?php
var_dump([[1, 2], [3, 4]] === [[1, 2], [3, 4]]);
var_dump([[1, 2], [3, 4]] === [[1, 2], [3, 5]]);
var_dump([["a" => [1]]] === [["a" => [1]]]);
var_dump([["a" => [1]]] === [["a" => [2]]]);
"#,
    );
    assert_eq!(out, "bool(true)\nbool(false)\nbool(true)\nbool(false)\n");
}


/// Regression: heterogeneous arrays (mixed element types, stored as boxed Mixed slots) compare
/// with full per-element type precision.
#[test]
fn test_strict_eq_heterogeneous_arrays() {
    let out = compile_and_run(
        r#"<?php
var_dump([1, "a", 3] === [1, "a", 3]);
var_dump([1, "a", 3] === [1, "b", 3]);
var_dump([1, "a", 3] === [1, "a", 4]);
"#,
    );
    assert_eq!(out, "bool(true)\nbool(false)\nbool(false)\n");
}


/// Regression: `!==` is the negation of the deep `===` for arrays.
#[test]
fn test_strict_not_eq_arrays() {
    let out = compile_and_run(
        r#"<?php
var_dump([1, 2] !== [1, 3]);
var_dump([1, 2] !== [1, 2]);
var_dump(["a" => 1] !== ["a" => 1]);
"#,
    );
    assert_eq!(out, "bool(true)\nbool(false)\nbool(false)\n");
}

/// Verifies a single `.` chain whose result exceeds the shared 64 KiB concat scratch buffer keeps
/// every byte instead of writing past the scratch end into the adjacent BSS globals. This exact
/// program used to segfault before `__rt_concat` reserved bounded destination storage.
#[test]
fn test_concat_result_larger_than_concat_scratch() {
    let out = compile_and_run(
        r#"<?php
$long = str_repeat("Z", 100000);
$s = "A" . $long . "B";
echo strlen($s), "|", $s[0], "|", $s[100001];
"#,
    );
    assert_eq!(out, "100002|A|B");
}


/// Verifies a `.=` accumulation loop that grows well past the 64 KiB concat scratch buffer stays
/// byte-exact. Every append beyond the scratch capacity takes the heap fallback, and the appended
/// result is taken over in place by `__rt_str_persist` so the loop does not exhaust the heap.
#[test]
fn test_concat_assign_loop_past_concat_scratch() {
    let out = compile_and_run(
        r#"<?php
$s = "";
for ($i = 0; $i < 200; $i++) { $s .= str_repeat("ab", 250); }
echo strlen($s), "|", substr($s, 0, 4), "|", substr($s, -4);
"#,
    );
    assert_eq!(out, "100000|abab|abab");
}


/// Verifies a `.=` accumulation loop that grows past one mebibyte — more than sixteen times the
/// concat scratch capacity — produces the exact PHP result. This is the shape reported as `.=`
/// heap corruption around 30 KB.
#[test]
fn test_concat_assign_loop_past_one_mebibyte() {
    let out = compile_and_run(
        r#"<?php
$s = "";
for ($i = 0; $i < 1500; $i++) { $s .= str_repeat("xy", 500); }
echo strlen($s), "|", substr($s, 0, 4), "|", substr($s, -4), "|", $s[1499999];
"#,
    );
    assert_eq!(out, "1500000|xyxy|xyxy|y");
}


/// Verifies an oversized concat result survives another string builtin running in between and a
/// later large allocation, which is the classic concat-scratch invalidation shape: a scratch-backed
/// result would have been overwritten, a heap-backed one must stay intact.
#[test]
fn test_concat_result_survives_later_string_work() {
    let out = compile_and_run(
        r#"<?php
$a = str_repeat("a", 40000);
$b = str_repeat("b", 40000);
$s = $a . $b;
$t = strtoupper($a);
$u = $s . $t;
$v = str_repeat("z", 70000);
echo strlen($s), "|", strlen($u), "|", $s[0], $s[39999], $s[40000], $s[79999];
echo "|", $u[0], $u[80000], $u[119999], "|", strlen($v);
"#,
    );
    assert_eq!(out, "80000|120000|aabb|aAA|70000");
}


/// Verifies `<>` behaves exactly like `!=` at runtime, including the loose numeric-string
/// comparison and array comparison cases. Expected output matches `php -r` on 8.4.
#[test]
fn test_angle_not_equal_matches_not_equal() {
    let out = compile_and_run(
        r#"<?php
var_dump(1 <> 2);
var_dump(1 <> 1);
var_dump("1" <> 1);
var_dump(1 + 1 <> 2);
var_dump(true <> false);
var_dump([1, 2] <> [1, 3]);
$a = 5;
var_dump($a <> 5, $a <> 6);
"#,
    );
    assert_eq!(
        out,
        "bool(true)\nbool(false)\nbool(false)\nbool(false)\nbool(true)\nbool(true)\nbool(false)\nbool(true)\n"
    );
}


/// Verifies `< >` separated by whitespace is still two relational operators, so the
/// `<>` token does not swallow chained comparisons such as `(1 < 2) > 0`.
#[test]
fn test_angle_not_equal_does_not_capture_spaced_comparisons() {
    let out = compile_and_run("<?php var_dump((1 < 2) > 0);");
    assert_eq!(out, "bool(true)\n");
}


/// Verifies `<<` with a shift count of 64 or more yields `0` instead of masking the count.
///
/// Raw AArch64 `lsl` / x86_64 `shl` by register mask the count to six bits, so `1 << 64` used
/// to produce `1` and `1 << 100` produced `68719476736`. Operands go through `$argc` so the
/// constant folders cannot evaluate the shift at compile time.
#[test]
fn test_shift_left_count_at_or_above_word_size_is_zero() {
    let out = compile_and_run(
        r#"<?php
$n = $argc;
var_dump((1 * $n) << (64 * $n));
var_dump((1 * $n) << (100 * $n));
var_dump((-8 * $n) << (64 * $n));
var_dump((1 * $n) << (63 * $n));
"#,
    );
    assert_eq!(
        out,
        "int(0)\nint(0)\nint(0)\nint(-9223372036854775808)\n"
    );
}


/// Verifies `>>` with a shift count of 64 or more saturates to a full sign fill like PHP.
///
/// PHP yields `0` for a non-negative value and `-1` for a negative one; the masked hardware
/// shift produced whatever `count % 64` happened to give.
#[test]
fn test_shift_right_count_at_or_above_word_size_saturates_to_sign() {
    let out = compile_and_run(
        r#"<?php
$n = $argc;
var_dump((8 * $n) >> (64 * $n));
var_dump((-8 * $n) >> (64 * $n));
var_dump((-8 * $n) >> (100 * $n));
var_dump((-1 * $n) >> (64 * $n));
var_dump(PHP_INT_MIN >> (65 * $n));
"#,
    );
    assert_eq!(out, "int(0)\nint(-1)\nint(-1)\nint(-1)\nint(-1)\n");
}


/// Verifies ordinary in-range shift counts are unaffected by the PHP shift guards.
#[test]
fn test_shift_in_range_counts_are_unchanged() {
    let out = compile_and_run(
        r#"<?php
$n = $argc;
echo (1 * $n) << (0 * $n), "|", (1 * $n) << (3 * $n), "|", (-8 * $n) >> (2 * $n);
$a = 5; $a <<= (2 * $n);
$b = 40; $b >>= (3 * $n);
echo "|", $a, "|", $b;
"#,
    );
    assert_eq!(out, "1|8|-2|20|5");
}


/// Verifies `PHP_INT_MIN % -1` evaluates to `0` on both supported targets.
///
/// x86_64 `idiv` raises `#DE` (SIGFPE) for that operand pair, so the lowering must answer the
/// `-1` divisor without reaching the divide unit; AArch64's `sdiv`/`msub` already wraps to `0`.
#[test]
fn test_int_min_modulo_negative_one_is_zero() {
    let out = compile_and_run(
        r#"<?php
$n = $argc;
$min = intdiv(PHP_INT_MIN, $n);
var_dump($min % (-1 * $n));
var_dump((-7 * $n) % (-1 * $n));
var_dump((7 * $n) % (-3 * $n));
"#,
    );
    assert_eq!(out, "int(0)\nint(0)\nint(1)\n");
}


/// Verifies PHP 8's `==` table for an array against a non-array operand.
///
/// An array converts to bool only against `null`/`bool` (`[] == null` and
/// `[] == false` are true, `[0] == true` is true), and is never loosely equal to
/// an int, float or string — `[] == 0` and `[1] == "1"` are both false. Values are
/// derived from `$argc` so the comparison survives AST folding and exercises the
/// EIR backend path.
#[test]
fn test_loose_equality_array_versus_scalar() {
    let out = compile_and_run(
        r#"<?php
$n = $argc;
$e = [];
$a = [1, 2, $n];
var_dump($e == null, $e == false, $e == true, $e == 0, $e == "");
var_dump([0 * $n] == true, [$n] == "1", 1 == [$n], $a == null);
"#,
    );
    assert_eq!(
        out,
        "bool(true)\nbool(true)\nbool(false)\nbool(false)\nbool(false)\n\
         bool(true)\nbool(false)\nbool(false)\nbool(false)\n"
    );
}


/// Verifies PHP's order-independent `==` between two arrays.
///
/// `==` requires equal counts and, for every key of the left array, the same key on
/// the right with a loosely equal value: `["a"=>1,"b"=>2] == ["b"=>2,"a"=>1]` is
/// true while `[1,2] == [2=>1,3=>2]` is false. `["a"=>null] == ["b"=>null]` pins
/// that a MISSING key never matches a stored `null`.
#[test]
fn test_loose_equality_array_versus_array() {
    let out = compile_and_run(
        r#"<?php
$n = $argc;
$a = [1, 2, $n];
$b = [1, 2, $n];
$c = [$n + 9, 2, 1];
var_dump($a == $b, $a == $c, $a != $c, $a == [1, 2]);
var_dump(["a" => $n, "b" => 2] == ["b" => 2, "a" => $n]);
var_dump([1, 2] == [2 => 1, 3 => 2]);
var_dump([[1, $n], [3]] == [[1, $n], [3]], [[1, $n], [3]] == [[1, $n], [4]]);
var_dump([null] == [0 * $n], ["a" => null] == ["b" => null]);
"#,
    );
    assert_eq!(
        out,
        "bool(true)\nbool(false)\nbool(true)\nbool(false)\n\
         bool(true)\nbool(false)\n\
         bool(true)\nbool(false)\n\
         bool(true)\nbool(false)\n"
    );
}


/// Verifies statement-position `++`/`--` on `$this` members, in both prefix and postfix
/// spelling, over an int property, an array-element property, and a float property.
/// Regression for the parser accepting `$obj->n++` but rejecting `$this->n++`.
#[test]
fn test_incdec_on_this_members() {
    let out = compile_and_run(
        r#"<?php
class C {
    public int $n = 0;
    public array $arr = [1, 2];
    public float $f = 1.5;
    public function bumpPost(): void { $this->n++; }
    public function bumpPre(): void { ++$this->n; }
    public function dropPost(): void { $this->n--; }
    public function dropPre(): void { --$this->n; }
    public function bumpElem(): void { $this->arr[0]++; }
    public function bumpElemPre(): void { ++$this->arr[1]; }
    public function bumpFloat(): void { $this->f++; }
}
$c = new C();
$c->bumpPost(); $c->bumpPre(); $c->bumpPost();
echo $c->n, "|";
$c->dropPost(); $c->dropPre();
echo $c->n, "|";
$c->bumpElem(); $c->bumpElemPre();
echo $c->arr[0], ",", $c->arr[1], "|";
$c->bumpFloat();
echo $c->f;
"#,
    );
    assert_eq!(out, "3|1|2,3|2.5");
}


/// Verifies prefix `++`/`--` in statement position on the complex targets that previously
/// only worked in postfix spelling: object properties and array elements.
#[test]
fn test_prefix_incdec_on_complex_targets() {
    let out = compile_and_run(
        r#"<?php
class C { public int $n = 0; public array $arr = [1, 2]; }
$c = new C();
++$c->n; ++$c->n; --$c->n; ++$c->arr[0];
echo $c->n, ",", $c->arr[0], "|";
$a = [1, 2];
++$a[0]; --$a[1];
echo $a[0], ",", $a[1], "|";
$x = 1; ++$x; $x++;
echo $x;
"#,
    );
    assert_eq!(out, "1,2|2,1|3");
}


/// Verifies `$this` member increments still work inside nested control flow, where the
/// statement parser is re-entered from a loop or conditional body.
#[test]
fn test_incdec_on_this_members_inside_control_flow() {
    let out = compile_and_run(
        r#"<?php
class C {
    public int $n = 0;
    public function run(): void { for ($i = 0; $i < 3; $i++) { $this->n++; } }
}
$c = new C();
$c->run();
echo $c->n, "|";
while ($c->n < 5) { $c->n++; }
echo $c->n;
"#,
    );
    assert_eq!(out, "3|5");
}


/// Verifies `++`/`--` on float locals in every spelling, including the value each form
/// returns and the IEEE edge cases. Expected output matches `php -r` on 8.4.
#[test]
fn test_incdec_on_float_locals() {
    let out = compile_and_run(
        r#"<?php
$f = 1.5; $f++; var_dump($f);
$g = 1.5; $g--; var_dump($g);
$h = -0.5; ++$h; var_dump($h);
$i = 2.25; --$i; var_dump($i);
$j = 1.5; var_dump($j++); var_dump($j);
$k = 1.5; var_dump(++$k); var_dump($k);
$m = 1.0e308; $m++; var_dump($m);
$inf = INF; $inf++; var_dump($inf);
$nan = NAN; $nan++; var_dump($nan);
"#,
    );
    assert_eq!(
        out,
        "float(2.5)\nfloat(0.5)\nfloat(0.5)\nfloat(1.25)\nfloat(1.5)\nfloat(2.5)\n\
         float(2.5)\nfloat(2.5)\nfloat(1.0E+308)\nfloat(INF)\nfloat(NAN)\n"
    );
}


/// Verifies a float local incremented in a loop accumulates like PHP, so the float
/// increment path also works when the local is register-allocated across a loop.
#[test]
fn test_float_increment_in_loop() {
    let out = compile_and_run(
        r#"<?php
$l = 0.1;
for ($n = 0; $n < 3; $n++) { $l++; }
var_dump($l);
"#,
    );
    assert_eq!(out, "float(3.1)\n");
}


/// Verifies PHP's perl-style string increment, including the alphanumeric carry
/// (`"az"` → `"ba"`, `"Zz"` → `"AAa"`, `"zz"` → `"aaa"`), the numeric-string retype
/// (`"9"` → `int(10)`, `"1.5"` → `float(2.5)`), the empty-string rule (`""` → `"1"`),
/// and the non-alphanumeric stop (`"a-"` is unchanged while `"-a"` becomes `"-b"`).
/// Expected output is `LC_ALL=C php 8.4.20` with its `E_DEPRECATED` lines removed —
/// elephc has no runtime deprecation channel and reproduces only the values.
#[test]
fn test_string_increment_matches_php() {
    let out = compile_and_run(
        r#"<?php
$a = "az"; $a++; var_dump($a);
$b = "Zz"; $b++; var_dump($b);
$c = "a9"; $c++; var_dump($c);
$d = "z";  $d++; var_dump($d);
$e = "Az"; $e++; var_dump($e);
$f = "zz"; $f++; var_dump($f);
$g = "A";  $g++; var_dump($g);
$h = "";   $h++; var_dump($h);
$i = "9";  $i++; var_dump($i);
$j = "a-"; $j++; var_dump($j);
$k = "-a"; $k++; var_dump($k);
$l = "9z"; $l++; var_dump($l);
$m = "1.5"; $m++; var_dump($m);
$n = "0x1A"; $n++; var_dump($n);
$o = "1e3"; $o++; var_dump($o);
"#,
    );
    assert_eq!(
        out,
        "string(2) \"ba\"\nstring(3) \"AAa\"\nstring(2) \"b0\"\nstring(2) \"aa\"\n\
         string(2) \"Ba\"\nstring(3) \"aaa\"\nstring(1) \"B\"\nstring(1) \"1\"\n\
         int(10)\nstring(2) \"a-\"\nstring(2) \"-b\"\nstring(3) \"10a\"\n\
         float(2.5)\nstring(4) \"0x1B\"\nfloat(1001)\n"
    );
}


/// Verifies PHP's string decrement: a non-numeric string is left ALONE (unlike `++`),
/// a numeric string decrements numerically and retypes, and the empty string becomes
/// `int(-1)`. Expected output is `LC_ALL=C php 8.4.20` without its deprecation lines.
#[test]
fn test_string_decrement_matches_php() {
    let out = compile_and_run(
        r#"<?php
$a = "az"; $a--; var_dump($a);
$b = "9";  $b--; var_dump($b);
$c = "";   $c--; var_dump($c);
$d = "1.5"; $d--; var_dump($d);
$e = "0";  $e--; var_dump($e);
"#,
    );
    assert_eq!(
        out,
        "string(2) \"az\"\nint(8)\nint(-1)\nfloat(0.5)\nint(-1)\n"
    );
}


/// Verifies the pre- and post-forms of a string increment: the post-form yields the value
/// the local held BEFORE the update (including across the numeric retype `"8"` → `int(9)`),
/// the pre-form yields the updated value.
#[test]
fn test_string_increment_pre_and_post_forms() {
    let out = compile_and_run(
        r#"<?php
$a = "az";
var_dump($a++);
var_dump($a);
$b = "az";
var_dump(++$b);
var_dump($b);
$c = "8";
var_dump($c++);
var_dump($c);
"#,
    );
    assert_eq!(
        out,
        "string(2) \"az\"\nstring(2) \"ba\"\nstring(2) \"ba\"\nstring(2) \"ba\"\n\
         string(1) \"8\"\nint(9)\n"
    );
}


/// Verifies the string increment survives the storage shapes that are not a plain
/// straight-line local: a loop-carried local (the spreadsheet-column idiom), a `string`
/// function parameter, and a boxed `mixed` local that happens to hold a string.
#[test]
fn test_string_increment_loop_parameter_and_mixed_local() {
    let out = compile_and_run(
        r#"<?php
$col = "A";
$out = [];
for ($i = 0; $i < 30; $i++) { $out[] = $col; $col++; }
echo implode(",", $out), "\n";
function bump(string $s): string { $s++; return $s; }
echo bump("az"), " ", bump("Zz"), " ", bump("a9"), "\n";
$m = $argc > 99 ? 1 : "az";
$m++;
var_dump($m);
"#,
    );
    assert_eq!(
        out,
        "A,B,C,D,E,F,G,H,I,J,K,L,M,N,O,P,Q,R,S,T,U,V,W,X,Y,Z,AA,AB,AC,AD\n\
         ba AAa b0\nstring(2) \"ba\"\n"
    );
}


/// Verifies the int/float boundary of a numeric-string increment: a value that still fits
/// stays an `int`, `PHP_INT_MAX` promotes to `float`, a 20-digit string is already a float,
/// and decrementing `PHP_INT_MAX` stays an exact `int`.
#[test]
fn test_numeric_string_increment_int_boundary() {
    let out = compile_and_run(
        r#"<?php
$a = "9223372036854775806"; $a++; var_dump($a);
$b = "9223372036854775807"; $b++; var_dump($b);
$c = "99999999999999999999"; $c++; var_dump($c);
$d = "9223372036854775807"; $d--; var_dump($d);
"#,
    );
    assert_eq!(
        out,
        "int(9223372036854775807)\nfloat(9.223372036854776E+18)\n\
         float(1.0E+20)\nint(9223372036854775806)\n"
    );
}

// --- Issue #362: PHP 8 numeric-string arithmetic ---
// PHP 8 accepts numeric and leading-numeric strings as arithmetic operands (`+ - * / % **`),
// coercing them at runtime: an integer-form string coerces to int, a float-form string
// (with a `.` or exponent) coerces to float, and a leading-numeric string uses its numeric
// prefix. Fully non-numeric string literals are rejected at compile time
// (see `error_tests::type_system::test_error_arithmetic_on_string`).

/// Verifies a pure integer-form string added to an int coerces to int: `"123" + 3` is `126`.
#[test]
fn test_numeric_string_plus_int() {
    let out = compile_and_run(r#"<?php echo "123" + 3;"#);
    assert_eq!(out, "126");
}

/// Verifies a float-form string operand routes through the runtime float path:
/// `"1.5" + 3` must be `float(4.5)`, not the truncated `int(4)`.
#[test]
fn test_numeric_string_float_form_plus_int() {
    let out = compile_and_run(r#"<?php var_dump("1.5" + 3);"#);
    assert_eq!(out, "float(4.5)\n");
}

/// Verifies a float-form string on the right operand also routes to float: `3 + "1.5"`.
#[test]
fn test_int_plus_numeric_string_float_form() {
    let out = compile_and_run(r#"<?php var_dump(3 + "1.5");"#);
    assert_eq!(out, "float(4.5)\n");
}

/// Verifies integer-form string subtraction stays int: `"100" - 30` is `int(70)`.
#[test]
fn test_numeric_string_subtraction() {
    let out = compile_and_run(r#"<?php var_dump("100" - 30);"#);
    assert_eq!(out, "int(70)\n");
}

/// Verifies float-form string subtraction routes to float: `"10.5" - 3` is `float(7.5)`.
#[test]
fn test_numeric_string_float_form_subtraction() {
    let out = compile_and_run(r#"<?php var_dump("10.5" - 3);"#);
    assert_eq!(out, "float(7.5)\n");
}

/// Verifies integer-form string multiplication stays int: `"2" * 5` is `int(10)`.
#[test]
fn test_numeric_string_multiplication() {
    let out = compile_and_run(r#"<?php var_dump("2" * 5);"#);
    assert_eq!(out, "int(10)\n");
}

/// Verifies string division follows PHP's float division: `"10" / 3` is `float(3.333...)`.
#[test]
fn test_numeric_string_division() {
    let out = compile_and_run(r#"<?php echo "10" / 3;"#);
    assert_eq!(out, "3.3333333333333");
}

/// Verifies string modulo coerces to int: `"10" % 3` is `int(1)`.
#[test]
fn test_numeric_string_modulo() {
    let out = compile_and_run(r#"<?php var_dump("10" % 3);"#);
    assert_eq!(out, "int(1)\n");
}

/// Verifies a leading-numeric string coerces its numeric prefix, matching PHP's
/// `"  +12foo" + 3` result of `int(15)` (with a compile-time non-numeric warning).
#[test]
fn test_leading_numeric_string_plus_int() {
    let out = compile_and_run(r#"<?php var_dump("  +12foo" + 3);"#);
    assert_eq!(out, "int(15)\n");
}

/// Verifies exponentiation accepts a numeric string operand: `"2" ** 3` prints `8`.
#[test]
fn test_numeric_string_exponentiation() {
    let out = compile_and_run(r#"<?php echo "2" ** 3;"#);
    assert_eq!(out, "8");
}

/// Verifies a scientific-notation string is float-form: `"1e3" + 1` is `float(1001)`.
#[test]
fn test_scientific_notation_string_plus_int() {
    let out = compile_and_run(r#"<?php var_dump("1e3" + 1);"#);
    assert_eq!(out, "float(1001)\n");
}

/// Verifies both operands may be numeric strings: `"6" * "7"` is `int(42)`.
#[test]
fn test_numeric_string_both_operands() {
    let out = compile_and_run(r#"<?php var_dump("6" * "7");"#);
    assert_eq!(out, "int(42)\n");
}

/// Verifies a numeric string variable (runtime value, not a literal) is coerced in
/// arithmetic: a float-form string still routes to the float path through the mixed helper.
#[test]
fn test_numeric_string_variable_operand() {
    let out = compile_and_run(
        r#"<?php $a = $argc > 99 ? "3" : "1.5"; var_dump($a + 3);"#,
    );
    assert_eq!(out, "float(4.5)\n");
}

/// Verifies an integer-form string wider than `i64` classifies as PHP's `IS_DOUBLE` before
/// the arithmetic runs: `"99999999999999999999" + 1` must be `float(1.0E+20)`, not the
/// `PHP_INT_MAX` value `strtoll` saturation would produce.
#[test]
fn test_numeric_string_integer_overflow_is_float() {
    let out = compile_and_run(r#"<?php var_dump("99999999999999999999" + 1);"#);
    assert_eq!(out, "float(1.0E+20)\n");
}

/// Verifies the same out-of-range classification on `*`, where a saturated integer path
/// would have kept the wrong `int(9223372036854775807)` result type as well as the value.
#[test]
fn test_numeric_string_integer_overflow_multiplication_is_float() {
    let out = compile_and_run(r#"<?php var_dump("99999999999999999999" * 1);"#);
    assert_eq!(out, "float(1.0E+20)\n");
}

/// Verifies a very wide integer-form string keeps full magnitude through the float path
/// instead of collapsing onto the 64-bit boundary.
#[test]
fn test_numeric_string_wide_integer_overflow_keeps_magnitude() {
    let out = compile_and_run(r#"<?php var_dump("123456789012345678901234567890" + 0);"#);
    assert_eq!(out, "float(1.2345678901234568E+29)\n");
}

/// Verifies the negative side of the range check: `"-99999999999999999999" - 1` is a float.
#[test]
fn test_negative_numeric_string_integer_overflow_is_float() {
    let out = compile_and_run(r#"<?php var_dump("-99999999999999999999" - 1);"#);
    assert_eq!(out, "float(-1.0E+20)\n");
}

/// Verifies the boundary itself still takes the integer path: `PHP_INT_MAX` spelled as a
/// string is `IS_LONG` in PHP, so it must stay `int` and keep every digit.
#[test]
fn test_numeric_string_at_int_max_stays_int() {
    let out = compile_and_run(r#"<?php var_dump("9223372036854775807" + 0);"#);
    assert_eq!(out, "int(9223372036854775807)\n");
}

/// Verifies `PHP_INT_MIN` spelled as a string also stays on the integer path.
#[test]
fn test_numeric_string_at_int_min_stays_int() {
    let out = compile_and_run(r#"<?php var_dump("-9223372036854775808" - 0);"#);
    assert_eq!(out, "int(-9223372036854775808)\n");
}

/// Verifies a runtime-valued out-of-range integer string is classified the same way as a
/// literal, so the compile-time and runtime paths agree.
#[test]
fn test_runtime_numeric_string_integer_overflow_is_float() {
    let out = compile_and_run(
        r#"<?php $s = $argc > 99 ? "1" : "99999999999999999999"; var_dump($s + 1);"#,
    );
    assert_eq!(out, "float(1.0E+20)\n");
}

/// Verifies a boxed `Mixed` string operand coming out of an array is coerced with the same
/// int-or-float rules as a typed `Str`, so `"1.5"` stored in an array is not truncated.
#[test]
fn test_boxed_mixed_numeric_string_float_form() {
    let out = compile_and_run(r#"<?php $a = ["x" => "1.5", "y" => 1]; var_dump($a["x"] + 3);"#);
    assert_eq!(out, "float(4.5)\n");
}

/// Verifies an integer-form string in a boxed `Mixed` cell stays an int.
#[test]
fn test_boxed_mixed_numeric_string_integer_form() {
    let out = compile_and_run(r#"<?php $a = ["x" => "3", "y" => 1]; var_dump($a["x"] + 3);"#);
    assert_eq!(out, "int(6)\n");
}

/// Verifies the `float` cast of a boxed string operand follows PHP's numeric-string grammar
/// rather than libc `strtod`: `"0x1A"` is `0`, not `26`, so `1.5 + "0x1A"` is `float(1.5)`.
#[test]
fn test_float_plus_hex_spelling_string_uses_php_grammar() {
    let out = compile_and_run(r#"<?php var_dump(1.5 + "0x1A");"#);
    assert_eq!(out, "float(1.5)\n");
}

/// Verifies the `INF` spelling libc `strtod` accepts cannot reach the float path through a
/// boxed `Mixed` operand: PHP has no `INF` numeric string, so the run is empty and the
/// result stays finite.
#[test]
fn test_float_plus_inf_spelling_string_is_not_infinite() {
    let out = compile_and_run(
        r#"<?php $a = ["x" => "INF", "y" => 1.5]; var_dump($a["y"] + $a["x"]);"#,
    );
    assert_eq!(out, "float(1.5)\n");
}

/// Verifies the underscore separator is not part of PHP's numeric-string grammar, so
/// `"1_000"` contributes only its `1` prefix.
#[test]
fn test_underscore_separator_string_uses_numeric_prefix_only() {
    let out = compile_and_run(r#"<?php var_dump("1_000" + 1);"#);
    assert_eq!(out, "int(2)\n");
}

/// Pins the documented incompatibility for a **runtime** non-numeric string: PHP raises a
/// `TypeError`, elephc coerces the missing numeric prefix to `0`. Only a constant string
/// literal is rejected at compile time (see
/// `error_tests::type_system::test_error_arithmetic_on_string`).
#[test]
fn test_runtime_non_numeric_string_coerces_to_zero() {
    let out = compile_and_run(r#"<?php $x = $argc > 99 ? "1" : "hi"; var_dump($x + 1);"#);
    assert_eq!(out, "int(1)\n");
}
