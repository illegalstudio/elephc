//! Purpose:
//! End-to-end regression coverage for scalar local promotion to EIR SSA.
//!
//! Called from:
//! - `cargo test --test codegen_tests_core optimizer::mem2reg`.
//!
//! Key details:
//! - Textual EIR proves local traffic becomes block parameters on joins and back edges.
//! - Native execution compares optimizer-on and optimizer-off PHP behavior.

use super::*;

const DYNAMIC_EVAL_SCALAR_SOURCE: &str = r#"<?php
function repeat_eval(string $code): void {
    for ($i = 0; $i < 3; $i++) {
        if ($i > 0) { eval($code); }
        $n = 7;
        if (strlen($code) > 1000) { $n = 9; }
        if (strlen($code) > 0) {
            $m = $n + 1;
            echo "m=", $m, "\n";
        }
    }
}
repeat_eval('echo "n=$n\\n";');
"#;

/// Emits only the main function's textual EIR for one PHP program.
fn main_ir(source: &str, optimized: bool) -> String {
    function_ir(source, optimized, "main")
}

/// Emits one named function's textual EIR without including other function bodies.
fn function_ir(source: &str, optimized: bool, name: &str) -> String {
    let dir = make_cli_test_dir("elephc_mem2reg_ir");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("write PHP fixture");
    let mut command = elephc_cli_command(&dir);
    command.arg("--emit-ir");
    if !optimized {
        command.arg("--no-ir-opt");
    }
    let output = command.arg(&php_path).output().expect("emit EIR");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let text = String::from_utf8(output.stdout).expect("EIR is UTF-8");
    let header = format!("  function {name}(");
    let body = text.split_once(&header).expect("named function").1;
    let function = body.split("\n  function ").next().expect("function body").to_string();
    fs::remove_dir_all(dir).expect("remove fixture directory");
    function
}

/// Compiles and executes one PHP program with the requested EIR optimization mode.
fn run_variant(source: &str, optimized: bool) -> String {
    run_variant_with_args(source, optimized, &[])
}

/// Compiles and executes one PHP program with the requested CLI arguments.
fn run_variant_with_args(source: &str, optimized: bool, args: &[&str]) -> String {
    let dir = make_cli_test_dir("elephc_mem2reg_run");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("write PHP fixture");
    let mut command = elephc_cli_command(&dir);
    if !optimized {
        command.arg("--no-ir-opt");
    }
    let compile = command.arg(&php_path).output().expect("compile PHP fixture");
    assert!(compile.status.success(), "{}", String::from_utf8_lossy(&compile.stderr));
    let run = run_binary_with_args(&dir.join("main"), &dir, args);
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    let stdout = String::from_utf8(run.stdout).expect("stdout is UTF-8");
    fs::remove_dir_all(dir).expect("remove fixture directory");
    stdout
}

/// The example's counter and accumulator become loop parameters with back-edge values.
#[test]
fn test_mem2reg_promotes_loop_counter_and_accumulator() {
    let source = include_str!("../../../examples/mem2reg-loop/main.php");
    let plain = main_ir(source, false);
    let optimized = main_ir(source, true);
    for slot in ["slot[2]", "slot[3]"] {
        assert!(plain.contains(&format!("load_local {slot}")), "{plain}");
        assert!(!optimized.contains(&format!("load_local {slot}")), "{optimized}");
        assert!(!optimized.lines().any(|line| line.contains("store_local") && line.contains(slot)), "{optimized}");
    }
    assert!(optimized.contains("while.cond("), "loop header takes SSA values: {optimized}");
    assert!(optimized.contains("br bb1("), "back edge passes SSA values: {optimized}");
    assert_eq!(run_variant(source, false), "10");
    assert_eq!(run_variant(source, true), "10");
}

/// Distinct assignments on both sides of a branch merge through an EIR parameter.
#[test]
fn test_mem2reg_promotes_branch_join() {
    let source = "<?php $x = 0; if ($argc > 1) { $x = 11; } else { $x = 22; } echo $x; while ($x > 0) { $x = ($x - 1) & 255; } echo $x;";
    let optimized = main_ir(source, true);
    assert!(optimized.contains("if.merge("), "join takes an SSA parameter: {optimized}");
    for optimized in [false, true] {
        assert_eq!(run_variant_with_args(source, optimized, &[]), "220");
        assert_eq!(run_variant_with_args(source, optimized, &["extra"]), "110");
    }
}

