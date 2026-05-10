//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of type-related builtins, includes include paths and errors, including include nested, include subdirectory, and include variables shared scope.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Multi-file fixtures exercise include/require resolution, temporary project layout, and native binary output.

use super::*;

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
