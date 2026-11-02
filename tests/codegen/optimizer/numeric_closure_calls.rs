//! Purpose:
//! Executes scalar-specialized closure calls and their conservative fallback boundaries.
//!
//! Called from:
//! - The optimizer integration suite.
//!
//! Key details:
//! - Both IR modes preserve actual PHP output and clean ownership diagnostics.
//! - Allocation counters validate removal of per-call argument boxes and private copies.

use super::*;

/// Runs one program in an explicit IR mode with heap checks and an optional PHP oracle.
fn run(source: &str, optimized: bool) -> (String, String) {
    let dir = make_cli_test_dir("elephc_numeric_closure_calls");
    let path = dir.join("main.php");
    fs::write(&path, source).unwrap();
    let mut command = elephc_cli_command(&dir);
    command.args(["--heap-debug", "--gc-stats"]);
    if !optimized { command.arg("--no-ir-opt"); }
    let compiled = command.arg(&path).output().unwrap();
    assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
    let output = run_binary(&path.with_extension(""), &dir);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
    if std::env::var_os("ELEPHC_PHP_CHECK").is_some() {
        let php = Command::new("php").arg("-n").arg(&path).output().unwrap();
        assert!(php.status.success(), "{}", String::from_utf8_lossy(&php.stderr));
        assert_eq!(stdout.as_bytes(), php.stdout, "actual PHP oracle");
    }
    fs::remove_dir_all(dir).unwrap();
    (stdout, stderr)
}

/// The original numeric loop removes scalar argument cells and owned-shadow clones per call.
#[test]
fn test_numeric_closure_calls_reduce_argument_allocations() {
    let source = include_str!("../../../examples/bounded-closure-counter/main.php");
    let plain = run(source, false);
    let optimized = run(source, true);
    assert_eq!(plain.0, "500500");
    assert_eq!(optimized.0, plain.0);
    let (plain_allocs, _) = parse_gc_stats(&plain.1);
    let (allocs, frees) = parse_gc_stats(&optimized.1);
    assert_eq!(allocs, frees);
    assert!(allocs < 2500, "two scalar owners per iteration should remain: {allocs}");
    assert!(allocs + 1000 < plain_allocs);
}

/// Exact scalar captures, doubles and integer overflow retain their original PHP values and tags.
#[test]
fn test_numeric_closure_calls_scalar_capture_and_overflow() {
    let source = r#"<?php
function captures_numeric(int $n): int {
    $step = 3; $captured_sum_fn = function($x) use ($step) { return $x + $step; };
    $sum = 0;
    for ($i = 0; $i < $n; $i++) { $sum += $captured_sum_fn($i); }
    return (int)$sum;
}

$f = function(mixed $x) { return $x + 1; };
$overflow = $f(PHP_INT_MAX);
echo captures_numeric($argc + 2), '|', gettype($overflow), ':', @(int)$overflow, '|', $f(1.5);
"#;
    for optimized in [false, true] {
        assert_eq!(run(source, optimized).0, "12|double:-9223372036854775808|2.5");
    }
}

/// An admitted integer call cannot authorize another call with null or bool on a later backedge.
#[test]
fn test_numeric_closure_calls_keep_per_site_type_and_identity_guards() {
    let source = r#"<?php
function nullable_site(int $n): void {
    $f = function($x) { return $x; }; $value = 10; $result = 0;
    for ($i = 0; $i < $n; $i++) { $result = $f($value); $value = null; $sink = $f($i); }
    echo gettype($result), ':', $result, '|';
}
function bool_site(int $n): void {
    $f = function($x) { return $x; }; $value = 10; $result = 0;
    for ($i = 0; $i < $n; $i++) { $result = $f($value); $value = true; $sink = $f($i); }
    echo gettype($result), ':', $result, '|';
}
function reference_site(int $n): int {
    $step = 1; $f = function($x) use (&$step) { return $x + $step; }; $sum = 0;
    for ($i = 0; $i < $n; $i++) { $sum += $f($i); $step++; }
    return (int)$sum;
}
nullable_site($argc + 1); bool_site($argc + 1); echo reference_site($argc + 2);
"#;
    for optimized in [false, true] {
        assert_eq!(run(source, optimized).0, "NULL:|boolean:1|9");
    }
}

