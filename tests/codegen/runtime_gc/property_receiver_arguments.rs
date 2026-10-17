//! Purpose:
//! Regression tests for live property receivers during mutating builtin argument evaluation.
//!
//! Called from:
//! - `tests/codegen/runtime_gc.rs` through the native codegen harness.
//!
//! Key details:
//! - PHP property references outlive their objects without delaying object destruction.
//! - Mutating arguments must preserve the current property value and COW aliases.

use crate::support::{compile_and_run_with_heap_debug, elephc_cli_command, make_cli_test_dir};

/// A nested shift updates the live receiver while an earlier COW alias remains independent.
#[test]
fn test_property_receiver_observes_argument_mutation() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Bag { public $items = []; }
$bag = new Bag;
$bag->items = [1, 2]; $copy = $bag->items;
array_unshift($bag->items, (int)array_shift($bag->items));
echo count($bag->items), "|", implode(",", $bag->items), "|", implode(",", $copy), "\n";
$local = [1, 2];
array_unshift($local, (int)array_shift($local));
echo implode(",", $local), "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "2|1,2|1,2\n1,2\n");
    assert!(!out.stderr.contains("Warning:"), "{}", out.stderr);
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}

/// The temporary object dies before the following argument while its property cell survives.
#[test]
fn test_property_receiver_temporary_owner_destruction_order() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Bag {
    public $items = [1, 2];
    public function __destruct() { echo "D:", implode(",", $this->items), ";"; }
}
function make() { echo "O;"; return new Bag; }
function value() { echo "A;"; return 3; }
array_unshift(make()->items, value());
echo "end\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "O;D:1,2;A;end\n");
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}

/// Argument rebinding cannot redirect the outer mutation to the replacement object.
#[test]
fn test_property_receiver_keeps_identity_when_owner_is_rebound() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Bag {
    public $items = [1, 2]; public $name = "old";
    public function __construct($name, $items) { $this->name = $name; $this->items = $items; }
    public function __destruct() { echo "D:", $this->name, ":", implode(",", $this->items), ";"; }
}
function swap(&$object) {
    echo "S;"; $object = new Bag("new", [8, 9]); echo "s;"; return 3;
}
$bag = new Bag("old", [1, 2]);
array_unshift($bag->items, swap($bag));
echo "N:", implode(",", $bag->items), ";"; unset($bag); echo "end\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "S;D:old:1,2;s;N:8,9;D:new:8,9;end\n");
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}

/// A same-frame catch retires the hidden property-cell owner before entering the handler.
#[test]
fn test_property_receiver_argument_throw_retires_reference_owner() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Payload { public function __destruct() { echo "V;"; } }
class Bag {
    public $items = [];
    public function __construct() { $this->items = [1, new Payload]; }
    public function __destruct() { echo "O;"; }
}
function make() { return new Bag; }
function fail(): int { echo "F;"; throw new Exception("failure"); }
try { array_unshift(make()->items, fail()); }
catch (Exception $error) { echo "C;"; unset($error); }
echo "end\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "O;F;V;C;end\n");
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}

/// Packed Mixed receivers own concrete prepended values independently of their argument slots.
#[test]
fn test_property_receiver_mixed_values_keep_owned_payloads() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Payload {
    public int $id;
    public function __construct(int $id) { $this->id = $id; }
    public function __destruct() { echo "D", $this->id, ";"; }
}
class Bag { public $items = []; }
$bag = new Bag; $bag->items = [0, "old"]; $copy = $bag->items;
$value = new Payload(1);
array_unshift($bag->items, $value, "head", null); unset($value);
echo $bag->items[0]->id, ":", count($bag->items), ":", count($copy), "|";
unset($bag, $copy); echo "end\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "1:5:2|D1;end\n");
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}

/// An already boxed argument keeps its own owner after being retained into a Mixed receiver.
#[test]
fn test_property_receiver_boxed_value_keeps_owned_payload() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Payload {
    public int $id;
    public function __construct(int $id) { $this->id = $id; }
    public function __destruct() { echo "D", $this->id, ";"; }
}
class Bag { public $items = []; }
function value(): mixed { return new Payload(2); }
$bag = new Bag; $bag->items = [0, "old"];
$value = value(); array_unshift($bag->items, $value); unset($value);
echo $bag->items[0]->id, ":", count($bag->items), "|";
unset($bag); echo "end\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "2:3|D2;end\n");
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}

