use super::*;

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
