//! Purpose:
//! Verifies native property write-fetch lifetime transfer from temporary XML parents.
//!
//! Called from:
//! - Managed-libxml2 codegen and strict heap CI on executable targets.
//!
//! Key details:
//! - Stock PHP destroys the parent before mutating an independently owned child view.
//! - Throwing parent destructors must unwind the child without running the pending mutation.

use crate::support::*;

/// Native callers retain eval subclass identity after returning from the declaring fragment.
#[test]
fn test_simplexml_eval_subclass_factories_after_eval_returns() {
    if skip_without_xml_native("test_simplexml_eval_subclass_factories_after_eval_returns") { return; }
    let out = compile_and_run_with_heap_debug(r#"<?php
function nativeEvalFactoryClassCheck(string $class, bool $allow): bool {
    return is_a($class, 'SimpleXMLElement', $allow);
}
$source = $argc > 0 ? 'class ReturnedEvalFactoryXml extends SimpleXMLElement {
    public static int $calls = 0;
    public function __construct(string $xml, int $marker) { self::$calls++; parent::__construct($xml); }
}' : '';
eval($source);
$class = 'ReturnedEvalFactoryXml';
$root = new $class('<r id="x"><item>1</item></r>', 7);
$child = $root->item;
$children = $root->children();
$attributes = $root->attributes();
$paths = $root->xpath('//item');
$loaded = simplexml_load_string('<loaded/>', $class);
$imported = simplexml_import_dom($root, $class);
echo (int)nativeEvalFactoryClassCheck($class, true), ':', (int)nativeEvalFactoryClassCheck($class, false), '|';
echo get_class($child), ':', $child->getName(), '|';
echo get_class($children), ':', count($children), '|';
echo get_class($attributes), ':', count($attributes), '|';
echo get_class($paths[0]), ':', $paths[0]->getName(), '|';
echo get_class($loaded), ':', $loaded->getName(), '|';
echo get_class($imported), ':', $imported->getName(), '|';
echo ReturnedEvalFactoryXml::$calls;
unset($root, $child, $children, $attributes, $paths, $loaded, $imported, $class, $source);
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "1:0|ReturnedEvalFactoryXml:item|ReturnedEvalFactoryXml:1|ReturnedEvalFactoryXml:1|ReturnedEvalFactoryXml:item|ReturnedEvalFactoryXml:loaded|ReturnedEvalFactoryXml:r|1");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Eval subclass selections and factories allocate without re-entering the user constructor.
#[test]
fn test_simplexml_eval_subclass_constructorless_factories() {
    if skip_without_xml_native("test_simplexml_eval_subclass_constructorless_factories") { return; }
    let body = r#"
class EvalConstructorlessFactoryXml extends SimpleXMLElement {
    public static int $calls = 0;
    public function __construct(string $xml, int $marker) {
        self::$calls++;
        parent::__construct($xml);
    }
}
$root = new EvalConstructorlessFactoryXml('<r id="x"><item>1</item></r>', 7);
$child = $root->item;
$children = $root->children();
$attributes = $root->attributes();
$paths = $root->xpath('//item');
$loaded = simplexml_load_string('<loaded/>', EvalConstructorlessFactoryXml::class);
$imported = simplexml_import_dom($root, EvalConstructorlessFactoryXml::class);
echo get_class($child), ':', $child->getName(), '|';
echo get_class($children), ':', count($children), '|';
echo get_class($attributes), ':', count($attributes), '|';
echo get_class($paths[0]), ':', $paths[0]->getName(), '|';
echo get_class($loaded), ':', $loaded->getName(), '|';
echo get_class($imported), ':', $imported->getName(), '|';
echo EvalConstructorlessFactoryXml::$calls;
unset($root, $child, $children, $attributes, $paths, $loaded, $imported);
"#;
    let escaped = body.replace('\\', "\\\\").replace('\'', "\\'");
    let out = compile_and_run_with_heap_debug(&format!("<?php $source = $argc > 0 ? '{escaped}' : ''; eval($source); unset($source);"));
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "EvalConstructorlessFactoryXml:item|EvalConstructorlessFactoryXml:1|EvalConstructorlessFactoryXml:1|EvalConstructorlessFactoryXml:item|EvalConstructorlessFactoryXml:loaded|EvalConstructorlessFactoryXml:r|1");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Computed native and erased receivers retire their parent before each child dimension update.
#[test]
fn test_simplexml_temporary_parent_handoff_bundle() {
    if skip_without_xml_native("test_simplexml_temporary_parent_handoff_bundle") { return; }
    let source = r#"<?php
interface XmlHandoffMarker {}
class XmlHandoffNode extends SimpleXMLElement implements XmlHandoffMarker {
    public function __destruct() { echo 'destroy:', str_replace("\n", '', $this->asXML()), '|'; }
}
class XmlHandoffThrow extends SimpleXMLElement {
    public function __destruct() {
        echo 'throw-destroy:', str_replace("\n", '', $this->asXML()), '|';
        if ($this->getName() === 'r') { throw new Exception('parent'); }
    }
}
interface XmlHandoffHolderMarker {}
class XmlHandoffHolder implements XmlHandoffHolderMarker {
    public SimpleXMLElement $items;
    public function __construct() { $this->items = (new SimpleXMLElement('<r><item>1</item></r>'))->item; }
    public function __destruct() { echo 'holder:', count($this->items), ':', $this->items->asXML(), '|'; }
}
class XmlHandoffNullableHolder {
    public ?SimpleXMLElement $items;
    public function __construct() { $this->items = (new SimpleXMLElement('<r><item>1</item></r>'))->item; }
    public function __destruct() { echo 'nullable-holder:', count($this->items), ':', $this->items->asXML(), '|'; }
}
function XmlHandoffHeldNullable(): mixed { return new XmlHandoffNullableHolder(); }
class XmlHandoffHolderThrow extends XmlHandoffHolder {
    public function __destruct() {
        echo 'holder-throw:', count($this->items), ':', $this->items->asXML(), '|';
        throw new Exception('holder');
    }
}
function XmlHandoffHeldMixed(): mixed { return new XmlHandoffHolder(); }
function XmlHandoffHeldObject(): object { return new XmlHandoffHolder(); }
function XmlHandoffHeldInterface(): XmlHandoffHolderMarker { return new XmlHandoffHolder(); }
function XmlHandoffHeldThrowing(): mixed { return new XmlHandoffHolderThrow(); }
function XmlHandoffHeld(): XmlHandoffHolder { return new XmlHandoffHolder(); }
function XmlHandoffKnown(): XmlHandoffNode { return new XmlHandoffNode('<r><item>1</item></r>'); }
function XmlHandoffMixed(): mixed { return new XmlHandoffNode('<r><item>1</item></r>'); }
function XmlHandoffObject(): object { return new XmlHandoffNode('<r><item>1</item></r>'); }
function XmlHandoffInterface(): XmlHandoffMarker { return new XmlHandoffNode('<r><item>1</item></r>'); }
function XmlHandoffThrowing(): mixed { return new XmlHandoffThrow('<r><item>1</item></r>'); }
XmlHandoffKnown()->item[] = 2; echo 'known|';
XmlHandoffMixed()->item[] = 2; echo 'mixed|';
XmlHandoffObject()->item[0] = 3; echo 'object|';
XmlHandoffInterface()->item[0] += 2; echo 'interface|';
try { XmlHandoffThrowing()->item[] = 2; }
catch (Throwable $error) { echo 'caught:', $error->getMessage(), '|'; }
XmlHandoffHeld()->items[] = 2; echo 'held-known-append|';
XmlHandoffHeld()->items[0] = 3; echo 'held-known-index|';
XmlHandoffHeld()->items[0] .= 'x'; echo 'held-known-compound|';
XmlHandoffHeldMixed()->items[] = 2; echo 'held-mixed-append|';
XmlHandoffHeldMixed()->items[0] = 3; echo 'held-mixed-index|';
XmlHandoffHeldMixed()->items[0] .= 'x'; echo 'held-mixed-compound|';
XmlHandoffHeldObject()->items[] = 2; echo 'held-object-append|';
XmlHandoffHeldObject()->items[0] = 3; echo 'held-object-index|';
XmlHandoffHeldObject()->items[0] .= 'x'; echo 'held-object-compound|';
XmlHandoffHeldInterface()->items[] = 2; echo 'held-interface-append|';
XmlHandoffHeldInterface()->items[0] = 3; echo 'held-interface-index|';
XmlHandoffHeldInterface()->items[0] .= 'x'; echo 'held-interface-compound|';
try { XmlHandoffHeldThrowing()->items[] = 2; }
catch (Throwable $error) { echo 'caught:', $error->getMessage(), '|'; }
XmlHandoffHeldNullable()->items[] = 2; echo 'nullable-held|';
echo 'done|';
"#;
    let expected = "destroy:<?xml version=\"1.0\"?><r><item>1</item></r>|destroy:<item>1</item>|known|destroy:<?xml version=\"1.0\"?><r><item>1</item></r>|destroy:<item>1</item>|mixed|destroy:<?xml version=\"1.0\"?><r><item>1</item></r>|destroy:<item>3</item>|object|destroy:<?xml version=\"1.0\"?><r><item>1</item></r>|destroy:<item>3</item>|destroy:<item>3</item>|interface|throw-destroy:<?xml version=\"1.0\"?><r><item>1</item></r>|throw-destroy:<item>1</item>|caught:parent|holder:1:<item>1</item>|held-known-append|holder:1:<item>1</item>|held-known-index|holder:1:<item>1</item>|held-known-compound|holder:1:<item>1</item>|held-mixed-append|holder:1:<item>1</item>|held-mixed-index|holder:1:<item>1</item>|held-mixed-compound|holder:1:<item>1</item>|held-object-append|holder:1:<item>1</item>|held-object-index|holder:1:<item>1</item>|held-object-compound|holder:1:<item>1</item>|held-interface-append|holder:1:<item>1</item>|held-interface-index|holder:1:<item>1</item>|held-interface-compound|holder-throw:1:<item>1</item>|caught:holder|nullable-holder:1:<item>1</item>|nullable-held|done|";
    let renamed = source.replace("XmlHandoff", "EvalXmlHandoff");
    let body = renamed.strip_prefix("<?php").unwrap().replace('\\', "\\\\").replace('\'', "\\'");
    let eval = format!("<?php $source = $argc > 0 ? '{body}' : ''; eval($source);");
    assert_bundle_with(&[
        BundleCase { name: "temporary_xml_parent_native", source, expected },
        BundleCase { name: "temporary_xml_parent_eval", source: &eval, expected },
    ], |source| {
        let out = compile_and_run_with_heap_debug(source);
        assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
        out.stdout
    });
}
