//! Purpose:
//! End-to-end regressions for switch case guards across fall-through entries.
//!
//! Called from:
//! - `cargo test --test codegen_tests_core dead_code_elimination::switches`.
//!
//! Key details:
//! - Fall-through bodies retain live structural/range alternatives, while
//!   direct-only case bodies still prune contradictions from their matched pattern.

use super::*;

/// Verifies fall-through never inherits the current pattern, while direct entry still does.
#[test]
fn test_dead_code_elimination_keeps_switch_fallthrough_guard_paths() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_switch_fallthrough_guards");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function range_fallthrough(int $x) {
    switch (true) {
        case $x > 0:
            echo "A";
        case $x > 100:
            if ($x > 50) {
                echo "range-big";
            } else {
                echo "range-small";
            }
            break;
    }
}

function structural_fallthrough(int $x) {
    switch (true) {
        case $x > 0:
            echo "A";
        case $x > 100:
            if ($x > 100) {
                echo "structural-big";
            } else {
                echo "structural-small";
            }
            break;
    }
}

function direct_only(int $x) {
    switch (true) {
        case $x < 0:
            echo "negative";
            break;
        case $x > 100:
            if ($x > 50) {
                echo "direct-big";
            } else {
                echo "dead-direct-small";
            }
            break;
    }
}

range_fallthrough(5);
echo "|";
range_fallthrough(200);
echo "|";
structural_fallthrough(5);
echo "|";
direct_only(200);
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
    assert_eq!(out, "Arange-small|Arange-big|Astructural-small|direct-big");
    assert!(user_asm.contains("range-small"));
    assert!(user_asm.contains("structural-small"));
    assert!(!user_asm.contains("dead-direct-small"));
}

/// Verifies a break-only case cannot fall through into a later case body or default after DCE.
#[test]
fn test_dead_code_elimination_preserves_break_only_switch_case_barrier() {
    let out = compile_and_run(
        r#"<?php
switch (true) {
    case $argc > 0:
        break;
    case $argc > 100:
        echo "second";
        break;
}
echo "|";

switch (true) {
    case $argc > 0:
        break;
    default:
        echo "default";
}
echo "|";
"#,
    );

    assert_eq!(out, "||");
}
