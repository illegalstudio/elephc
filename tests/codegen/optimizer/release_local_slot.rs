//! Purpose:
//! Integration coverage for deferred `ReleaseLocalSlot` pruning and ref-cell boundaries.
//! Compares optimized and unoptimized EIR plus native heap-debug behavior.
//!
//! Called from:
//! - `cargo test --test codegen_tests optimizer::release_local_slot`.
//!
//! Key details:
//! - A promotion after a loop must not suppress releases inside that loop.
//! - Scalar and already-ref-bound slots must not retain unnecessary raw-slot releases.

use super::*;

/// Emits textual EIR for one source fixture with the requested IR optimizer mode.
fn emit_release_ir(source: &str, ir_opt: bool) -> String {
    let dir = make_cli_test_dir("elephc_release_local_slot_emit_ir");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("failed to write PHP fixture");
    let mode = if ir_opt { "--ir-opt=on" } else { "--ir-opt=off" };
    let output = elephc_cli_command(&dir)
        .arg("--emit-ir")
        .arg(mode)
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

/// Compiles and runs one heap-debug fixture through the CLI in a fixed optimizer mode.
fn run_release_fixture(source: &str, ir_opt: bool) -> (String, String) {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "elephc_release_local_slot_runtime_{}_{}",
        std::process::id(),
        id
    ));
    fs::create_dir_all(&dir).expect("failed to create CLI fixture directory");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("failed to write PHP fixture");
    let mode = if ir_opt { "--ir-opt=on" } else { "--ir-opt=off" };
    let compile = elephc_cli_command(&dir)
        .arg("--heap-debug")
        .arg(mode)
        .arg(&php_path)
        .output()
        .expect("failed to compile ReleaseLocalSlot fixture");
    assert!(
        compile.status.success(),
        "fixture compilation failed: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let output = Command::new(dir.join("main"))
        .output()
        .expect("failed to run ReleaseLocalSlot fixture");
    assert!(
        output.status.success(),
        "fixture execution failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout should be UTF-8");
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    let _ = fs::remove_dir_all(&dir);
    (stdout, stderr)
}

/// Verifies a late by-reference promotion preserves the loop release in both EIR modes.
#[test]
fn test_release_local_slot_survives_late_ref_promotion_with_optimizer_on_and_off() {
    let source = r#"<?php
$x = 0;
for ($i = 0; $i < 100; $i++) {
    $x = 0;
    $x++;
}
$alias =& $x;
echo $alias;
"#;

    for ir_opt in [false, true] {
        let ir = emit_release_ir(source, ir_opt);
        let release = ir
            .find("release_local_slot")
            .unwrap_or_else(|| panic!("missing deferred release with ir_opt={ir_opt}:\n{ir}"));
        let promotion = ir
            .find("promote_local_ref_cell")
            .unwrap_or_else(|| panic!("missing ref-cell promotion with ir_opt={ir_opt}:\n{ir}"));
        assert!(
            release < promotion,
            "late promotion must remain after the loop release with ir_opt={ir_opt}:\n{ir}"
        );

        let (stdout, stderr) = run_release_fixture(source, ir_opt);
        assert_eq!(stdout, "1");
        assert!(
            stderr.contains("HEAP DEBUG: leak summary: clean"),
            "expected clean heap with ir_opt={ir_opt}, got: {stderr}"
        );
    }
}

/// Verifies deferred releases are pruned when the loop-carried slot stays scalar.
#[test]
fn test_release_local_slot_is_pruned_for_scalar_loop_slot_in_both_modes() {
    let source = r#"<?php
for ($i = 0; $i < $argc; $i++) {
    $value = 7;
    echo $value;
}
"#;
    for ir_opt in [false, true] {
        let ir = emit_release_ir(source, ir_opt);
        assert!(
            !ir.contains("release_local_slot"),
            "scalar slot should not retain deferred release with ir_opt={ir_opt}:\n{ir}"
        );
    }
}

/// Verifies a slot promoted before the loop uses ref-cell stores without raw-slot releases.
#[test]
fn test_release_local_slot_is_excluded_for_already_ref_bound_slot() {
    let source = r#"<?php
$value = 0;
$alias =& $value;
for ($i = 0; $i < $argc; $i++) {
    $value = 0;
    $value++;
}
echo $alias;
"#;
    for ir_opt in [false, true] {
        let ir = emit_release_ir(source, ir_opt);
        assert!(ir.contains("promote_local_ref_cell"));
        assert!(ir.contains("store_ref_cell"));
        assert!(
            !ir.contains("release_local_slot"),
            "ref-bound slot should not use raw release with ir_opt={ir_opt}:\n{ir}"
        );
    }
}
