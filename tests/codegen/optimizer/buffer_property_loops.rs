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

/// Foreign direct and inlined property loops produce the same values without heap leaks.
#[test]
fn test_buffer_object_loop_example_matches_both_modes() {
    for optimized in [false, true] {
        let output = run_property_loop(include_str!("../../../examples/buffer-object-loop/main.php"), optimized);
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(String::from_utf8_lossy(&output.stdout), "0 2 0 2\n");
        assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"));
    }
}

/// Uninitialized foreign properties stay unread on zero trips, including through an accessor.
#[test]
fn test_buffer_object_loop_zero_trip_preserves_uninitialized_property() {
    for access in ["$table->slots[$index]", "$table->at($index)"] {
        let source = format!(r#"<?php
final class EmptyObjectSlots {{
    public buffer<int> $slots;
    public function at(int $index): int {{ return $this->slots[$index]; }}
}}
function scan(EmptyObjectSlots $table, int $count): void {{
    for ($index = 0; $index < $count; $index++) {{
        if ({access} === 7) {{ continue; }}
    }}
}}
$table = new EmptyObjectSlots();
scan($table, $argc - 1);
echo 'zero|';
scan($table, $argc);
"#);
        for optimized in [false, true] {
            let output = run_property_loop(&source, optimized);
            assert!(!output.status.success());
            assert_eq!(String::from_utf8_lossy(&output.stdout), "zero|");
            assert!(String::from_utf8_lossy(&output.stderr).contains("must not be accessed before initialization"));
        }
    }
}

/// A relocated foreign read still runs its warning handler and fails on the first access.
#[test]
fn test_buffer_object_loop_null_warning_keeps_first_access_fatal() {
    let source = r#"<?php
final class NullObjectSlots { public buffer<int> $slots; }
function scan(NullObjectSlots $table, int $count): void {
    for ($index = 0; $index < $count; $index++) {
        if ($table->slots[$index] === 7) { continue; }
    }
    echo 'wrong';
}
$count = $argc;
set_error_handler(function (int $errno, string $message) use (&$count): bool {
    $count = 0;
    echo 'warning|';
    return $errno > 0 || $message !== '';
});
$scan = 'scan';
$scan(null, $count);
"#;
    for optimized in [false, true] {
        let output = run_property_loop(source, optimized);
        assert!(!output.status.success());
        assert_eq!(String::from_utf8_lossy(&output.stdout), "warning|");
        assert!(String::from_utf8_lossy(&output.stderr).contains("use of buffer after buffer_free()"));
    }
    let dir = make_cli_test_dir("elephc_null_property_loop_ir");
    let path = dir.join("main.php");
    fs::write(&path, source).unwrap();
    let emitted = elephc_cli_command(&dir).arg("--emit-ir").arg(&path).output().unwrap();
    fs::remove_dir_all(dir).unwrap();
    assert!(emitted.status.success(), "{}", String::from_utf8_lossy(&emitted.stderr));
    let ir = String::from_utf8(emitted.stdout).unwrap();
    let scan = ir.split_once("  function scan(").unwrap().1.split("\n  function ").next().unwrap();
    assert!(scan.contains("buffer_property_entry_"), "{scan}");
    assert_eq!(scan.matches("= buffer_resolve ").count(), 1, "{scan}");
}

/// Rebinding a receiver and replacing a property through an alias retain their per-iteration reads.
#[test]
fn test_buffer_object_loop_receiver_and_property_mutations_stay_visible() {
    let source = r#"<?php
final class MutableObjectSlots {
    public buffer<int> $slots;
    public function __construct(int $value) {
        $this->slots = buffer_new<int>(2);
        $this->slots[0] = $value;
        $this->slots[1] = $value;
    }
}
function scan(MutableObjectSlots $first, MutableObjectSlots $second): void {
    $table = $first;
    for ($index = 0; $index < 2; $index++) {
        echo $table->slots[$index];
        $table = $second;
    }
    $alias = $first;
    for ($index = 0; $index < 2; $index++) {
        echo $first->slots[$index];
        $alias->slots = $second->slots;
    }
}
$first = new MutableObjectSlots(1);
$second = new MutableObjectSlots(3);
$old = $first->slots;
scan($first, $second);
buffer_free($old);
$slots = $second->slots;
buffer_free($slots);
"#;
    for optimized in [false, true] {
        let output = run_property_loop(source, optimized);
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(String::from_utf8_lossy(&output.stdout), "1313");
        assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"));
    }
}

/// A stale foreign buffer still fails only at the first entered access in either accessor form.
#[test]
fn test_buffer_object_loop_stale_handle_preserves_entry_timing() {
    for access in ["$table->slots[$index]", "$table->at($index)"] {
        let source = format!(r#"<?php
final class FreedObjectSlots {{
    public buffer<int> $slots;
    public function __construct() {{ $this->slots = buffer_new<int>(2); }}
    public function at(int $index): int {{ return $this->slots[$index]; }}
}}
function scan(FreedObjectSlots $table, int $count): void {{
    for ($index = 0; $index < $count; $index++) {{ if ({access} === 7) {{ continue; }} }}
}}
$table = new FreedObjectSlots();
$slots = $table->slots;
buffer_free($slots);
scan($table, $argc - 1);
echo 'zero|';
scan($table, $argc);
"#);
        for optimized in [false, true] {
            let output = run_property_loop(&source, optimized);
            assert!(!output.status.success());
            assert_eq!(String::from_utf8_lossy(&output.stdout), "zero|");
            assert!(String::from_utf8_lossy(&output.stderr).contains("use of buffer after buffer_free()"));
        }
    }
}

/// The inlined null receiver check stays after the zero-trip condition and keeps its method name.
#[test]
fn test_buffer_object_loop_inlined_null_receiver_preserves_error() {
    let source = r#"<?php
final class NullAccessorSlots {
    private buffer<int> $slots;
    public function at(int $index): int { return $this->slots[$index]; }
}

function scan(NullAccessorSlots $table, int $count): void {
    for ($index = 0; $index < $count; $index++) {
        if ($table->at($index) === 7) { continue; }
    }
}
$scan = 'scan';
$scan(null, $argc - 1);
echo 'zero|';
$scan(null, $argc);
"#;
    for optimized in [false, true] {
        let output = run_property_loop(source, optimized);
        assert!(!output.status.success());
        // PHP's uncaught report names the throw site and prints the stack trace; the file is the
        // per-run temporary `main.php`, so only its fixed parts are compared.
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.starts_with("zero|\nFatal error: Uncaught Error: Call to a member function at() on null in "),
            "{stdout}"
        );
        assert!(stdout.contains("\nStack trace:\n") && stdout.ends_with(" on line 9\n"), "{stdout}");
    }
}

/// An inner accessor guard must not redirect its latch through the outer loop's initial index.
#[test]
fn test_buffer_object_loop_nested_accessor_condition_keeps_outer_counter() {
    let source = r#"<?php
final class NestedSlots {
    public buffer<int> $slots;
    public function __construct() { $this->slots = buffer_new<int>(3); }
    public function at(int $index): int { return $this->slots[$index]; }
}
function scan(NestedSlots $table, int $count): void {
    for ($index = 0; $index < $count; $index++) {
        while ($table->at($index) === 7) { $table->slots[$index] = 0; }
        echo $index;
    }
}
$table = new NestedSlots();
$table->slots[1] = 7;
scan($table, 3);
echo "\n";
$slots = $table->slots;
buffer_free($slots);
"#;
    for optimized in [false, true] {
        let output = run_property_loop(source, optimized);
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(String::from_utf8_lossy(&output.stdout), "012\n");
        assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"));
    }
}

/// Formatting after an inlined accessor must reset scratch storage before each iteration.
#[test]
fn test_buffer_object_loop_formatted_output_resets_each_iteration() {
    let row = format!("{}0", " ".repeat(39999));
    let expected = format!("{row}{row}\ndone\n");
    for optimized in [false, true] {
        let output = run_property_loop(include_str!("../../fixtures/optimizer/buffer_object_concat_reset.php"), optimized);
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(output.stdout.len(), expected.len());
        assert!(output.stdout == expected.as_bytes(), "formatted loop output differs");
        assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"));
    }
}

/// Reassigning an inlined by-value parameter must change only its local index before the read.
#[test]
fn test_buffer_object_loop_reassigned_parameter_matches_both_modes() {
    for optimized in [false, true] {
        let output = run_property_loop(include_str!("../../fixtures/optimizer/buffer_object_reassigned_parameter.php"), optimized);
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(String::from_utf8_lossy(&output.stdout), "2 7 7 7 \n");
        assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"));
    }
}
