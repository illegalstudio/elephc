use crate::support::*;

// --- Strict comparison (=== / !==) ---

#[test]
fn test_strict_eq_int_same() {
    let out = compile_and_run("<?php echo 1 === 1;");
    assert_eq!(out, "1");
}

#[test]
fn test_strict_eq_int_different() {
    let out = compile_and_run("<?php echo 1 === 2;");
    assert_eq!(out, "");
}

#[test]
fn test_strict_neq_int_same() {
    let out = compile_and_run("<?php echo 1 !== 1;");
    assert_eq!(out, "");
}

#[test]
fn test_strict_neq_int_different() {
    let out = compile_and_run("<?php echo 1 !== 2;");
    assert_eq!(out, "1");
}

#[test]
fn test_strict_eq_int_vs_bool() {
    // 1 === true should be false (different types)
    let out = compile_and_run("<?php echo 1 === true;");
    assert_eq!(out, "");
}

#[test]
fn test_strict_neq_int_vs_bool() {
    // 1 !== true should be true (different types)
    let out = compile_and_run("<?php echo 1 !== true;");
    assert_eq!(out, "1");
}

#[test]
fn test_strict_eq_int_vs_string() {
    // 1 === "1" should be false (different types)
    let out = compile_and_run("<?php echo 1 === \"1\";");
    assert_eq!(out, "");
}

#[test]
fn test_strict_eq_string_same() {
    let out = compile_and_run("<?php echo \"hello\" === \"hello\";");
    assert_eq!(out, "1");
}

#[test]
fn test_strict_eq_string_different() {
    let out = compile_and_run("<?php echo \"hello\" === \"world\";");
    assert_eq!(out, "");
}

#[test]
fn test_strict_neq_string() {
    let out = compile_and_run("<?php echo \"abc\" !== \"def\";");
    assert_eq!(out, "1");
}

#[test]
fn test_strict_eq_bool_true() {
    let out = compile_and_run("<?php echo true === true;");
    assert_eq!(out, "1");
}

#[test]
fn test_strict_eq_bool_false() {
    let out = compile_and_run("<?php echo false === false;");
    assert_eq!(out, "1");
}

#[test]
fn test_strict_eq_bool_mixed() {
    let out = compile_and_run("<?php echo true === false;");
    assert_eq!(out, "");
}

#[test]
fn test_strict_eq_null() {
    let out = compile_and_run("<?php echo null === null;");
    assert_eq!(out, "1");
}

#[test]
fn test_strict_eq_null_vs_int() {
    // null === 0 should be false
    let out = compile_and_run("<?php echo null === 0;");
    assert_eq!(out, "");
}

#[test]
fn test_strict_eq_null_vs_false() {
    // null === false should be false (different types)
    let out = compile_and_run("<?php echo null === false;");
    assert_eq!(out, "");
}

#[test]
fn test_strict_eq_float_same() {
    let out = compile_and_run("<?php echo 3.14 === 3.14;");
    assert_eq!(out, "1");
}

#[test]
fn test_strict_eq_float_different() {
    let out = compile_and_run("<?php echo 3.14 === 2.71;");
    assert_eq!(out, "");
}

#[test]
fn test_strict_eq_float_vs_int() {
    // 1.0 === 1 should be false (different types)
    let out = compile_and_run("<?php echo 1.0 === 1;");
    assert_eq!(out, "");
}

#[test]
fn test_strict_eq_in_if() {
    let out = compile_and_run(
        r#"<?php
$x = 5;
if ($x === 5) {
    echo "yes";
} else {
    echo "no";
}
"#,
    );
    assert_eq!(out, "yes");
}

#[test]
fn test_strict_neq_in_if() {
    let out = compile_and_run(
        r#"<?php
$x = "hello";
if ($x !== "world") {
    echo "different";
} else {
    echo "same";
}
"#,
    );
    assert_eq!(out, "different");
}

