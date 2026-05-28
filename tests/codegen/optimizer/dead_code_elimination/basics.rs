//! Purpose:
//! Integration or regression tests for optimizer-sensitive codegen coverage of optimizer, dead-code elimination basics, including dead code elimination prunes pure builtin expr statement, dead code elimination drops shadowed match arm from user assembly, and dead code elimination inverts single live else branch.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled and run so folding, propagation, or pruning stays behavior-preserving.

use super::*;

/// Verifies that a pure `strlen(...)` expression statement is removed from user assembly
/// and does not appear as a runtime call. The `echo 7` output confirms behavior is preserved.
#[test]
fn test_dead_code_elimination_prunes_pure_builtin_expr_statement() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_pure_builtin_expr_stmt");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
strlen("abc");
echo 7;
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("strlen()"),
        "pure builtin expr statements should disappear from user assembly:\n{}",
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
    assert_eq!(out, "7");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies that a duplicated match arm (same value 1 => "a", 1 => "shadowed") produces
/// output "a!" with "shadowed" absent from user assembly.
#[test]
fn test_dead_code_elimination_drops_shadowed_match_arm_from_user_assembly() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_shadowed_match_arm");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function id($value) {
    return $value;
}

echo match (id(1)) {
    1 => "a",
    1 => "shadowed",
    default => "z",
};
echo "!";
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("shadowed"),
        "shadowed match arm should not remain in user assembly:\n{}",
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

/// Verifies that a single live `else` branch is inverted to a plain block when the `if`
/// condition is constant false. Confirms "e".
#[test]
fn test_dead_code_elimination_inverts_single_live_else_branch() {
    let out = compile_and_run(
        r#"<?php
$flag = false;
if ($flag) {
} else {
    echo "e";
}
"#,
    );

    assert_eq!(out, "e");
}

/// Verifies that a pure pipe expression (`strtoupper(...) |> strlen(...)`) statement is
/// removed from user assembly. The `echo 7` output confirms behavior is preserved.
#[test]
fn test_dead_code_elimination_prunes_pure_pipe_expr_statement() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_pure_pipe_expr_stmt");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
strtoupper("hello") |> strlen(...);
echo 7;
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("strlen()"),
        "pure pipe expr statements should disappear from user assembly:\n{}",
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
    assert_eq!(out, "7");

    let _ = fs::remove_dir_all(&dir);
}