/// A clone made before the call keeps its independent cell and shared array payload alive.
#[test]
fn test_property_receiver_clone_before_mutation() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Bag { public $items = [1, 2]; }
$bag = new Bag; $copy = clone $bag;
array_unshift($bag->items, 3);
echo implode(",", $bag->items), "|", implode(",", $copy->items), "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "3,1,2|1,2\n");
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}

/// Cloning during an argument shares the live reference until the outer call releases it.
#[test]
fn test_property_receiver_clone_during_argument() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Bag { public $items = [1, 2]; }
function value(Bag $bag): int {
    $copy = clone $bag; array_shift($copy->items); return 3;
}
$bag = new Bag;
array_unshift($bag->items, value($bag));
echo implode(",", $bag->items), "\n";
$copy = clone $bag; array_shift($copy->items);
echo implode(",", $bag->items), "|", implode(",", $copy->items), "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "3,2\n3,2|2\n");
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}

/// Static properties also reload the live receiver after nested argument mutation.
#[test]
fn test_static_property_receiver_observes_argument_mutation() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Bag { public static $items = [1, 2]; }
array_unshift(Bag::$items, (int)array_shift(Bag::$items));
echo implode(",", Bag::$items), "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "1,2\n");
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}

/// Dynamic receiver validation still happens after evaluating the remaining arguments.
#[test]
fn test_property_receiver_dynamic_type_error_preserves_argument_effects() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function value(): int { echo "V;"; return 3; }
function prepend(mixed &$items): void { array_unshift($items, value()); }
$items = 1;
try { prepend($items); } catch (TypeError $error) { echo "T;"; unset($error); }
echo $items, "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "V;T;1\n");
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}

/// Capturing a property reference must respect readonly slots and explicit backend limits.
#[test]
fn test_property_receiver_rejects_readonly_and_unknown_object_class() {
    let dir = make_cli_test_dir("elephc_property_receiver_errors");
    let path = dir.join("main.php");
    for (source, expected) in [
        (r#"<?php
class Bag {
    public readonly array $items;
    public function __construct() { $this->items = [1, 2]; }
}
$bag = new Bag; array_unshift($bag->items, 3);
"#, "Cannot modify readonly property Bag::$items by reference"),
        (r#"<?php
class Bag { public $items = [1, 2]; }
function make(): mixed { return new Bag; }
array_unshift(make()->items, 3);
"#, "Unsupported array_unshift property reference: the object's class is not statically known"),
    ] {
        std::fs::write(&path, source).expect("write invalid property receiver");
        let output = elephc_cli_command(&dir).arg(&path).output().expect("compile invalid receiver");
        assert!(!output.status.success(), "accepted {source}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(expected), "{stderr}");
    }
    std::fs::remove_dir_all(dir).expect("remove property receiver fixtures");
}

/// All supported targets lower a live property reference before the nested mutation.
#[test]
fn test_property_receiver_live_reference_all_target_lowering() {
    let dir = make_cli_test_dir("elephc_property_receiver_targets");
    let path = dir.join("main.php");
    for call in [
        "array_unshift($bag->items, (int)array_shift($bag->items));",
        "$prepend = array_unshift(...); $prepend($bag->items, (int)array_shift($bag->items));",
        "$prepend = \"array_unshift\"; $prepend($bag->items, (int)array_shift($bag->items));",
    ] {
        let source = r#"<?php
class Bag { public $items = [1, 2]; }
function rotate(Bag $bag): void {
    __CALL__
}
rotate(new Bag);
"#.replace("__CALL__", call);
        std::fs::write(&path, source).expect("write property receiver target fixture");
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut command = elephc_cli_command(&dir);
            command.args(["--emit-ir", "--target", target]);
            if target.starts_with("ios-") {
                command.args(["--emit", "staticlib"]);
            }
            let output = command.arg(&path).output().expect("emit property receiver EIR");
            assert!(output.status.success(), "{target}: {}", String::from_utf8_lossy(&output.stderr));
            let ir = String::from_utf8(output.stdout).expect("UTF-8 target EIR");
            let body = ir.split("function rotate(").nth(1).expect("rotate function")
                .split("\n  function ").next().expect("rotate body");
            let capture = body.find(" = load_prop_ref_cell ").expect("capture property reference");
            let shift = body.find("runtime.array_shift").expect("nested shift");
            let unshift = body.find("runtime.array_unshift").expect("outer unshift");
            assert!(capture < shift && shift < unshift, "{target}: {body}");
            assert!(body.contains("retain_local_ref_cell "), "{target}: {body}");
            assert!(body[unshift..].contains("pop_call_operand_owner "), "{target}: {body}");
        }
    }
    std::fs::remove_dir_all(dir).expect("remove property receiver target fixture");
}

/// First-class builtin syntax captures the same property cell as a direct invocation.
#[test]
fn test_property_receiver_first_class_callable_observes_argument_mutation() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Bag { public $items = [1, 2]; }
$bag = new Bag;
$prepend = array_unshift(...);
$prepend($bag->items, (int)array_shift($bag->items));
echo count($bag->items), "|", implode(",", $bag->items), "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "2|1,2\n");
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}

/// A string callable resolved during lowering still captures before evaluating its value.
#[test]
fn test_property_receiver_string_callable_observes_argument_mutation() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Bag { public $items = [1, 2]; }
$bag = new Bag;
$prepend = "array_unshift";
$prepend($bag->items, (int)array_shift($bag->items));
echo count($bag->items), "|", implode(",", $bag->items), "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "2|1,2\n");
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}

