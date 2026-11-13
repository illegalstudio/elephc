//! Purpose:
//! Regression tests for references bound to the public mirrors of eval-built reflectors.
//!
//! Called from:
//! - `cargo test --test codegen_tests eval::reflection_mirror_references` through Rust's test
//!   harness.
//!
//! Key details:
//! - Binding `$x = &$reflector->name` anywhere makes `name` an object-owned reference
//!   property, so every reflector must store that mirror in a reference cell, including the
//!   ones libelephc-magician materializes. Expected output is php 8.5's.

use crate::support::*;

/// References to the `name` / `class` mirrors of eval-built method, parameter and attribute
/// reflectors write through a real reference cell: the private metadata is untouched and the
/// heap stays intact (a plain mirror string used to take the reference write's 16 bytes).
#[test]
fn test_eval_reflector_mirror_references_use_owned_cells() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
#[Attribute]
class EvalRefMirrorMarker {}
#[EvalRefMirrorMarker]
class EvalRefMirrorTarget {
    public function action(string $value): void {}
}
function bindEvalMethodMirrors(ReflectionMethod $method): void {
    $nameAlias =& $method->name;
    $nameAlias = 'renamed';
    $classAlias =& $method->class;
    $classAlias = 'changed';
    echo $method->getName(), ':', $method->getDeclaringClass()->getName(), '|';
}
function bindEvalParameterMirror(ReflectionParameter $parameter): void {
    $alias =& $parameter->name;
    $alias = 'changed';
    echo $parameter->getName(), '|';
}
function bindEvalAttributeMirror(ReflectionAttribute $attribute): void {
    $alias =& $attribute->name;
    $alias = 'changed';
    echo $attribute->getName(), "\n";
}
function probe(): void {
    bindEvalMethodMirrors(eval('return new ReflectionMethod("EvalRefMirrorTarget", "action");'));
    bindEvalParameterMirror(eval('return (new ReflectionMethod("EvalRefMirrorTarget", "action"))->getParameters()[0];'));
    bindEvalAttributeMirror(eval('return (new ReflectionClass("EvalRefMirrorTarget"))->getAttributes()[0];'));
}
probe();
"#,
    );
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "action:EvalRefMirrorTarget|value|EvalRefMirrorMarker\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
