use crate::support::*;

#[test]
fn test_ptr_null_and_is_null() {
    let out = compile_and_run(
        r#"<?php
$p = ptr_null();
echo ptr_is_null($p) ? "1" : "0";
"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_ptr_null_echo() {
    let out = compile_and_run(
        r#"<?php
echo ptr_null();
"#,
    );
    assert_eq!(out, "0x0");
}

#[test]
fn test_ptr_take_address() {
    let out = compile_and_run(
        r#"<?php
$x = 42;
$p = ptr($x);
echo ptr_is_null($p) ? "null" : "not null";
"#,
    );
    assert_eq!(out, "not null");
}

#[test]
fn test_ptr_get_roundtrip() {
    let out = compile_and_run(
        r#"<?php
$x = 42;
$p = ptr($x);
echo ptr_get($p);
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_ptr_set_modifies_variable() {
    let out = compile_and_run(
        r#"<?php
$x = 10;
$p = ptr($x);
ptr_set($p, 99);
echo $x;
"#,
    );
    assert_eq!(out, "99");
}

#[test]
fn test_ptr_offset() {
    let out = compile_and_run(
        r#"<?php
$x = 42;
$p = ptr($x);
$q = ptr_offset($p, 0);
echo ptr_get($q);
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_ptr_cast() {
    let out = compile_and_run(
        r#"<?php
$x = 42;
$p = ptr($x);
$q = ptr_cast<int>($p);
echo ptr_get($q);
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_ptr_strict_equal_after_cast() {
    let out = compile_and_run(
        r#"<?php
$x = 42;
$p = ptr($x);
$q = ptr_cast<int>($p);
echo $p === $q ? "1" : "0";
"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_ptr_sizeof_int() {
    let out = compile_and_run(
        r#"<?php
echo ptr_sizeof("int");
"#,
    );
    assert_eq!(out, "8");
}

#[test]
fn test_ptr_sizeof_string() {
    let out = compile_and_run(
        r#"<?php
echo ptr_sizeof("string");
"#,
    );
    assert_eq!(out, "16");
}

#[test]
fn test_ptr_sizeof_float() {
    let out = compile_and_run(
        r#"<?php
echo ptr_sizeof("float");
"#,
    );
    assert_eq!(out, "8");
}

#[test]
fn test_ptr_sizeof_ptr() {
    let out = compile_and_run(
        r#"<?php
echo ptr_sizeof("ptr");
"#,
    );
    assert_eq!(out, "8");
}

#[test]
fn test_ptr_sizeof_class() {
    let out = compile_and_run(
        r#"<?php
class Point {
    public $x;
    public $y;
}
echo ptr_sizeof("Point");
"#,
    );
    // class_id(8) + 2 properties * 16 = 40
    assert_eq!(out, "40");
}

#[test]
fn test_ptr_sizeof_extern_class() {
    let out = compile_and_run(
        r#"<?php
extern class Point {
    public int $x;
    public int $y;
}
echo ptr_sizeof("Point");
"#,
    );
    assert_eq!(out, "16");
}

#[test]
fn test_ptr_strict_equal() {
    let out = compile_and_run(
        r#"<?php
$a = ptr_null();
$b = ptr_null();
echo $a === $b ? "1" : "0";
"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_ptr_strict_not_equal() {
    let out = compile_and_run(
        r#"<?php
$x = 1;
$a = ptr_null();
$b = ptr($x);
echo $a !== $b ? "1" : "0";
"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_ptr_echo_hex() {
    let out = compile_and_run(
        r#"<?php
$p = ptr_null();
echo $p;
"#,
    );
    assert_eq!(out, "0x0");
}

#[test]
fn test_ptr_gettype() {
    let out = compile_and_run(
        r#"<?php
$p = ptr_null();
echo gettype($p);
"#,
    );
    assert_eq!(out, "pointer");
}

#[test]
fn test_ptr_empty_null_and_non_null() {
    let out = compile_and_run(
        r#"<?php
$x = 1;
$p = ptr($x);
$n = ptr_null();
echo empty($n) ? "1" : "0";
echo empty($p) ? "1" : "0";
"#,
    );
    assert_eq!(out, "10");
}

#[test]
fn test_ptr_in_function() {
    let out = compile_and_run(
        r#"<?php
function double_via_ptr($p) {
    $val = ptr_get($p);
    ptr_set($p, $val * 2);
}
$x = 21;
double_via_ptr(ptr($x));
echo $x;
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_ptr_in_loop() {
    let out = compile_and_run(
        r#"<?php
$sum = 0;
$p = ptr($sum);
for ($i = 1; $i <= 10; $i++) {
    ptr_set($p, ptr_get($p) + $i);
}
echo $sum;
"#,
    );
    assert_eq!(out, "55");
}

#[test]
fn test_ptr_read8_and_write8() {
    let out = compile_and_run(
        r#"<?php
extern function malloc(int $size): ptr;
extern function free(ptr $p): void;

$buf = malloc(1);
ptr_write8($buf, 255);
echo ptr_read8($buf);
free($buf);
"#,
    );
    assert_eq!(out, "255");
}

#[test]
fn test_ptr_read32_and_write32() {
    let out = compile_and_run(
        r#"<?php
extern function malloc(int $size): ptr;
extern function free(ptr $p): void;

$buf = malloc(4);
ptr_write32($buf, 305419896);
echo ptr_read32($buf);
free($buf);
"#,
    );
    assert_eq!(out, "305419896");
}

#[test]
fn test_ptr_null_dereference_reports_runtime_error() {
    let err = compile_and_run_expect_failure(
        r#"<?php
$p = ptr_null();
echo ptr_get($p);
"#,
    );
    assert!(err.contains("Fatal error: null pointer dereference"));
}