/// Potential reference storage does not make by-value callable arguments observe later writes.
#[test]
fn test_property_receiver_value_callable_keeps_its_snapshot() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Bag { public $items = [1, 2]; }
function observe($items, $ignored) { echo implode(",", $items), "|"; }
$bag = new Bag;
$invoke = "observe";
$invoke($bag->items, (int)array_shift($bag->items));
echo implode(",", $bag->items), "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "1,2|2\n");
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}

/// An unresolved by-value callable can read readonly array properties without taking a reference.
#[test]
fn test_property_receiver_value_callable_accepts_readonly_storage() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Bag {
    public readonly array $items;
    public function __construct() { $this->items = [1, 2]; }
}
function size(array $items) { return count($items); }
$bag = new Bag;
$invoke = "size";
echo $invoke($bag->items), "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "2\n");
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}

/// Runtime callable parameters retain their explicit non-local reference diagnostic.
#[test]
fn test_property_receiver_runtime_descriptor_reports_unsupported_reference_binding() {
    let dir = make_cli_test_dir("elephc_property_runtime_descriptor_boundary");
    let path = dir.join("main.php");
    std::fs::write(&path, r#"<?php
class Bag { public $items = [1, 2]; }
function invoke(callable $fn, Bag $bag): void {
    $fn($bag->items, (int)array_shift($bag->items));
}
$bag = new Bag;
invoke(array_unshift(...), $bag);
echo count($bag->items), "|", implode(",", $bag->items), "\n";
"#).unwrap();
    let output = elephc_cli_command(&dir).arg("--check").arg(&path).output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("callable $fn parameter $array must be passed a variable"), "{stderr}");
    std::fs::remove_dir_all(dir).unwrap();
}

/// A throwing temporary-object destructor leaves the property cell reachable by unwind cleanup.
#[test]
fn test_property_receiver_throwing_owner_retires_reference_lease() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Payload { public function __destruct() { echo "P;"; } }
class Bag {
 public $items = [];
 public function __construct() { $this->items = [1, new Payload]; }
 public function __destruct() { echo "D;"; throw new Exception("owner"); }
}
function make() { return new Bag; }
function value() { echo "V;"; return 3; }
try { array_unshift(make()->items, value()); }
catch (Exception $error) { echo "C;"; unset($error); }
echo "end\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "D;P;C;end\n");
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}

/// Concrete owning temporaries retain their payloads after the call's argument roots are retired.
#[test]
fn test_property_receiver_fresh_mixed_values_keep_owned_payloads() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Payload {
 public $number = 7;
 public function __destruct() { echo "D;"; }
}
class Bag { public $items = [0, null]; }
$bag = new Bag;
array_unshift($bag->items, new Payload, str_repeat("x", $argc + 2));
echo $bag->items[0]->number, ":", strlen($bag->items[1]), ";";
unset($bag); echo "end\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "7:3;D;end\n");
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}

/// Retiring the hidden lease leaves receiver self-cycles available to the normal collector.
#[test]
fn test_property_receiver_self_cycle_is_collectable_after_the_call() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Bag {
 public $items = [0, null];
 public function __destruct() { echo "D;"; }
}
$bag = new Bag;
array_unshift($bag->items, $bag);
unset($bag);
gc_collect_cycles();
echo "end\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "D;end\n");
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}
