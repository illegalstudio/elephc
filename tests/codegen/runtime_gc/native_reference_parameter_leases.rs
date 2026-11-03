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

/// Eval staging stays invisible after parameter unset while live PHP aliases still preserve sharing.
#[test]
fn test_eval_native_reference_parameter_leases_bundle() {
    let source = r#"<?php
class EvalNativeParameterLeaseProbe { public mixed $value = 'before'; }
function copyWithEvalNativeLease(mixed &$value, EvalNativeParameterLeaseProbe $object, bool $detach): EvalNativeParameterLeaseProbe {
    if ($detach) { unset($value); }
    return clone $object;
}
function throwWithEvalNativeLease(mixed &$value): void {
    $value = 'written';
    throw new Exception('caught');
}
$code = $argc > 0 ? '
$active = new EvalNativeParameterLeaseProbe();
$activeCopy = copyWithEvalNativeLease($active->value, $active, false);
$active->value = "after";
echo $activeCopy->value, "|";
$detached = new EvalNativeParameterLeaseProbe();
$detachedCopy = copyWithEvalNativeLease($detached->value, $detached, true);
$detached->value = "after";
echo $detachedCopy->value, "|";
$shared = new EvalNativeParameterLeaseProbe();
$alias = "before";
$shared->value =& $alias;
$sharedCopy = copyWithEvalNativeLease($shared->value, $shared, true);
$alias = "after";
echo $sharedCopy->value, "|";
$throwing = new EvalNativeParameterLeaseProbe();
try { throwWithEvalNativeLease($throwing->value); }
catch (Exception $error) { echo $error->getMessage(), "|"; }
$throwCopy = clone $throwing;
$throwing->value = "after";
echo $throwCopy->value, "|";
unset($activeCopy, $active, $detachedCopy, $detached, $sharedCopy, $shared, $alias,
    $error, $throwCopy, $throwing);
' : '';
eval($code); unset($code);
"#;
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "stdout={} stderr={}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "after|before|after|caught|written|");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Pending, duplicate, nested and aborted calls preserve exactly their live PHP bindings.
#[test]
fn test_native_reference_argument_handoff_preserves_binding_lifetimes() {
    let source = r#"<?php
class NativeArgumentHandoffProbe { public mixed $value = 'before'; }
function writeNativeArgument(mixed &$value, NativeArgumentHandoffProbe $copy): void {
    $value = 'after'; echo $copy->value, '|';
}
function detachNativeArguments(mixed &$first, mixed &$second, NativeArgumentHandoffProbe $object): NativeArgumentHandoffProbe {
    unset($first, $second); return clone $object;
}
function nestedNativeArgument(mixed &$value, NativeArgumentHandoffProbe $object): NativeArgumentHandoffProbe {
    unset($value); return clone $object;
}
function abortNativeArgument(): NativeArgumentHandoffProbe { throw new Exception('late'); }
$pending = new NativeArgumentHandoffProbe();
writeNativeArgument($pending->value, clone $pending);
echo $pending->value, '|';
$duplicate = new NativeArgumentHandoffProbe();
$copy = detachNativeArguments($duplicate->value, $duplicate->value, $duplicate);
$duplicate->value = 'after'; echo $copy->value, '|';
$nested = new NativeArgumentHandoffProbe();
writeNativeArgument($nested->value, nestedNativeArgument($nested->value, $nested));
$aborted = new NativeArgumentHandoffProbe();
try { writeNativeArgument($aborted->value, abortNativeArgument()); }
catch (Exception $error) { echo $error->getMessage(), '|'; }
$abortedCopy = clone $aborted;
$aborted->value = 'after'; echo $abortedCopy->value, '|';
"#;
    for optimize in [true, false] {
        let output = if optimize {
            compile_and_run_with_heap_debug(source)
        } else {
            without_ir_opt(|| compile_and_run_with_heap_debug(source))
        };
        assert!(output.success, "ir-opt={optimize}: stdout={} stderr={}", output.stdout, output.stderr);
        assert_eq!(output.stdout, "after|after|before|after|late|before|", "ir-opt={optimize}");
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
    }
}
