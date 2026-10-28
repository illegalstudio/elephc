//! Purpose:
//! Preserves shared native arithmetic and non-null object boundaries for XML selections.
//!
//! Called from:
//! - The SimpleXML codegen CI suite with managed libxml2.
//!
//! Key details:
//! - AOT and opaque eval use independent declarations in one heap-checked bundle.
//! - Expected TypeError text excludes only the PHP oracle's variable callsite paths.

use crate::support::*;

const ARITHMETIC: &str = include_str!("../../fixtures/simplexml/iterator_arithmetic.php");
const BOUNDARIES: &str = include_str!("../../fixtures/simplexml/typed_object_boundaries.php");
const ARITHMETIC_EXPECTED: &str = "int(5)\nint(6)\nint(-1)\nfloat(1.5)\nint(1)\nint(9)\nint(7)\nint(7)\nint(7)\nint(-2)\nint(-5)\nint(3)\nint(6)\n";
const BOUNDARIES_EXPECTED: &str = "acceptBoundaryXml(): Argument #1 ($node) must be of type SimpleXMLElement, null given\nacceptSpecificBoundaryXml(): Argument #1 ($node) must be of type BoundaryXmlElement, null given\nacceptObjectBoundaryXml(): Argument #1 ($node) must be of type object, null given\nacceptPeerBoundaryXml(): Argument #1 ($node) must be of type BoundaryXmlPeer, null given\nacceptUnionBoundaryXml(): Argument #1 ($node) must be of type SimpleXMLElement|SimpleXMLIterator, null given\nacceptBoundaryXml(): Argument #1 ($node) must be of type SimpleXMLElement, null given\nacceptBoundaryXml(): Argument #1 ($node) must be of type SimpleXMLElement, null given\nacceptRefBoundaryXml(): Argument #1 ($node) must be of type SimpleXMLElement, null given\nreturnBoundaryXml(): Return value must be of type SimpleXMLElement, null returned\nreturnUnionBoundaryXml(): Return value must be of type SimpleXMLElement|SimpleXMLIterator, null returned\nreferenceReturnBoundaryXml(): Return value must be of type SimpleXMLElement, null returned\nCannot assign null to property BoundaryXmlBox::$node of type SimpleXMLElement\nCannot assign null to property BoundaryXmlBox::$node of type SimpleXMLElement\npresent\nCannot assign null to property BoundaryXmlBox::$union of type SimpleXMLElement|int\nint(1)\nCannot assign null to property BoundaryXmlBox::$staticNode of type SimpleXMLElement\npresent\nCannot assign null to property BoundaryXmlStaticChild::$node of type SimpleXMLElement\npresent\nBoundaryXmlConstructor::__construct(): Argument #1 ($node) must be of type SimpleXMLElement, null given\nBoundaryXmlMethods::accept(): Argument #1 ($node) must be of type SimpleXMLElement, null given\nBoundaryXmlMethods::acceptStatic(): Argument #1 ($node) must be of type SimpleXMLElement, null given\nacceptBoundaryXml(): Argument #1 ($node) must be of type SimpleXMLElement, null given\nacceptBoundaryXml(): Argument #1 ($node) must be of type SimpleXMLElement, null given\nArgument #1 ($node) must be of type SimpleXMLElement, null given\nBoundaryXmlHook::$node::set(): Argument #1 ($value) must be of type SimpleXMLElement, null given\nNULL\nNULL\npresent\nentered|reference|peer|nullable|done\n";

/// Makes the renamed fixture reach Magician with runtime-unknown source, without extra compiles.
fn opaque_fixture(source: &str) -> String {
    let body = source.strip_prefix("<?php").unwrap().replace('\\', "\\\\").replace('\'', "\\'");
    format!("<?php $source = $argc > 0 ? '{body}' : ''; eval($source);")
}

/// Both native class lineages and nullable reads preserve PHP results and strict heap cleanup.
#[test]
fn test_simplexml_review_type_boundaries_bundle() {
    if skip_without_xml_native("test_simplexml_review_type_boundaries_bundle") { return; }
    let arithmetic_eval = opaque_fixture(&ARITHMETIC.replace("LineageXml", "EvalLineageXml"));
    let boundaries_eval = opaque_fixture(&BOUNDARIES.replace("BoundaryXml", "EvalBoundaryXml"));
    let boundaries_eval_expected = BOUNDARIES_EXPECTED.replace("BoundaryXml", "EvalBoundaryXml");
    assert_bundle_with(&[
        BundleCase { name: "lineage_aot", source: ARITHMETIC, expected: ARITHMETIC_EXPECTED },
        BundleCase { name: "lineage_eval", source: &arithmetic_eval, expected: ARITHMETIC_EXPECTED },
        BundleCase { name: "boundaries_aot", source: BOUNDARIES, expected: BOUNDARIES_EXPECTED },
        BundleCase { name: "boundaries_eval", source: &boundaries_eval, expected: &boundaries_eval_expected },
    ], |source| {
        let out = compile_and_run_with_heap_debug(source);
        assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
        assert!(!out.stderr.contains("Warning:"), "{}", out.stderr);
        out.stdout
    });
}
