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
    assert_eq!(out.stdout, "Unsupported unset(): opaque array property storage has no attached removal path|[1,2]");
}

/// Nullable holders, runtime holder classes and typed dynamic XML names keep handle mutations.
#[test]
fn test_simplexml_opaque_unset_holders_bundle() {
    if skip_without_xml_native("test_simplexml_opaque_unset_holders_bundle") { return; }
    let source = r###"<?php
interface OpaqueUnsetHolderMarker {}
class OpaqueUnsetPage implements OpaqueUnsetHolderMarker {
    public SimpleXMLElement $xml;
    public function __construct(SimpleXMLElement $xml) { $this->xml = $xml; }
}
class OpaqueUnsetUninitialized { public SimpleXMLElement $absent; }
class OpaqueUnsetBox {
    public ArrayObject $bag;
    public function __construct() { $this->bag = new ArrayObject(['k' => 1, 'j' => 2]); }
}
function OpaqueUnsetPageDrop(?OpaqueUnsetPage $page): void { unset($page->xml['id']); }
function OpaqueUnsetBoxDrop(?OpaqueUnsetBox $box): void { unset($box->bag['k']); }
function OpaqueUnsetHolderDrop(object $holder): void { unset($holder->xml['id']); }
function OpaqueUnsetMarkerDrop(OpaqueUnsetHolderMarker $holder): void { unset($holder->xml['id']); }
function OpaqueUnsetMissing(object $holder): void { unset($holder->absent['id']); }
class OpaqueUnsetGetter {
    public SimpleXMLElement $xml;
    public function __construct(SimpleXMLElement $xml) { $this->xml = $xml; }
    public function __isset(string $name): bool { echo 'bad-isset|'; return false; }
    public function __get(string $name): mixed { echo 'get|'; return $this->xml; }
}
class OpaqueUnsetDerived extends SimpleXMLElement {
    public function removeNamed(string $name): void { unset($this->{$name}[0]); }
}
$page = new OpaqueUnsetPage(new SimpleXMLElement('<p id="1" keep="2"/>'));
OpaqueUnsetPageDrop($page); OpaqueUnsetPageDrop(null); echo $page->xml->asXML();
$box = new OpaqueUnsetBox(); OpaqueUnsetBoxDrop($box); OpaqueUnsetBoxDrop(null); echo json_encode($box->bag->getArrayCopy()), '|';
$holder = new OpaqueUnsetPage(new SimpleXMLElement('<o id="1" keep="2"/>'));
OpaqueUnsetHolderDrop($holder); echo $holder->xml->asXML();
$marker = new OpaqueUnsetPage(new SimpleXMLElement('<i id="1" keep="2"/>'));
OpaqueUnsetMarkerDrop($marker); echo $marker->xml->asXML();
$missing = new stdClass(); OpaqueUnsetMissing($missing);
echo property_exists($missing, 'absent') ? 'created|' : 'missing|';
$missing->absent = null; OpaqueUnsetMissing($missing); echo 'null|';
$uninitialized = new OpaqueUnsetUninitialized(); OpaqueUnsetMissing($uninitialized); echo 'uninitialized|';
$getter = new OpaqueUnsetGetter(new SimpleXMLElement('<g id="1" keep="2"/>'));
OpaqueUnsetMissing($getter); echo $getter->xml->asXML();
$typed = new SimpleXMLElement('<t><a>1</a><a>2</a></t>'); $name = 'a';
unset($typed->{$name}[0]); echo $typed->asXML();
$derived = new OpaqueUnsetDerived('<d><a>1</a><a>2</a></d>');
$derived->removeNamed($name); echo $derived->asXML();
$chain = new SimpleXMLElement('<c><child><a>1</a><a>2</a></child></c>');
unset($chain->child->{$name}[0]); echo $chain->asXML();
unset($chain->missing->{$name}[0]); echo $chain->asXML();
unset($page, $box, $holder, $marker, $missing, $uninitialized, $getter, $typed, $derived, $chain, $name);
"###;
    let renamed = source.replace("OpaqueUnset", "EvalOpaqueUnset");
    let body = renamed.strip_prefix("<?php").unwrap().replace('\\', "\\\\").replace('\'', "\\'");
    let eval = format!("<?php $source = $argc > 0 ? '{body}' : ''; eval($source);");
    let expected = "<?xml version=\"1.0\"?>\n<p keep=\"2\"/>\n{\"j\":2}|<?xml version=\"1.0\"?>\n<o keep=\"2\"/>\n<?xml version=\"1.0\"?>\n<i keep=\"2\"/>\ncreated|null|uninitialized|get|<?xml version=\"1.0\"?>\n<g keep=\"2\"/>\n<?xml version=\"1.0\"?>\n<t><a>2</a></t>\n<?xml version=\"1.0\"?>\n<d><a>2</a></d>\n<?xml version=\"1.0\"?>\n<c><child><a>2</a></child></c>\n<?xml version=\"1.0\"?>\n<c><child><a>2</a></child><missing/></c>\n";
    assert_bundle_with(&[
        BundleCase { name: "opaque_holders_aot", source, expected },
        BundleCase { name: "opaque_holders_eval", source: &eval, expected },
    ], |source| {
        let out = compile_and_run_with_heap_debug(source);
        assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
        assert!(!out.stderr.contains("Warning:") && !out.stderr.contains("Notice:"), "{}", out.stderr);
        out.stdout
    });
}

