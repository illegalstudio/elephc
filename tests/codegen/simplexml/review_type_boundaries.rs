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
const REFERENCES: &str = include_str!("../../fixtures/simplexml/reference_object_boundaries.php");
const GENERATORS: &str = include_str!("../../fixtures/simplexml/generator_object_boundaries.php");
const REFERENCES_EXPECTED: &str = "referenceReturnBoundaryXml(): Return value must be of type SimpleXMLElement, null returned\npresent\n";
const GENERATORS_EXPECTED: &str = "generatorBoundaryXml(): Argument #1 ($node) must be of type SimpleXMLElement, null given\nvariadicGeneratorBoundaryXml(): Argument #2 must be of type SimpleXMLElement, null given\niterableGeneratorBoundaryXml(): Argument #1 ($nodes) must be of type Traversable|array, null given\nBoundaryXmlGeneratorMethods::generate(): Argument #1 ($node) must be of type SimpleXMLElement, null given\nBoundaryXmlGeneratorElement::relativeGenerator(): Argument #1 ($node) must be of type BoundaryXmlGeneratorElement, null given\nArgument #1 ($node) must be of type SimpleXMLElement, null given\n1\ngenerator_created|generator_body|1|generator_done\n";
const ARITHMETIC_EXPECTED: &str = "int(5)\nint(6)\nint(-1)\nfloat(1.5)\nint(1)\nint(9)\nint(7)\nint(7)\nint(7)\nint(-2)\nint(-5)\nint(3)\nint(6)\n";
const BOUNDARIES_EXPECTED: &str = "acceptBoundaryXml(): Argument #1 ($node) must be of type SimpleXMLElement, null given\nacceptSpecificBoundaryXml(): Argument #1 ($node) must be of type BoundaryXmlElement, null given\nacceptObjectBoundaryXml(): Argument #1 ($node) must be of type object, null given\nacceptPeerBoundaryXml(): Argument #1 ($node) must be of type BoundaryXmlPeer, null given\nacceptUnionBoundaryXml(): Argument #1 ($node) must be of type SimpleXMLElement|SimpleXMLIterator, null given\nacceptBoundaryXml(): Argument #1 ($node) must be of type SimpleXMLElement, null given\nacceptBoundaryXml(): Argument #1 ($node) must be of type SimpleXMLElement, null given\nacceptRefBoundaryXml(): Argument #1 ($node) must be of type SimpleXMLElement, null given\nacceptIterableBoundaryXml(): Argument #1 ($nodes) must be of type Traversable|array, null given\nvariadicBoundaryXml(): Argument #3 must be of type SimpleXMLElement, null given\nvariadicBoundaryXml(): Argument #3 must be of type SimpleXMLElement, null given\nvariadicBoundaryXml(): Argument #2 must be of type SimpleXMLElement, null given\nreferenceVariadicBoundaryXml(): Argument #1 must be of type SimpleXMLElement, null given\norderedVariadicBoundaryXml(): Argument #1 ($first) must be of type SimpleXMLElement, null given\norderedVariadicBoundaryXml(): Argument #1 ($first) must be of type SimpleXMLElement, null given\norderedVariadicBoundaryXml(): Argument #1 ($first) must be of type SimpleXMLElement, null given\nreturnBoundaryXml(): Return value must be of type SimpleXMLElement, null returned\nreturnUnionBoundaryXml(): Return value must be of type SimpleXMLElement|SimpleXMLIterator, null returned\nreturnIterableBoundaryXml(): Return value must be of type Traversable|array, null returned\naliasedReturnBoundaryXml(): Return value must be of type SimpleXMLElement, null returned\nCannot assign null to property BoundaryXmlBox::$node of type SimpleXMLElement\nCannot assign null to property BoundaryXmlBox::$node of type SimpleXMLElement\nCannot assign null to property BoundaryXmlBox::$node of type SimpleXMLElement\nCannot assign null to property BoundaryXmlBox::$node of type SimpleXMLElement\npresent\nCannot assign null to property BoundaryXmlBox::$union of type SimpleXMLElement|int\nint(1)\nCannot assign null to property BoundaryXmlBox::$items of type Traversable|array\nCannot assign null to property BoundaryXmlBox::$staticItems of type Traversable|array\nNULL\nCannot assign null to property BoundaryXmlBox::$staticNode of type SimpleXMLElement\npresent\nCannot assign null to property BoundaryXmlStaticChild::$node of type SimpleXMLElement\npresent\n1:Cannot assign null to property BoundaryXmlBox::$staticNode of type SimpleXMLElement\n1:Cannot assign null to property BoundaryXmlBox::$staticNode of type SimpleXMLElement\nBoundaryXmlReturnBase::missingStatic(): Return value must be of type BoundaryXmlReturnChild, null returned\nBoundaryXmlReturnBase::missingSelf(): Return value must be of type BoundaryXmlReturnBase, null returned\nBoundaryXmlReturnBase::missingStaticCall(): Return value must be of type BoundaryXmlReturnChild, null returned\nBoundaryXmlConstructor::__construct(): Argument #1 ($node) must be of type SimpleXMLElement, null given\nBoundaryXmlMethods::__propset_node(): Argument #1 ($node) must be of type SimpleXMLElement, null given\nBoundaryXmlMethods::__propget_node(): Return value must be of type SimpleXMLElement, null returned\nBoundaryXmlMethods::accept(): Argument #1 ($node) must be of type SimpleXMLElement, null given\nBoundaryXmlMethods::acceptStatic(): Argument #1 ($node) must be of type SimpleXMLElement, null given\nBoundaryXmlElement::relativeVariadic(): Argument #1 must be of type BoundaryXmlElement, null given\nacceptBoundaryXml(): Argument #1 ($node) must be of type SimpleXMLElement, null given\nacceptBoundaryXml(): Argument #1 ($node) must be of type SimpleXMLElement, null given\nArgument #1 ($node) must be of type SimpleXMLElement, null given\nBoundaryXmlHook::$node::set(): Argument #1 ($value) must be of type SimpleXMLElement, null given\nNULL\nNULL\npresent\nentered|reference|peer|nullable|iterable|variadic|reference_variadic|done\n";

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
    // Magician rejects reference-return declarations and yield; compiled PHP covers both.
    let boundaries_aot = format!("{BOUNDARIES}\n{}\n{}",
        REFERENCES.strip_prefix("<?php").unwrap(), GENERATORS.strip_prefix("<?php").unwrap());
    let boundaries_aot_expected = format!("{BOUNDARIES_EXPECTED}{REFERENCES_EXPECTED}{GENERATORS_EXPECTED}");
    assert_bundle_with(&[
        BundleCase { name: "lineage_aot", source: ARITHMETIC, expected: ARITHMETIC_EXPECTED },
        BundleCase { name: "lineage_eval", source: &arithmetic_eval, expected: ARITHMETIC_EXPECTED },
        BundleCase { name: "boundaries_aot", source: &boundaries_aot, expected: &boundaries_aot_expected },
        BundleCase { name: "boundaries_eval", source: &boundaries_eval, expected: &boundaries_eval_expected },
    ], |source| {
        let out = compile_and_run_with_heap_debug(source);
        assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
        assert!(!out.stderr.contains("Warning:"), "{}", out.stderr);
        out.stdout
    });
}
