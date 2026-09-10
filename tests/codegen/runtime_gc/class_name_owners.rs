//! Purpose:
//! Pins the lifetime of object temporaries inspected through class-name metadata.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - Returned names do not own or borrow the inspected object or an array read cell.
//! - Destructors expose both delayed cleanup and invalid early release of borrowed inputs.

use crate::support::*;

/// AOT class-name reads retire eval bridge cells and preserve names after their objects die.
#[test]
fn test_core_eval_class_name_bridge_results_have_independent_owners() {
    let (out, assembly) = compile_and_run_with_heap_debug_and_asm(r#"<?php
class EvalNameBase {}
class EvalNameChild extends EvalNameBase {}
$source = 'return new EvalNameChild();' . ' // ' . $argc;
for ($iteration = 0; $iteration < 3; $iteration++) {
    $object = eval($source);
    echo GET_CLASS($object), ":", \get_parent_class($object), "|";
    $name = get_class($object);
    $parent = get_parent_class($object);
    unset($object);
    echo $name, ":", $parent, "|";
    unset($name, $parent);
}
echo "done";
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, format!("{}done", "EvalNameChild:EvalNameBase|".repeat(6)),
        "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}\nuser assembly:\n{}", out.stderr, assembly);
}

/// Boxed reads retire after metadata lookup while the retained source still owns its object.
#[test]
fn test_core_class_name_reads_do_not_retain_inspected_objects() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class InspectedNameBase {}
class InspectedNameChild extends InspectedNameBase {
    public function __destruct() { echo "drop|"; }
}
function inspectedNameRows(): array { return [new InspectedNameChild()]; }
$rows = inspectedNameRows();
$name = GET_CLASS($rows[0]);
$parent = \get_parent_class($rows[0]);
echo $name, ":", $parent, "|";
unset($rows);
echo $name, ":", $parent, "|done";
unset($name, $parent);
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout,
        "InspectedNameChild:InspectedNameBase|drop|InspectedNameChild:InspectedNameBase|done",
        "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Temporary object destruction happens before the class-name result is consumed, even on throw.
#[test]
fn test_core_class_name_temporary_destructor_throw_remains_catchable() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ThrowingNameOwner {
    public function __destruct() { echo "drop|"; throw new Exception("stop"); }
}
function throwingNameRows(): array { return [new ThrowingNameOwner()]; }
try { echo get_class(throwingNameRows()[0]), "unexpected"; }
catch (Exception $error) { echo $error->getMessage(), "|"; unset($error); }
echo "done";
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "drop|stop|done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Boxed closure reads and incomplete objects retain their independent metadata-name results.
#[test]
fn test_core_special_class_names_outlive_their_inspected_cells() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ClosureNameOwner { public function __destruct() { echo "drop|"; } }
function closureNameRows(): array {
    $owner = new ClosureNameOwner();
    return [function() use ($owner): void {}];
}
$rows = closureNameRows();
$name = get_class($rows[0]);
unset($rows);
echo $name, "|";
$unknown = get_class(unserialize('O:7:"Missing":0:{}'));
echo $unknown;
unset($name, $unknown);
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "drop|Closure|__PHP_Incomplete_Class", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
