//! Purpose:
//! Integration or regression tests for optimizer-sensitive codegen coverage of optimizer, constant folding pruning, including constant folding prunes constant if branch from user assembly, constant folding prunes while false body from user assembly, and constant folding prunes for false body and update from user assembly.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled and run so folding, propagation, or pruning stays behavior-preserving.

use super::*;

/// Verifies that a constant `if (false)` branch is pruned so no `pow` call appears
/// in user assembly and the else branch is taken.
#[test]
fn test_constant_folding_prunes_constant_if_branch_from_user_assembly() {
    let dir = make_cli_test_dir("elephc_constant_folding_if_prune");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$n = 8;
if (false) {
    echo 2 ** $n;
} else {
    echo 3;
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "constant false if-branch should be pruned from user assembly:\n{}",
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
    assert_eq!(out, "3");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies that a `while (false)` loop body is pruned so no `pow` call appears
/// in user assembly and the following statement executes.
#[test]
fn test_constant_folding_prunes_while_false_body_from_user_assembly() {
    let dir = make_cli_test_dir("elephc_constant_folding_while_prune");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$n = 8;
while (false) {
    echo 2 ** $n;
}
echo 3;
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "while(false) body should be pruned from user assembly:\n{}",
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
    assert_eq!(out, "3");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies that `for (false)` eliminates both the loop body and the update clause
/// from user assembly; only the post-loop statement executes.
#[test]
fn test_constant_folding_prunes_for_false_body_and_update_from_user_assembly() {
    let dir = make_cli_test_dir("elephc_constant_folding_for_prune");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$n = 8;
for ($i = 1; false; $i = 2 ** $n) {
    echo 2 ** $n;
}
echo $i;
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "for(false) body and update should be pruned from user assembly:\n{}",
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
    assert_eq!(out, "1");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies that a constant `match` expression selects the correct arm and prunes
/// all dead arms so no `pow` call appears in user assembly.
#[test]
fn test_constant_folding_prunes_match_to_selected_arm_in_user_assembly() {
    let dir = make_cli_test_dir("elephc_constant_folding_match_prune");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$n = 8;
echo match (3) {
    1 => 2 ** $n,
    3 => 7,
    default => 9,
};
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "constant match should not leave dead arms in user assembly:\n{}",
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

/// Verifies that a constant `switch` expression prunes leading dead cases so no
/// `pow` call appears in user assembly for the unselected case.
#[test]
fn test_constant_folding_prunes_switch_leading_cases_from_user_assembly() {
    let dir = make_cli_test_dir("elephc_constant_folding_switch_prune");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$n = 8;
switch (3) {
    case 1:
        echo 2 ** $n;
        break;
    case 3:
        echo 7;
        break;
    default:
        echo 9;
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "constant switch should not leave dead leading cases in user assembly:\n{}",
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

/// Verifies that statements dominated by a function `return` are pruned so no
/// `pow` call appears in user assembly after the return.
#[test]
fn test_constant_folding_prunes_dead_statements_after_return_from_user_assembly() {
    let dir = make_cli_test_dir("elephc_constant_folding_return_dce");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function answer() {
    return 7;
    echo 2 ** 8;
}
echo answer();
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "dead statements after return should not remain in user assembly:\n{}",
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

/// Verifies that pure expression statements (e.g., `strlen(...)` with no output)
/// are pruned from user assembly as dead code.
#[test]
fn test_constant_folding_prunes_pure_expr_statements_from_user_assembly() {
    let dir = make_cli_test_dir("elephc_constant_folding_pure_expr_stmt_dce");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
strlen(...);
echo 7;
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
    assert_eq!(out, "7");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies that statements after an unconditional `break` in a switch are pruned
/// so no `pow` call appears in user assembly.
#[test]
fn test_constant_folding_prunes_dead_statements_after_break_from_user_assembly() {
    let dir = make_cli_test_dir("elephc_constant_folding_break_dce");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
switch (1) {
    case 1:
        echo 7;
        break;
        echo 2 ** 8;
    default:
        echo 9;
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "dead statements after break should not remain in user assembly:\n{}",
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

/// Verifies that an exhaustive `if` (both branches return) eliminates post-if
/// statements so no `pow` call appears in user assembly.
#[test]
fn test_constant_folding_prunes_dead_statements_after_exhaustive_if_from_user_assembly() {
    let dir = make_cli_test_dir("elephc_constant_folding_exhaustive_if_dce");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function answer($flag) {
    if ($flag) {
        return 7;
    } else {
        return 8;
    }
    echo 2 ** 8;
}
echo answer(true);
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "dead statements after exhaustive if should not remain in user assembly:\n{}",
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

/// Verifies that an exhaustive `switch` (every case returns) eliminates post-switch
/// statements so no `pow` call appears in user assembly.
#[test]
fn test_constant_folding_prunes_dead_statements_after_exhaustive_switch_from_user_assembly() {
    let dir = make_cli_test_dir("elephc_constant_folding_exhaustive_switch_dce");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function answer($flag) {
    switch ($flag) {
        case 1:
            return 7;
        default:
            return 8;
    }
    echo 2 ** 8;
}
echo answer(1);
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "dead statements after exhaustive switch should not remain in user assembly:\n{}",
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

/// Verifies that a ternary with a constant true condition prunes the unselected
/// pure branch so no `pow` call appears in user assembly.
#[test]
fn test_constant_folding_prunes_unused_pure_ternary_branch_from_user_assembly() {
    let dir = make_cli_test_dir("elephc_constant_folding_ternary_dead_branch");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function answer() {
    return 7;
}
echo true ? answer() : (2 ** 8);
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "unused pure ternary branch should not remain in user assembly:\n{}",
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
