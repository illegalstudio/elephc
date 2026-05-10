//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of control flow, assignments compound and values, including pre increment, post increment, and pre decrement.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

#[test]
fn test_pre_increment() {
    let out = compile_and_run("<?php $i = 1; $k = ++$i; echo $i . \" \" . $k;");
    assert_eq!(out, "2 2");
}

#[test]
fn test_post_increment() {
    let out = compile_and_run("<?php $i = 1; $k = $i++; echo $i . \" \" . $k;");
    assert_eq!(out, "2 1");
}

#[test]
fn test_pre_decrement() {
    let out = compile_and_run("<?php $i = 5; $k = --$i; echo $i . \" \" . $k;");
    assert_eq!(out, "4 4");
}

#[test]
fn test_post_decrement() {
    let out = compile_and_run("<?php $i = 5; $k = $i--; echo $i . \" \" . $k;");
    assert_eq!(out, "4 5");
}

#[test]
fn test_plus_assign() {
    let out = compile_and_run("<?php $x = 10; $x += 5; echo $x;");
    assert_eq!(out, "15");
}

#[test]
fn test_minus_assign() {
    let out = compile_and_run("<?php $x = 10; $x -= 3; echo $x;");
    assert_eq!(out, "7");
}

#[test]
fn test_star_assign() {
    let out = compile_and_run("<?php $x = 6; $x *= 7; echo $x;");
    assert_eq!(out, "42");
}

#[test]
fn test_slash_assign() {
    let out = compile_and_run("<?php $x = 84; $x /= 2; echo $x;");
    assert_eq!(out, "42");
}

#[test]
fn test_percent_assign() {
    let out = compile_and_run("<?php $x = 10; $x %= 3; echo $x;");
    assert_eq!(out, "1");
}

#[test]
fn test_dot_assign() {
    let out = compile_and_run("<?php $s = \"hello\"; $s .= \" world\"; echo $s;");
    assert_eq!(out, "hello world");
}

#[test]
fn test_pow_assign() {
    let out = compile_and_run("<?php $x = 2; $x **= 3; echo $x;");
    assert_eq!(out, "8");
}

#[test]
fn test_bitwise_compound_assignments() {
    let out = compile_and_run(
        r#"<?php
$x = 6;
$x &= 3;
echo $x . ",";
$x = 4;
$x |= 1;
echo $x . ",";
$x = 7;
$x ^= 3;
echo $x . ",";
$x = 1;
$x <<= 5;
echo $x . ",";
$x = 64;
$x >>= 3;
echo $x;
"#,
    );
    assert_eq!(out, "2,5,4,32,8");
}

#[test]
fn test_assignment_expression_returns_assigned_value() {
    let out = compile_and_run("<?php echo ($x = 5); echo ':'; echo $x;");
    assert_eq!(out, "5:5");
}

#[test]
fn test_string_assignment_expression_returns_assigned_value() {
    let out = compile_and_run(r#"<?php echo ($s = "hi"); echo ":"; echo $s;"#);
    assert_eq!(out, "hi:hi");
}

#[test]
fn test_assignment_expression_word_and_uses_php_precedence() {
    let out = compile_and_run(
        r#"<?php
$x = true and false;
echo $x ? "T" : "F";
"#,
    );
    assert_eq!(out, "T");
}

#[test]
fn test_assignment_expression_in_condition_updates_local() {
    let out = compile_and_run(
        r#"<?php
if ($x = 3) {
    echo $x;
}
"#,
    );
    assert_eq!(out, "3");
}

#[test]
fn test_compound_assignment_expression_returns_new_value() {
    let out = compile_and_run("<?php $x = 4; echo ($x += 3); echo ':'; echo $x;");
    assert_eq!(out, "7:7");
}

#[test]
fn test_null_coalesce_assignment_expression_returns_existing_mixed_value() {
    let out = compile_and_run(
        r#"<?php
function maybe(bool $flag): mixed {
    return $flag ? 7 : null;
}
$x = maybe(true);
echo ($x ??= 5);
echo ":";
echo $x;
"#,
    );
    assert_eq!(out, "7:7");
}

#[test]
fn test_null_coalesce_assignment_expression_returns_default_for_mixed_null() {
    let out = compile_and_run(
        r#"<?php
function maybe(bool $flag): mixed {
    return $flag ? 7 : null;
}
$x = maybe(false);
echo ($x ??= 5);
echo ":";
echo $x;
"#,
    );
    assert_eq!(out, "5:5");
}

#[test]
fn test_array_assignment_expression_returns_assigned_value() {
    let out = compile_and_run("<?php $items = [1, 2]; echo ($items[1] = 9); echo ':' . $items[1];");
    assert_eq!(out, "9:9");
}

#[test]
fn test_array_assignment_expression_variable_index_returns_assigned_value() {
    let out = compile_and_run("<?php $items = [1, 2]; $i = 1; echo ($items[$i] = 9); echo ':' . $items[1];");
    assert_eq!(out, "9:9");
}

#[test]
fn test_array_compound_assignment_expression_returns_new_value() {
    let out = compile_and_run("<?php $items = [3]; echo ($items[0] += 4); echo ':' . $items[0];");
    assert_eq!(out, "7:7");
}

#[test]
fn test_assoc_array_assignment_expression_returns_assigned_value() {
    let out = compile_and_run(
        r#"<?php
$items = ["count" => 2];
echo ($items["count"] += 5);
echo ":" . $items["count"];
"#,
    );
    assert_eq!(out, "7:7");
}

#[test]
fn test_array_null_coalesce_assignment_expression_returns_slot_value() {
    let out = compile_and_run(
        r#"<?php
$items = [5, 8];
echo ($items[0] ??= 5);
echo ":";
echo ($items[1] ??= 6);
echo ":" . $items[0] . ":" . $items[1];
"#,
    );
    assert_eq!(out, "5:8:5:8");
}

#[test]
fn test_property_assignment_expression_returns_assigned_value() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public $value = 1;
}
$box = new Box();
echo ($box->value += 4);
echo ":" . $box->value;
"#,
    );
    assert_eq!(out, "5:5");
}

#[test]
fn test_property_array_assignment_expression_returns_assigned_value() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public $items = [2, 4];
}
$box = new Box();
echo ($box->items[1] *= 3);
echo ":" . $box->items[1];
"#,
    );
    assert_eq!(out, "12:12");
}

#[test]
fn test_static_property_assignment_expression_returns_assigned_value() {
    let out = compile_and_run(
        r#"<?php
class Registry {
    public static $count = 10;
}
echo (Registry::$count += 5);
echo ":" . Registry::$count;
"#,
    );
    assert_eq!(out, "15:15");
}

#[test]
fn test_static_property_array_assignment_expression_returns_assigned_value() {
    let out = compile_and_run(
        r#"<?php
class Registry {
    public static $items = [3, 5];
}
echo (Registry::$items[0] += 3);
echo ":" . Registry::$items[0];
"#,
    );
    assert_eq!(out, "6:6");
}

#[test]
fn test_static_property_null_coalesce_assignment_expression_returns_value() {
    let out = compile_and_run(
        r#"<?php
class Registry {
    public static ?int $value = null;
}
echo (Registry::$value ??= 6);
echo ":" . Registry::$value;
"#,
    );
    assert_eq!(out, "6:6");
}

