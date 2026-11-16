//! Purpose:
//! Regression coverage for checked arithmetic at float parameters and strict identity
//! consumers (task a0fca1a4).
//!
//! Called from:
//! - The codegen integration test harness.
//!
//! Key details:
//! - Runtime inputs prevent AST folding, and both EIR modes keep PHP overflow promotion.
//! - Allocation counts compare each workload with an otherwise identical control variant.

use super::*;

/// Cross-checks PHP-compatible fixtures when the focused run opts into the PHP oracle.
fn php_oracle(source: &str, expected: &str) {
    if std::env::var_os("ELEPHC_PHP_CHECK").is_none() { return; }
    let output = Command::new("php").args(["-n", "-r", source.trim_start_matches("<?php")])
        .output().expect("run PHP numeric consumer oracle");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout), expected);
}

/// Compiles one variant and validates its heap before returning output and diagnostics.
fn run(source: &str, optimized: bool) -> (String, String) {
    let dir = make_cli_test_dir("elephc_numeric_consumers");
    let path = dir.join("main.php");
    fs::write(&path, source).unwrap();
    let mut command = elephc_cli_command(&dir);
    command.args(["--heap-debug", "--gc-stats"]);
    if !optimized { command.arg("--no-ir-opt"); }
    let compile = command.arg(&path).output().unwrap();
    assert!(compile.status.success(), "{}", String::from_utf8_lossy(&compile.stderr));
    let output = run_binary(&path.with_extension(""), &dir);
    fs::remove_dir_all(&dir).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(output.status.success(), "{stdout}\n{stderr}");
    assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
    (stdout, stderr)
}

/// Extracts the allocation count without depending on heap-debug line ordering.
fn gc_allocs(stderr: &str) -> u64 {
    stderr.split("GC: allocs=").nth(1).unwrap()
        .split_whitespace().next().unwrap().parse().unwrap()
}

/// A checked product passed to a `float` parameter in a loop allocates no Mixed cells.
#[test]
fn test_numeric_consumers_float_parameter_loop_is_allocation_free() {
    let source = r#"<?php
function sink(float $value): float { return $value; }
function run(int $count): void {
    $sum = 0.0;
    for ($i = 0; $i < $count; $i++) { $sum += sink(($i & 3) * 16); }
    echo $sum;
}
run(10000);
"#;
    php_oracle(source, "240000");
    let control = source.replace("($i & 3) * 16", "(float) ($i & 3)");
    for optimized in [false, true] {
        let (stdout, stderr) = run(source, optimized);
        let (_, control_stderr) = run(&control, optimized);
        assert_eq!(stdout, "240000");
        assert_eq!(gc_allocs(&stderr), gc_allocs(&control_stderr), "{stderr}\n{control_stderr}");
    }
}

/// Float parameters still receive PHP's promoted double after an overflowing tree.
#[test]
fn test_numeric_consumers_float_parameter_keeps_overflow_promotion() {
    let source = r#"<?php
function show(float $value): float { return $value; }
$one = $argc;
var_dump(show(PHP_INT_MAX + $one), show((PHP_INT_MAX + $one) - PHP_INT_MAX));
var_dump(show(9007199254740993 * $one), show(-$one * 3));
var_dump(show(value: PHP_INT_MIN - $one));
"#;
    let expected = "float(9.223372036854776E+18)\nfloat(0)\nfloat(9007199254740992)\n\
float(-3)\nfloat(-9.223372036854776E+18)\n";
    php_oracle(source, expected);
    for optimized in [false, true] {
        assert_eq!(run(source, optimized).0, expected);
    }
}

