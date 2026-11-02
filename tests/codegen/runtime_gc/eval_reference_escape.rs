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
' : '';
eval($source); unset($source);
"#;
    let (out, assembly) = compile_and_run_with_heap_debug_and_asm(source);
    let expected = "old|a|old|p|old|s|old+c|old+c+c|b1|a1|b2|h2|b3|s3|x1|non-escaping|shared|";
    preserve_stdout_mismatches(source, &assembly, std::slice::from_ref(&out), &[expected]);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert!(!out.stderr.contains("Warning:"), "{}", out.stderr);
}
