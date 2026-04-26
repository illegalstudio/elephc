use crate::support::*;

// --- IIFE (Immediately Invoked Function Expression) ---

#[test]
fn test_iife_returns_string() {
    let out = compile_and_run(
        r#"<?php
$result = (function() { return "hello"; })();
echo $result;
"#,
    );
    assert_eq!(out, "hello");
}

#[test]
fn test_iife_returns_int() {
    let out = compile_and_run(
        r#"<?php
echo (function($x) { return $x * 2; })(21);
"#,
    );
    assert_eq!(out, "42");
}

// --- Empty input / EOF handling ---

#[test]
fn test_empty_php_file() {
    let out = compile_and_run("<?php\n");
    assert_eq!(out, "");
}

#[test]
fn test_only_open_tag() {
    let out = compile_and_run("<?php ");
    assert_eq!(out, "");
}

// --- Syntactic return type inference ---

#[test]
fn test_callback_return_from_dowhile() {
    let out = compile_and_run(
        r#"<?php
function find_first($arr) {
    $i = 0;
    do {
        if ($arr[$i] > 5) { return $arr[$i]; }
        $i = $i + 1;
    } while ($i < count($arr));
    return 0;
}
echo find_first([1, 3, 7, 2]);
"#,
    );
    assert_eq!(out, "7");
}

#[test]
fn test_mixed_return_types_widened() {
    let out = compile_and_run(
        r#"<?php
function describe($n) {
    if ($n > 100) { return "big"; }
    if ($n < 0) { return "negative"; }
    return $n;
}
echo describe(200);
"#,
    );
    assert_eq!(out, "big");
}

#[test]
fn test_null_coalesce_allocates_for_string_default() {
    let out = compile_and_run(
        r#"<?php
function test() {
    $x = null;
    $result = $x ?? "fallback";
    echo $result;
}
test();
"#,
    );
    assert_eq!(out, "fallback");
}

#[test]
fn test_null_coalesce_runtime_null_to_string_default() {
    let out = compile_and_run(
        r#"<?php
$x = false ? 1 : null;
$result = $x ?? "fallback";
echo $result;
"#,
    );
    assert_eq!(out, "fallback");
}

#[test]
fn test_null_coalesce_assignment_assigns_when_null() {
    let out = compile_and_run(
        r#"<?php
$x = null;
$x ??= 7;
echo $x;
"#,
    );
    assert_eq!(out, "7");
}

#[test]
fn test_null_coalesce_assignment_skips_rhs_when_non_null() {
    let out = compile_and_run(
        r#"<?php
function fallback() {
    echo "bad";
    return 99;
}
$x = 5;
$x ??= fallback();
echo $x;
"#,
    );
    assert_eq!(out, "5");
}

#[test]
fn test_null_coalesce_assignment_updates_runtime_null() {
    let out = compile_and_run(
        r#"<?php
$x = false ? 1 : null;
$x ??= 9;
echo $x;
"#,
    );
    assert_eq!(out, "9");
}

#[test]
fn test_null_coalesce_assignment_keeps_non_null_string() {
    let out = compile_and_run(
        r#"<?php
$x = "keep";
$x ??= "fallback";
echo $x;
"#,
    );
    assert_eq!(out, "keep");
}

#[test]
fn test_null_coalesce_assignment_in_for_init() {
    let out = compile_and_run(
        r#"<?php
$i = null;
for ($i ??= 0; $i < 3; $i++) {
    echo $i;
}
"#,
    );
    assert_eq!(out, "012");
}

#[test]
fn test_closure_return_type_from_nested_branch() {
    let out = compile_and_run(
        r#"<?php
$describe = function($n) {
    if ($n > 0) {
        return "positive";
    }
    return 0;
};
$result = $describe(3);
echo $result;
"#,
    );
    assert_eq!(out, "positive");
}

#[test]
fn test_assigned_user_function_call_string_result() {
    let out = compile_and_run(
        r#"<?php
function greet($name) {
    return "Hello, " . $name;
}
function run() {
    $message = greet("World");
    echo $message;
}
run();
"#,
    );
    assert_eq!(out, "Hello, World");
}

#[test]
fn test_ternary_allocates_for_wider_type() {
    let out = compile_and_run(
        r#"<?php
function test($flag) {
    $val = $flag ? 42 : "none";
    echo $val;
}
test(false);
"#,
    );
    assert_eq!(out, "none");
}

#[test]
fn test_ternary_both_branches_in_function() {
    let out = compile_and_run(
        r#"<?php
function label($n) {
    $result = $n > 0 ? "positive" : "zero or negative";
    return $result;
}
echo label(5) . "|" . label(-1);
"#,
    );
    assert_eq!(out, "positive|zero or negative");
}
