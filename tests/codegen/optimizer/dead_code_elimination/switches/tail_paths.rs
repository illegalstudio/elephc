//! Purpose:
//! Integration or regression tests for optimizer-sensitive codegen coverage of optimizer, dead-code elimination, switches tail paths, including dead code elimination collapses empty switch shell after branch dce, dead code elimination sinks tail into switch exit paths, and dead code elimination sinks tail into switch break paths.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled and run so folding, propagation, or pruning stays behavior-preserving.

use super::*;

#[test]
fn test_dead_code_elimination_collapses_empty_switch_shell_after_branch_dce() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_empty_switch_shell");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function poke() {
    echo "s";
    return 1;
}

switch (poke()) {
    case 1:
        strlen("abc");
        break;
}

echo "!";
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("switch_end"),
        "empty switch shells should not survive user assembly after DCE:\n{}",
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
    assert_eq!(out, "s!");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_dead_code_elimination_sinks_tail_into_switch_exit_paths() {
    let out = compile_and_run(
        r#"<?php
function run(int $flag) {
    switch ($flag) {
        case 1:
            echo "a";
        case 2:
            echo "b";
        default:
            echo "c";
    }
    echo "!";
}

run(1);
run(2);
run(3);
"#,
    );

    assert_eq!(out, "abc!bc!c!");
}

#[test]
fn test_dead_code_elimination_sinks_tail_into_switch_break_paths() {
    let out = compile_and_run(
        r#"<?php
function run(int $flag) {
    switch ($flag) {
        case 1:
            echo "a";
            break;
        case 2:
            echo "b";
        default:
            echo "c";
    }
    echo "!";
}

run(1);
run(2);
run(3);
"#,
    );

    assert_eq!(out, "a!bc!c!");
}
