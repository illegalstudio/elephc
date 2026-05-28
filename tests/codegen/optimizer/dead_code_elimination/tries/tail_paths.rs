//! Purpose:
//! Integration or regression tests for optimizer-sensitive codegen coverage of optimizer, dead-code elimination, tries tail paths, including dead code elimination sinks tail into try fallthrough paths, and dead code elimination sinks tail into try catch only fallthrough paths.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled and run so folding, propagation, or pruning stays behavior-preserving.

use super::*;

/// Verifies that code after a `try/catch` fallthrough sinks correctly. Confirms "ab!".
#[test]
fn test_dead_code_elimination_sinks_tail_into_try_fallthrough_paths() {
    let out = compile_and_run(
        r#"<?php
function run(bool $flag) {
    try {
        if ($flag) {
            throw new Exception("boom");
        }
        echo "a";
    } catch (Exception $e) {
        return;
    }
    echo "b";
}

run(false);
run(true);
echo "!";
"#,
    );

    assert_eq!(out, "ab!");
}

/// Verifies that code after a catch-only fallthrough (no try fallthrough) sinks correctly.
/// Confirms "ab!".
#[test]
fn test_dead_code_elimination_sinks_tail_into_try_catch_only_fallthrough_paths() {
    let out = compile_and_run(
        r#"<?php
function run(bool $flag) {
    try {
        if ($flag) {
            throw new Exception("boom");
        }
        return;
    } catch (Exception $e) {
        echo "a";
    }
    echo "b";
}

run(true);
run(false);
echo "!";
"#,
    );

    assert_eq!(out, "ab!");
}
