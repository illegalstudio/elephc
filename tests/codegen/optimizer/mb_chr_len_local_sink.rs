//! Purpose:
//! Verifies that a local assigned from `mb_chr()` and observed only through `strlen()` or
//! `ord($local[0])` is retargeted to the allocation-free observation kernels.
//!
//! Called from:
//! - The codegen optimizer suite through `optimizer`.
//!
//! Key details:
//! - Multiple `strlen()` observations share one retargeted `MbChrLen` call; `ord($local[0])`
//!   observations use `MbChrFirst`, and a mixed local uses `MbChrPacked` so both observations
//!   share one encoding pass.
//! - UTF-16, U+0000, invalid codepoints, and the bridge fallback keep PHP's byte length and
//!   first-byte values.
//! - Any value observation (`echo $local`, identity checks) or a nonzero string offset keeps
//!   the boxed `mb_chr()` path.

use crate::support::*;

/// Emits the optimized `main` function body for one fixture.
fn emit_main_ir(source: &str) -> String {
    let dir = make_cli_test_dir("elephc_mb_chr_len_local_ir");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("write mb_chr length fixture");
    let output = elephc_cli_command(&dir)
        .arg("--emit-ir")
        .arg("--ir-opt=on")
        .arg(&php_path)
        .output()
        .expect("run elephc --emit-ir");
    assert!(
        output.status.success(),
        "emit-ir failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).expect("EIR is UTF-8");
    let main = text
        .split("function main(")
        .nth(1)
        .expect("EIR contains main")
        .split("\n  function ")
        .next()
        .expect("main body")
        .to_string();
    let _ = fs::remove_dir_all(dir);
    main
}

/// UTF-8 locals observed only by `strlen()` use the length helper and keep each byte width.
#[test]
fn test_mb_chr_len_local_sink_fuses_strlen_only_locals() {
    let source = r#"<?php
$a = mb_chr(0x20AC, "UTF-8");
echo strlen($a), "|";
$b = mb_chr(0x1F600, "UTF-8");
echo strlen($b);
"#;
    assert_eq!(compile_and_run(source), "3|4");
    let main = emit_main_ir(source);
    assert_eq!(main.matches("runtime.mb_chr_len").count(), 2, "{main}");
    assert!(!main.contains("runtime.mb_chr "), "{main}");
    assert!(!main.contains("exception.guard_owned"), "{main}");
    assert!(!main.contains("exception.unguard_owned"), "{main}");
}

/// Multiple length observations share one retargeted call; UTF-16, invalid codepoints, and
/// the bridge fallback keep PHP's exact byte length.
#[test]
fn test_mb_chr_len_local_sink_multiple_and_fallback_paths() {
    let source = r#"<?php
$a = mb_chr(65, "UTF-8");
echo strlen($a), "|", strlen($a), "|";
$b = mb_chr(65, "UTF-16");
echo strlen($b), "|";
$c = mb_chr(-1, "UTF-8");
echo strlen($c);
"#;
    assert_eq!(compile_and_run(source), "1|1|2|0");
    let main = emit_main_ir(source);
    assert_eq!(main.matches("runtime.mb_chr_len").count(), 3, "{main}");
    assert!(!main.contains("runtime.mb_chr "), "{main}");
}

/// A loop-hoisted UTF-8 encoding drops the bridge's owned-argument exception guard.
#[test]
fn test_mb_chr_len_local_sink_drops_pure_argument_guard() {
    let source = r#"<?php
$codepoints = [0x41, 0xA9, 0x20AC, 0x1F600];
$n = 0;
for ($i = 0; $i < 32; $i++) {
    $encoded = mb_chr($codepoints[$i & 3], "UTF-8");
    $n += strlen($encoded);
}
echo $n, "\n";
"#;
    assert_eq!(compile_and_run(source), "80\n");
    let main = emit_main_ir(source);
    assert!(main.contains("runtime.mb_chr_len"), "{main}");
    assert!(!main.contains("exception.guard_owned"), "{main}");
    assert!(!main.contains("exception.unguard_owned"), "{main}");
}

/// A packed `array<int>` element is a tagged scalar: an integer payload uses the
/// allocation-free length kernel, while a missing-key null keeps the bridge's deprecation.
#[test]
fn test_mb_chr_len_local_sink_handles_tagged_array_elements() {
    let source = r#"<?php
$codepoints = [65];
$hit = mb_chr($codepoints[$argc - 1], "UTF-8");
$miss = mb_chr($codepoints[$argc], "UTF-8");
echo strlen($hit), "|", strlen($miss), "\n";
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "1|1\n");
    assert!(out.stderr.contains("Deprecated: mb_chr()"), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    let main = emit_main_ir(source);
    // Both observations keep the length kernel: the tagged-null fallback lives in codegen and
    // still reproduces the bridge's deprecation at run time.
    assert_eq!(main.matches("runtime.mb_chr_len").count(), 2, "{main}");
}

/// A dynamic encoding keeps the bridge's owned-argument exception guard.
#[test]
fn test_mb_chr_len_local_sink_keeps_impure_argument_guard() {
    let source = r#"<?php
$enc = $argc > 0 ? "UTF-16" : "UTF-8";
$n = 0;
for ($i = 0; $i < 8; $i++) {
    $encoded = mb_chr($i + 65, $enc);
    $n += strlen($encoded);
}
echo $n, "\n";
"#;
    assert_eq!(compile_and_run(source), "16\n");
    let main = emit_main_ir(source);
    assert!(main.contains("runtime.mb_chr_len"), "{main}");
    assert!(main.contains("exception.guard_owned"), "{main}");
    assert!(main.contains("exception.unguard_owned"), "{main}");
}

/// Echoing the string value keeps the boxed call because the length helper no longer returns it.
#[test]
fn test_mb_chr_len_local_sink_keeps_value_observation() {
    let source = r#"<?php
$x = mb_chr(65, "UTF-8");
echo strlen($x), "|", $x;
"#;
    assert_eq!(compile_and_run(source), "1|A");
    let main = emit_main_ir(source);
    assert_eq!(main.matches("runtime.mb_chr ").count(), 1, "{main}");
    assert!(!main.contains("runtime.mb_chr_len"), "{main}");
}

/// A value identity check also keeps the boxed result, including the `false` branch.
#[test]
fn test_mb_chr_len_local_sink_keeps_identity_observation() {
    let source = r#"<?php
$x = mb_chr(-1, "UTF-8");
echo strlen($x), "|", $x === false ? "false" : "string";
"#;
    assert_eq!(compile_and_run(source), "0|false");
    let main = emit_main_ir(source);
    assert_eq!(main.matches("runtime.mb_chr ").count(), 1, "{main}");
    assert!(!main.contains("runtime.mb_chr_len"), "{main}");
}

/// `ord($local[0])` observations share one allocation-free first-byte call.
#[test]
fn test_mb_chr_ord_local_sink_fuses_first_byte() {
    let source = r#"<?php
$a = mb_chr(0x20AC, "UTF-8");
echo ord($a[0]), "|", ord($a[0]), "|";
$b = mb_chr(0x1F600, "UTF-8");
echo ord($b[0]), "|";
$c = mb_chr(65, "UTF-8");
echo ord($c[0]);
"#;
    assert_eq!(compile_and_run(source), "226|226|240|65");
    let main = emit_main_ir(source);
    assert_eq!(main.matches("runtime.mb_chr_first").count(), 3, "{main}");
    assert!(!main.contains("runtime.mb_chr "), "{main}");
}

/// A mixed `strlen($local) + ord($local[0])` shape uses one packed call and extracts each half.
#[test]
fn test_mb_chr_packed_local_sink_fuses_length_and_first_byte() {
    let source = r#"<?php
$codepoints = [0x41, 0xA9, 0x20AC, 0x1F600];
$n = 0;
for ($i = 0; $i < 4; $i++) {
    $x = mb_chr($codepoints[$i], "UTF-8");
    $n += strlen($x) + ord($x[0]);
}
echo $n;
"#;
    assert_eq!(compile_and_run(source), "735");
    let main = emit_main_ir(source);
    assert_eq!(main.matches("runtime.mb_chr_packed").count(), 1, "{main}");
    assert!(!main.contains("runtime.mb_chr "), "{main}");
    assert_eq!(main.matches("ishr_a").count(), 1, "{main}");
    assert_eq!(main.matches("ibit_and").count(), 1, "{main}");
}

/// U+0000 stays a one-byte string, invalid codepoints stay false, and the non-UTF-8 bridge
/// still reports PHP's byte length and first byte.
#[test]
fn test_mb_chr_packed_handles_nul_invalid_and_non_utf8() {
    let source = r#"<?php
$x = mb_chr(0, "UTF-8");
echo strlen($x), ":", ord($x[0]), "|";
$y = mb_chr(-1, "UTF-8");
echo strlen($y), ":", ord($y[0]), "|";
$z = mb_chr(65, "UTF-16");
echo strlen($z), ":", ord($z[0]);
"#;
    assert_eq!(compile_and_run(source), "1:0|0:0|2:0");
    let main = emit_main_ir(source);
    assert_eq!(main.matches("runtime.mb_chr_packed").count(), 3, "{main}");
    assert!(!main.contains("runtime.mb_chr "), "{main}");
}

/// Echoing the string value keeps the boxed call because the observation no longer returns it.
#[test]
fn test_mb_chr_ord_local_sink_keeps_value_observation() {
    let source = r#"<?php
$x = mb_chr(65, "UTF-8");
echo ord($x[0]), "|", $x;
"#;
    assert_eq!(compile_and_run(source), "65|A");
    let main = emit_main_ir(source);
    assert_eq!(main.matches("runtime.mb_chr ").count(), 1, "{main}");
    assert!(!main.contains("runtime.mb_chr_first"), "{main}");
    assert!(!main.contains("runtime.mb_chr_packed"), "{main}");
}

/// A nonzero offset observes a later byte, so the first-byte sink must not apply.
#[test]
fn test_mb_chr_ord_local_sink_keeps_nonzero_offset() {
    let source = r#"<?php
$x = mb_chr(0x20AC, "UTF-8");
echo ord($x[1]);
"#;
    assert_eq!(compile_and_run(source), "130");
    let main = emit_main_ir(source);
    assert_eq!(main.matches("runtime.mb_chr ").count(), 1, "{main}");
    assert!(!main.contains("runtime.mb_chr_first"), "{main}");
    assert!(!main.contains("runtime.mb_chr_packed"), "{main}");
}

/// The first-byte and packed kernels release the bridge result and every temporary cleanly.
#[test]
fn test_mb_chr_observation_heap_debug_is_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = mb_chr(0x20AC, "UTF-8");
$b = mb_chr(-1, "UTF-8");
$c = mb_chr(65, "UTF-16");
echo strlen($a) + ord($a[0]), "|", strlen($b) + ord($b[0]), "|", strlen($c) + ord($c[0]);
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "229|0|2");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// Compiles and runs one fixture through the CLI with an explicit EIR optimizer mode.
fn run_with_ir_opt(source: &str, mode: &str) -> String {
    let dir = make_cli_test_dir("elephc_mb_chr_observation_ir_opt");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("write mb_chr observation IR-opt fixture");
    let compiled = elephc_cli_command(&dir)
        .arg(mode)
        .arg(&php_path)
        .output()
        .expect("compile mb_chr observation IR-opt fixture");
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let output = std::process::Command::new(dir.join("main"))
        .output()
        .expect("run mb_chr observation IR-opt fixture");
    let _ = fs::remove_dir_all(dir);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("mb_chr observation output is UTF-8")
}

