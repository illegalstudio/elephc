//! Purpose:
//! Checks selected enum object-reference contracts without losing the caller's place.
//!
//! Called from:
//! - The codegen types integration suite.
//!
//! Key details:
//! - Object reference widening is ABI-compatible, unlike scalar-to-Mixed reference widening.
//! - All source effects precede validation of the captured reference's current referent.

use crate::support::*;

const DECLARATIONS: &str = r#"<?php
class ReferenceBase { public int $number = 1; }
class ReferenceNarrow extends ReferenceBase {}
interface ObjectRefSelected { public function f(ReferenceNarrow &$value, int $tail = 0): string; }
enum AObjectRefTight implements ObjectRefSelected {
    case A;
    public function f(ReferenceNarrow &$value, int $tail = 0): string {
        echo 'M'; $value->number += $tail; return get_class($value);
    }
}
enum ZObjectRefLoose implements ObjectRefSelected {
    case A;
    public function f(ReferenceBase &$value, int $tail = 0): string {
        echo 'M'; $value->number += $tail; return get_class($value);
    }
}
function referenceTail(): int { echo 'E'; return 2; }
"#;

const ERROR: &str = "AObjectRefTight::f(): Argument #1 ($value) must be of type ReferenceNarrow, ReferenceBase given";

/// Selected object checks reject a wider class after source effects and preserve valid aliases.
#[test]
fn test_enum_reference_type_review_selected_objects() {
    let source = format!(r#"{DECLARATIONS}
function selectedObject(ObjectRefSelected $target, ReferenceBase $value): void {{
    try {{ echo $target->f($value, referenceTail()); }}
    catch (TypeError $error) {{ echo $error->getMessage(); }}
    echo ':', $value->number, '|';
    try {{ echo $target->f(tail: referenceTail(), value: $value); }}
    catch (TypeError $error) {{ echo $error->getMessage(); }}
    echo ':', $value->number, '|';
}}
selectedObject(ZObjectRefLoose::A, new ReferenceBase());
selectedObject(AObjectRefTight::A, new ReferenceBase());
selectedObject(AObjectRefTight::A, new ReferenceNarrow());
"#);
    let out = compile_and_run_with_heap_debug(&source);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, format!("EMReferenceBase:3|EMReferenceBase:5|E{ERROR}:1|E{ERROR}:1|EMReferenceNarrow:3|EMReferenceNarrow:5|"));
    let summary = out.stderr.lines().next().unwrap();
    assert!(summary.contains("live_blocks=2 live_bytes=80"), "{}", out.stderr);
}

/// Object-reference contracts retain the same guards without EIR optimization.
#[test]
fn test_enum_reference_type_review_selected_objects_no_ir_opt() {
    without_ir_opt(test_enum_reference_type_review_selected_objects);
}

/// Guards read the captured cell after source effects rather than an early value snapshot.
#[test]
fn test_enum_reference_type_review_current_referent() {
    let source = format!(r#"{DECLARATIONS}
function referenceBecomeNarrow(ReferenceBase &$value): int {{ echo 'E'; $value = new ReferenceNarrow(); return 2; }}
function referenceBecomeBase(ReferenceBase &$value): int {{ echo 'E'; $value = new ReferenceBase(); return 2; }}
function selectedCurrent(ObjectRefSelected $target, ReferenceBase $value, bool $narrow): void {{
    try {{
        if ($narrow) {{ echo $target->f($value, referenceBecomeNarrow($value)); }}
        else {{ echo $target->f($value, referenceBecomeBase($value)); }}
    }} catch (TypeError $error) {{ echo $error->getMessage(); }}
    echo ':', get_class($value), ':', $value->number, '|';
}}
selectedCurrent(AObjectRefTight::A, new ReferenceBase(), true);
selectedCurrent(AObjectRefTight::A, new ReferenceNarrow(), false);
"#);
    assert_eq!(compile_and_run(&source), format!("EMReferenceNarrow:ReferenceNarrow:3|E{ERROR}:ReferenceBase:1|"));
}

/// Current-reference reads and stabilized dimensions do not depend on EIR optimization.
#[test]
fn test_enum_reference_type_review_current_referent_no_ir_opt() {
    without_ir_opt(test_enum_reference_type_review_current_referent);
}

/// A reference-return lease survives checked argument aliases and preserves a rejected target.
#[test]
fn test_enum_reference_type_review_returned_reference() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ReturnedReferenceBase { public int $number = 1; }
class ReturnedReferenceNarrow extends ReturnedReferenceBase {}
interface ReturnedReferenceContract { public function &f(ReturnedReferenceNarrow &$value): mixed; }
enum AReturnedReferenceTight implements ReturnedReferenceContract {
    case A; public function &f(ReturnedReferenceNarrow &$value): mixed { return $value; }
}
enum ZReturnedReferenceLoose implements ReturnedReferenceContract {
    case A; public function &f(ReturnedReferenceBase &$value): mixed { return $value; }
}
function invokeReturnedReference(ReturnedReferenceContract $target, ReturnedReferenceBase $value): void {
    $anchor = new ReturnedReferenceBase(); $result = &$anchor;
    try {
        $result = &$target->f($value); $result->number += 3;
        echo get_class($result), ':', $value->number;
    } catch (TypeError $error) {
        echo $error->getMessage(), ':', $value->number, ':', $result->number;
    }
    echo '|';
}
invokeReturnedReference(ZReturnedReferenceLoose::A, new ReturnedReferenceBase());
invokeReturnedReference(AReturnedReferenceTight::A, new ReturnedReferenceBase());
invokeReturnedReference(AReturnedReferenceTight::A, new ReturnedReferenceNarrow());
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "ReturnedReferenceBase:4|AReturnedReferenceTight::f(): Argument #1 ($value) must be of type ReturnedReferenceNarrow, ReturnedReferenceBase given:1:1|ReturnedReferenceNarrow:4|");
    assert!(out.stderr.lines().next().unwrap().contains("live_blocks=2 live_bytes=80"), "{}", out.stderr);
}

/// Returned managed references remain valid without EIR optimization.
#[test]
fn test_enum_reference_type_review_returned_reference_no_ir_opt() {
    without_ir_opt(test_enum_reference_type_review_returned_reference);
}
