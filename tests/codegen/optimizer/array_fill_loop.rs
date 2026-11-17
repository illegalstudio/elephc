//! Purpose:
//! End-to-end coverage for the constant-bound typed-array fill-loop pass.
//!
//! Called from:
//! - The codegen optimizer suite through `optimizer`.
//!
//! Key details:
//! - A pure fill loop reserves capacity, uses the reserved append, and hoists the invariant
//!   array-pointer load to the preheader.
//! - Reassigning the array inside the loop keeps the generic checked append and load.

use crate::support::*;

/// Emits the optimized EIR text for one fixture.
fn optimized_ir(source: &str) -> String {
    let dir = make_cli_test_dir("elephc_array_fill_loop_ir");
    let path = dir.join("main.php");
    fs::write(&path, source).expect("write array fill loop fixture");
    let output = elephc_cli_command(&dir)
        .arg("--emit-ir")
        .arg(&path)
        .output()
        .expect("run elephc --emit-ir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let _ = fs::remove_dir_all(dir);
    String::from_utf8(output.stdout).expect("EIR is UTF-8")
}

/// Returns the first `while` body slice of an EIR module dump.
fn first_while_body(ir: &str) -> &str {
    ir.split("while.body")
        .nth(1)
        .expect("fixture has a while body")
        .split("while.exit")
        .next()
        .expect("fixture has a while exit")
}

/// A constant-bound fill loop reads the array pointer once from the preheader.
#[test]
fn test_array_fill_loop_hoists_array_load() {
    let source = r#"<?php
$data = [];
$i = 0;
while ($i < 32) {
    $data[] = $i % 7;
    $i += 1;
}
echo $data[3], $data[31], "\n";
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "33\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
    let ir = optimized_ir(source);
    let body = first_while_body(&ir);
    assert!(!body.contains("load_local"), "{body}");
}

/// Reassigning the array inside the loop keeps the checked append and its load.
#[test]
fn test_array_fill_loop_keeps_reassigned_array() {
    let source = r#"<?php
$data = [];
$i = 0;
while ($i < 10) {
    $data = array_slice($data, 0);
    $data[] = $i;
    $i += 1;
}
echo count($data), ":", $data[0], $data[9], "\n";
"#;
    assert_eq!(compile_and_run(source), "10:09\n");
    let ir = optimized_ir(source);
    let body = first_while_body(&ir);
    assert!(body.contains("load_local"), "{body}");
}

/// A non-zero fill start keeps the length-based append so keys stay zero-based.
#[test]
fn test_array_fill_loop_keeps_nonzero_start_keys() {
    let source = r#"<?php
$data = [];
$i = 5;
while ($i < 10) {
    $data[] = $i;
    $i += 1;
}
echo count($data), ":", $data[0], ":", $data[4], "\n";
"#;
    assert_eq!(compile_and_run(source), "5:5:9\n");
}

/// Every supported target lowers the reserved append, the rotating remainder and the final
/// length publication.
#[test]
fn test_array_fill_loop_emits_on_all_supported_targets() {
    let source = r#"<?php
$data = [];
$i = 0;
while ($i < 32) {
    $data[] = $i % 7;
    $i += 1;
}
echo $data[3], "\n";
"#;
    for target in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let dir = make_cli_test_dir("elephc_array_fill_loop_targets");
        let path = dir.join("main.php");
        fs::write(&path, source).expect("write array fill loop target fixture");
        let ir = elephc_cli_command(&dir)
            .args(["--emit-ir", "--target", target])
            .arg(&path)
            .output()
            .expect("run elephc --emit-ir");
        assert!(
            ir.status.success(),
            "emit-ir for {target} failed: {}",
            String::from_utf8_lossy(&ir.stderr)
        );
        let ir = String::from_utf8(ir.stdout).expect("EIR is UTF-8");
        for op in ["array_push_reserved", "iwrap_increment", "array_set_length"] {
            assert!(ir.contains(op), "{target}: missing {op} in EIR");
        }
        let assembly = elephc_cli_command(&dir)
            .args(["--emit", "staticlib", "--emit-asm", "--target", target])
            .arg(&path)
            .output()
            .expect("run elephc --emit-asm");
        assert!(
            assembly.status.success(),
            "emit-asm for {target} failed: {}",
            String::from_utf8_lossy(&assembly.stderr)
        );
        let text = fs::read_to_string(path.with_extension("s"))
            .expect("read array fill loop target assembly");
        assert!(!text.is_empty(), "{target}: empty assembly");
        let _ = fs::remove_dir_all(dir);
    }
}

/// The x86_64 wrap select zeroes its source before the compare so `cmove` sees the flags.
#[test]
fn test_array_fill_loop_x86_wrap_zeroes_before_compare() {
    let source = r#"<?php
$data = [];
$i = 0;
while ($i < 32) {
    $data[] = $i % 7;
    $i += 1;
}
echo $data[3], "\n";
"#;
    let dir = make_cli_test_dir("elephc_array_fill_loop_x86_wrap");
    let path = dir.join("main.php");
    fs::write(&path, source).expect("write x86 wrap fixture");
    let output = elephc_cli_command(&dir)
        .args(["--emit-asm", "--target", "linux-x86_64"])
        .arg(&path)
        .output()
        .expect("run elephc --emit-asm");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let assembly = fs::read_to_string(path.with_extension("s"))
        .expect("read x86 wrap assembly");
    let start = assembly
        .find("array_push_reserved")
        .expect("the fill append in x86 assembly");
    let snippet = &assembly[start..];
    let add = snippet.find("add rax, 1").expect("rotating increment");
    let zero = snippet.find("xor ecx, ecx").expect("wrap source zeroing");
    let compare = snippet.find("cmp rax, r10").expect("divisor comparison");
    let select = snippet.find("cmove rax, rcx").expect("wrap select");
    assert!(
        add < zero && zero < compare && compare < select,
        "the wrap zeroing must precede the compare:\n{}",
        &snippet[..snippet.len().min(600)]
    );
    let _ = fs::remove_dir_all(dir);
}
