//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of control flow functions, including function call integer, function call string, and function void.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

// Compiles a function returning the sum of two integers and verifies the result.
#[test]
fn test_function_call_int() {
    let out = compile_and_run("<?php function add($a, $b) { return $a + $b; } echo add(10, 32);");
    assert_eq!(out, "42");
}

// Compiles a function returning a concatenated string and verifies the output.
#[test]
fn test_function_call_string() {
    let out = compile_and_run(
        "<?php function greet($name) { return \"Hello, \" . $name; } echo greet(\"World\");",
    );
    assert_eq!(out, "Hello, World");
}

// Verifies that string concatenation inside a function return is preserved when
// the returned value is used in further concatenation operations.
#[test]
fn test_function_returned_concat_survives_outer_concat() {
    let out = compile_and_run(
        r#"<?php
function label($name) { return "[" . $name . "]"; }
echo label("title") . "|" . label("slug");
"#,
    );
    assert_eq!(out, "[title]|[slug]");
}

// Compiles a void function that echoes a value and returns early, then verifies
// the side effect occurs correctly when the function is called as a statement.
#[test]
fn test_function_void() {
    let out = compile_and_run("<?php function say() { echo \"hi\"; return; } say();");
    assert_eq!(out, "hi");
}

// Verifies that variables inside a function body do not leak to the outer scope,
// and that the global variable remains unchanged after the function call.
#[test]
fn test_function_local_scope() {
    let out = compile_and_run(
        "<?php $x = 1; function get_two() { $x = 2; return $x; } echo $x . \" \" . get_two();",
    );
    assert_eq!(out, "1 2");
}

// Compiles a recursive function computing factorial and verifies correct evaluation
// of 5! = 120.
#[test]
fn test_function_recursive() {
    let out = compile_and_run(
        "<?php function fact($n) { if ($n <= 1) { return 1; } return $n * fact($n - 1); } echo fact(5);",
    );
    assert_eq!(out, "120");
}

// Verifies that a function can be called multiple times with different arguments
// and each call returns the correct independent result.
#[test]
fn test_function_multiple_calls() {
    let out = compile_and_run(
        "<?php function double($x) { return $x * 2; } echo double(3) . \" \" . double(7);",
    );
    assert_eq!(out, "6 14");
}

// Verifies that the return value of a function can be passed directly as an
// argument to another function call, with correct evaluation order.
#[test]
fn test_function_as_argument() {
    let out = compile_and_run(
        "<?php function add($a, $b) { return $a + $b; } echo add(add(1, 2), add(3, 4));",
    );
    assert_eq!(out, "10");
}

// Compiles a function with no parameters that returns a constant integer.
#[test]
fn test_function_no_args() {
    let out = compile_and_run("<?php function answer() { return 42; } echo answer();");
    assert_eq!(out, "42");
}

// --- Logical operators ---
