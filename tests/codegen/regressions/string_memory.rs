//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of regressions string memory, including string replace in foreach assoc function, concat loop 1000, and concat assignment loop 5000.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

#[test]
fn test_str_replace_in_foreach_assoc_function() {
    let out = compile_and_run(
        r#"<?php
function transform($map, $text) {
    foreach ($map as $key => $value) {
        $text = str_replace($key, $value, $text);
    }
    return $text;
}
$map = ["hello" => "world", "foo" => "bar"];
echo transform($map, "hello foo");
"#,
    );
    assert_eq!(out, "world bar");
}

// --- Bug fix: fmod sign (frintm → frintz) ---

#[test]
fn test_concat_loop_1000() {
    // Regression test for issue #21: concat buffer overflow after ~362 iterations
    let out = compile_and_run(
        r#"<?php
$s = "";
for ($i = 0; $i < 1000; $i++) {
    $s .= "x";
}
echo strlen($s);
"#,
    );
    assert_eq!(out, "1000");
}

#[test]
fn test_concat_assignment_loop_5000() {
    // Regression for x86_64 local-string cleanup: `$s = $s . "x"` must release old heap strings.
    let out = compile_and_run(
        r#"<?php
$s = "";
for ($i = 0; $i < 5000; $i++) {
    $s = $s . "x";
}
echo strlen($s);
"#,
    );
    assert_eq!(out, "5000");
}

#[test]
fn test_string_function_in_loop() {
    let out = compile_and_run(
        r#"<?php
for ($i = 0; $i < 500; $i++) {
    $x = strtolower("HELLO WORLD");
}
echo $x;
"#,
    );
    assert_eq!(out, "hello world");
}

#[test]
fn test_string_reassignment_loop() {
    // Tests that old string values are freed on reassignment (free-list reuse)
    let out = compile_and_run(
        r#"<?php
for ($i = 0; $i < 2000; $i++) {
    $s = str_repeat("a", 100);
}
echo strlen($s);
"#,
    );
    assert_eq!(out, "100");
}

#[test]
fn test_string_variables_survive_statements() {
    // Tests that string persist works: values survive across statement boundaries
    let out = compile_and_run(
        r#"<?php
$a = "foo" . "bar";
$b = "baz" . "qux";
echo $a . $b;
"#,
    );
    assert_eq!(out, "foobarbazqux");
}

#[test]
fn test_unset_frees_string() {
    let out = compile_and_run(
        r#"<?php
$x = "hello" . " world";
echo strlen($x);
unset($x);
echo is_null($x) ? "1" : "0";
"#,
    );
    assert_eq!(out, "111");
}

#[test]
fn test_multiple_string_vars_independent() {
    // Ensure multiple string variables don't interfere after concat_buf reset
    let out = compile_and_run(
        r#"<?php
$a = "hello";
$b = "world";
$c = $a . " " . $b;
$d = strtoupper($a);
echo $c . "|" . $d;
"#,
    );
    assert_eq!(out, "hello world|HELLO");
}

#[test]
fn test_str_replace_in_loop() {
    let out = compile_and_run(
        r#"<?php
$result = "";
for ($i = 0; $i < 100; $i++) {
    $result = str_replace("x", "y", "xox");
}
echo $result;
"#,
    );
    assert_eq!(out, "yoy");
}