/// Specialized entry signatures use existing call lowering on all supported target ABIs.
#[test]
fn test_numeric_closure_calls_all_supported_targets() {
    let source = include_str!("../../../examples/bounded-closure-counter/main.php");
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for optimized in [false, true] {
            let dir = make_cli_test_dir("elephc_numeric_closure_calls_target");
            let path = dir.join("main.php"); fs::write(&path, source).unwrap();
            let mut command = elephc_cli_command(&dir);
            command.args(["--emit-asm", "--target", target]);
            if target.starts_with("ios-") { command.args(["--emit", "staticlib"]); }
            if !optimized { command.arg("--no-ir-opt"); }
            let output = command.arg(&path).output().unwrap();
            assert!(output.status.success(), "{target}: {}", String::from_utf8_lossy(&output.stderr));
            let asm = fs::read_to_string(path.with_extension("s")).unwrap();
            assert_eq!(asm.contains("__eir_numeric_closure_"), optimized, "{target}");
            fs::remove_dir_all(dir).unwrap();
        }
    }
}

/// Integer and float variants use their own register banks and overflow argument stack slots.
#[test]
fn test_numeric_closure_calls_stack_arguments_preserve_scalar_abi() {
    let source = r#"<?php
$f = function(mixed $a, mixed $b, mixed $c, mixed $d, mixed $e, mixed $f, mixed $g, mixed $h, mixed $i) { return $a + $i; };
echo $f(1,2,3,4,5,6,7,8,9), '|', $f(1.5,2.5,3.5,4.5,5.5,6.5,7.5,8.5,9.5);
"#;
    for optimized in [false, true] { assert_eq!(run(source, optimized).0, "10|11"); }
}

/// Floating-point variants preserve signed zero, infinities and NaN; typed returns retain errors.
#[test]
fn test_numeric_closure_calls_float_edges_and_typed_return_fallback() {
    let source = r#"<?php
$f = function(mixed $x): mixed { return $x * 1; };
echo is_nan($f(NAN)) ? 'nan|' : 'bad|';
echo is_infinite($f(INF)) ? 'inf|' : 'bad|';
echo fdiv(1.0, $f(-0.0)), '|';
$overflow_value = PHP_INT_MAX; $overflow_value += 1;
$typed = function(mixed $x): int { return $x; };
try { $typed($overflow_value); echo 'bad'; } catch (TypeError $error) { echo 'TypeError'; unset($error); }
"#;
    for optimized in [false, true] {
        assert_eq!(run(source, optimized).0, "nan|inf|-INF|TypeError");
    }
}

/// Every owned intermediate of a reconstructed numeric expression is released exactly once.
#[test]
fn test_numeric_closure_calls_reused_expression_nodes_are_heap_clean() {
    let source = r#"<?php
$f = function(mixed $x): mixed { $first=$x+1; $second=$first*2; return $second+$first; };
echo $f(PHP_INT_MAX), '|', $f(1.5);
"#;
    for optimized in [false, true] {
        assert_eq!(run(source, optimized).0, "2.7670116110564E+19|7.5");
    }
}

/// Specialized calls retain one source counter and the original instrumentation identity.
#[test]
fn test_numeric_closure_calls_preserve_monitoring_identity() {
    let source = include_str!("../../../examples/bounded-closure-counter/main.php");
    let dir = make_cli_test_dir("elephc_numeric_closure_identity");
    let path = dir.join("main.php"); fs::write(&path, source).unwrap();
    for optimized in [false, true] {
        let mut command = elephc_cli_command(&dir);
        command.args(["--heap-debug", "--counters"]);
        if !optimized { command.arg("--no-ir-opt"); }
        let compiled = command.arg(&path).output().unwrap();
        assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
        let output = run_binary(&path.with_extension(""), &dir);
        assert!(output.status.success());
        assert_eq!(String::from_utf8_lossy(&output.stdout), "500500");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"));
        let closure_counts: Vec<_> = stderr.lines().filter(|line| line.starts_with("elephc-counters: __eir_closure_")).collect();
        assert_eq!(closure_counts.len(), 1, "one source counter: {stderr}");
        assert!(closure_counts[0].ends_with(" 1000"), "{stderr}");
        assert!(!stderr.contains("elephc-counters: __eir_numeric_closure_"), "private variants retain their source name");
    }
    // Monitoring needs executable initialization; iOS libraries reject this capability.
    // The separate ABI emission test covers iOS device and Simulator without monitoring.
    for target in ["macos-aarch64", "linux-aarch64", "linux-x86_64"] {
        let mut command = elephc_cli_command(&dir);
        command.args(["--emit-asm", "--with-monitoring", "--target", target]);
        let compiled = command.arg(&path).output().unwrap();
        assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
        let asm = fs::read_to_string(path.with_extension("s")).unwrap();
        assert!(asm.contains("__eir_numeric_closure_"), "{target}");
        assert!(asm.matches("instrument: enter (--instrument)").count() >= 3, "original and specialized PHP entries retain hooks on {target}");
    }
    fs::remove_dir_all(dir).unwrap();
}

