//! Purpose:
//! Integration or regression tests for optimizer-sensitive codegen coverage of optimizer, dead-code elimination, tries catch pruning, including dead code elimination drops unreachable catch after non throwing try, dead code elimination drops unreachable catch before finally, and dead code elimination drops shadowed throwable catch from user assembly.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled and run so folding, propagation, or pruning stays behavior-preserving.

use super::*;

/// Verifies that a catch block is dropped when the try body cannot throw. Confirms "t!".
#[test]
fn test_dead_code_elimination_drops_unreachable_catch_after_non_throwing_try() {
    let out = compile_and_run(
        r#"<?php
try {
    echo "t";
} catch (Exception $e) {
    echo "c";
}
echo "!";
"#,
    );

    assert_eq!(out, "t!");
}

/// Verifies that a catch block preceding a finally block is dropped when unreachable.
/// Confirms "tf!".
#[test]
fn test_dead_code_elimination_drops_unreachable_catch_before_finally() {
    let out = compile_and_run(
        r#"<?php
try {
    echo "t";
} catch (Exception $e) {
    echo "c";
} finally {
    echo "f";
}
echo "!";
"#,
    );

    assert_eq!(out, "tf!");
}

/// Verifies that a shadowed catch (Throwable before Exception) is dropped from assembly.
/// Confirms "a!" with "shadowed" absent from user assembly.
#[test]
fn test_dead_code_elimination_drops_shadowed_throwable_catch_from_user_assembly() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_shadowed_throwable_catch");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
try {
    throw new Exception("boom");
} catch (Throwable $t) {
    echo "a";
} catch (Exception $e) {
    echo "shadowed";
}
echo "!";
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("shadowed"),
        "shadowed catch body should not remain in user assembly:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "a!");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies that adjacent catch blocks with identical bodies (same `pow` call) are merged.
/// Confirms output "1".
#[test]
fn test_dead_code_elimination_merges_identical_adjacent_catches() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_merge_identical_catches");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
class A extends Exception {}
class B extends Exception {}
function boom($flag) {
    if ($flag) {
        throw new A("a");
    }
    throw new B("b");
}
try {
    boom($argc > 1);
} catch (A $e) {
    echo pow($argc, 3);
} catch (B $e) {
    echo pow($argc, 3);
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );
    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    assert_eq!(out, "1");
}

/// Verifies that multi-catch types are deduplicated when their merged set is identical to
/// one branch. Confirms "8".
#[test]
fn test_dead_code_elimination_deduplicates_merged_catch_types() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_dedup_catch_types");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
class A extends Exception {}
class B extends Exception {}
class C extends Exception {}
function boom($flag) {
    if ($flag === 1) {
        throw new A("a");
    }
    if ($flag === 2) {
        throw new B("b");
    }
    throw new C("c");
}
try {
    boom($argc);
} catch (A | B $e) {
    echo pow(2, 3);
} catch (B | C $e) {
    echo pow(2, 3);
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );
    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    assert_eq!(out, "8");
}

/// Verifies that multi-catch with sorted types (Zed | Alpha | Mid) is accepted and handles
/// the catch correctly. Confirms "ok".
#[test]
fn test_dead_code_elimination_accepts_sorted_multi_catch_types() {
    let out = compile_and_run(
        r#"<?php
class Alpha extends Exception {}
class Mid extends Exception {}
class Zed extends Exception {}
function boom($flag) {
    if ($flag === 1) {
        throw new Zed("z");
    }
    if ($flag === 2) {
        throw new Alpha("a");
    }
    throw new Mid("m");
}
try {
    boom($argc);
} catch (Zed | Alpha | Mid $e) {
    echo "ok";
}
"#,
    );

    assert_eq!(out, "ok");
}
