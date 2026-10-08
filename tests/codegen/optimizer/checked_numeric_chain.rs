//! Purpose:
//! End-to-end coverage for allocation-free checked numeric chain specialization.
//!
//! Called from:
//! - `cargo test --test codegen_tests optimizer::checked_numeric_chain`.
//!
//! Key details:
//! - Tests pin optimized EIR, both target architecture lowerings, overflow precision at the
//!   exact PHP promotion point, optimizer on/off equivalence, and loop allocation removal.

use super::*;

/// Emits the main function's EIR with explicit optimizer arguments.
fn emit_main_ir(source: &str, extra_args: &[&str]) -> String {
    let dir = make_cli_test_dir("elephc_checked_numeric_chain_ir");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("write checked numeric chain EIR fixture");
    let mut command = elephc_cli_command(&dir);
    command.arg("--emit-ir").args(extra_args).arg(&php_path);
    let output = command.output().expect("run elephc build --emit-ir");
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

/// Emits target-specific user assembly for one optimized source fixture.
fn emit_target_assembly(source: &str, target: &str) -> String {
    let dir = make_cli_test_dir("elephc_checked_numeric_chain_asm");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("write checked numeric chain assembly fixture");
    let mut command = elephc_cli_command(&dir);
    command
        .arg("--emit-asm")
        .arg("--target")
        .arg(target)
        .arg(&php_path);
    let output = command.output().expect("run elephc build --emit-asm");
    assert!(
        output.status.success(),
        "emit-asm for {target} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let assembly = fs::read_to_string(php_path.with_extension("s"))
        .expect("read checked numeric chain assembly");
    let _ = fs::remove_dir_all(dir);
    assembly
}

/// Compiles and runs one source fixture, returning captured stdout and stderr.
fn compile_and_run_variant(source: &str, extra_args: &[&str]) -> (String, String) {
    let dir = make_cli_test_dir("elephc_checked_numeric_chain_run");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("write checked numeric chain runtime fixture");
    let mut command = elephc_cli_command(&dir);
    command.args(extra_args).arg(&php_path);
    let compile = command.output().expect("compile checked numeric chain fixture");
    assert!(
        compile.status.success(),
        "compile failed: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let run = run_binary(&dir.join("main"), &dir);
    assert!(
        run.status.success(),
        "fixture failed: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    let stdout = String::from_utf8(run.stdout).expect("program stdout is UTF-8");
    let stderr = String::from_utf8(run.stderr).expect("program stderr is UTF-8");
    let _ = fs::remove_dir_all(dir);
    (stdout, stderr)
}

/// Replaces the benchmark's boxed multiply/add region with one fused scalar EIR instruction.
#[test]
fn test_checked_numeric_chain_fuses_benchmark_shape() {
    let source = r#"<?php
$h = $argc;
$n = $argc + 4;
for ($i = 1; $i <= $n; $i++) {
    $h = ($h * 31 + $i) & 0x3fffffff;
}
echo $h;
"#;
    let unoptimized = emit_main_ir(source, &["--no-ir-opt"]);
    let optimized = emit_main_ir(source, &[]);

    assert!(unoptimized.contains("= ichecked_mul "));
    assert!(unoptimized.contains("= mixed_numeric_binop "));
    assert!(optimized.contains("= ichecked_numeric_chain_to_int "));
    assert!(optimized.contains("[mul,add]"));
    assert!(!optimized.contains("= mixed_numeric_binop "));
}

/// Checks signed-overflow guards and float suffixes via Linux x86_64 and AArch64 targets.
#[test]
fn test_checked_numeric_chain_has_target_aware_fast_and_slow_paths() {
    let source = "<?php $h = $argc; echo ($h * 31 + $argc) & 0x3fffffff;";
    let x86 = emit_target_assembly(source, "linux-x86_64");
    assert!(x86.contains("op=ichecked_numeric_chain_to_int"));
    assert!(x86.contains("imul rax"));
    assert!(x86.contains("jo "));
    assert!(x86.contains("mulsd xmm0, xmm1"));
    assert!(x86.contains("addsd xmm0, xmm1"));
    assert!(!x86.contains("call __rt_mixed_numeric_add"));

    let arm = emit_target_assembly(source, "linux-aarch64");
    assert!(arm.contains("op=ichecked_numeric_chain_to_int"));
    assert!(arm.contains("smulh "));
    assert!(arm.contains("b.ne "));
    assert!(arm.contains("b.vs "));
    assert!(arm.contains("fmul d0, d0, d1"));
    assert!(arm.contains("fadd d0, d0, d1"));
    assert!(!arm.contains("bl __rt_mixed_numeric_add"));
}

/// Preserves integer precision until the exact operation that first overflows.
#[test]
fn test_checked_numeric_chain_promotes_at_exact_overflow_point() {
    let source = r#"<?php
$one = $argc;
echo (int) ((9007199254740993 + $one) * 1024);
"#;
    let (unoptimized, unoptimized_stderr) =
        compile_and_run_variant(source, &["--no-ir-opt"]);
    let (optimized, optimized_stderr) = compile_and_run_variant(source, &[]);

    assert_eq!(optimized, unoptimized);
    assert_eq!(optimized_stderr, unoptimized_stderr);
    assert_eq!(optimized, "-9223372036854773760");
}

/// Keeps add, sub, and mul overflow suffixes equivalent with optimization enabled or disabled.
#[test]
fn test_checked_numeric_chain_overflow_suffixes_match_unoptimized_php_semantics() {
    let source = r#"<?php
$one = $argc;
$three = $argc + 2;
echo (int) (PHP_INT_MAX * $three + 5), "|";
echo (int) ((PHP_INT_MAX - 3) + $one + $one + $one + $one), "|";
echo (int) ((PHP_INT_MIN + $one) - PHP_INT_MAX);
"#;
    let (unoptimized, unoptimized_stderr) =
        compile_and_run_variant(source, &["--no-ir-opt"]);
    let (optimized, optimized_stderr) = compile_and_run_variant(source, &[]);
    assert_eq!(optimized, unoptimized);
    assert_eq!(optimized_stderr, unoptimized_stderr);
}

/// Matches the checked-multiply sink when `INT_MIN * -1` enters a fused overflow path.
#[test]
fn test_checked_numeric_chain_int_min_overflow_matches_checked_mul_sink() {
    let source = r#"<?php
$one = $argc;
echo (int) (PHP_INT_MIN * -$one), "|";
echo (int) (PHP_INT_MIN * -$one + $one);
"#;
    let optimized_ir = emit_main_ir(source, &[]);
    assert!(
        optimized_ir.contains("= ichecked_mul_to_int "),
        "optimized EIR did not contain the checked multiply sink:\n{optimized_ir}"
    );
    assert!(
        optimized_ir.contains("= ichecked_numeric_chain_to_int "),
        "optimized EIR did not contain the fused chain:\n{optimized_ir}"
    );
    assert!(
        optimized_ir.contains("[mul,add]"),
        "optimized EIR did not retain the multiply/add order:\n{optimized_ir}"
    );

    let (unoptimized, unoptimized_stderr) =
        compile_and_run_variant(source, &["--no-ir-opt"]);
    let (optimized, optimized_stderr) = compile_and_run_variant(source, &[]);
    assert_eq!(optimized, unoptimized);
    assert_eq!(optimized_stderr, unoptimized_stderr);
    assert_eq!(optimized, "-9223372036854775808|-9223372036854775808");
}

/// Eliminates all transient heap allocations in the benchmark loop's optimized hot path.
#[test]
fn test_checked_numeric_chain_benchmark_loop_allocates_nothing() {
    let source = r#"<?php
$h = 1;
$n = 2000;
for ($i = 1; $i <= $n; $i++) {
    $h = ($h * 31 + $i) & 0x3fffffff;
}
echo $h;
"#;
    let (unoptimized_stdout, unoptimized_stderr) =
        compile_and_run_variant(source, &["--no-ir-opt", "--gc-stats"]);
    let (optimized_stdout, optimized_stderr) = compile_and_run_variant(source, &["--gc-stats"]);

    assert_eq!(optimized_stdout, unoptimized_stdout);
    assert!(unoptimized_stderr.contains("GC: allocs="));
    assert!(!unoptimized_stderr.contains("GC: allocs=0 "));
    assert!(optimized_stderr.contains("GC: allocs=0 frees=0"));
}
