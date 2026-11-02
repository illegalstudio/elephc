//! Purpose:
//! Keeps escaped eval parameter references independent of temporary object receivers.
//!
//! Called from:
//! - The executable runtime ownership CI suite.
//!
//! Key details:
//! - Arrays, instance/static properties and closures retain persistent reference cells.
//! - Surviving receivers and ordinary variables continue to observe writes in both directions.

use crate::support::*;

/// Native numeric and named variadic markers publish immediately into one eval reference.
#[test]
fn test_eval_native_mixed_variadic_markers_keep_shared_reference_bundle() {
    let source = r#"<?php
function nativeMixedVariadicMarkers(callable $reader, &...$values): void {
    $values[0] = 'first';
    echo $reader(), '|';
    $values['named'] = 'last';
    echo $reader(), '|';
}
eval('class NativeVariadicStagingEvalMarker {}
$value = "before";
$reader = function () use (&$value): mixed { return $value; };
nativeMixedVariadicMarkers($reader, $value, named: $value);
echo $value, "|";
unset($reader, $value);');
"#;
    let expected = "first|last|last|";
    let (output, assembly) = compile_and_run_with_heap_debug_and_asm(source);
    preserve_stdout_mismatches(source, &assembly, std::slice::from_ref(&output), &[expected]);
    assert!(output.success, "stdout={} stderr={}", output.stdout, output.stderr);
    assert_eq!(output.stdout, expected);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
    assert!(!output.stderr.contains("Warning:"), "{}", output.stderr);
}

/// Native reference returns can retain a Mixed argument staged from an existing eval PHPRef.
#[test]
fn test_eval_native_mixed_reference_return_owns_staging_cell() {
    let source = r#"<?php
function &nativeMixedStagingIdentity(mixed &$value): mixed { return $value; }
eval('class NativeMixedStagingEvalMarker {}
$fresh = "fresh";
echo nativeMixedStagingIdentity($fresh), "|";
$value = "before";
$alias =& $value;
echo nativeMixedStagingIdentity($value), ":";
$value = "after";
echo $alias, "|";
unset($alias, $value, $fresh);');
"#;
    let expected = "fresh|before:after|";
    let (output, assembly) = compile_and_run_with_heap_debug_and_asm(source);
    preserve_stdout_mismatches(source, &assembly, std::slice::from_ref(&output), &[expected]);
    assert!(output.success, "stdout={} stderr={}", output.stdout, output.stderr);
    assert_eq!(output.stdout, expected);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
    assert!(!output.stderr.contains("Warning:"), "{}", output.stderr);
}

/// A native output builtin publishes before a reentrant eval reader observes the shared reference.
#[test]
fn test_eval_native_mixed_signal_output_keeps_reference_identity() {
    let source = r#"<?php
function nativeMixedSignalOutput(mixed &$output, callable $reader): void {
    pcntl_sigprocmask(SIG_BLOCK, [SIGKILL], $output);
    echo (int)is_array($reader()), '|';
}
eval('class NativeSignalStagingEvalMarker {}
$value = "before";
$alias =& $value;
$reader = function () use (&$value): mixed { return $value; };
nativeMixedSignalOutput($value, $reader);
echo (int)is_array($value), ":", (int)is_array($alias), "|";
unset($reader, $alias, $value);');
"#;
    let expected = "1|1:1|";
    let (output, assembly) = compile_and_run_with_heap_debug_and_asm(source);
    preserve_stdout_mismatches(source, &assembly, std::slice::from_ref(&output), &[expected]);
    assert!(output.success, "stdout={} stderr={}", output.stdout, output.stderr);
    assert_eq!(output.stdout, expected);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
    assert!(!output.stderr.contains("Warning:"), "{}", output.stderr);
}

/// Array-property writes separate orphan references while retaining genuinely shared elements.
#[test]
fn test_eval_property_array_clone_separates_orphan_references() {
    let source = r#"<?php
class NativeOrphanArrayClone { public array $items = []; }
$source = $argc > 0 ? '
class EvalOrphanArrayClone { public array $items = []; }
function exerciseOrphanArrayClone($original, $key, bool $shared): void {
    $value = "before";
    $original->items = [$key => &$value];
    if (!$shared) { unset($value); }
    $copy = clone $original;
    $copy->items[$key] = "after";
    echo $original->items[$key], ":", $copy->items[$key];
    if ($shared) { echo ":", $value; }
    echo "|";
    unset($value, $copy, $original);
}
foreach ([0, "key\\0part"] as $key) {
    exerciseOrphanArrayClone(new NativeOrphanArrayClone(), $key, false);
    exerciseOrphanArrayClone(new NativeOrphanArrayClone(), $key, true);
    exerciseOrphanArrayClone(new EvalOrphanArrayClone(), $key, false);
    exerciseOrphanArrayClone(new EvalOrphanArrayClone(), $key, true);
}
unset($key);
' : '';
eval($source); unset($source);
"#;
    let (out, assembly) = compile_and_run_with_heap_debug_and_asm(source);
    let expected = "before:after|after:after:after|".repeat(4);
    preserve_stdout_mismatches(source, &assembly, std::slice::from_ref(&out), &[&expected]);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert!(!out.stderr.contains("Warning:"), "{}", out.stderr);
}

/// Escaped property references outlive temporary receivers and retain caller alias identities.
#[test]
fn test_eval_property_reference_escape_owners_bundle() {
    let source = r#"<?php
$source = $argc > 0 ? '
class EscBox { public $value = "old"; public $ref; }
class EscHolder { public static $ref; }
function escFresh(): EscBox { return new EscBox(); }
function escArray(&$value) { return [&$value]; }
function escProperty(EscBox $holder, &$value) { $holder->ref = &$value; }
function escStatic(&$value) { EscHolder::$ref = &$value; }
function escClosure(&$value) { return function () use (&$value) { $value .= "+c"; return $value; }; }
$a = escArray(escFresh()->value); $f1 = new EscBox(); $f1->value = "filler";
echo $a[0], "|"; $a[0] = "a"; echo $a[0], "|";
$holder = new EscBox(); escProperty($holder, escFresh()->value); $f2 = new EscBox(); $f2->value = "filler";
echo $holder->ref, "|"; $holder->ref = "p"; echo $holder->ref, "|";
escStatic(escFresh()->value); $f3 = new EscBox(); $f3->value = "filler";
echo EscHolder::$ref, "|"; EscHolder::$ref = "s"; echo EscHolder::$ref, "|";
$closure = escClosure(escFresh()->value); echo $closure(), "|", $closure(), "|";
EscHolder::$ref = null; unset($a, $holder, $closure, $f1, $f2, $f3);
$box = new EscBox(); $a = escArray($box->value);
$box->value = "b1"; echo $a[0], "|"; $a[0] = "a1"; echo $box->value, "|";
$holder = new EscBox(); escProperty($holder, $box->value);
$box->value = "b2"; echo $holder->ref, "|"; $holder->ref = "h2"; echo $box->value, "|";
escStatic($box->value); $box->value = "b3"; echo EscHolder::$ref, "|";
EscHolder::$ref = "s3"; echo $box->value, "|";
$x = "x0"; $control = escArray($x); $x = "x1"; echo $control[0], "|";
EscHolder::$ref = null; unset($box, $a, $holder, $x, $control);
function escNoEscape(&$value): void { $value = "non-escaping"; }
$plain = new EscBox(); escNoEscape($plain->value);
$copy = clone $plain; $copy->value = "copy"; echo $plain->value, "|";
$shared = escArray($plain->value); $copy = clone $plain;
$copy->value = "shared"; echo $plain->value, "|";
unset($plain, $copy, $shared);
$orphan = "before"; $original = [&$orphan]; unset($orphan);
$copy = $original; $copy[0] = "copied";
echo $original[0], ":", $copy[0], "|";
unset($original); echo $copy[0], "|"; unset($copy);
' : '';
eval($source); unset($source);
"#;
    let (out, assembly) = compile_and_run_with_heap_debug_and_asm(source);
    let expected = "old|a|old|p|old|s|old+c|old+c+c|b1|a1|b2|h2|b3|s3|x1|non-escaping|shared|before:copied|copied|";
    preserve_stdout_mismatches(source, &assembly, std::slice::from_ref(&out), &[expected]);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert!(!out.stderr.contains("Warning:"), "{}", out.stderr);
}