#[test]
fn test_strict_eq_string_variables() {
    let out = compile_and_run(
        r#"<?php
$a = "test";
$b = "test";
echo $a === $b;
"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_strict_neq_string_variables() {
    let out = compile_and_run(
        r#"<?php
$a = "foo";
$b = "bar";
echo $a !== $b;
"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_strict_eq_side_effects_preserved() {
    // Both operands must be evaluated even when types differ
    let out = compile_and_run(
        r#"<?php
function effect() { echo "X"; return 1; }
$r = 1.0 === effect();
echo $r;
"#,
    );
    assert_eq!(out, "X");
}

#[test]
fn test_strict_eq_assign_result() {
    let out = compile_and_run(
        r#"<?php
$x = 1 === 1;
echo $x;
"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_strict_neq_assign_result() {
    let out = compile_and_run(
        r#"<?php
$x = 1 !== 2;
echo $x;
"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_strict_compare_mixed_uses_payload_type_and_value() {
    let out = compile_and_run(
        r#"<?php
$map = [
    "int_a" => 42,
    "int_b" => 42,
    "int_c" => 7,
    "str_a" => "42",
    "str_b" => "42",
    "bool_t" => true,
];
echo $map["int_a"] === $map["int_b"] ? "1" : "0";
echo $map["int_a"] === $map["int_c"] ? "1" : "0";
echo $map["int_a"] === $map["str_a"] ? "1" : "0";
echo $map["str_a"] === $map["str_b"] ? "1" : "0";
echo $map["int_a"] !== $map["str_a"] ? "1" : "0";
echo $map["bool_t"] === true ? "1" : "0";
"#,
    );
    assert_eq!(out, "100111");
}

// --- Include / Require ---

#[test]
fn test_include_basic() {
    let out = compile_and_run_files(
        &[
            ("main.php", "<?php include 'helper.php'; echo greet();"),
            ("helper.php", "<?php function greet() { return \"hello\"; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "hello");
}

#[test]
fn test_require_basic() {
    let out = compile_and_run_files(
        &[
            ("main.php", "<?php require 'math.php'; echo add(3, 4);"),
            ("math.php", "<?php function add($a, $b) { return $a + $b; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "7");
}

#[test]
fn test_include_with_parens() {
    let out = compile_and_run_files(
        &[
            ("main.php", "<?php include('helper.php'); echo greet();"),
            ("helper.php", "<?php function greet() { return \"hi\"; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "hi");
}

#[test]
fn test_include_top_level_code() {
    // Top-level code in included file executes at the include point
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php echo \"before\"; include 'mid.php'; echo \"after\";",
            ),
            ("mid.php", "<?php echo \"middle\";"),
        ],
        "main.php",
    );
    assert_eq!(out, "beforemiddleafter");
}

#[test]
fn test_include_once() {
    // include_once should only include the file once
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
include_once 'counter.php';
include_once 'counter.php';
echo $x;
"#,
            ),
            ("counter.php", "<?php $x = 42;"),
        ],
        "main.php",
    );
    assert_eq!(out, "42");
}

#[test]
fn test_require_once() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
require_once 'lib.php';
require_once 'lib.php';
echo double(5);
"#,
            ),
            ("lib.php", "<?php function double($n) { return $n * 2; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "10");
}

#[test]
fn test_include_nested() {
    // a.php includes b.php which includes c.php
    let out = compile_and_run_files(
        &[
            ("main.php", "<?php include 'a.php'; echo c_func();"),
            ("a.php", "<?php include 'b.php';"),
            ("b.php", "<?php include 'c.php';"),
            ("c.php", "<?php function c_func() { return \"deep\"; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "deep");
}

#[test]
fn test_include_subdirectory() {
    let out = compile_and_run_files(
        &[
            ("main.php", "<?php include 'lib/utils.php'; echo greet();"),
            (
                "lib/utils.php",
                "<?php function greet() { return \"from lib\"; }",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "from lib");
}

#[test]
fn test_include_variables_shared_scope() {
    // Variables from included file are in the same scope
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
$prefix = "Hello";
include 'greet.php';
"#,
            ),
            ("greet.php", "<?php echo $prefix . \" World\";"),
        ],
        "main.php",
    );
    assert_eq!(out, "Hello World");
}

#[test]
fn test_include_multiple_files() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
include 'a.php';
include 'b.php';
echo add(1, 2) . " " . mul(3, 4);
"#,
            ),
            ("a.php", "<?php function add($x, $y) { return $x + $y; }"),
            ("b.php", "<?php function mul($x, $y) { return $x * $y; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "3 12");
}

#[test]
fn test_circular_include_error() {
    assert!(compile_files_fails(
        &[
            ("main.php", "<?php include 'a.php';"),
            ("a.php", "<?php include 'b.php';"),
            ("b.php", "<?php include 'a.php';"),
        ],
        "main.php"
    ));
}

#[test]
fn test_require_missing_file_error() {
    assert!(compile_files_fails(
        &[("main.php", "<?php require 'nonexistent.php';"),],
        "main.php"
    ));
}

// --- Division returns float ---

#[test]
fn test_int_division_returns_float() {
    let out = compile_and_run("<?php echo 10 / 3;");
    assert_eq!(out, "3.3333333333333");
}

#[test]
fn test_int_division_exact() {
    // Even exact division returns float-formatted output
    let out = compile_and_run("<?php echo 10 / 2;");
    assert_eq!(out, "5");
}

#[test]
fn test_division_assign_updates_type() {
    let out = compile_and_run("<?php $x = 10; $x /= 3; echo $x;");
    assert_eq!(out, "3.3333333333333");
}

#[test]
fn test_division_in_expression() {
    let out = compile_and_run("<?php echo 1 / 3 + 1 / 3 + 1 / 3;");
    assert_eq!(out, "1");
}

#[test]
fn test_intdiv_still_returns_int() {
    let out = compile_and_run("<?php echo intdiv(10, 3);");
    assert_eq!(out, "3");
}

#[test]
fn test_intdiv_exact() {
    let out = compile_and_run("<?php echo intdiv(10, 5);");
    assert_eq!(out, "2");
}

#[test]
fn test_intdiv_negative() {
    let out = compile_and_run("<?php echo intdiv(-7, 2);");
    assert_eq!(out, "-3");
}

// --- INF, NAN, is_nan, is_finite, is_infinite ---

#[test]
fn test_inf_constant() {
    let out = compile_and_run("<?php echo INF;");
    assert_eq!(out, "INF");
}

#[test]
fn test_nan_constant() {
    let out = compile_and_run("<?php echo NAN;");
    assert_eq!(out, "NAN");
}

#[test]
fn test_negative_inf() {
    let out = compile_and_run("<?php echo -INF;");
    assert_eq!(out, "-INF");
}

#[test]
fn test_is_nan_true() {
    let out = compile_and_run("<?php echo is_nan(NAN);");
    assert_eq!(out, "1");
}

#[test]
fn test_is_nan_false() {
    let out = compile_and_run("<?php echo is_nan(42.0);");
    assert_eq!(out, "");
}

#[test]
fn test_is_nan_int() {
    let out = compile_and_run("<?php echo is_nan(0);");
    assert_eq!(out, "");
}

#[test]
fn test_is_infinite_true() {
    let out = compile_and_run("<?php echo is_infinite(INF);");
    assert_eq!(out, "1");
}

#[test]
fn test_is_infinite_neg_inf() {
    let out = compile_and_run("<?php echo is_infinite(-INF);");
    assert_eq!(out, "1");
}

#[test]
fn test_is_infinite_false() {
    let out = compile_and_run("<?php echo is_infinite(42.0);");
    assert_eq!(out, "");
}

#[test]
fn test_is_finite_true() {
    let out = compile_and_run("<?php echo is_finite(42.0);");
    assert_eq!(out, "1");
}

#[test]
fn test_is_finite_inf() {
    let out = compile_and_run("<?php echo is_finite(INF);");
    assert_eq!(out, "");
}

#[test]
fn test_is_finite_nan() {
    let out = compile_and_run("<?php echo is_finite(NAN);");
    assert_eq!(out, "");
}

#[test]
fn test_inf_arithmetic() {
    let out = compile_and_run("<?php echo INF + 1;");
    assert_eq!(out, "INF");
}

#[test]
fn test_division_by_zero_inf() {
    let out = compile_and_run("<?php echo 1.0 / 0.0;");
    assert_eq!(out, "INF");
}

