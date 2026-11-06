//! Purpose:
//! End-to-end coverage of EIR tail self and mutual recursion and conservative fallbacks.
//!
//! Called from:
//! - `cargo test --test codegen_tests optimizer::tail_call`.
//!
//! Key details:
//! - Deep calls exceed ordinary native stack limits; bounded fixtures compare both optimizer modes.
//! - Structural EIR checks ensure successful execution actually exercises tail-call elimination.

use super::*;

/// Emits one function's EIR with reliable cleanup even when an assertion fails.
fn function_ir(source: &str, name: &str, optimized: bool) -> String {
    struct Fixture(std::path::PathBuf);
    impl Drop for Fixture {
        /// Removes exactly the temporary directory owned by this fixture.
        fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
    }
    let fixture = Fixture(make_cli_test_dir("elephc_tail_call_ir"));
    let path = fixture.0.join("main.php");
    fs::write(&path, source).unwrap();
    let mut command = elephc_cli_command(&fixture.0);
    command.arg("--emit-ir");
    if !optimized { command.arg("--no-ir-opt"); }
    let output = command.arg(path).output().expect("emit EIR");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let text = String::from_utf8(output.stdout).unwrap();
    text.split_once(&format!("  function {name}(")).expect("named function").1
        .split("\n  function ").next().unwrap().to_string()
}

/// Deep accumulator recursion uses bounded stack and does not hoist mutable parameter reads.
#[test]
fn test_tail_call_deep_self_recursion() {
    let source = r#"<?php
function sumDown(int $n, int $sum): int {
    if ($n <= 0) { return $sum; }
    return sumDown($n - 1, $sum + $n);
}
echo sumDown($argc * 300000, 0);
"#;
    let ir = function_ir(source, "sumDown", true);
    assert!(ir.contains("tail_rebind_sumDown"), "{ir}");
    assert!(!ir.contains("= call "), "{ir}");
    assert_eq!(compile_and_run(source), "45000150000");
}

/// A three-function mutual cycle can have distinct arities and retains every public entry.
#[test]
fn test_tail_call_deep_mutual_recursion_with_different_arities() {
    let source = r#"<?php
function first(int $n): int { if ($n <= 0) { return 10; } return second($n - 1, 20); }
function second(int $n, int $answer): int { if ($n <= 0) { return $answer; } return third($n - 1); }
function third(int $n): int { if ($n <= 0) { return 30; } return first($n - 1); }
echo first($argc * 300000), ",", second(300001, 9), ",", third(300002);
"#;
    for name in ["first", "second", "third"] {
        let ir = function_ir(source, name, true);
        assert!(ir.contains("tail_rebind_"), "{ir}");
        assert!(!ir.contains("= call "), "{ir}");
    }
    assert_eq!(compile_and_run(source), "10,30,20");
}

/// Mixed integer, float and boolean loop parameters are rebound simultaneously.
#[test]
fn test_tail_call_parameter_swaps_and_float_state() {
    let source = r#"<?php
function rotate(int $n, int $a, int $b, float $x, float $y, bool $flip): float {
    if ($n <= 0) { echo $a, ",", $b, ",", (int)$flip, ":"; return $x; }
    return rotate($n - 1, $b, $a, $y, $x, !$flip);
}
echo rotate($argc * 9, 2, 7, 1.5, 2.5, false);
"#;
    assert!(function_ir(source, "rotate", true).contains("tail_rebind_rotate"));
    assert_eq!(compile_and_run(source), "7,2,1:2.5");
    assert_eq!(without_ir_opt(|| compile_and_run(source)), "7,2,1:2.5");
}

/// Named argument source order and default argument binding survive the tail edge.
#[test]
fn test_tail_call_named_arguments_and_defaults() {
    let source = r#"<?php
function tick(int $n): int { echo $n; return $n; }
function loopNamed(int $n, int $a = 4, int $b = 5): int {
    if ($n <= 0) { return $a * 10 + $b; }
    return loopNamed(b: tick($a), n: $n - 1, a: tick($b));
}
echo ":", loopNamed($argc), ":", loopNamed(0);
"#;
    let ir = function_ir(source, "loopNamed", true);
    assert!(ir.contains("tail_rebind_loopNamed"), "{ir}");
    assert_eq!(compile_and_run(source), ":4554:45");
    assert_eq!(without_ir_opt(|| compile_and_run(source)), ":4554:45");
}

/// Ordinary local initialization runs anew on each branch to a recursive body entry.
#[test]
fn test_tail_call_resets_locals_and_preserves_non_tail_calls() {
    let source = r#"<?php
function countLocal(int $n): int {
    $local = 3;
    if ($n <= 0) { return $local; }
    $local = 8;
    return countLocal($n - 1);
}
function factorial(int $n): int { if ($n <= 1) { return 1; } return $n * factorial($n - 1); }
echo countLocal($argc * 30), ",", factorial(7);
"#;
    assert!(function_ir(source, "countLocal", true).contains("tail_rebind_countLocal"));
    assert!(!function_ir(source, "factorial", true).contains("tail_rebind_factorial"));
    assert_eq!(compile_and_run(source), "3,5040");
    assert_eq!(without_ir_opt(|| compile_and_run(source)), "3,5040");
}

