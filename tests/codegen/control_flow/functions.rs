//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of control flow functions, including function call integer, function call string, and function void.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

#[test]
fn test_function_call_int() {
    let out = compile_and_run("<?php function add($a, $b) { return $a + $b; } echo add(10, 32);");
    assert_eq!(out, "42");
}

#[test]
fn test_function_call_string() {
    let out = compile_and_run(
        "<?php function greet($name) { return \"Hello, \" . $name; } echo greet(\"World\");",
    );
    assert_eq!(out, "Hello, World");
}

#[test]
fn test_function_void() {
    let out = compile_and_run("<?php function say() { echo \"hi\"; return; } say();");
    assert_eq!(out, "hi");
}

#[test]
fn test_function_local_scope() {
    let out = compile_and_run(
        "<?php $x = 1; function get_two() { $x = 2; return $x; } echo $x . \" \" . get_two();",
    );
    assert_eq!(out, "1 2");
}

#[test]
fn test_function_recursive() {
    let out = compile_and_run(
        "<?php function fact($n) { if ($n <= 1) { return 1; } return $n * fact($n - 1); } echo fact(5);",
    );
    assert_eq!(out, "120");
}

#[test]
fn test_function_multiple_calls() {
    let out = compile_and_run(
        "<?php function double($x) { return $x * 2; } echo double(3) . \" \" . double(7);",
    );
    assert_eq!(out, "6 14");
}

#[test]
fn test_function_as_argument() {
    let out = compile_and_run(
        "<?php function add($a, $b) { return $a + $b; } echo add(add(1, 2), add(3, 4));",
    );
    assert_eq!(out, "10");
}

#[test]
fn test_function_no_args() {
    let out = compile_and_run("<?php function answer() { return 42; } echo answer();");
    assert_eq!(out, "42");
}

// --- Logical operators ---
