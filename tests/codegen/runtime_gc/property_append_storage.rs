//! Purpose:
//! Exercises attached property append storage through erased receiver types.
//!
//! Called from:
//! - Executable runtime GC tests on CI.
//!
//! Key details:
//! - Large loops expose a full array copy on every append through their normal timeout.
//! - Snapshot and clone assertions require COW while aliases must retain write-through.

use crate::support::*;

/// Mixed, object and interface appends remain linear and preserve independent snapshots.
#[test]
fn test_erased_property_append_storage_bundle() { verify(false); }

/// XML runtime dispatch preserves the attached ordinary-property append path.
#[test]
fn test_erased_property_xml_append_storage_bundle() {
    if skip_without_xml_native("test_erased_property_xml_append_storage_bundle") { return; }
    verify(true);
}

/// Covers declared arrays, ordinary dynamic hashes, object clones and managed reference entries.
fn verify(xml: bool) {
    let source = r#"<?php
interface AppendStorageMarker {}
class AppendStorageDeclared implements AppendStorageMarker { public array $items = []; }
#[AllowDynamicProperties]
class AppendStorageDynamic implements AppendStorageMarker {}
function appendStorageMixed(mixed $box, int $n): void {
    for ($i = 0; $i < $n; $i++) { $box->items[] = $i; }
}
function appendStorageObject(object $box, int $n): void {
    for ($i = 0; $i < $n; $i++) { $box->items[] = $i; }
}
function appendStorageInterface(AppendStorageMarker $box, int $n): void {
    for ($i = 0; $i < $n; $i++) { $box->items[] = $i; }
}
$a = new AppendStorageDeclared(); appendStorageMixed($a, 200000);
echo count($a->items), ':', $a->items[199999], '|'; unset($a);
$b = new AppendStorageDeclared(); appendStorageObject($b, 200000);
echo count($b->items), ':', $b->items[199999], '|'; unset($b);
$c = new AppendStorageDeclared(); appendStorageInterface($c, 200000);
echo count($c->items), ':', $c->items[199999], '|'; unset($c);
$d = new stdClass(); $d->items = []; appendStorageMixed($d, 200000);
echo count($d->items), ':', $d->items[199999], '|'; unset($d);
$e = new AppendStorageDynamic(); $e->items = []; appendStorageInterface($e, 200000);
echo count($e->items), ':', $e->items[199999], '|'; unset($e);
$node = new stdClass(); $node->items = ['a'];
$snapshot = $node->items; $clone = clone $node; $alias = $node;
appendStorageMixed($node, 1);
echo implode(',', $snapshot), ':', implode(',', $clone->items), ':', implode(',', $alias->items), '|';
$reference =& $node->items;
appendStorageObject($node, 1);
echo implode(',', $reference), ':', implode(',', $clone->items), '|';
$missing = new stdClass(); appendStorageMixed($missing, 2);
echo implode(',', $missing->items), '|';
$dynamic = new AppendStorageDynamic(); $dynamic->items = ['b'];
$dynamicSnapshot = $dynamic->items; $dynamicClone = clone $dynamic;
appendStorageInterface($dynamic, 1);
echo implode(',', $dynamicSnapshot), ':', implode(',', $dynamicClone->items), ':', implode(',', $dynamic->items), '|';
class AppendStorageDeprecated {}
$plain = new AppendStorageDeprecated(); $reserved = clone $plain;
set_error_handler(function(int $level, string $message): bool { throw new Exception('blocked'); });
try { appendStorageMixed($plain, 1); }
catch (Throwable $error) { echo $error->getMessage(), ':'; }
restore_error_handler();
echo isset($plain->items) ? 'changed|' : 'empty|';
"#;
    let capability = if xml { "function appendStorageXmlCapability(): void { simplexml_load_string('<r/>'); }" } else { "" };
    let source = format!("{source}\n{capability}");
    let out = compile_and_run_with_heap_debug(&source);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, format!("{}a:a:a,0|a,0,0:a|0,1|b:b:b,0|blocked:empty|", "200000:199999|".repeat(5)));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
