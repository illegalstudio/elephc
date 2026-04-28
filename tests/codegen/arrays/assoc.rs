use crate::support::*;

// --- Phase 12: v0.6 — Associative arrays, switch, match ---

#[test]
fn test_assoc_array_basic() {
    let out = compile_and_run(
        r#"<?php
$m = ["name" => "Alice", "city" => "NYC"];
echo $m["name"];
"#,
    );
    assert_eq!(out, "Alice");
}

#[test]
fn test_assoc_array_int_values() {
    let out = compile_and_run(
        r#"<?php
$m = ["a" => 1, "b" => 2, "c" => 3];
echo $m["a"] + $m["b"] + $m["c"];
"#,
    );
    assert_eq!(out, "6");
}

#[test]
fn test_assoc_array_assign() {
    let out = compile_and_run(
        r#"<?php
$m = ["x" => 10];
$m["y"] = 20;
echo $m["x"] + $m["y"];
"#,
    );
    assert_eq!(out, "30");
}

#[test]
fn test_assoc_array_update() {
    let out = compile_and_run(
        r#"<?php
$m = ["key" => "old"];
$m["key"] = "new";
echo $m["key"];
"#,
    );
    assert_eq!(out, "new");
}

#[test]
fn test_assoc_array_integer_and_numeric_string_keys() {
    let out = compile_and_run(
        r#"<?php
$m = [1 => "one", "2" => "two", "01" => "leading"];
echo $m[1] . "|" . $m["1"] . "|" . $m[2] . "|" . $m["01"];
"#,
    );
    assert_eq!(out, "one|one|two|leading");
}

#[test]
fn test_assoc_array_numeric_string_key_boundaries() {
    let out = compile_and_run(
        r#"<?php
$m = [
    "0" => "zero",
    "00" => "double-zero",
    "-1" => "negative",
    "-0" => "negative-zero",
    "9223372036854775807" => "max",
    "9223372036854775808" => "overflow",
    "-9223372036854775808" => "min",
    "-9223372036854775809" => "underflow",
];
echo $m[0] . "|" . $m["00"] . "|" . $m[-1] . "|" . $m["-0"] . "|";
echo $m[PHP_INT_MAX] . "|" . $m["9223372036854775808"] . "|";
echo $m[PHP_INT_MIN] . "|" . $m["-9223372036854775809"];
"#,
    );
    assert_eq!(
        out,
        "zero|double-zero|negative|negative-zero|max|overflow|min|underflow"
    );
}

#[test]
fn test_assoc_array_numeric_string_assignment_updates_integer_key() {
    let out = compile_and_run(
        r#"<?php
$m = [1 => "left"];
$m["1"] = "right";
$m["01"] = "leading";
echo count($m) . ":" . $m[1] . ":" . $m["01"];
"#,
    );
    assert_eq!(out, "2:right:leading");
}

#[test]
fn test_assoc_array_union_keeps_left_duplicate_keys() {
    let out = compile_and_run(
        r#"<?php
$left = ["a" => "left", "b" => "keep"];
$right = ["a" => "right", "c" => "new"];
$result = $left + $right;
echo count($result) . ":";
foreach ($result as $k => $v) {
    echo $k . "=" . $v . " ";
}
"#,
    );
    assert_eq!(out, "3:a=left b=keep c=new ");
}

#[test]
fn test_assoc_array_union_normalizes_numeric_string_duplicates() {
    let out = compile_and_run(
        r#"<?php
$left = [1 => "left"];
$right = ["1" => "right", 2 => "new"];
$result = $left + $right;
echo count($result) . ":" . $result[1] . ":" . $result[2];
"#,
    );
    assert_eq!(out, "2:left:new");
}

#[test]
fn test_assoc_array_union_int_values() {
    let out = compile_and_run(
        r#"<?php
$left = ["a" => 1, "b" => 2];
$right = ["b" => 99, "c" => 3];
$result = $left + $right;
echo $result["a"] + $result["b"] + $result["c"];
"#,
    );
    assert_eq!(out, "6");
}

#[test]
fn test_assoc_array_union_with_assoc_builtin_operands() {
    let out = compile_and_run(
        r#"<?php
$left = array_fill_keys(["a", "b"], 1);
$right = array_combine(["b", "c"], [99, 3]);
$result = $left + $right;
echo $result["a"] + $result["b"] + $result["c"];
"#,
    );
    assert_eq!(out, "5");
}

