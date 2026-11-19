//! Purpose:
//! End-to-end coverage for AST DCE facts derived from pre-tested loop conditions.
//!
//! Called from:
//! - `cargo test --test codegen_tests_core dead_code_elimination::guards`.
//!
//! Key details:
//! - The fixture proves typed-int pruning in `while` while retaining a float gap branch.

use super::*;

/// Verifies a pure `while` condition prunes an impossible nested int branch but not a float gap.
#[test]
fn test_dead_code_elimination_strengthens_while_body_from_loop_condition() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_while_condition_guard");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
int $x = $argc + 11;
while ($x > 10) {
    if ($x > 5) {
        echo "a";
    } else {
        echo "dead-while-range";
    }
    break;
}

float $f = 10.5;
while ($f > 10) {
    if ($f >= 11) {
        echo "whole";
    } else {
        echo "fractional";
    }
    break;
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
    assert_eq!(out, "afractional");
    assert!(
        !user_asm.contains("dead-while-range"),
        "typed int loop condition should prune the nested contradiction"
    );
    assert!(user_asm.contains("whole"));
    assert!(user_asm.contains("fractional"));
}
