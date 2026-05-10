//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of type-related builtins, includes basic, including include basic, require basic, and include with parens.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Multi-file fixtures exercise include/require resolution, temporary project layout, and native binary output.

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
fn test_include_once_skipped_branch_does_not_claim_file() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
if (false) {
    include_once 'piece.php';
}
include_once 'piece.php';
"#,
            ),
            ("piece.php", "<?php echo \"piece\";"),
        ],
        "main.php",
    );
    assert_eq!(out, "piece");
}

#[test]
fn test_include_once_in_loop_executes_file_once() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
$i = 0;
while ($i < 3) {
    include_once 'tick.php';
    $i = $i + 1;
}
"#,
            ),
            ("tick.php", "<?php echo \"tick\";"),
        ],
        "main.php",
    );
    assert_eq!(out, "tick");
}

#[test]
fn test_require_once_in_function_is_global_once() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
function load_piece() {
    require_once 'piece.php';
}
load_piece();
load_piece();
"#,
            ),
            ("piece.php", "<?php echo \"piece\";"),
        ],
        "main.php",
    );
    assert_eq!(out, "piece");
}

#[test]
fn test_require_once_in_method_is_global_once() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
class Loader {
    public function load() {
        require_once 'piece.php';
    }
}
$loader = new Loader();
$loader->load();
$loader->load();
"#,
            ),
            ("piece.php", "<?php echo \"piece\";"),
        ],
        "main.php",
    );
    assert_eq!(out, "piece");
}

#[test]
fn test_require_once_in_closure_is_global_once() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
$load = function() {
    require_once 'piece.php';
};
$load();
$load();
"#,
            ),
            ("piece.php", "<?php echo \"piece\";"),
        ],
        "main.php",
    );
    assert_eq!(out, "piece");
}

#[test]
fn test_regular_include_in_closure_marks_later_include_once() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
$load = function() {
    include 'piece.php';
};
$load();
include_once 'piece.php';
"#,
            ),
            ("piece.php", "<?php echo \"piece\";"),
        ],
        "main.php",
    );
    assert_eq!(out, "piece");
}

#[test]
fn test_regular_include_marks_later_include_once_declarations() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
include 'lib.php';
include_once 'lib.php';
echo seven();
"#,
            ),
            ("lib.php", "<?php function seven() { return 7; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "7");
}

#[test]
fn test_skipped_regular_include_does_not_make_include_once_skip() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
if (false) {
    include 'piece.php';
}
include_once 'piece.php';
"#,
            ),
            ("piece.php", "<?php echo \"piece\";"),
        ],
        "main.php",
    );
    assert_eq!(out, "piece");
}

