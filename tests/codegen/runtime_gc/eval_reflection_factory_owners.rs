//! Purpose:
//! Checks owners transferred into Reflection factory results and static metadata snapshots.
//!
//! Called from:
//! - The runtime GC integration suite.
//!
//! Key details:
//! - A class and a typed parameter exercise nested metadata owners repeatedly.
//! - Constructorless object creation keeps its ordinary eval semantics.
//! - Native static-property snapshots retain borrowed boxed values independently.

use crate::support::*;

/// Factory results retire their original object owner along with nested Reflection metadata.
#[test]
fn test_eval_reflection_factory_results_release_metadata_graphs() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$source = $argc > 0 ? '
class EvalReflectionFactoryTarget { public string $name = "kept"; }
function evalReflectionFactoryArgument(string $text = "default"): string { return $text; }
for ($index = 0; $index < 3; $index++) {
    $class = new ReflectionClass("EvalReflectionFactoryTarget");
    $object = $class->newInstanceWithoutConstructor();
    $parameter = new ReflectionParameter("evalReflectionFactoryArgument", 0);
    echo $object->name, ":", $class->getName(), ":", $parameter->getName(), ":", $parameter->getDefaultValue(), "|";
    unset($class, $object, $parameter);
}
' : '';
eval($source);
unset($source);
"#);
    assert!(out.success, "stdout={:?}\n{}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "kept:EvalReflectionFactoryTarget:text:default|".repeat(3), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Reflection allocation transfer remains balanced without EIR optimization.
#[test]
fn test_eval_reflection_factory_results_release_metadata_graphs_no_ir_opt() {
    without_ir_opt(test_eval_reflection_factory_results_release_metadata_graphs);
}

/// Destroying native Reflection metadata leaves each original boxed static value alive.
#[test]
fn test_native_reflection_static_snapshots_preserve_boxed_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ReflectionStaticSnapshotTarget {
    public static mixed $text = null;
    public static ?float $number = null;
    public float $literal = 3;
}
ReflectionStaticSnapshotTarget::$text = str_repeat('x', 81);
ReflectionStaticSnapshotTarget::$number = 3.0;
for ($index = 0; $index < 3; $index++) {
    $reflection = new ReflectionClass(ReflectionStaticSnapshotTarget::class);
    $defaults = $reflection->getDefaultProperties();
    echo (int)($defaults['literal'] === 3.0), ':';
    unset($defaults, $reflection);
    echo strlen(ReflectionStaticSnapshotTarget::$text), ':',
        (int)(ReflectionStaticSnapshotTarget::$number === 3.0), '|';
}
ReflectionStaticSnapshotTarget::$text = null;
ReflectionStaticSnapshotTarget::$number = null;
"#);
    assert!(out.success, "stdout={:?}\n{}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "1:81:1|".repeat(3), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Boxed static snapshot owners remain independent without EIR optimization.
#[test]
fn test_native_reflection_static_snapshots_preserve_boxed_owners_no_ir_opt() {
    without_ir_opt(test_native_reflection_static_snapshots_preserve_boxed_owners);
}
