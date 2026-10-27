//! Purpose:
//! Checks ownership transferred from eval Reflection factories into their result cells.
//!
//! Called from:
//! - The runtime GC integration suite.
//!
//! Key details:
//! - A class and a typed parameter exercise nested metadata owners repeatedly.
//! - Constructorless object creation keeps its ordinary eval semantics.

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
