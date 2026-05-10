//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of type-related builtins, includes include-loaded function variants, including conditional include function variants dispatch false branch, conditional include function variants dispatch true branch, and conditional include single function variant marks loaded branch.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Multi-file fixtures exercise include/require resolution, temporary project layout, and native binary output.

use super::*;

#[test]
fn test_conditional_include_function_variants_dispatch_false_branch() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
$pick = 0;
if ($pick) {
    include 'left.php';
} else {
    include 'right.php';
}
echo selected();
"#,
            ),
            ("left.php", "<?php function selected() { return 'left'; }"),
            ("right.php", "<?php function selected() { return 'right'; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "right");
}

#[test]
fn test_conditional_include_function_variants_dispatch_true_branch() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
$pick = 1;
if ($pick) {
    include 'left.php';
} else {
    include 'right.php';
}
echo selected_true();
"#,
            ),
            ("left.php", "<?php function selected_true() { return 'left'; }"),
            ("right.php", "<?php function selected_true() { return 'right'; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "left");
}

#[test]
fn test_conditional_include_single_function_variant_marks_loaded_branch() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
$pick = 1;
if ($pick) {
    include 'lib.php';
}
echo optional_selected();
"#,
            ),
            ("lib.php", "<?php function optional_selected() { return 'loaded'; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "loaded");
}

#[test]
fn test_conditional_include_function_exists_tracks_unloaded_variant() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
if ($argc > 1) {
    include 'lib.php';
}
if (function_exists('optional_exists')) {
    echo optional_exists();
} else {
    echo 'missing';
}
"#,
            ),
            ("lib.php", "<?php function optional_exists() { return 'loaded'; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "missing");
}

#[test]
fn test_conditional_include_function_exists_tracks_loaded_variant() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
if ($argc >= 1) {
    include 'lib.php';
}
if (function_exists('optional_exists_loaded')) {
    echo optional_exists_loaded();
} else {
    echo 'missing';
}
"#,
            ),
            (
                "lib.php",
                "<?php function optional_exists_loaded() { return 'loaded'; }",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "loaded");
}

#[test]
fn test_include_discovered_function_exists_tracks_runtime_load_order() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
function load_lib() {
    include 'lib.php';
}
echo function_exists('runtime_loaded') ? 'yes-before' : 'no-before';
load_lib();
echo '|';
echo function_exists('runtime_loaded') ? runtime_loaded() : 'no-after';
"#,
            ),
            ("lib.php", "<?php function runtime_loaded() { return 'yes-after'; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "no-before|yes-after");
}

#[test]
fn test_include_discovered_function_call_before_runtime_load_fails() {
    let err = compile_and_run_files_expect_failure(
        &[
            (
                "main.php",
                r#"<?php
function load_lib() {
    include 'lib.php';
}
echo runtime_loaded_late();
load_lib();
"#,
            ),
            (
                "lib.php",
                "<?php function runtime_loaded_late() { return 'loaded'; }",
            ),
        ],
        "main.php",
    );
    assert!(err.contains("Call to undefined function runtime_loaded_late()"));
}

#[test]
fn test_include_once_discovered_function_exists_tracks_runtime_load_order() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
function load_once_lib() {
    include_once 'lib.php';
}
echo function_exists('runtime_loaded_once') ? 'yes-before' : 'no-before';
load_once_lib();
load_once_lib();
echo '|';
echo function_exists('runtime_loaded_once') ? runtime_loaded_once() : 'no-after';
"#,
            ),
            (
                "lib.php",
                "<?php function runtime_loaded_once() { return 'yes-after'; }",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "no-before|yes-after");
}

#[test]
fn test_conditional_include_function_exists_is_case_insensitive_in_namespace() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
namespace App;
if ($argc >= 1) {
    include 'lib.php';
}
echo function_exists('OPTIONAL_CASE') ? optional_case() : 'missing';
"#,
            ),
            (
                "lib.php",
                "<?php namespace App; function optional_case() { return 'loaded'; }",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "loaded");
}

#[test]
fn test_conditional_include_function_variants_preserve_namespace() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
namespace App;
$pick = 0;
if ($pick) {
    include 'left.php';
} else {
    include 'right.php';
}
echo selected_ns();
"#,
            ),
            (
                "left.php",
                "<?php namespace App; function selected_ns() { return 'left'; }",
            ),
            (
                "right.php",
                "<?php namespace App; function selected_ns() { return 'right'; }",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "right");
}

