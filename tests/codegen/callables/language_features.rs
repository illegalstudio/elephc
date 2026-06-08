//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of callables language features, including default param string, default param override, and default param integer.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use crate::support::*;

// ===== Feature 1: Default parameter values =====

/// Verifies that a string default parameter is used when no argument is provided.
#[test]
fn test_default_param_string() {
    let out = compile_and_run(
        r#"<?php
function greet($name = "world") {
    echo "Hello " . $name;
}
greet();
"#,
    );
    assert_eq!(out, "Hello world");
}

/// Verifies that a provided argument overrides the string default parameter.
#[test]
fn test_default_param_override() {
    let out = compile_and_run(
        r#"<?php
function greet($name = "world") {
    echo "Hello " . $name;
}
greet("PHP");
"#,
    );
    assert_eq!(out, "Hello PHP");
}

/// Verifies that an integer default parameter of 0 is used when only one argument is passed.
#[test]
fn test_default_param_int() {
    let out = compile_and_run(
        r#"<?php
function add($a, $b = 0) {
    return $a + $b;
}
echo add(5);
"#,
    );
    assert_eq!(out, "5");
}

/// Verifies that a provided second argument overrides the integer default parameter.
#[test]
fn test_default_param_int_override() {
    let out = compile_and_run(
        r#"<?php
function add($a, $b = 0) {
    return $a + $b;
}
echo add(5, 3);
"#,
    );
    assert_eq!(out, "8");
}

/// Verifies that all three defaults (1, 2, 3) are used when no arguments are provided to a three-param function.
#[test]
fn test_default_param_multiple() {
    let out = compile_and_run(
        r#"<?php
function multi($a = 1, $b = 2, $c = 3) {
    echo $a + $b + $c;
}
multi();
"#,
    );
    assert_eq!(out, "6");
}

/// Verifies that partial application uses the first positional argument (10) while the remaining two defaults (2, 3) are applied.
#[test]
fn test_default_param_partial() {
    let out = compile_and_run(
        r#"<?php
function multi($a = 1, $b = 2, $c = 3) {
    echo $a + $b + $c;
}
multi(10);
"#,
    );
    assert_eq!(out, "15");
}

// ===== Feature 2: Null coalescing operator ?? =====

/// Verifies that ?? returns the right-hand side when the left-hand side is null.
#[test]
fn test_null_coalesce_null_value() {
    let out = compile_and_run(
        r#"<?php
$x = null;
echo $x ?? "default";
"#,
    );
    assert_eq!(out, "default");
}

/// Verifies that ?? returns the left-hand side when it is a non-null integer.
#[test]
fn test_null_coalesce_non_null() {
    let out = compile_and_run(
        r#"<?php
$x = 42;
echo $x ?? 0;
"#,
    );
    assert_eq!(out, "42");
}

/// Verifies that a chained ?? returns the last non-null value after two nulls.
#[test]
fn test_null_coalesce_chained() {
    let out = compile_and_run(
        r#"<?php
$x = null;
$y = null;
echo $x ?? $y ?? "found";
"#,
    );
    assert_eq!(out, "found");
}

/// Verifies that ?? returns the right-hand side when the left-hand side is the null literal.
#[test]
fn test_null_coalesce_literal_null() {
    let out = compile_and_run(
        r#"<?php
echo null ?? "fallback";
"#,
    );
    assert_eq!(out, "fallback");
}

/// Verifies that ?? returns the non-null string variable when provided.
#[test]
fn test_null_coalesce_string() {
    let out = compile_and_run(
        r#"<?php
$name = "Alice";
echo $name ?? "default";
"#,
    );
    assert_eq!(out, "Alice");
}

/// Verifies that ?? returns the right-hand side when the string variable is null.
#[test]
fn test_null_coalesce_null_to_string() {
    let out = compile_and_run(
        r#"<?php
$name = null;
echo $name ?? "default";
"#,
    );
    assert_eq!(out, "default");
}

/// Verifies that ?? returns the left-hand side when it is an empty string (not null).
#[test]
fn test_null_coalesce_empty_string() {
    let out = compile_and_run(
        r#"<?php
$val = "";
echo ($val ?? "fallback") . "|done";
"#,
    );
    assert_eq!(out, "|done");
}

/// Verifies that ?? returns the non-null integer variable when provided.
#[test]
fn test_null_coalesce_int() {
    let out = compile_and_run(
        r#"<?php
$x = 42;
echo $x ?? 0;
"#,
    );
    assert_eq!(out, "42");
}

/// Verifies that ?? returns the right-hand side when the integer variable is null.
#[test]
fn test_null_coalesce_null_to_int() {
    let out = compile_and_run(
        r#"<?php
$x = null;
echo $x ?? 99;
"#,
    );
    assert_eq!(out, "99");
}