/// Flags initialized to false retain boolean SSA parameters through conditional loop updates.
#[test]
fn test_mem2reg_promotes_false_initialized_boolean_flag() {
    let source = r#"<?php
function findFlag(int $n, int $key): bool {
    $found = false;
    for ($i = 0; $i < $n; $i++) {
        if ($i === $key) { $found = true; }
    }
    return $found;
}
echo findFlag($argc + 3, $argc) ? "found" : "missing";
echo "|", findFlag($argc + 3, $argc + 4) ? "found" : "missing";
"#;
    let plain = function_ir(source, false, "findFlag");
    let optimized = function_ir(source, true, "findFlag");
    let boolean_load = |line: &str| line.contains("load_local")
        && (line.contains("php=bool") || line.contains("php=false"));
    assert!(plain.lines().any(boolean_load), "{plain}");
    assert!(!optimized.lines().any(boolean_load), "{optimized}");
    assert!(optimized.lines().any(|line| line.contains("for.cond(") && line.contains("php=bool")),
        "the loop carries a boolean SSA parameter: {optimized}");
    for optimized in [false, true] {
        assert_eq!(run_variant(source, optimized), "found|missing");
    }
}

/// Floating-point loop state is promoted without changing PHP's printed result.
#[test]
fn test_mem2reg_promotes_float_loop() {
    let source = "<?php $f = 0.5; $i = 0; while ($i < 3) { $f = $f + 0.25; $i++; } echo $f;";
    let plain = main_ir(source, false);
    let optimized = main_ir(source, true);
    assert!(plain.lines().any(|line| line.contains("php=float") && line.contains("load_local")), "{plain}");
    assert!(!optimized.lines().any(|line| line.contains("php=float") && line.contains("load_local")), "{optimized}");
    assert!(optimized.contains("while.cond("), "float state reaches the loop header: {optimized}");
    assert_eq!(run_variant(source, false), "1.25");
    assert_eq!(run_variant(source, true), "1.25");
}

/// A call-free bitwise loop keeps two scalar values live through its back edge.
#[test]
fn test_mem2reg_promotes_two_call_free_loop_values() {
    let source = "<?php $counter = $argc & 1; $acc = 0; do { $acc = $acc ^ ($counter | 4); $counter = $counter ^ 1; } while ($counter > 0); echo $acc;";
    let optimized = main_ir(source, true);
    assert!(optimized.contains("do.body(v"), "loop body takes both values as parameters: {optimized}");
    assert!(!optimized.lines().any(|line| line.contains("load_local slot[1]") || line.contains("load_local slot[2]")), "{optimized}");
    assert!(!optimized.contains("store_local"), "{optimized}");
    assert_eq!(run_variant(source, false), "5");
    assert_eq!(run_variant(source, true), "5");
}

/// By-reference mutation remains visible after the call in both optimizer modes.
#[test]
fn test_mem2reg_preserves_by_reference_local() {
    let source = "<?php function change(int &$x): void { $x = 7; } $x = 2; change($x); echo $x;";
    assert_eq!(run_variant(source, false), "7");
    assert_eq!(run_variant(source, true), "7");
}

/// Dynamic eval sees a later scalar that would otherwise qualify for SSA promotion.
#[test]
fn test_mem2reg_keeps_dynamic_eval_visible_local_in_memory() {
    let expected = "m=8\nn=7\nm=8\nn=7\nm=8\n";
    assert_eq!(run_variant(DYNAMIC_EVAL_SCALAR_SOURCE, false), expected);
    assert_eq!(run_variant(DYNAMIC_EVAL_SCALAR_SOURCE, true), expected);
}