#[test]
fn test_conditional_include_once_function_variants_dispatch_loaded_branch() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
$pick = 1;
if ($pick) {
    include_once 'left.php';
} else {
    include_once 'right.php';
}
echo selected_once();
"#,
            ),
            ("left.php", "<?php function selected_once() { return 'left'; }"),
            ("right.php", "<?php function selected_once() { return 'right'; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "left");
}

#[test]
fn test_conditional_include_function_variants_require_matching_signatures() {
    assert!(compile_files_fails(
        &[
            (
                "main.php",
                r#"<?php
$pick = 0;
if ($pick) {
    include 'int.php';
} else {
    include 'string.php';
}
echo selected_mismatch();
"#,
            ),
            ("int.php", "<?php function selected_mismatch(): int { return 1; }"),
            (
                "string.php",
                "<?php function selected_mismatch(): string { return 'one'; }",
            ),
        ],
        "main.php",
    ));
}

#[test]
fn test_same_branch_conditional_includes_still_report_duplicate_function() {
    assert!(compile_files_fails(
        &[
            (
                "main.php",
                r#"<?php
$pick = 1;
if ($pick) {
    include 'a.php';
    include 'b.php';
}
"#,
            ),
            ("a.php", "<?php function same_branch_duplicate() { return 1; }"),
            ("b.php", "<?php function same_branch_duplicate() { return 2; }"),
        ],
        "main.php",
    ));
}

#[test]
fn test_regular_include_in_constant_false_branch_does_not_duplicate_later_include() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
if (false) {
    include 'lib.php';
}
include 'lib.php';
echo false_branch_value();
"#,
            ),
            ("lib.php", "<?php function false_branch_value() { return 'ok'; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "ok");
}

#[test]
fn test_regular_include_in_constant_false_elseif_chain_does_not_duplicate_later_include() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
if (false) {
    include 'lib.php';
} elseif (false) {
    include 'lib.php';
}
include 'lib.php';
echo false_elseif_value();
"#,
            ),
            ("lib.php", "<?php function false_elseif_value() { return 'ok'; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "ok");
}

#[test]
fn test_regular_include_possible_branch_then_later_include_still_reports_duplicate() {
    assert!(compile_files_fails(
        &[
            (
                "main.php",
                r#"<?php
if (time() > 0) {
    include 'lib.php';
}
include 'lib.php';
"#,
            ),
            ("lib.php", "<?php function maybe_duplicated() { return 1; }"),
        ],
        "main.php",
    ));
}

#[test]
fn test_regular_include_declaration_in_loop_reports_duplicate() {
    assert!(compile_files_fails(
        &[
            (
                "main.php",
                r#"<?php
$i = 0;
while ($i < 2) {
    include 'lib.php';
    $i = $i + 1;
}
"#,
            ),
            ("lib.php", "<?php function loop_duplicated() { return 1; }"),
        ],
        "main.php",
    ));
}

#[test]
fn test_include_once_in_loop_with_nested_regular_include_discovers_once() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
$i = 0;
while ($i < 2) {
    include_once 'outer.php';
    $i = $i + 1;
}
echo nested_once_value();
"#,
            ),
            ("outer.php", "<?php include 'inner.php';"),
            ("inner.php", "<?php function nested_once_value() { return 'ok'; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "ok");
}

#[test]
fn test_include_once_possible_branch_then_later_include_once_discovers_once() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
if (time() < 0) {
    include_once 'lib.php';
}
include_once 'lib.php';
echo once_later_value();
"#,
            ),
            ("lib.php", "<?php function once_later_value() { return 'ok'; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "ok");
}

#[test]
fn test_include_once_exclusive_branches_scan_context_sensitive_nested_includes() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
$pick = time() < 0;
if ($pick) {
    define('TARGET_FILE', 'a.php');
    include_once 'outer.php';
    echo branch_a_value();
} else {
    define('TARGET_FILE', 'b.php');
    include_once 'outer.php';
    echo branch_b_value();
}
"#,
            ),
            ("outer.php", "<?php include TARGET_FILE;"),
            ("a.php", "<?php function branch_a_value() { return 'a'; }"),
            ("b.php", "<?php function branch_b_value() { return 'b'; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "b");
}

