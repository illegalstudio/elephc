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
    // Compiles main.php that includes helper.php and calls the exported function.
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
    // Compiles main.php that requires math.php and calls the exported function.
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
    // Verifies `include` with parentheses (functional syntax) works correctly.
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
    // Verifies top-level code in an included file executes at the include point, interleaving with main file output.
fn test_include_top_level_code() {
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
    // Verifies `include_once` only executes the file the first time; subsequent calls in the same runtime are no-ops.
fn test_include_once() {
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
    // Verifies `require_once` only executes the file once; function is callable after first load.
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
    // Verifies constants and functions declared in a `require_once` file are accessible after loading.
fn test_require_once_const_visible_inside_included_function() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
require_once 'lib.php';
echo LIB_CONST;
echo from_func();
"#,
            ),
            (
                "lib.php",
                r#"<?php
const LIB_CONST = 42;
function from_func() { return LIB_CONST; }
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "4242");
}

#[test]
    // Verifies `include_once` in a constant-false branch does not claim the file; later `include_once` still executes it.
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
    // Verifies `include_once` in a loop only executes the file once across all iterations.
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
    // Verifies `require_once` inside a function has globalOnce semantics; subsequent calls do not re-execute.
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
    // Verifies `require_once` inside a class method has globalOnce semantics across calls on the same instance.
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
    // Verifies `require_once` inside a closure has globalOnce semantics across closure invocations.
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
    // Verifies a regular `include` inside a closure marks the file as loaded, causing a later `include_once` to skip it.
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
    // Verifies declarations from a regular `include` are visible to a subsequent `include_once` (no duplicate error).
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
    // Verifies `include_once` in a constant-false branch does not claim the file; later `include_once` still executes and finds the declaration.
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
