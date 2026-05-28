//! Purpose:
//! Integration or regression tests for optimizer-sensitive codegen coverage of optimizer, dead-code elimination, switches switch case shadowing, including dead code elimination drops shadowed switch case from user assembly, dead code elimination prunes dead label inside live mixed switch case, and dead code elimination merges identical adjacent switch cases.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled and run so folding, propagation, or pruning stays behavior-preserving.

use super::*;

/// Verifies that a shadowed switch case (duplicate `case 1:` with "a" first and "shadowed" second)
/// is dropped from assembly. Confirms "a!" with "shadowed" absent from user assembly.
#[test]
fn test_dead_code_elimination_drops_shadowed_switch_case_from_user_assembly() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_shadowed_switch_case");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
switch (1) {
    case 1:
        echo "a";
        break;
    case 1:
        echo "shadowed";
        break;
    default:
        echo "z";
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
        "shadowed switch case body should not remain in user assembly:\n{}",
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

/// Verifies that unreachable case labels inside a live mixed switch case are pruned while the
/// live case body remains. Confirms "A" with "dead-first-case" and "dead-default" absent.
#[test]
fn test_dead_code_elimination_prunes_dead_label_inside_live_mixed_switch_case() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_switch_live_case_label_prune");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function run($value) {
    if ($value) {
        if ($value !== 1) {
            switch ($value) {
                case 0:
                    echo "dead-first-case";
                    break;
                case 1:
                case 2:
                case true:
                    echo "A";
                    break;
                default:
                    echo "dead-default";
            }
        }
    }
}

run(true);
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

    assert_eq!(out, "A");
    assert!(!user_asm.contains("dead-first-case"));
    assert!(!user_asm.contains("dead-default"));
}

/// Verifies that identical adjacent switch cases (both `case 1:` and `case 2:` echo "A")
/// are merged by the optimizer. Uses `step()` to confirm side effects execute exactly once
/// and output is "sA".
#[test]
fn test_dead_code_elimination_merges_identical_adjacent_switch_cases() {
    let out = compile_and_run(
        r#"<?php
function step($label, $ret) {
    echo $label;
    return $ret;
}
switch (step("s", 2)) {
    case 1:
        echo "A";
        break;
    case 2:
        echo "A";
        break;
    default:
        echo "D";
}
"#,
    );

    assert_eq!(out, "sA");
}

/// Verifies that fallthrough switch labels (case 1, 2, 3) are merged into one case body.
/// Confirms "sA".
#[test]
fn test_dead_code_elimination_merges_fallthrough_switch_labels_into_next_case() {
    let out = compile_and_run(
        r#"<?php
function step($label, $ret) {
    echo $label;
    return $ret;
}
switch (step("s", 2)) {
    case 1:
    case 2:
    case 3:
        echo "A";
        break;
    default:
        echo "D";
}
"#,
    );

    assert_eq!(out, "sA");
}