/// The packed observation produces the same output with the EIR optimizer on and off.
#[test]
fn test_mb_chr_observation_ir_opt_parity() {
    let source = r#"<?php
$codepoints = [0x41, 0xA9, 0x20AC, 0x1F600, 0];
$n = 0;
for ($i = 0; $i < 5; $i++) {
    $x = mb_chr($codepoints[$i], "UTF-8");
    $n += strlen($x) + ord($x[0]);
}
echo $n, "\n";
"#;
    assert_eq!(run_with_ir_opt(source, "--ir-opt=on"), "736\n");
    assert_eq!(run_with_ir_opt(source, "--ir-opt=off"), "736\n");
}

/// Emits target assembly for one fixture, using staticlib output for the iOS targets.
fn emit_target_assembly(source: &str, target: &str) -> String {
    let dir = make_cli_test_dir("elephc_mb_chr_observation_targets");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("write mb_chr observation target fixture");
    let mut command = elephc_cli_command(&dir);
    command.args(["--emit", "staticlib"]);
    command
        .arg("--emit-asm")
        .arg("--target")
        .arg(target)
        .arg(&php_path);
    let output = command.output().expect("run elephc --emit-asm");
    assert!(
        output.status.success(),
        "emit-asm for {target} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let assembly = fs::read_to_string(php_path.with_extension("s"))
        .expect("read mb_chr observation target assembly");
    let _ = fs::remove_dir_all(dir);
    assembly
}

/// Every supported target emits the lead-byte classifiers and the bridge fallback.
#[test]
fn test_mb_chr_observation_emits_all_supported_targets() {
    let source = r#"<?php
#[Export]
function observe(int $cp): int {
    $x = mb_chr($cp, "UTF-8");
    $y = mb_chr(65, "UTF-16");
    return strlen($x) + ord($x[0]) + strlen($y) + ord($y[0]);
}
"#;
    for target in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let assembly = emit_target_assembly(source, target);
        assert!(assembly.contains("mb_chr_observation_two"), "{target}");
        assert!(assembly.contains("mb_chr_observation_three"), "{target}");
        assert!(assembly.contains("mb_chr_observation_four"), "{target}");
        assert!(assembly.contains("__rt_mixed_unbox"), "{target}");
        assert!(assembly.contains("__rt_mbstring_native"), "{target}");
        if target == "linux-x86_64" {
            assert!(assembly.contains("or r10, 0xC0"), "{target}");
            assert!(assembly.contains("or r10, 0xF0"), "{target}");
            assert!(assembly.contains("add rax, 0x200"), "{target}");
        } else {
            assert!(assembly.contains("orr x0, x9, #0xC0"), "{target}");
            assert!(assembly.contains("orr x0, x9, #0xF0"), "{target}");
            assert!(assembly.contains("add x0, x0, #0x200"), "{target}");
        }
    }
}