/// Verifies that a triple ?? chain returns the first non-null value in a chain of three null-coalesced variables.
#[test]
fn test_null_coalesce_chain() {
    let out = compile_and_run(
        r#"<?php
$a = null;
$b = null;
$c = "found";
echo $a ?? $b ?? $c;
"#,
    );
    assert_eq!(out, "found");
}

/// Verifies that ?? returns the non-null float variable when provided.
#[test]
fn test_null_coalesce_float() {
    let out = compile_and_run(
        r#"<?php
$x = 3.14;
echo $x ?? 0.0;
"#,
    );
    assert_eq!(out, "3.14");
}

/// Verifies that ?? returns the right-hand side when the float variable is null.
#[test]
fn test_null_coalesce_null_to_float() {
    let out = compile_and_run(
        r#"<?php
$x = null;
echo $x ?? 2.718;
"#,
    );
    assert_eq!(out, "2.718");
}

/// Verifies that a null-coalesced float participates correctly in arithmetic and is passed to a builtin function (round).
#[test]
fn test_null_coalesce_float_in_calc() {
    let out = compile_and_run(
        r#"<?php
$pi = null;
$val = $pi ?? 3.14159;
echo round($val * 2, 4);
"#,
    );
    assert_eq!(out, "6.2832");
}

/// Verifies that a null-coalesced result survives being passed as an argument to a builtin function (round) and used in a string concatenation.
#[test]
fn test_null_coalesce_result_survives_nested_function_calls_in_concat() {
    let out = compile_and_run(
        r#"<?php
function fallback_pi($x) {
    return $x ?? 3.14159;
}

echo round(fallback_pi(2), 1) . "|" . round(fallback_pi(null), 4);
"#,
    );
    assert_eq!(out, "2|3.1416");
}

// ===== Feature 3: Bitwise operators =====

/// Verifies bitwise AND of two small positive integers (5 & 3 = 1).
#[test]
fn test_bitwise_and() {
    let out = compile_and_run("<?php echo 5 & 3;");
    assert_eq!(out, "1");
}

/// Verifies bitwise OR of two small positive integers (5 | 3 = 7).
#[test]
fn test_bitwise_or() {
    let out = compile_and_run("<?php echo 5 | 3;");
    assert_eq!(out, "7");
}

/// Verifies bitwise XOR of two small positive integers (5 ^ 3 = 6).
#[test]
fn test_bitwise_xor() {
    let out = compile_and_run("<?php echo 5 ^ 3;");
    assert_eq!(out, "6");
}

/// Verifies bitwise NOT of zero (~0 = -1 in two's complement).
#[test]
fn test_bitwise_not() {
    let out = compile_and_run("<?php echo ~0;");
    assert_eq!(out, "-1");
}

/// Verifies left shift of 1 by 4 positions (1 << 4 = 16).
#[test]
fn test_shift_left() {
    let out = compile_and_run("<?php echo 1 << 4;");
    assert_eq!(out, "16");
}

/// Verifies right shift of 16 by 2 positions (16 >> 2 = 4).
#[test]
fn test_shift_right() {
    let out = compile_and_run("<?php echo 16 >> 2;");
    assert_eq!(out, "4");
}

/// Verifies a combined expression using bitwise AND then OR: (255 & 15) | 48 = 63.
#[test]
fn test_bitwise_combined() {
    let out = compile_and_run("<?php echo (255 & 15) | 48;");
    assert_eq!(out, "63");
}

/// Verifies bitwise NOT of a positive byte (~255 = -256 in two's complement).
#[test]
fn test_bitwise_not_positive() {
    let out = compile_and_run("<?php echo ~255;");
    assert_eq!(out, "-256");
}

/// Verifies left shift doubles as multiplication by powers of two (3 << 3 = 24).
#[test]
fn test_shift_left_multiply() {
    let out = compile_and_run("<?php echo 3 << 3;");
    assert_eq!(out, "24");
}

/// Verifies that right shift of a negative integer performs arithmetic shift preserving the sign (-16 >> 2 = -4).
#[test]
fn test_shift_right_negative() {
    let out = compile_and_run("<?php echo -16 >> 2;");
    assert_eq!(out, "-4");
}

// ===== Feature 4: Spaceship operator <=> =====

/// Verifies that the spaceship operator returns -1 when the left operand is less than the right (1 <=> 2).
#[test]
fn test_spaceship_less() {
    let out = compile_and_run("<?php echo 1 <=> 2;");
    assert_eq!(out, "-1");
}

/// Verifies that the spaceship operator returns 0 when both operands are equal (2 <=> 2).
#[test]
fn test_spaceship_equal() {
    let out = compile_and_run("<?php echo 2 <=> 2;");
    assert_eq!(out, "0");
}

/// Verifies that the spaceship operator returns 1 when the left operand is greater than the right (3 <=> 2).
#[test]
fn test_spaceship_greater() {
    let out = compile_and_run("<?php echo 3 <=> 2;");
    assert_eq!(out, "1");
}

