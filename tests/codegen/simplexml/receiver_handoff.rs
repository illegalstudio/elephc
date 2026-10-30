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
class XmlHandoffHolder {
    public SimpleXMLElement $items;
    public function __construct() { $this->items = (new SimpleXMLElement('<r><item>1</item></r>'))->item; }
    public function __destruct() { echo 'holder:', $this->items->asXML(), '|'; }
}
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
XmlHandoffHeld()->items[] = 2; echo 'held|';
echo 'done|';
"#;
    let expected = "destroy:<?xml version=\"1.0\"?><r><item>1</item></r>|destroy:<item>1</item>|known|destroy:<?xml version=\"1.0\"?><r><item>1</item></r>|destroy:<item>1</item>|mixed|destroy:<?xml version=\"1.0\"?><r><item>1</item></r>|destroy:<item>3</item>|object|destroy:<?xml version=\"1.0\"?><r><item>1</item></r>|destroy:<item>3</item>|destroy:<item>3</item>|interface|throw-destroy:<?xml version=\"1.0\"?><r><item>1</item></r>|throw-destroy:<item>1</item>|caught:parent|holder:<item>1</item>|held|done|";
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
