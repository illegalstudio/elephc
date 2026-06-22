//! Purpose:
//! Integration coverage for EIR dead store elimination over PHP local slots in
//! the real CLI optimization pipeline.
//!
//! Called from:
//! - `cargo test --test codegen_tests optimizer::dead_store_elimination`.
//!
//! Key details:
//! - Uses `--emit-ir` so the test can confirm the dead `store_local` (and the now
//!   dead pure value feeding it) disappear from optimized EIR without depending on
//!   target-specific assembly. Fixtures use `$argc` so the dead store survives
//!   AST-level optimization and only the EIR pass can remove it.

use super::*;

/// Emits textual EIR for a source snippet through the CLI.
fn emit_ir(source: &str, ir_opt: bool) -> String {
    let dir = make_cli_test_dir("elephc_dead_store_elimination_emit_ir");
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

/// A scalar local written and then unconditionally overwritten before any read
/// has its first store removed by the EIR pass; the now-dead `imul` feeding it is
/// then cleaned up by dead instruction elimination.
#[test]
fn test_dead_store_elimination_removes_overwritten_scalar_store() {
    let source = "<?php $x = $argc * 7; $x = $argc + 1; echo $x;";

    let unoptimized = emit_ir(source, false);
    let unoptimized_main = function_ir(&unoptimized, "main()");
    assert!(
        unoptimized_main.contains("imul"),
        "--no-ir-opt should keep the dead store's multiply:\n{}",
        unoptimized_main
    );
    assert!(
        unoptimized_main.contains("const_i64 7"),
        "--no-ir-opt should keep the dead store's literal:\n{}",
        unoptimized_main
    );

    let optimized = emit_ir(source, true);
    let optimized_main = function_ir(&optimized, "main()");
    assert!(
        !optimized_main.contains("imul"),
        "the dead store's multiply should be gone after DSE + DIE:\n{}",
        optimized_main
    );
    assert!(
        !optimized_main.contains("const_i64 7"),
        "the dead store's literal should be gone after DSE + DIE:\n{}",
        optimized_main
    );

    let out = compile_and_run(source);
    assert_eq!(out, "2");
}

/// A dead store overwritten only across a branch merge is removed through
/// cross-block slot liveness while the program still prints the live value.
#[test]
fn test_dead_store_elimination_handles_cross_block_overwrite() {
    let source = "<?php $x = $argc * 9; if ($argc > 0) { $x = 11; } else { $x = 22; } echo $x;";

    let optimized = emit_ir(source, true);
    let optimized_main = function_ir(&optimized, "main()");
    assert!(
        !optimized_main.contains("imul"),
        "the dead store before the branch should be removed:\n{}",
        optimized_main
    );

    let out = compile_and_run(source);
    assert_eq!(out, "11");
}

/// A dead store to a refcounted (string) slot is preserved so its owning value is
/// still released; the program reassigns and prints the final string correctly.
#[test]
fn test_dead_store_elimination_preserves_refcounted_slot_store() {
    let source = "<?php $s = str_repeat(\"x\", $argc + 2); $s = \"final\"; echo $s;";

    let optimized = emit_ir(source, true);
    let optimized_main = function_ir(&optimized, "main()");
    assert!(
        optimized_main.matches("store_local").count() >= 2,
        "both string stores must survive to keep refcounting balanced:\n{}",
        optimized_main
    );

    let out = compile_and_run(source);
    assert_eq!(out, "final");
}
