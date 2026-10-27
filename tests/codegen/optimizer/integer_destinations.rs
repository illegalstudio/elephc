//! Purpose:
//! Regression coverage for checked arithmetic at integer destinations (task 2b890a89).
//!
//! Called from:
//! - The codegen integration test harness.
//!
//! Key details:
//! - Runtime inputs prevent AST folding, and both EIR modes retain PHP promotion order.
//! - Allocation counters cover nested expressions, typed reassignments and call operands.

use super::*;

/// Cross-checks PHP-compatible fixtures when the focused run opts into the PHP oracle.
fn php_oracle(source: &str, expected: &str) {
    if std::env::var_os("ELEPHC_PHP_CHECK").is_none() { return; }
    let output = Command::new("php").args(["-n", "-r", source.trim_start_matches("<?php")])
        .output().expect("run PHP arithmetic oracle");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout), expected);
}

/// Compiles one variant and validates its heap before returning output and diagnostics.
fn run(source: &str, optimized: bool) -> (String, String) {
    let dir = make_cli_test_dir("elephc_integer_destinations");
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

/// Keeps the whole declared integer slot scalar through arithmetic reassignment.
#[test]
fn test_integer_destinations_declared_reassignment_is_scalar() {
    let source = r#"<?php
int $result = ($argc * 7) + $argc;
$result = $result + 6;
$result += $argc * 3;
echo $result;
"#;
    let ir = super::checked_numeric_chain::emit_main_ir(source, &[]);
    assert!(!ir.lines().any(|line| line.contains("Heap(Mixed)") && !line.contains(" = nop")), "{ir}");
    assert!(!ir.contains("push_call_operand_owner"), "{ir}");
    for optimized in [false, true] {
        let (stdout, stderr) = run(source, optimized);
        assert_eq!(stdout, "17");
        assert!(stderr.contains("GC: allocs=0 frees=0"), "{stderr}");
    }
}

/// Nested assignment expressions add no heap cost to the loop's existing counter seed.
#[test]
fn test_integer_destinations_repeated_nested_assignments_keep_scalar_storage() {
    let source = r#"<?php
int $value = $argc;
int $total = 0;
$limit = (int) ($argc * 100);
for ($i = 0; $i < $limit; $i++) {
    $total += ($value = $value + $argc * 3);
}
echo $value, '|', $total;
"#;
    let control = source.replace("$value + $argc * 3", "$value");
    for optimized in [false, true] {
        let (stdout, stderr) = run(source, optimized);
        let (_, control_stderr) = run(&control, optimized);
        assert_eq!(stdout, "301|15250");
        assert_eq!(gc_allocs(&stderr), gc_allocs(&control_stderr), "{stderr}\n{control_stderr}");
    }
}

/// Redeclaring an int local with a wider type preserves subsequent float promotion.
#[test]
fn test_integer_destinations_redeclaration_keeps_float_promotion() {
    let source = r#"<?php
function redeclaredMixed(int $one): mixed {
    int $value = 0;
    mixed $value = 0;
    $value = PHP_INT_MAX + $one;
    return $value;
}
function redeclaredUnion(int $one): mixed {
    int $value = 0;
    int|float $value = 0;
    $value = PHP_INT_MAX + $one;
    return $value;
}
echo gettype(redeclaredMixed($argc)), '|', gettype(redeclaredUnion($argc));
"#;
    for optimized in [false, true] {
        assert_eq!(run(source, optimized).0, "double|double");
    }
}

/// Untaken declarations cannot constrain sibling arms, later writes or zero-trip loop exits.
#[test]
fn test_integer_destinations_conditional_declarations_keep_float_promotion() {
    let source = r#"<?php
function conditionalDeclaration(int $one): mixed {
    mixed $value = 0;
    if ($one > 100) { int $value = 0; }
    $value = PHP_INT_MAX + $one;
    return $value;
}
function siblingDeclaration(int $one): mixed {
    mixed $value = 0;
    if ($one > 100) { int $value = 0; }
    else { $value = PHP_INT_MAX + $one; }
    return $value;
}
function loopDeclaration(int $one): mixed {
    mixed $value = 0;
    while ($one > 100) { int $value = 0; break; }
    return ($value = PHP_INT_MAX + $one);
}
function forDeclaration(int $one): mixed {
    mixed $value = 0;
    for ($i = 0; $i > $one; $i++) { int $value = 0; }
    return ($value = PHP_INT_MAX + $one);
}
function switchDeclaration(int $one): mixed {
    mixed $value = 0;
    switch ($one) { case 100: int $value = 0; break; default: break; }
    $value = PHP_INT_MAX + $one;
    return $value;
}
function catchDeclaration(int $one): mixed {
    mixed $value = 0;
    try { if ($one > 100) { throw new Exception('unused'); } }
    catch (Exception $e) { int $value = 0; }
    $value = PHP_INT_MAX + $one;
    return $value;
}
echo gettype(conditionalDeclaration($argc)), '|';
echo gettype(siblingDeclaration($argc)), '|', gettype(loopDeclaration($argc)), '|';
echo gettype(forDeclaration($argc)), '|', gettype(switchDeclaration($argc)), '|';
echo gettype(catchDeclaration($argc));
"#;
    for optimized in [false, true] {
        assert_eq!(run(source, optimized).0, "double|double|double|double|double|double");
    }
}

/// Coerces direct, method, static, closure, named and statically unpacked integer operands.
#[test]
fn test_integer_destinations_call_surfaces_allocate_no_arithmetic_cells() {
    let source = r#"<?php
function sink(int $value): int { return $value; }
final class IntegerDestination {
    public function method(int $value): int { return $value; }
    public static function stat(int $value): int { return $value; }
}
$object = new IntegerDestination();
$closure = fn(int $value): int => $value;
echo sink($argc * 7), '|', $object->method($argc - 1), '|';
echo IntegerDestination::stat(($argc * 3) + $argc), '|';
echo $closure($argc + 8), '|', sink(value: $argc * 5), '|';
echo sink(...['value' => $argc + 2]), '|', intdiv($argc * 12, 5);
"#;
    php_oracle(source, "7|0|4|9|5|3|2");
    let control = source.replace("$argc * 7", "7").replace("$argc - 1", "0")
        .replace("($argc * 3) + $argc", "4").replace("$argc + 8", "9")
        .replace("$argc * 5", "5").replace("$argc + 2", "3")
        .replace("$argc * 12", "12");
    for optimized in [false, true] {
        let (stdout, stderr) = run(source, optimized);
        let (control_stdout, control_stderr) = run(&control, optimized);
        assert_eq!(stdout, "7|0|4|9|5|3|2");
        assert_eq!(stdout, control_stdout);
        assert_eq!(gc_allocs(&stderr), gc_allocs(&control_stderr), "{stderr}\n{control_stderr}");
    }
}

/// Extracts the allocation count without depending on heap-debug line ordering.
fn gc_allocs(stderr: &str) -> u64 {
    stderr.split("GC: allocs=").nth(1).unwrap()
        .split_whitespace().next().unwrap().parse().unwrap()
}

/// Right-associated and balanced trees keep exact integer subtrees until promotion is needed.
#[test]
fn test_integer_destinations_nested_tree_promotion_is_not_truncated_early() {
    let source = r#"<?php
function sink(int $value): int { return $value; }
$one = $argc;
echo sink((PHP_INT_MAX + $one) - PHP_INT_MAX), '|';
echo sink($one - ((PHP_INT_MAX + $one) - PHP_INT_MAX)), '|';
echo sink((PHP_INT_MAX + $one) - (PHP_INT_MAX + $one)), '|';
echo sink((9007199254740993 + $one) - (9007199254740993 + $one)), '|';
echo sink($one + ((PHP_INT_MAX + $one) - PHP_INT_MAX)), '|';
echo sink($one * ((PHP_INT_MAX + $one) - PHP_INT_MAX));
"#;
    php_oracle(source, "0|1|0|0|1|0");
    for optimized in [false, true] {
        let (stdout, stderr) = run(source, optimized);
        assert_eq!(stdout, "0|1|0|0|1|0");
        assert!(stderr.contains("GC: allocs=0 frees=0"), "{stderr}");
    }
}

/// Scalar call conversions retain source evaluation order and let a later argument throw.
#[test]
fn test_integer_destinations_source_order_and_unwinding() {
    let source = r#"<?php
function mark(int $value): int { echo $value; return $value; }
function fail(): int { throw new Exception('stop'); }
function pair(int $a, int $b): int { return $a - $b; }
echo pair(b: mark(2) * 3, a: mark(1) + 8), '|';
try { pair(mark(3) * 2, fail()); } catch (Exception $e) { echo 'caught'; }
"#;
    php_oracle(source, "213|3caught");
    for optimized in [false, true] {
        assert_eq!(run(source, optimized).0, "213|3caught");
    }
}

/// All supported targets consume the same scalar argument and declared-local EIR.
#[test]
fn test_integer_destinations_supported_target_emitters() {
    let source = r#"<?php
function sink(int $value): int { return $value; }
int $value = ($argc * 3) + $argc;
$value = $value + 2;
echo sink($value * 4), intdiv($argc * 12, 5);
"#;
    for target in ["linux-x86_64", "linux-aarch64", "macos-aarch64", "ios-arm64", "ios-sim-arm64"] {
        let assembly = super::checked_numeric_chain::emit_target_assembly(source, target);
        assert!(!assembly.contains("call __rt_int_mul_checked"), "{target}");
        assert!(!assembly.contains("bl __rt_int_mul_checked"), "{target}");
        assert!(!assembly.contains("push_call_operand_owner"), "{target}");
    }
}
