//! Purpose:
//! Checks nullable object diagnostics at native ReflectionProperty raw writes.
//!
//! Called from:
//! - Runtime GC integration tests on the executable CI targets.
//!
//! Key details:
//! - Literal eval reaches native storage while preserving the original property value.

use crate::support::*;

/// A raw reflection write rejects null with a catchable TypeError before native storage changes.
#[test]
fn test_native_raw_property_null_guard_preserves_value() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class NativeRawPropertyNode {}
class NativeRawPropertyOwner { public NativeRawPropertyNode $node; }
$box = new NativeRawPropertyOwner(); $box->node = new NativeRawPropertyNode();
$property = new ReflectionProperty(NativeRawPropertyOwner::class, 'node');
$property->setValue($box, $box->node);
if (!$property->isInitialized($box)) { echo 'uninitialized|'; }
eval('$rawProperty = new ReflectionProperty(NativeRawPropertyOwner::class, "node"); try { $rawProperty->setRawValue($box, null); } catch (TypeError $error) { echo $error->getMessage(), "|"; } unset($rawProperty);');
echo get_class($box->node);
unset($box, $property, $error);
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "Cannot assign null to property NativeRawPropertyOwner::$node of type NativeRawPropertyNode|NativeRawPropertyNode");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Reflected targets retire after a successful call and when a later argument throws.
#[test]
fn test_reflection_property_target_owners_survive_argument_evaluation() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ReflectionLeaseTarget {
    public string $value = '';
    public function __destruct() { echo 'drop:', $this->value, '|'; }
}
function reflectionLeaseThrow(): string { throw new RuntimeException('rhs'); }
$property = new ReflectionProperty(ReflectionLeaseTarget::class, 'value');
try { $property->setValue(new ReflectionLeaseTarget(), reflectionLeaseThrow()); }
catch (RuntimeException $error) { echo $error->getMessage(), '|'; }
$property->setValue(new ReflectionLeaseTarget(), 'stored');
var_dump($property->isInitialized(new ReflectionLeaseTarget()));
unset($property, $error);
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "drop:|rhs|drop:stored|drop:|bool(true)\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