/// `!==` on a checked product costs no more than the frame-cell relational control.
///
/// Optimized code allocates nothing; `--no-ir-opt` keeps its assignment clone in both variants,
/// while the boxed literal partner of the old strict-equality call is gone.
#[test]
fn test_numeric_consumers_strict_identity_loop_is_allocation_free() {
    let source = r#"<?php
function run(int $count): void {
    $sum = 0;
    for ($i = 0; $i < $count; $i++) { $value = $i * 4; if ($value !== 20) { $sum ^= $i; } }
    echo $sum;
}
run(10000);
"#;
    php_oracle(source, "5");
    let control = source.replace("$value !== 20", "$value > 20");
    for optimized in [false, true] {
        let (stdout, stderr) = run(source, optimized);
        let (_, control_stderr) = run(&control, optimized);
        assert_eq!(stdout, "5");
        assert_eq!(gc_allocs(&stderr), gc_allocs(&control_stderr), "{stderr}\n{control_stderr}");
    }
}

/// Strict identity distinguishes promoted floats from integers and compares payloads exactly.
#[test]
fn test_numeric_consumers_strict_identity_keeps_type_identity() {
    let source = r#"<?php
$one = $argc;
$big = PHP_INT_MAX + $one;
$small = $one * 4;
$wrapped = PHP_INT_MIN;
echo var_export($big === PHP_INT_MIN, true), '|';
echo var_export($big === 9.2233720368547758E+18, true), '|';
echo var_export($big !== 9.2233720368547758E+18, true), '|';
echo var_export($small === 4, true), '|';
echo var_export($small === 4.0, true), '|';
echo var_export($small !== 5, true), '|';
echo var_export(($one * 9007199254740993) === 9007199254740993, true), '|';
echo var_export(($one * 9007199254740993) === 9007199254740992, true), '|';
echo var_export(($wrapped - $one) === -9.2233720368547758E+18, true);
"#;
    let expected = "false|true|false|true|false|true|true|false|true";
    php_oracle(source, expected);
    for optimized in [false, true] {
        assert_eq!(run(source, optimized).0, expected);
    }
}

/// Every supported target lowers the numeric identity without the boxed strict-equality call.
#[test]
fn test_numeric_consumers_supported_target_emitters() {
    let source = r#"<?php
function sink(float $value): float { return $value; }
$value = $argc * 4;
if ($value !== 20) { echo sink(($argc & 3) * 16); }
"#;
    for target in ["linux-x86_64", "linux-aarch64", "macos-aarch64", "ios-arm64", "ios-sim-arm64"] {
        let assembly = super::checked_numeric_chain::emit_target_assembly(source, target);
        assert!(!assembly.contains("__rt_mixed_strict_eq"), "{target}");
        assert!(!assembly.contains("__rt_mixed_cast_float"), "{target}");
    }
}

/// Builtin float parameters keep the boxed checked operand their own passes match.
///
/// Only a source-typed `float` parameter exposes the conversion; `sqrt($i + c)` must still see
/// the `ichecked_add` result directly, as the sqrt range reducer requires.
#[test]
fn test_numeric_consumers_builtin_float_parameters_keep_checked_operands() {
    let source = r#"<?php
function sink(float $value): float { return $value; }
echo sink($argc + 1), sqrt($argc + 2);
"#;
    let ir = super::checked_numeric_chain::emit_main_ir(source, &["--no-ir-opt"]);
    let lines: Vec<&str> = ir.lines().map(str::trim).collect();
    let defined_by = |value: &str| lines.iter()
        .find(|line| line.starts_with(&format!("{value}:")))
        .copied()
        .unwrap_or_else(|| panic!("{value} is undefined in\n{ir}"));
    let operand = |line: &str, op: &str| line.split(op).nth(1).unwrap()
        .split_whitespace().next().unwrap().to_string();
    let call = lines.iter().find(|line| line.contains("= call ")).expect("user call");
    let cast = defined_by(&operand(call, "= call "));
    assert!(cast.contains("= cast ") && cast.contains(" F64"), "{ir}");
    let sqrt = lines.iter().find(|line| line.contains("runtime.sqrt")).expect("sqrt call");
    assert!(defined_by(&operand(sqrt, "= runtime_call ")).contains("= ichecked_add "), "{ir}");
}
