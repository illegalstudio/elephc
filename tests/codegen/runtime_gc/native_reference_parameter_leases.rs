//! Purpose:
//! Covers active, detached and unwound native parameter reference bindings during object cloning.
//!
//! Called from:
//! - Executable runtime ownership tests in CI on every host target.
//!
//! Key details:
//! - A managed parameter binding is a visible alias only while its PHP name remains attached.
//! - Return and throw must retire its owner independently of the caller and property bindings.

use crate::support::*;

/// Native frames preserve active aliases and retire detached or throwing bindings without leaks.
#[test]
fn test_native_reference_parameter_leases_cover_clone_detach_and_throw() {
    let source = r#"<?php
class NativeParameterLeaseProbe { public mixed $value = 'before'; }
function copyWithNativeLease(mixed &$value, NativeParameterLeaseProbe $object, bool $detach): NativeParameterLeaseProbe {
    if ($detach) { unset($value); }
    return clone $object;
}
function throwWithNativeLease(mixed &$value): void {
    $value = 'written';
    throw new Exception('caught');
}
$active = new NativeParameterLeaseProbe();
$activeCopy = copyWithNativeLease($active->value, $active, false);
$active->value = 'after';
echo $activeCopy->value, '|';
$detached = new NativeParameterLeaseProbe();
$detachedCopy = copyWithNativeLease($detached->value, $detached, true);
$detached->value = 'after';
echo $detachedCopy->value, '|';
$throwing = new NativeParameterLeaseProbe();
try { throwWithNativeLease($throwing->value); }
catch (Exception $error) { echo $error->getMessage(), '|'; }
$throwCopy = clone $throwing;
$throwing->value = 'after';
echo $throwCopy->value, '|';
"#;
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "stdout={} stderr={}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "after|before|caught|written|");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}
