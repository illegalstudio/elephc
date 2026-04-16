use crate::support::*;

// ===== Feature 1: Default parameter values =====

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

#[test]
fn test_null_coalesce_literal_null() {
    let out = compile_and_run(
        r#"<?php
echo null ?? "fallback";
"#,
    );
    assert_eq!(out, "fallback");
}

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

#[test]
fn test_bitwise_and() {
    let out = compile_and_run("<?php echo 5 & 3;");
    assert_eq!(out, "1");
}

#[test]
fn test_bitwise_or() {
    let out = compile_and_run("<?php echo 5 | 3;");
    assert_eq!(out, "7");
}

#[test]
fn test_bitwise_xor() {
    let out = compile_and_run("<?php echo 5 ^ 3;");
    assert_eq!(out, "6");
}

#[test]
fn test_bitwise_not() {
    let out = compile_and_run("<?php echo ~0;");
    assert_eq!(out, "-1");
}

#[test]
fn test_shift_left() {
    let out = compile_and_run("<?php echo 1 << 4;");
    assert_eq!(out, "16");
}

#[test]
fn test_shift_right() {
    let out = compile_and_run("<?php echo 16 >> 2;");
    assert_eq!(out, "4");
}

#[test]
fn test_bitwise_combined() {
    let out = compile_and_run("<?php echo (255 & 15) | 48;");
    assert_eq!(out, "63");
}

#[test]
fn test_bitwise_not_positive() {
    let out = compile_and_run("<?php echo ~255;");
    assert_eq!(out, "-256");
}

#[test]
fn test_shift_left_multiply() {
    let out = compile_and_run("<?php echo 3 << 3;");
    assert_eq!(out, "24");
}

#[test]
fn test_shift_right_negative() {
    // Arithmetic shift preserves sign
    let out = compile_and_run("<?php echo -16 >> 2;");
    assert_eq!(out, "-4");
}

// ===== Feature 4: Spaceship operator <=> =====

#[test]
fn test_spaceship_less() {
    let out = compile_and_run("<?php echo 1 <=> 2;");
    assert_eq!(out, "-1");
}

#[test]
fn test_spaceship_equal() {
    let out = compile_and_run("<?php echo 2 <=> 2;");
    assert_eq!(out, "0");
}

#[test]
fn test_spaceship_greater() {
    let out = compile_and_run("<?php echo 3 <=> 2;");
    assert_eq!(out, "1");
}

#[test]
fn test_spaceship_negative() {
    let out = compile_and_run("<?php echo -5 <=> 5;");
    assert_eq!(out, "-1");
}

// ===== Feature 5: Heredoc / Nowdoc strings =====

#[test]
fn test_heredoc_basic() {
    let out = compile_and_run("<?php\necho <<<EOT\nHello World\nEOT;\n");
    assert_eq!(out, "Hello World");
}

#[test]
fn test_heredoc_multiline() {
    let out = compile_and_run("<?php\necho <<<EOT\nLine 1\nLine 2\nLine 3\nEOT;\n");
    assert_eq!(out, "Line 1\nLine 2\nLine 3");
}

#[test]
fn test_heredoc_escapes() {
    let out = compile_and_run("<?php\necho <<<EOT\nHello\\tWorld\\n\nEOT;\n");
    assert_eq!(out, "Hello\tWorld\n");
}

#[test]
fn test_nowdoc_basic() {
    let out = compile_and_run("<?php\necho <<<'EOT'\nHello World\nEOT;\n");
    assert_eq!(out, "Hello World");
}

#[test]
fn test_nowdoc_no_escapes() {
    let out = compile_and_run("<?php\necho <<<'EOT'\nHello\\tWorld\nEOT;\n");
    assert_eq!(out, "Hello\\tWorld");
}

#[test]
fn test_heredoc_interpolation() {
    let out =
        compile_and_run("<?php\n$name = \"World\";\n$s = <<<EOT\nHello $name\nEOT;\necho $s;\n");
    assert_eq!(out, "Hello World");
}

#[test]
fn test_heredoc_interpolation_multiple_vars() {
    let out = compile_and_run(
        "<?php\n$first = \"Hello\";\n$second = \"World\";\necho <<<EOT\n$first $second\nEOT;\n",
    );
    assert_eq!(out, "Hello World");
}

#[test]
fn test_heredoc_interpolation_multiline() {
    let out = compile_and_run(
        "<?php\n$name = \"Alice\";\necho <<<EOT\nHello $name\nWelcome $name\nEOT;\n",
    );
    assert_eq!(out, "Hello Alice\nWelcome Alice");
}

#[test]
fn test_nowdoc_no_interpolation() {
    let out = compile_and_run("<?php\n$name = \"World\";\necho <<<'EOT'\nHello $name\nEOT;\n");
    assert_eq!(out, "Hello $name");
}

#[test]
fn test_heredoc_escaped_dollar() {
    let out = compile_and_run("<?php\necho <<<EOT\nPrice is \\$100\nEOT;\n");
    assert_eq!(out, "Price is $100");
}

