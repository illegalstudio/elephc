//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of control flow ternary, including ternary true, ternary false, and ternary integer.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Tests ternary true branch using equality comparison that evaluates to true.
#[test]
fn test_ternary_true() {
    let out = compile_and_run("<?php echo 1 == 1 ? \"yes\" : \"no\";");
    assert_eq!(out, "yes");
}

/// Tests ternary false branch using equality comparison that evaluates to false.
#[test]
fn test_ternary_false() {
    let out = compile_and_run("<?php echo 1 == 2 ? \"yes\" : \"no\";");
    assert_eq!(out, "no");
}

/// Tests ternary with integer comparison and integer branches, selecting the larger of two values.
#[test]
fn test_ternary_int() {
    let out = compile_and_run("<?php $x = 3; $y = 7; echo $x > $y ? $x : $y;");
    assert_eq!(out, "7");
}

/// Tests ternary with mixed types when array_pop returns null on empty array.
#[test]
fn test_ternary_mixed_types_str_vs_int() {
    let out = compile_and_run(
        "<?php $a = [1]; array_pop($a); $v = array_pop($a); echo is_null($v) ? \"null\" : \"has value\";",
    );
    assert_eq!(out, "null");
}

/// Tests ternary with int condition (0) selecting false branch.
#[test]
fn test_ternary_mixed_types_then_branch_str() {
    let out = compile_and_run("<?php $x = 0; echo $x ? \"yes\" : \"no\";");
    assert_eq!(out, "no");
}

/// Tests ternary with bool true condition selecting int branch over string.
#[test]
fn test_ternary_int_string() {
    let out = compile_and_run(
        r#"<?php
$x = true;
echo $x ? 42 : "none";
"#,
    );
    assert_eq!(out, "42");
}

/// Tests ternary with bool false condition selecting string branch over int (result is "0").
#[test]
fn test_ternary_string_int() {
    let out = compile_and_run(
        r#"<?php
$x = false;
echo $x ? "yes" : 0;
"#,
    );
    assert_eq!(out, "0");
}

/// Tests ternary with bool true condition selecting string branch over another string.
#[test]
fn test_ternary_string_string() {
    let out = compile_and_run(
        r#"<?php
$x = true;
echo $x ? "hello" : "world";
"#,
    );
    assert_eq!(out, "hello");
}

/// Tests ternary with bool true condition selecting int branch over another int.
#[test]
fn test_ternary_int_int() {
    let out = compile_and_run(
        r#"<?php
$x = true;
echo $x ? 1 : 0;
"#,
    );
    assert_eq!(out, "1");
}

/// Tests ternary nested inside string concatenation with int and string result types.
#[test]
fn test_ternary_mixed_in_concat() {
    let out = compile_and_run(
        r#"<?php
$count = 5;
echo "Items: " . ($count > 0 ? $count : "none");
"#,
    );
    assert_eq!(out, "Items: 5");
}

/// Tests ternary with bool false condition selecting string branch over float.
#[test]
fn test_ternary_float_string() {
    let out = compile_and_run(
        r#"<?php
$x = false;
echo $x ? 3.14 : "zero";
"#,
    );
    assert_eq!(out, "zero");
}

/// Tests nested ternary with int condition (0) and string/int branches in both outer and inner ternary.
#[test]
fn test_ternary_nested_mixed() {
    let out = compile_and_run(
        r#"<?php
$a = 0;
echo $a ? "yes" : ($a === 0 ? "zero" : "no");
"#,
    );
    assert_eq!(out, "zero");
}

/// Tests ternary with variable as the true branch result, condition is bool true.
#[test]
fn test_ternary_variable_string() {
    let out = compile_and_run(
        r#"<?php
$name = "Alice";
$greeting = true ? $name : "nobody";
echo $greeting;
"#,
    );
    assert_eq!(out, "Alice");
}

/// Tests ternary with user-defined function call as the true branch result.
#[test]
fn test_ternary_function_result() {
    let out = compile_and_run(
        r#"<?php
function get_name() { return "Bob"; }
echo true ? get_name() : "default";
"#,
    );
    assert_eq!(out, "Bob");
}

/// Tests ternary with comparison expression as condition and int variable vs string variable as branches.
#[test]
fn test_ternary_variable_int_vs_string() {
    let out = compile_and_run(
        r#"<?php
$count = 5;
$label = "none";
echo ($count > 0) ? $count : $label;
"#,
    );
    assert_eq!(out, "5");
}

/// Tests ternary with method call on object as the true branch result.
#[test]
fn test_ternary_method_call_result() {
    let out = compile_and_run(
        r#"<?php
class Box { public $val;
    public function __construct($v) { $this->val = $v; }
    public function get() { return $this->val; }
}
$b = new Box("hello");
echo true ? $b->get() : "fallback";
"#,
    );
    assert_eq!(out, "hello");
}

/// Regression: ternary branches that index string-typed arrays must keep the
/// result string-typed. The branch merge type was inferred syntactically (no
/// element-type lookup), defaulting `$arr[$i]` to int and coercing the chosen
/// string through `str_to_i` to "0" (broke `jdmonthname(.., CAL_MONTH_JEWISH)`).
#[test]
fn test_ternary_string_array_index_branches() {
    let out = compile_and_run(
        r#"<?php
$leap = ["", "Tishri", "Heshvan"];
$reg  = ["", "Apple", "Banana"];
$month = 1;
$isLeap = true;
echo $isLeap ? $leap[$month] : $reg[$month];
echo ",";
echo $isLeap ? $leap[1] : $reg[1];
echo ",";
$isLeap = false;
echo $isLeap ? $leap[2] : $reg[2];
"#,
    );
    assert_eq!(out, "Tishri,Tishri,Banana");
}

/// Regression: ternary branches reading string-typed object properties must
/// keep the result string-typed, mirroring the array-index branch-merge fix.
#[test]
fn test_ternary_string_property_branches() {
    let out = compile_and_run(
        r#"<?php
class Names {
    public string $a = "Tishri";
    public string $b = "Heshvan";
}
$n = new Names();
$pick = true;
echo $pick ? $n->a : $n->b;
"#,
    );
    assert_eq!(out, "Tishri");
}
