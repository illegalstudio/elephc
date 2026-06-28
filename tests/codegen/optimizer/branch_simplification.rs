//! Purpose:
//! Integration coverage for EIR branch simplification in the real CLI
//! optimization pipeline.
//!
//! Called from:
//! - `cargo test --test codegen_tests optimizer::branch_simplification`.
//!
//! Key details:
//! - A `while (true)` loop lowers to a constant-condition `cond_br` at EIR, which
//!   the pass folds; the loop header then becomes an empty forwarding block that
//!   is threaded out and the never-taken edge is neutralized to `unreachable`.
//!   Uses `$argc` so the loop bound is runtime-unknown and the body is not folded
//!   away at the AST level.

use super::*;

/// Emits textual EIR for a source snippet through the CLI.
fn emit_ir(source: &str, ir_opt: bool) -> String {
    let dir = make_cli_test_dir("elephc_branch_simplification_emit_ir");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("failed to write PHP fixture");

    let mut command = elephc_cli_command(&dir);
    command.arg("--emit-ir");
    if !ir_opt {
        command.arg("--no-ir-opt");
    }
    let output = command
        .arg(&php_path)
        .output()
        .expect("failed to run elephc --emit-ir");
    assert!(
        output.status.success(),
        "elephc --emit-ir failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).expect("EIR output should be UTF-8");
    let _ = fs::remove_dir_all(&dir);
    text
}

/// Extracts the textual EIR for one function from a printed module.
fn function_ir<'a>(module: &'a str, signature_prefix: &str) -> &'a str {
    let marker = format!("  function {}", signature_prefix);
    let start = module
        .find(&marker)
        .unwrap_or_else(|| panic!("missing function `{}` in:\n{}", signature_prefix, module));
    let rest = &module[start..];
    let next = rest[marker.len()..]
        .find("\n  function ")
        .map(|offset| marker.len() + offset)
        .unwrap_or(rest.len());
    &rest[..next]
}

const WHILE_TRUE_LOOP: &str =
    "<?php $i = 0; while (true) { echo $i; $i++; if ($i >= $argc + 3) break; } echo \"!\";";

/// The constant `while (true)` condition lowers to a `cond_br` on `const_bool
/// true`; branch simplification folds it away while the loop still runs and
/// prints correctly.
#[test]
fn test_branch_simplification_folds_constant_loop_condition() {
    let unoptimized = emit_ir(WHILE_TRUE_LOOP, false);
    let unoptimized_main = function_ir(&unoptimized, "main()");
    assert!(
        unoptimized_main.contains("const_bool true"),
        "--no-ir-opt should show the while(true) condition:\n{}",
        unoptimized_main
    );
    assert!(
        unoptimized_main.contains("cond_br"),
        "--no-ir-opt should show the conditional loop branch:\n{}",
        unoptimized_main
    );

    let optimized = emit_ir(WHILE_TRUE_LOOP, true);
    let optimized_main = function_ir(&optimized, "main()");
    assert!(
        !optimized_main.contains("const_bool true"),
        "the constant loop condition should be folded away:\n{}",
        optimized_main
    );

    let out = compile_and_run(WHILE_TRUE_LOOP);
    assert_eq!(out, "0123!");
}

/// Folding the constant branch leaves the never-taken edge unreachable; the pass
/// neutralizes those blocks to `unreachable`, which only appears after IR opt.
#[test]
fn test_branch_simplification_neutralizes_unreachable_blocks() {
    let unoptimized = emit_ir(WHILE_TRUE_LOOP, false);
    let unoptimized_main = function_ir(&unoptimized, "main()");
    assert!(
        !unoptimized_main.contains("unreachable"),
        "--no-ir-opt main should have no unreachable terminators:\n{}",
        unoptimized_main
    );

    let optimized = emit_ir(WHILE_TRUE_LOOP, true);
    let optimized_main = function_ir(&optimized, "main()");
    assert!(
        optimized_main.contains("unreachable"),
        "branch simplification should neutralize the never-taken blocks:\n{}",
        optimized_main
    );
}