/// Neither literal nor dynamic lookup can discover private entries, including a collision suffix.
#[test]
fn test_numeric_closure_calls_hide_private_function_names() {
    let hidden = r#"<?php
$f = function(mixed $x): mixed { return $x + 1; };
echo $f($argc), '|';
$name = '__eir_numeric_closure_' . ($argc - 1) . '_i_0';
echo function_exists('__eir_numeric_closure_0_i_0') ? 'bad' : 'hidden', '|';
echo function_exists($name) ? 'bad' : 'hidden';
"#;
    let collision = r#"<?php
function __eir_numeric_closure_0_i_0(): int { return 42; }
$f = function(mixed $x): mixed { return $x + 1; };
echo $f($argc), '|';
$name = '__eir_numeric_closure_0_i_' . $argc;
echo function_exists('__eir_numeric_closure_0_i_0') ? 'public' : 'bad', '|';
echo function_exists($name) ? 'bad' : 'hidden', '|', __eir_numeric_closure_0_i_0();
"#;
    for optimized in [false, true] {
        assert_eq!(run(hidden, optimized).0, "2|hidden|hidden");
        assert_eq!(run(collision, optimized).0, "2|public|hidden|42");
    }
}

/// Method callers receive the same exact-argument entry variants as ordinary functions.
#[test]
fn test_numeric_closure_calls_method_caller_reduces_argument_allocations() {
    let source = r#"<?php
final class NumericClosureCaller {
    public static function total(int $n): int {
        $f = function($x) { return $x + 1; };
        $sum = 0;
        for ($i = 0; $i < $n; $i++) { $sum += $f($i); }
        return (int)$sum;
    }
}
echo NumericClosureCaller::total($argc * 1000);
"#;
    let plain = run(source, false);
    let optimized = run(source, true);
    assert_eq!(plain.0, "500500");
    assert_eq!(optimized.0, plain.0);
    let (plain_allocs, _) = parse_gc_stats(&plain.1);
    let (allocs, frees) = parse_gc_stats(&optimized.1);
    assert_eq!(allocs, frees);
    assert!(allocs + 1000 < plain_allocs, "method allocations: {plain_allocs} -> {allocs}");
    let dir = make_cli_test_dir("elephc_numeric_closure_method_target");
    let path = dir.join("main.php"); fs::write(&path, source).unwrap();
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let mut command = elephc_cli_command(&dir);
        command.args(["--emit-asm", "--target", target]);
        if target.starts_with("ios-") { command.args(["--emit", "staticlib"]); }
        let output = command.arg(&path).output().unwrap();
        assert!(output.status.success(), "{target}: {}", String::from_utf8_lossy(&output.stderr));
        assert!(fs::read_to_string(path.with_extension("s")).unwrap().contains("__eir_numeric_closure_"), "{target}");
    }
    fs::remove_dir_all(dir).unwrap();
}

/// Scalar calls exposed before and after counter specialization share one private body.
#[test]
fn test_numeric_closure_calls_later_round_reuses_the_same_entry() {
    let source = r#"<?php
function late_numeric_entry(int $n): int {
    $f = function($x) { return $x + 1; };
    $prefix = $f($n); $sum = 0;
    for ($i = 0; $i < $n; $i++) { $sum += $f($i); }
    return (int)$sum + (int)$prefix;
}
echo late_numeric_entry($argc * 1000);
"#;
    for optimized in [false, true] { assert_eq!(run(source, optimized).0, "501501"); }
    let dir = make_cli_test_dir("elephc_numeric_closure_later_entry");
    let path = dir.join("main.php"); fs::write(&path, source).unwrap();
    let output = elephc_cli_command(&dir).arg("--emit-ir").arg(&path).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let ir = String::from_utf8(output.stdout).unwrap();
    assert_eq!(ir.lines().filter(|line| line.trim_start().starts_with("function __eir_numeric_closure_")).count(), 1, "{ir}");
    fs::remove_dir_all(dir).unwrap();
}

/// Distinct physical closure symbols cannot inherit a case-folded owner's numeric body.
#[test]
fn test_numeric_closure_calls_preserve_case_distinct_owners() {
    let source = r#"<?php
namespace Foo {
    function bar(int $n): int {
        $f = function($x) { return $x + 100; }; $sum = 0;
        for ($i = 0; $i < $n; $i++) { $sum += $f($i); }
        return (int)$sum;
    }
}
namespace {
    function foo_bar(int $n): int {
        $f = function($x) { return $x + 1; }; $sum = 0;
        for ($i = 0; $i < $n; $i++) { $sum += $f($i); }
        return (int)$sum;
    }
    echo foo_bar($argc + 2), '|', \Foo\bar($argc + 2);
}
"#;
    for optimized in [false, true] { assert_eq!(run(source, optimized).0, "6|303"); }
}