#[test]
fn test_assoc_array_union_with_key_filter_builtin_operand() {
    let out = compile_and_run(
        r#"<?php
$left = array_diff_key(["a" => 1, "b" => 2], ["a" => 0]);
$right = ["b" => 99, "c" => 3];
$result = $left + $right;
echo $result["b"] + $result["c"];
"#,
    );
    assert_eq!(out, "5");
}

#[test]
fn test_assoc_foreach_key_value() {
    let out = compile_and_run(
        r#"<?php
$m = ["a" => "1", "b" => "2"];
foreach ($m as $k => $v) {
    echo $k . "=" . $v . " ";
}
"#,
    );
    assert_eq!(out, "a=1 b=2 ");
}

#[test]
fn test_assoc_foreach_mixed_integer_and_string_keys() {
    let out = compile_and_run(
        r#"<?php
$m = [1 => "a", "02" => "b"];
foreach ($m as $k => $v) {
    echo $k . "=" . $v . ";";
}
"#,
    );
    assert_eq!(out, "1=a;02=b;");
}

#[test]
fn test_assoc_foreach_preserves_order_after_overwrite() {
    let out = compile_and_run(
        r#"<?php
$m = ["a" => "1", "b" => "2"];
$m["a"] = "3";
foreach ($m as $k => $v) {
    echo $k . "=" . $v . " ";
}
"#,
    );
    assert_eq!(out, "a=3 b=2 ");
}

#[test]
fn test_assoc_foreach_preserves_order_after_growth() {
    let out = compile_and_run(
        r#"<?php
$m = ["k0" => "0"];
$m["k1"] = "1";
$m["k2"] = "2";
$m["k3"] = "3";
$m["k4"] = "4";
$m["k5"] = "5";
$m["k6"] = "6";
$m["k7"] = "7";
$m["k8"] = "8";
$m["k9"] = "9";
$m["k10"] = "10";
$m["k11"] = "11";
$m["k12"] = "12";
foreach ($m as $k => $v) {
    echo $k . "=" . $v . " ";
}
"#,
    );
    assert_eq!(
        out,
        "k0=0 k1=1 k2=2 k3=3 k4=4 k5=5 k6=6 k7=7 k8=8 k9=9 k10=10 k11=11 k12=12 "
    );
}

#[test]
fn test_indexed_foreach_key_value() {
    let out = compile_and_run(
        r#"<?php
$arr = [10, 20, 30];
foreach ($arr as $i => $v) {
    echo $i . ":" . $v . " ";
}
"#,
    );
    assert_eq!(out, "0:10 1:20 2:30 ");
}

#[test]
fn test_switch_basic() {
    let out = compile_and_run(
        r#"<?php
$x = 2;
switch ($x) {
    case 1:
        echo "one";
        break;
    case 2:
        echo "two";
        break;
    case 3:
        echo "three";
        break;
}
"#,
    );
    assert_eq!(out, "two");
}

#[test]
fn test_switch_default() {
    let out = compile_and_run(
        r#"<?php
$x = 99;
switch ($x) {
    case 1:
        echo "one";
        break;
    default:
        echo "other";
        break;
}
"#,
    );
    assert_eq!(out, "other");
}

#[test]
fn test_switch_fallthrough() {
    let out = compile_and_run(
        r#"<?php
$x = 1;
switch ($x) {
    case 1:
        echo "a";
    case 2:
        echo "b";
        break;
    case 3:
        echo "c";
        break;
}
"#,
    );
    assert_eq!(out, "ab");
}

#[test]
fn test_switch_string() {
    let out = compile_and_run(
        r#"<?php
$s = "hello";
switch ($s) {
    case "hi":
        echo "A";
        break;
    case "hello":
        echo "B";
        break;
    default:
        echo "C";
        break;
}
"#,
    );
    assert_eq!(out, "B");
}

#[test]
fn test_match_basic() {
    let out = compile_and_run(
        r#"<?php
$x = 2;
$result = match($x) {
    1 => "one",
    2 => "two",
    3 => "three",
    default => "other",
};
echo $result;
"#,
    );
    assert_eq!(out, "two");
}

#[test]
fn test_match_default() {
    let out = compile_and_run(
        r#"<?php
$x = 99;
echo match($x) {
    1 => "one",
    default => "unknown",
};
"#,
    );
    assert_eq!(out, "unknown");
}