/// Both optimizer modes retain two stores and a real AOT load for the eval-visible scalar.
#[test]
fn test_mem2reg_keeps_dynamic_eval_slot_traffic() {
    let plain = function_ir(DYNAMIC_EVAL_SCALAR_SOURCE, false, "repeat_eval");
    let optimized = function_ir(DYNAMIC_EVAL_SCALAR_SOURCE, true, "repeat_eval");
    let value = plain.lines()
        .find_map(|line| line.split_once(" = const_i64 7").map(|(definition, _)| definition))
        .expect("the eval-visible scalar is initialized to seven")
        .split(':').next().expect("constant result").trim();
    let store = format!("store_local {value} ");
    let slot = plain.lines()
        .find_map(|line| line.trim().strip_prefix(&store))
        .expect("the scalar is written to a local slot")
        .split_whitespace().next().expect("local slot");
    for ir in [&plain, &optimized] {
        assert!(ir.contains(&format!("load_local {slot}")), "AOT reads the eval-visible scalar: {ir}");
        assert_eq!(ir.lines().filter(|line| line.contains("store_local") && line.contains(slot)).count(),
            2, "both eval-visible assignments stay in memory: {ir}");
    }
}

/// Dead-store and immutable-load passes also retain a single later eval-visible store.
#[test]
fn test_mem2reg_keeps_single_store_visible_to_dynamic_eval() {
    let source = r#"<?php
function repeat_eval(string $code): void {
    for ($i = 0; $i < 3; $i++) {
        if ($i > 0) { eval($code); }
        $n = 7;
        echo "step\n";
    }
}
repeat_eval('echo "n=$n\\n";');
"#;
    let expected = "step\nn=7\nstep\nn=7\nstep\n";
    assert_eq!(run_variant(source, false), expected);
    assert_eq!(run_variant(source, true), expected);
}

/// A scalar defined before a loop remains promotable when its back edge has no store.
#[test]
fn test_mem2reg_promotes_read_only_loop_value() {
    let source = "<?php $x = $argc; if ($argc > 5) { $x = 3; } $i = 0; $s = 0; while ($i < 3) { $s = ($s + $x) & 255; $i++; } echo $s;";
    let optimized = main_ir(source, true);
    assert!(!optimized.contains("load_local slot[1]"), "loop-invariant scalar is promoted: {optimized}");
    assert_eq!(run_variant(source, false), "3");
    assert_eq!(run_variant(source, true), "3");
}

/// A promoted loop value survives runtime string work before its next use.
#[test]
fn test_mem2reg_preserves_value_across_runtime_call() {
    let source = "<?php $x = $argc; if ($argc > 5) { $x = 3; } $sum = 0; $i = 0; while ($i < 3) { echo strlen('z' . $argc); $sum = ($sum + $x) & 255; $i++; } echo $sum;";
    let optimized = main_ir(source, true);
    assert!(!optimized.contains("load_local slot[1]"), "loop value is promoted: {optimized}");
    assert_eq!(run_variant(source, false), "2223");
    assert_eq!(run_variant(source, true), "2223");
}

/// Loop-carried scalar swaps preserve parallel edge-copy semantics.
#[test]
fn test_mem2reg_preserves_swap_across_back_edge() {
    let source = "<?php $a = $argc; $b = 2; $i = 0; while ($i < 3) { $t = $a; $a = $b; $b = $t; $i++; } echo $a, ':', $b;";
    let optimized = main_ir(source, true);
    assert!(optimized.contains("while.cond("), "swap uses loop parameters: {optimized}");
    assert_eq!(run_variant(source, false), "2:1");
    assert_eq!(run_variant(source, true), "2:1");
}

/// Identity folding leaves a dead loop parameter without corrupting the returned scalar.
#[test]
fn test_mem2reg_dead_loop_parameter_preserves_live_result() {
    let source = r#"<?php
function select_loop_value(int $input): int {
    $c = $input; $a = 7; $b = 42;
    while ($c > 0) {
        $a = $c ^ 3; $b = $c ^ 42; $c = 0;
    }
    return ($a ^ $a) ^ $b;
}
echo select_loop_value($argc), ":", select_loop_value(0), "\n";
"#;
    for optimized in [false, true] {
        assert_eq!(run_variant(source, optimized), "43:42\n");
    }
}

/// An inlined scalar return survives runtime calls and a catch reached through finally.
#[test]
fn test_mem2reg_inlined_return_survives_finally_handler() {
    let source = r#"<?php
function pick(int $c): int { return ($c & 255) ^ 41; }
function guarded(int $c): int {
    try { return pick($c); }
    finally {
        $z = strlen("z" . $c);
        try { throw new Exception("x"); }
        catch (Exception $e) { echo $z, "\n"; }
    }
}
echo guarded($argc), "\n";
"#;
    for optimized in [true, false] {
        assert_eq!(run_variant(source, optimized), "2\n40\n");
    }
}
