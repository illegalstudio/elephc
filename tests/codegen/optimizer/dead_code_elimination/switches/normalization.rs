//! Purpose:
//! Integration or regression tests for optimizer-sensitive codegen coverage of optimizer, dead-code elimination, switches normalization, including dead code elimination inlines default only switch, dead code elimination normalizes single case switch with effectful subject, and dead code elimination materializes constant switch match.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled and run so folding, propagation, or pruning stays behavior-preserving.

use super::*;

#[test]
fn test_dead_code_elimination_inlines_default_only_switch() {
    let out = compile_and_run(
        r#"<?php
$x = 1;
switch ($x) {
    default:
        echo "d";
}
"#,
    );

    assert_eq!(out, "d");
}

#[test]
fn test_dead_code_elimination_normalizes_single_case_switch_with_effectful_subject() {
    let out = compile_and_run(
        r#"<?php
function step($label, $ret) {
    echo $label;
    return $ret;
}
switch (step("s", 1)) {
    case step("a", 1):
        echo "A";
        break;
    default:
        echo "D";
}
"#,
    );

    assert_eq!(out, "saA");
}

#[test]
fn test_dead_code_elimination_materializes_constant_switch_match() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_switch_match");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
switch (2) {
    case 1:
        echo 2 ** 8;
        break;
    case 2:
        echo 7;
        break;
    default:
        echo 2 ** 9;
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "constant switch match should inline the selected path and drop dead pow calls:\n{}",
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

#[test]
fn test_dead_code_elimination_materializes_constant_switch_fallthrough() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_switch_fallthrough");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
switch (1) {
    case 1:
    case 2:
        echo 7;
        break;
    default:
        echo 2 ** 9;
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "constant switch fallthrough should inline the selected tail and drop dead pow calls:\n{}",
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

#[test]
fn test_dead_code_elimination_materializes_constant_switch_default() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_switch_default");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
switch (3) {
    case 1:
        echo 2 ** 8;
        break;
    default:
        echo 7;
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "constant switch default should inline the default path and drop dead pow calls:\n{}",
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
