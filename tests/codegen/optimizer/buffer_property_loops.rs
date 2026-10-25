//! Purpose:
//! Checks buffer-property loop hoisting, zero-trip reads and stale-handle diagnostic timing.
//!
//! Called from:
//! - The optimizer codegen integration suite.
//!
//! Key details:
//! - Native fixtures compare both optimizer modes, and the guarded form emits on every target.

use super::*;

/// Runs a fixture in one optimizer mode, leaving execution failures available to the assertion.
fn run_property_loop(source: &str, optimized: bool) -> std::process::Output {
    let dir = make_cli_test_dir("elephc_property_loop");
    let path = dir.join("main.php");
    fs::write(&path, source).unwrap();
    let mut command = elephc_cli_command(&dir);
    command.arg("--heap-debug");
    if !optimized { command.arg("--no-ir-opt"); }
    let compile = command.arg(&path).output().unwrap();
    assert!(compile.status.success(), "{}", String::from_utf8_lossy(&compile.stderr));
    let output = run_binary(&dir.join("main"), &dir);
    fs::remove_dir_all(dir).unwrap();
    output
}

/// The property read and descriptor move together into the taken-only entry block.
#[test]
fn test_buffer_property_loop_guarded_resolution_matches_both_modes() {
    let source = include_str!("../../../examples/buffer-property-loop/main.php");
    for optimized in [false, true] {
        let output = run_property_loop(source, optimized);
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(String::from_utf8_lossy(&output.stdout), "7770");
        assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"));
    }
    let dir = make_cli_test_dir("elephc_property_loop_ir");
    let path = dir.join("main.php");
    fs::write(&path, source).unwrap();
    let emitted = elephc_cli_command(&dir).arg("--emit-ir").arg(&path).output().unwrap();
    assert!(emitted.status.success(), "{}", String::from_utf8_lossy(&emitted.stderr));
    let ir = String::from_utf8(emitted.stdout).unwrap();
    let fill = ir.split_once("  function LoopSlots::fill(").expect("fill method").1
        .split("\n  function ").next().unwrap();
    assert!(fill.contains("buffer_property_entry_"), "{fill}");
    assert_eq!(fill.matches("= buffer_resolve ").count(), 1, "{fill}");
    assert!(fill.contains("buffer_set_resolved"), "{fill}");
    fs::remove_dir_all(dir).unwrap();
}

/// An uninitialized property stays unread on zero-trip and fails on the first entered read.
#[test]
fn test_buffer_property_loop_zero_trip_preserves_uninitialized_property() {
    let source = r#"<?php
final class MissingLoopSlots {
    public buffer<int> $slots;
    public function fill(int $count): void {
        for ($index = 0; $index < $count; $index++) { $this->slots[$index] = 7; }
    }
}
$slots = new MissingLoopSlots();
$slots->fill($argc - 1);
echo 'zero|';
$slots->fill($argc);
"#;
    for optimized in [false, true] {
        let output = run_property_loop(source, optimized);
        assert!(!output.status.success());
        assert_eq!(String::from_utf8_lossy(&output.stdout), "zero|");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("must not be accessed before initialization"), "{stderr}");
    }
}

/// Non-faulting entry resolution leaves a stale-handle fatal at the first entered access.
#[test]
fn test_buffer_property_loop_stale_handle_fails_only_after_entry() {
    let source = r#"<?php
final class StaleLoopSlots {
    public buffer<int> $slots;
    public function __construct() { $this->slots = buffer_new<int>(4); }
    public function fill(int $count): void {
        for ($index = 0; $index < $count; $index++) { $this->slots[$index] = 7; }
    }
}
$slots = new StaleLoopSlots();
$handle = $slots->slots;
buffer_free($handle);
$slots->fill($argc - 1);
echo 'zero|';
$slots->fill($argc);
"#;
    for optimized in [false, true] {
        let output = run_property_loop(source, optimized);
        assert!(!output.status.success());
        assert_eq!(String::from_utf8_lossy(&output.stdout), "zero|");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("use of buffer after buffer_free()"), "{stderr}");
    }
}

/// The property-loop example emits through every supported target-aware backend.
#[test]
fn test_buffer_property_loop_emits_on_all_supported_targets() {
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let dir = make_cli_test_dir("elephc_property_loop_target");
        let path = dir.join("main.php");
        fs::write(&path, include_str!("../../../examples/buffer-property-loop/main.php")).unwrap();
        let mut command = elephc_cli_command(&dir);
        command.args(["--emit-asm", "--target", target]);
        if target.starts_with("ios-") { command.args(["--emit", "staticlib"]); }
        let output = command.arg(&path).output().unwrap();
        assert!(output.status.success(), "{target}: {}", String::from_utf8_lossy(&output.stderr));
        fs::remove_dir_all(dir).unwrap();
    }
}
