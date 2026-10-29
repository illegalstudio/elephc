//! Purpose:
//! Preserves XML property-dimension removal through loader unions and erased base objects.
//!
//! Called from:
//! - Managed XML codegen CI for compiled PHP and opaque eval.
//!
//! Key details:
//! - Quiet false/null bases and deferred name conversion retain PHP evaluation order.
//! - Unsupported ordinary opaque storage raises an explicit Error instead of mutating a copy.

use crate::support::*;

/// Loader results, XPath values and opaque parameters retain native removal and owner lifetimes.
#[test]
fn test_simplexml_opaque_property_unset_bundle() {
    if skip_without_xml_native("test_simplexml_opaque_property_unset_bundle") { return; }
    let source = r###"<?php
function OpaqueUnsetMixed(mixed $value): void { unset($value->a[0]); }
function OpaqueUnsetObject(object $value): void { unset($value->a[0]); }
function OpaqueUnsetKey(): int { echo 'key|'; return 0; }
class OpaqueUnsetName {
    public function __toString(): string {
        global $computed;
        echo 'convert|'; $computed = simplexml_load_string('<new><a>new</a></new>'); return 'a';
    }
}
class OpaqueUnsetQuietName { public function __toString(): string { echo 'bad-conversion|'; return 'a'; } }
function OpaqueUnsetBase(): mixed { global $computed; echo 'base|'; return $computed; }
function OpaqueUnsetGetName(): object { echo 'name-value|'; return new OpaqueUnsetName(); }
function OpaqueUnsetQuiet(mixed $value): void {
    $name = new OpaqueUnsetQuietName();
    unset($value->a[OpaqueUnsetKey()]); unset($value->{$name}[OpaqueUnsetKey()]); echo 'quiet|';
}
function OpaqueUnsetSwap(): int {
    global $swap; echo 'swap|'; $swap = simplexml_load_string('<new><a>1</a><a>2</a></new>'); return 0;
}
$xml = simplexml_load_string('<r><a>1</a><a>2</a></r>');
unset($xml->a[0]); echo $xml->asXML();
$file = simplexml_load_file('data://text/plain;base64,' . base64_encode('<f><a>1</a><a>2</a></f>'));
unset($file->a[0]); echo $file->asXML();
foreach ($xml->xpath('/r') as $node) { unset($node->a[0]); }
echo $xml->asXML();
$mixed = new SimpleXMLElement('<m><a>1</a><a>2</a></m>');
OpaqueUnsetMixed($mixed); echo $mixed->asXML();
$object = new SimpleXMLElement('<o><a>1</a><a>2</a></o>');
OpaqueUnsetObject($object); echo $object->asXML();
$dynamic = simplexml_load_string('<d><a>1</a><a>2</a></d>'); $name = 'a';
unset($dynamic->{$name}[OpaqueUnsetKey()]); echo $dynamic->asXML();
$computed = simplexml_load_string('<old><a>old</a></old>'); $saved = $computed;
unset(OpaqueUnsetBase()->{OpaqueUnsetGetName()}[OpaqueUnsetKey()]);
echo $saved->asXML(), $computed->asXML();
$swap = simplexml_load_string('<old><a>1</a><a>2</a></old>'); $before = $swap;
unset($swap->a[OpaqueUnsetSwap()]); echo $before->asXML(), $swap->asXML();
OpaqueUnsetQuiet(false); OpaqueUnsetQuiet(null);
unset($xml, $file, $node, $mixed, $object, $dynamic, $computed, $saved, $swap, $before, $name);
"###;
    let renamed = source.replace("OpaqueUnset", "EvalOpaqueUnset");
    let body = renamed.strip_prefix("<?php").unwrap().replace('\\', "\\\\").replace('\'', "\\'");
    let eval = format!("<?php $source = $argc > 0 ? '{body}' : ''; eval($source);");
    let expected = "<?xml version=\"1.0\"?>\n<r><a>2</a></r>\n<?xml version=\"1.0\"?>\n<f><a>2</a></f>\n<?xml version=\"1.0\"?>\n<r/>\n<?xml version=\"1.0\"?>\n<m><a>2</a></m>\n<?xml version=\"1.0\"?>\n<o><a>2</a></o>\nkey|<?xml version=\"1.0\"?>\n<d><a>2</a></d>\nbase|name-value|key|convert|<?xml version=\"1.0\"?>\n<old/>\n<?xml version=\"1.0\"?>\n<new><a>new</a></new>\nswap|<?xml version=\"1.0\"?>\n<old><a>1</a><a>2</a></old>\n<?xml version=\"1.0\"?>\n<new><a>2</a></new>\nkey|key|quiet|key|key|quiet|";
    assert_bundle_with(&[
        BundleCase { name: "opaque_property_unset_aot", source, expected },
        BundleCase { name: "opaque_property_unset_eval", source: &eval, expected },
    ], |source| {
        let out = compile_and_run_with_heap_debug(source);
        assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
        assert!(!out.stderr.contains("Warning:") && !out.stderr.contains("Notice:"), "{}", out.stderr);
        out.stdout
    });
}

/// Unsupported ordinary opaque storage reports an Error without silently changing a temporary copy.
#[test]
fn test_simplexml_opaque_ordinary_property_unset_error() {
    if skip_without_xml_native("test_simplexml_opaque_ordinary_property_unset_error") { return; }
    let out = compile_and_run_with_heap_debug(r#"<?php
function removeOpaqueOrdinaryProperty(mixed $value): void { unset($value->a[0]); }
$xml = new SimpleXMLElement('<r/>');
$ordinary = new stdClass(); $ordinary->a = [1, 2];
try { removeOpaqueOrdinaryProperty($ordinary); }
catch (Error $error) { echo $error->getMessage(), '|'; }
echo json_encode($ordinary->a);
unset($ordinary, $xml, $error);
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert!(!out.stderr.contains("Warning:") && !out.stderr.contains("Notice:"), "{}", out.stderr);
    assert_eq!(out.stdout, "Unsupported unset(): opaque ordinary object property storage has no attached removal path|[1,2]");
}
