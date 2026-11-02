//! Purpose:
//! Preserves temporary SimpleXML selections passed to reference parameters.
//!
//! Called from:
//! - Managed-libxml2 executable CI through the SimpleXML test module.
//!
//! Key details:
//! - Replacing a reference selection leaves XML intact while mutations through its view stay live.
//! - Native and opaque eval programs check properties, dimensions, casts, escaping and destruction.

use crate::support::*;

/// Reference arguments keep selected views without replaying a parent property store.
#[test]
fn test_simplexml_reference_argument_selections_bundle() {
    if skip_without_xml_native("test_simplexml_reference_argument_selections_bundle") { return; }
    let body = r#"
class ReferenceSelectionXml extends SimpleXMLElement {}
function replaceReferenceSelection(&$value): void { $value = "new"; }
function escapeReferenceSelection(&$value): array { return [&$value]; }
function touchReferenceSelection(&$value): void { $value["id"] = "kept"; }
function throwReferenceSelection(&$value): void { $value = "throw"; throw new RuntimeException("stop"); }
function exerciseReferenceSelection(SimpleXMLElement $root): void {
    replaceReferenceSelection($root->p); echo (string)$root->p, "|";
    $references = escapeReferenceSelection($root->p);
    $root->p = "changed"; echo (string)$references[0], "|";
    $references[0] = "detached"; echo (string)$root->p, "|";
    replaceReferenceSelection($root->p[0]); echo (string)$root->p, "|";
    replaceReferenceSelection($root->a->b); echo (string)$root->a->b, "|";
    $name = "p"; replaceReferenceSelection($root->{$name}); echo (string)$root->p, "|";
    echo (int)settype($root->q, "integer"), ":", (string)$root->q, "|";
    echo (int)settype(type: "integer", var: $root->a->b), ":", (string)$root->a->b, "|";
    touchReferenceSelection($root->p); echo (string)$root->p["id"], "|";
    try { throwReferenceSelection($root->p); }
    catch (RuntimeException $error) { echo $error->getMessage(), ":", (string)$root->p, "|"; unset($error); }
    unset($references);
}
$base = new SimpleXMLElement("<r><p>old</p><q>4</q><a><b>8</b></a></r>");
exerciseReferenceSelection($base); unset($base);
$subclass = new ReferenceSelectionXml("<r><p>old</p><q>4</q><a><b>8</b></a></r>");
exerciseReferenceSelection($subclass); unset($subclass);
class ReferenceSelectionDestructorXml extends SimpleXMLElement {
    public function __destruct() { echo "drop:", $this->getName(), "|"; }
}
function makeReferenceSelection(): ReferenceSelectionDestructorXml {
    return new ReferenceSelectionDestructorXml("<r><p>old</p></r>");
}
function consumeReferenceSelection(&$value): void { echo "call|"; $value = "new"; echo "tail|"; }
consumeReferenceSelection(makeReferenceSelection()->p);
"#;
    let native = format!("<?php {body}");
    let encoded = body.replace('\\', "\\\\").replace('\'', "\\'");
    let eval = format!("<?php $capability = new SimpleXMLElement('<r/>'); unset($capability); $source = $argc > 0 ? '{encoded}' : ''; eval($source); unset($source);");
    let row = "old|changed|changed|changed|8|changed|1:4|1:8|kept|stop:changed|";
    let expected = row.repeat(2) + "drop:r|call|drop:p|tail|";
    for (mode, source) in [native, eval].iter().enumerate() {
        let (out, assembly) = compile_and_run_with_heap_debug_and_asm(source);
        preserve_stdout_mismatches(source, &assembly, std::slice::from_ref(&out), &[&expected]);
        assert!(out.success, "mode={mode}: stdout={} stderr={}", out.stdout, out.stderr);
        assert_eq!(out.stdout, expected, "mode={mode}");
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "mode={mode}: {}", out.stderr);
        assert!(!out.stderr.contains("Warning:"), "mode={mode}: {}", out.stderr);
    }
}