/// Verifies that the spaceship operator returns -1 for a negative left operand (-5 <=> 5).
#[test]
fn test_spaceship_negative() {
    let out = compile_and_run("<?php echo -5 <=> 5;");
    assert_eq!(out, "-1");
}

// ===== Feature 5: Heredoc / Nowdoc strings =====

/// Verifies basic heredoc syntax with a single line of content.
#[test]
fn test_heredoc_basic() {
    let out = compile_and_run("<?php\necho <<<EOT\nHello World\nEOT;\n");
    assert_eq!(out, "Hello World");
}

/// Verifies that a heredoc correctly preserves newlines between multiple lines.
#[test]
fn test_heredoc_multiline() {
    let out = compile_and_run("<?php\necho <<<EOT\nLine 1\nLine 2\nLine 3\nEOT;\n");
    assert_eq!(out, "Line 1\nLine 2\nLine 3");
}

/// Verifies that a heredoc processes escape sequences like \t and \n.
#[test]
fn test_heredoc_escapes() {
    let out = compile_and_run("<?php\necho <<<EOT\nHello\\tWorld\\n\nEOT;\n");
    assert_eq!(out, "Hello\tWorld\n");
}

/// Verifies basic nowdoc syntax (single-quoted heredoc) with a single line of content.
#[test]
fn test_nowdoc_basic() {
    let out = compile_and_run("<?php\necho <<<'EOT'\nHello World\nEOT;\n");
    assert_eq!(out, "Hello World");
}

/// Verifies that a nowdoc does NOT process escape sequences and preserves backslashes literally.
#[test]
fn test_nowdoc_no_escapes() {
    let out = compile_and_run("<?php\necho <<<'EOT'\nHello\\tWorld\nEOT;\n");
    assert_eq!(out, "Hello\\tWorld");
}

/// Verifies that a heredoc interpolates a single variable into the output.
#[test]
fn test_heredoc_interpolation() {
    let out =
        compile_and_run("<?php\n$name = \"World\";\n$s = <<<EOT\nHello $name\nEOT;\necho $s;\n");
    assert_eq!(out, "Hello World");
}

/// Verifies that a heredoc correctly interpolates two variables on the same line.
#[test]
fn test_heredoc_interpolation_multiple_vars() {
    let out = compile_and_run(
        "<?php\n$first = \"Hello\";\n$second = \"World\";\necho <<<EOT\n$first $second\nEOT;\n",
    );
    assert_eq!(out, "Hello World");
}

/// Verifies that a heredoc interpolates a variable across multiple lines of output.
#[test]
fn test_heredoc_interpolation_multiline() {
    let out = compile_and_run(
        "<?php\n$name = \"Alice\";\necho <<<EOT\nHello $name\nWelcome $name\nEOT;\n",
    );
    assert_eq!(out, "Hello Alice\nWelcome Alice");
}

/// Verifies that a nowdoc does NOT interpolate variables and keeps the variable name as a literal string.
#[test]
fn test_nowdoc_no_interpolation() {
    let out = compile_and_run("<?php\n$name = \"World\";\necho <<<'EOT'\nHello $name\nEOT;\n");
    assert_eq!(out, "Hello $name");
}

/// Verifies that a heredoc allows escaping a dollar sign with a backslash so it appears literally in output.
#[test]
fn test_heredoc_escaped_dollar() {
    let out = compile_and_run("<?php\necho <<<EOT\nPrice is \\$100\nEOT;\n");
    assert_eq!(out, "Price is $100");
}

/// Verifies a heredoc passed as a function argument closes when its label is immediately
/// followed by `)` (PHP closes on any non-identifier char after the label).
#[test]
fn test_heredoc_closer_as_call_argument() {
    let out = compile_and_run("<?php\nfunction f($s) { return $s; }\necho f(<<<EOT\nhello\nEOT);\n");
    assert_eq!(out, "hello");
}

/// Verifies a heredoc whose label is followed by ` . "..."` closes and concatenates,
/// covering closer terminators other than `;`/newline.
#[test]
fn test_heredoc_closer_followed_by_concat() {
    let out = compile_and_run("<?php\necho <<<EOT\nhi\nEOT . \"!\";\n");
    assert_eq!(out, "hi!");
}

/// Verifies PHP 7.3+ flexible-heredoc indentation: the closing marker's leading whitespace
/// is stripped from every body line.
#[test]
fn test_heredoc_flexible_indentation() {
    let out = compile_and_run("<?php\necho <<<EOT\n    line1\n    line2\n    EOT;\n");
    assert_eq!(out, "line1\nline2");
}

/// Verifies the closing label is only recognized at the start of a line: a label appearing
/// mid-line (`a EOT b`) is body content, not a closer.
#[test]
fn test_heredoc_label_substring_midline_not_closer() {
    let out = compile_and_run("<?php\n$s = <<<EOT\na EOT b\nEOT;\necho $s;\n");
    assert_eq!(out, "a EOT b");
}