/// Unresolved reference-returning calls fail explicitly while object handles remain mutable.
#[test]
fn test_simplexml_opaque_unset_reference_calls() {
    if skip_without_xml_native("test_simplexml_opaque_unset_reference_calls") { return; }
    let out = compile_and_run_with_heap_debug(r###"<?php
class OpaqueUnsetRefBag {
    public mixed $data = ['a' => 1, 'b' => 2];
    public function &bag(): mixed { return $this->data; }
}
class OpaqueUnsetOtherBag { public function bag(int $unused): mixed { return []; } }
function OpaqueUnsetAmbiguous(mixed $holder): void { unset($holder->bag()['a']); }
class OpaqueUnsetParentBag { public function bag(): mixed { return []; } }
class OpaqueUnsetChildBag extends OpaqueUnsetParentBag {
    public mixed $data = ['a' => 1, 'b' => 2];
    public function &bag(): mixed { return $this->data; }
}
function OpaqueUnsetOverride(OpaqueUnsetParentBag $holder): void { unset($holder->bag()['a']); }
function OpaqueUnsetByValue(): mixed { return ['a' => 1, 'b' => 2]; }
$xml = new SimpleXMLElement('<r a="1" b="2"/>');
$data = ['a' => 1, 'b' => 2];
$ref = function &() use (&$data): mixed { return $data; };
try { unset($ref()['a']); } catch (Error $error) { echo $error->getMessage(), '|'; }
echo json_encode($data), '|';
$bag = new OpaqueUnsetRefBag();
try { OpaqueUnsetAmbiguous($bag); } catch (Error $error) { echo $error->getMessage(), '|'; }
echo json_encode($bag->data), '|';
$child = new OpaqueUnsetChildBag();
try { OpaqueUnsetOverride($child); } catch (Error $error) { echo $error->getMessage(), '|'; }
echo json_encode($child->data), '|';
unset(OpaqueUnsetByValue()['a']); echo 'value|';
$handle = function () use ($xml): mixed { return $xml; };
unset($handle()['a']); echo $xml->asXML();
unset($handle, $ref, $data, $bag, $child, $xml, $error);
"###);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert!(!out.stderr.contains("Warning:") && !out.stderr.contains("Notice:"), "{}", out.stderr);
    let message = "Unsupported unset(): array result of an unresolved or overridable call has no attached removal path";
    assert_eq!(out.stdout, format!("{message}|{{\"a\":1,\"b\":2}}|{message}|{{\"a\":1,\"b\":2}}|{message}|{{\"a\":1,\"b\":2}}|value|<?xml version=\"1.0\"?>\n<r b=\"2\"/>\n"));
}
