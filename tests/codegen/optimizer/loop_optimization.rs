//! Purpose:
//! End-to-end and emitted-code coverage for canonical EIR loop optimization.
//!
//! Called from:
//! - `cargo test --test codegen_tests optimizer::loop_optimization`.
//!
//! Key details:
//! - Compare optimizer modes with runtime-dependent inputs and cover all target emitters.
//! - Temporary fixtures are removed by Drop even when assertions fail.

use super::*;

/// Owns one isolated fixture directory, including cleanup during test unwinding.
struct Fixture(std::path::PathBuf);

impl Fixture {
    /// Writes the PHP source into a fresh fixture directory.
    fn new(source: &str) -> Self {
        let fixture = Self(make_cli_test_dir("elephc_loop_optimization"));
        fs::write(fixture.0.join("main.php"), source).expect("write loop fixture");
        fixture
    }

    /// Compiles the fixture with an explicit optimizer mode and extra emitter options.
    fn compile(&self, optimized: bool, options: &[&str]) -> std::process::Output {
        let mut command = elephc_cli_command(&self.0);
        command.args(options);
        command.arg(if optimized { "--ir-opt=on" } else { "--ir-opt=off" });
        let output = command.arg(self.0.join("main.php")).output().expect("compile loop fixture");
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        output
    }

    /// Runs the compiled fixture with optional PHP command-line arguments.
    fn run(&self, args: &[&str]) -> String {
        let output = run_binary_with_args(&self.0.join("main"), &self.0, args);
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        String::from_utf8(output.stdout).expect("UTF-8 output")
    }
}

impl Drop for Fixture {
    /// Removes task-owned source, assembly, object, and executable files.
    fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
}

/// Extracts the body of a named function from textual EIR.
fn function_ir(text: &str, name: &str) -> String {
    text.split_once(&format!("  function {name}(" )).expect("function in EIR").1
        .split("\n  function ").next().unwrap().to_owned()
}

/// Emits the example's EIR to prove counter coalescing, hoisting, and jump removal.
#[test]
fn test_loop_optimization_example_structure_and_behavior() {
    let fixture = Fixture::new(include_str!("../../../examples/loop-optimization/main.php"));
    let plain = function_ir(&String::from_utf8(fixture.compile(false, &["--emit-ir"]).stdout).unwrap(), "main");
    let optimized = function_ir(&String::from_utf8(fixture.compile(true, &["--emit-ir"]).stdout).unwrap(), "main");
    assert!(plain.contains("for.update"), "{plain}");
    assert!(optimized.contains("origin: licm"), "{optimized}");
    assert_eq!(plain.matches("= icmp ").count(), 2, "{plain}");
    assert_eq!(optimized.matches("= icmp ").count(), 1, "redundant counter test survives: {optimized}");
    let header = optimized.lines().find(|line| line.contains("for.cond(")).unwrap();
    assert_eq!(header.matches("I64").count(), 2, "only counter and checksum remain: {optimized}");
    assert!(optimized.contains("for.update:\n      unreachable"), "{optimized}");
    for mode in [false, true] {
        fixture.compile(mode, &[]);
        assert_eq!(fixture.run(&[]), "items=8; checksum=84\n");
        assert_eq!(fixture.run(&["extra"]), "items=9; checksum=108\n");
    }
}

/// Nested loops, empty iterations, and continue/break edges preserve their final values.
#[test]
fn test_loop_optimization_nested_control_flow_both_modes() {
    let source = r#"<?php
$n = $argc & 7;
$total = 0;
for ($i = 0; $i < $n; $i++) {
    for ($j = 5; $j > 0; $j--) {
        if ($j == 4) { continue; }
        if ($j == 1) { break; }
        $total = ($total + $j) & 255;
    }
}
$zero = $argc - 1;
while ($zero < 0) { echo "unexpected"; $zero++; }
do { $zero++; } while ($zero < 2);
echo $i, ":", $total, ":", $zero;
"#;
    let fixture = Fixture::new(source);
    for mode in [false, true] {
        fixture.compile(mode, &[]);
        assert_eq!(fixture.run(&[]), "1:10:2");
        assert_eq!(fixture.run(&["extra"]), "2:20:2");
    }
}