/// Try/finally, owning locals and by-reference parameters retain their ordinary call boundaries.
#[test]
fn test_tail_call_preserves_cleanup_and_reference_boundaries() {
    let source = r#"<?php
class Marker { public function __destruct() { echo "D"; } }
function owned(int $n): int { $m = new Marker(); if ($n <= 0) { return 4; } return owned($n - 1); }
function cleanup(int $n): int { try { if ($n <= 0) { return 5; } return cleanup($n - 1); } finally { echo "F"; } }
function reference(int &$n): int { if ($n <= 0) { return 6; } $n = (int)($n - 1); return reference($n); }
$n = 2;
echo owned(2), ",", cleanup(2), ",", reference($n), ":", $n;
"#;
    for name in ["owned", "cleanup", "reference"] {
        assert!(!function_ir(source, name, true).contains("tail_rebind_"));
    }
    let expected = "DDD4,FFF5,6:0";
    assert_eq!(compile_and_run(source), expected);
    assert_eq!(without_ir_opt(|| compile_and_run(source)), expected);
}

/// Void tail calls use zero-argument return edges without inventing a result value.
#[test]
fn test_tail_call_void_return() {
    let source = r#"<?php
function countdown(int $n): void {
    if ($n <= 0) { echo "done"; return; }
    countdown($n - 1);
}
countdown($argc * 300000);
"#;
    let ir = function_ir(source, "countdown", true);
    assert!(ir.contains("tail_rebind_countdown"), "{ir}");
    assert_eq!(compile_and_run(source), "done");
}

/// The documented Euclid/mutual-recursion example compiles and executes as shipped.
#[test]
fn test_tail_call_example() {
    let source = include_str!("../../../examples/tail-recursion/main.php");
    assert_eq!(compile_and_run(source), "gcd(1071, 462) = 21\n300000 steps: even\n");
}

/// A ternary return forwards the recursive result through a scalar return join.
#[test]
fn test_tail_call_ternary_return_join() {
    let source = r#"<?php
function ternaryWalk(int $n, int $answer): int { return $n <= 0 ? $answer : ternaryWalk($n - 1, $answer); }
echo ternaryWalk($argc * 300000, 42);
"#;
    let ir = function_ir(source, "ternaryWalk", true);
    assert!(ir.contains("tail_rebind_ternaryWalk"), "{ir}");
    assert!(!ir.contains("= call "), "{ir}");
    assert_eq!(compile_and_run(source), "42");
}

/// A reachable backtrace outside the recursive function preserves every PHP activation.
#[test]
fn test_tail_call_backtrace_preserves_frames() {
    let source = r#"<?php
function traceCount(int $n): int {
    if ($n <= 0) { return count(debug_backtrace(DEBUG_BACKTRACE_IGNORE_ARGS)); }
    return traceCount($n - 1);
}
echo traceCount($argc * 3);
"#;
    assert!(!function_ir(source, "traceCount", true).contains("tail_rebind_"));
    assert_eq!(compile_and_run(source), "4");
    assert_eq!(without_ir_opt(|| compile_and_run(source)), "4");
}

/// String parameter ownership keeps its real recursive frame and identical bounded output.
#[test]
fn test_tail_call_string_parameter_fallback() {
    let source = r#"<?php
function keepString(int $n, string $s): int {
    if ($n <= 0) { return strlen($s); }
    return keepString($n - 1, $s);
}
echo keepString($argc * 12, "hello");
"#;
    assert!(!function_ir(source, "keepString", true).contains("tail_rebind_"));
    assert_eq!(compile_and_run(source), "5");
    assert_eq!(without_ir_opt(|| compile_and_run(source)), "5");
}

/// Numeric argument owners are retired on each activation, including with a small debug heap.
#[test]
fn test_tail_call_numeric_owners_keep_heap_balanced() {
    let source = r#"<?php
function accumulateTail(int $n, int $sum): int {
    if ($n <= 0) { return $sum; }
    return accumulateTail($n - 1, $sum + 1);
}
echo accumulateTail($argc * 10000, 0);
"#;
    let output = compile_and_run_with_gc_stats(source);
    assert_eq!(output.stdout, "10000");
    let (allocations, frees) = parse_gc_stats(&output.stderr);
    assert_eq!(allocations, frees, "{}", output.stderr);
    assert_eq!(compile_and_run_with_heap_size(source, 65_536), "10000");
    let debug = compile_and_run_with_heap_debug(source);
    assert_eq!(debug.stdout, "10000");
}