/// Volatile bounds and side-effecting body calls must still be evaluated on every iteration.
#[test]
fn test_loop_optimization_mutable_bound_and_calls_both_modes() {
    let fixture = Fixture::new(r#"<?php
function limit(int &$calls): int { $calls++; return 4; }
$calls = (int) ($argc - 1);
$sum = 0;
for ($i = 0; $i < limit($calls); $i++) { $sum += $i; }
$bound = 4;
for ($j = 0; $j < $bound; $j++) { $bound--; }
echo $sum, ":", $calls, ":", $j, ":", $bound;
"#);
    for mode in [false, true] {
        fixture.compile(mode, &[]);
        assert_eq!(fixture.run(&[]), "6:5:2:2");
    }
}

/// Checked arithmetic at integer endpoints preserves PHP overflow and the exit value.
#[test]
fn test_loop_optimization_overflow_both_modes() {
    let fixture = Fixture::new(r#"<?php
$start = PHP_INT_MAX - ($argc - 1);
$i = $start;
$j = $start;
for ($k = 0; $k < 2; $k++) { $i++; $j += 2; }
echo is_float($i) ? "float" : "int", ":", is_float($j) ? "float" : "int";
"#);
    for mode in [false, true] {
        fixture.compile(mode, &[]);
        assert_eq!(fixture.run(&[]), "float:float");
    }
}

/// Exception paths and loop-carried values remain observable across catch boundaries.
#[test]
fn test_loop_optimization_exception_both_modes() {
    let fixture = Fixture::new(r#"<?php
$i = $argc - 1;
try {
    while ($i < 4) {
        $i++;
        if ($i == 2) { throw new Exception("stop"); }
    }
} catch (Exception $e) { echo $i, ":", $e->getMessage(); }
"#);
    for mode in [false, true] {
        fixture.compile(mode, &[]);
        assert_eq!(fixture.run(&[]), "2:stop");
    }
}

/// All supported targets generate both modes with invariant materializations outside loops.
#[test]
fn test_loop_optimization_all_supported_targets() {
    let fixture = Fixture::new(r#"<?php
#[Export]
function canonical_checksum(int $limit): int {
    $bound = $limit & 255;
    $left = 0;
    $right = 0;
    $checksum = 0;
    for (; $bound > $left; $left++, $right++) {
        if ($left > $right) { return -1; }
        $checksum = ($checksum + $left * 3) & 65535;
    }
    return $checksum;
}
"#);
    for target in ["linux-x86_64", "linux-aarch64", "macos-aarch64", "ios-arm64", "ios-sim-arm64"] {
        for mode in [false, true] {
            let mut options = vec!["--emit-asm", "--target", target];
            if target.starts_with("ios-") { options.extend(["--emit", "staticlib"]); }
            fixture.compile(mode, &options);
            let assembly = fs::read_to_string(fixture.0.join("main.s")).unwrap();
            assert_eq!(assembly.contains("origin=licm"), mode, "{target}, optimized={mode}");
            assert!(assembly.contains("op=icmp"), "{target}: missing loop comparison");
        }
    }
}

/// Zero-trip loops do not speculate faults, and mutable array lengths remain live bounds.
#[test]
fn test_loop_optimization_zero_trip_and_heap_state() {
    let fixture = Fixture::new(r#"<?php
for ($i = $argc; $i < 0; $i++) { echo intdiv(7, 0); }
$items = [1, 2, 3, 4];
for ($j = 0; $j < count($items); $j++) { array_pop($items); }
echo $i, ":", $j, ":", count($items);
"#);
    for mode in [false, true] {
        fixture.compile(mode, &[]);
        assert_eq!(fixture.run(&[]), "1:2:2");
    }
}
