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

/// Opaque receivers select collectors without changing eval overrides or reference arguments.
#[test]
fn test_simplexml_selected_variadic_method_signature_after_eval_bundle() {
    if skip_without_xml_native("test_simplexml_selected_variadic_method_signature_after_eval_bundle") { return; }
    let out = compile_and_run_with_heap_debug(r#"<?php
class SourceVariadicXml extends SimpleXMLElement {
    public function collect(self ...$nodes): void { echo 'collect:', count($nodes), '|'; }
    public function prefix(int $first, self ...$nodes): void { echo $first, ':', count($nodes), '|'; }
    public function change(mixed &...$values): void { $values[0] += 1; }
}
class SourceVariadicInherited extends SourceVariadicXml {}
function sourceVariadicMake(): mixed { return new SourceVariadicInherited('<r/>'); }
function sourceVariadicNullable(bool $present): ?SourceVariadicXml {
    return $present ? new SourceVariadicXml('<r/>') : null;
}
function sourceVariadicArgument(): SourceVariadicXml {
    echo 'unexpected|'; return new SourceVariadicXml('<r/>');
}
$node = sourceVariadicMake();
$present = $node;
$code = $argc > 0 ? '
class EvalVariadicOverride extends SourceVariadicXml {
    public function collect(?SourceVariadicXml ...$nodes): void { echo "eval-child:", count($nodes), "|"; }
}
class EvalVariadicFixed {
    public function collect(int $first, int $second = 2): void { echo "eval-fixed:", $first + $second, "|"; }
}
$evalChild = new EvalVariadicOverride("<r/>"); $other = new EvalVariadicFixed();
' : '';
eval($code);
$missing = $node->missing->child;
try { $node->collect($missing); }
catch (TypeError $error) { echo explode(', called in ', $error->getMessage())[0], '|'; }
$node->collect($present);
$node->collect();
$node->collect($present, $present);
$node->collect(first: $present, second: $present);
$node->collect(...[$present, $present]);
$node->prefix(7, $present, $present);
try { $node->prefix(); } catch (ArgumentCountError $error) {
    echo str_contains($error->getMessage(), 'exactly 1 expected') ? 'arity|' : 'wrong-arity|';
}
$number = 1; $node->change($number); echo $number, '|';
$evalChild->collect($missing);
$other->collect(1);
$nullable = sourceVariadicNullable($argc < 0);
$nullable?->collect(sourceVariadicArgument());
echo 'done';
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "SourceVariadicXml::collect(): Argument #1 must be of type SourceVariadicXml, null given|collect:1|collect:0|collect:2|collect:2|collect:2|7:2|arity|2|eval-child:1|eval-fixed:3|done");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert!(!out.stderr.contains("Warning:"), "{}", out.stderr);
}

/// Caller-side nullable guards retain closure and first-class function diagnostic names.
#[test]
fn test_simplexml_callable_argument_guards_keep_source_labels() {
    if skip_without_xml_native("test_simplexml_callable_argument_guards_keep_source_labels") { return; }
    let out = compile_and_run_with_heap_debug(r#"<?php
function callableLabelMaybe(bool $present): mixed {
    return $present ? new SimpleXMLElement('<r/>') : null;
}
function callableLabelFunction(SimpleXMLElement $node): void { echo 'unexpected|'; }
function callableLabelArguments(mixed $node): array { return [$node]; }
function callableLabelReport(TypeError $error): void {
    $message = explode(', called in ', $error->getMessage())[0];
    echo str_starts_with($message, '{closure:') ? 'closure|' : 'function|';
    echo substr($message, strpos($message, 'Argument #')), '|';
}
$missing = callableLabelMaybe($argc < 0);
$plain = function(SimpleXMLElement $node): void { echo 'unexpected|'; };
$generator = function(SimpleXMLElement $node): Generator { yield 1; };
$function = callableLabelFunction(...);
$arguments = callableLabelArguments($missing);
try { call_user_func_array($plain, $arguments); } catch (TypeError $error) { callableLabelReport($error); }
try { call_user_func_array($generator, $arguments); } catch (TypeError $error) { callableLabelReport($error); }
try { call_user_func_array($function, $arguments); } catch (TypeError $error) { callableLabelReport($error); }
foreach ($generator(new SimpleXMLElement('<r/>')) as $number) { echo $number; }
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    let argument = "Argument #1 ($node) must be of type SimpleXMLElement, null given|";
    assert_eq!(out.stdout, format!("closure|{argument}closure|{argument}function|{argument}1"));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

const ARITHMETIC: &str = include_str!("../../fixtures/simplexml/iterator_arithmetic.php");
const ARITHMETIC_EVAL: &str = include_str!("../../fixtures/simplexml/iterator_arithmetic_eval.php");
const BOUNDARIES: &str = include_str!("../../fixtures/simplexml/typed_object_boundaries.php");
const REFERENCES: &str = include_str!("../../fixtures/simplexml/reference_object_boundaries.php");
const GENERATORS: &str = include_str!("../../fixtures/simplexml/generator_object_boundaries.php");
const REFERENCES_EXPECTED: &str = "referenceReturnBoundaryXml(): Return value must be of type SimpleXMLElement, null returned\nthrowingFinallyReferenceBoundaryXml(): Return value must be of type SimpleXMLElement, null returned\npresent\nchanged\nalias\nUnsupported assignment through object reference $node: value must be of type SimpleXMLElement\n";
const GENERATORS_EXPECTED: &str = "generatorBoundaryXml(): Argument #1 ($node) must be of type SimpleXMLElement, null given\nvariadicGeneratorBoundaryXml(): Argument #2 must be of type SimpleXMLElement, null given\niterableGeneratorBoundaryXml(): Argument #1 ($nodes) must be of type Traversable|array, null given\nBoundaryXmlGeneratorMethods::generate(): Argument #1 ($node) must be of type SimpleXMLElement, null given\nBoundaryXmlGeneratorElement::relativeGenerator(): Argument #1 ($node) must be of type BoundaryXmlGeneratorElement, null given\nArgument #1 ($node) must be of type SimpleXMLElement, null given\n1\ngenerator_created|generator_body|1|generator_done\n";
const ARITHMETIC_EXPECTED: &str = "int(5)\nint(6)\nint(-1)\nfloat(1.5)\nint(1)\nint(9)\nint(7)\nint(7)\nint(7)\nint(-2)\nint(-5)\nint(3)\nint(6)\n";
const ARITHMETIC_EVAL_EXPECTED: &str = "int(5)\nint(6)\nint(-1)\nfloat(1.5)\nint(1)\nint(9)\nint(7)\nint(7)\nint(7)\nint(-2)\nint(-5)\n";
const BOUNDARIES_EXPECTED: &str = "acceptBoundaryXml(): Argument #1 ($node) must be of type SimpleXMLElement, null given\nacceptSpecificBoundaryXml(): Argument #1 ($node) must be of type BoundaryXmlElement, null given\nacceptObjectBoundaryXml(): Argument #1 ($node) must be of type object, null given\nacceptPeerBoundaryXml(): Argument #1 ($node) must be of type BoundaryXmlPeer, null given\nacceptUnionBoundaryXml(): Argument #1 ($node) must be of type SimpleXMLElement|SimpleXMLIterator, null given\nacceptBoundaryXml(): Argument #1 ($node) must be of type SimpleXMLElement, null given\nacceptBoundaryXml(): Argument #1 ($node) must be of type SimpleXMLElement, null given\nacceptRefBoundaryXml(): Argument #1 ($node) must be of type SimpleXMLElement, null given\nacceptIterableBoundaryXml(): Argument #1 ($nodes) must be of type Traversable|array, null given\nvariadicBoundaryXml(): Argument #3 must be of type SimpleXMLElement, null given\nvariadicBoundaryXml(): Argument #3 must be of type SimpleXMLElement, null given\nvariadicBoundaryXml(): Argument #2 must be of type SimpleXMLElement, null given\nreferenceVariadicBoundaryXml(): Argument #1 must be of type SimpleXMLElement, null given\norderedVariadicBoundaryXml(): Argument #1 ($first) must be of type SimpleXMLElement, null given\norderedVariadicBoundaryXml(): Argument #1 ($first) must be of type SimpleXMLElement, null given\norderedVariadicBoundaryXml(): Argument #1 ($first) must be of type SimpleXMLElement, null given\nreturnBoundaryXml(): Return value must be of type SimpleXMLElement, null returned\nreturnUnionBoundaryXml(): Return value must be of type SimpleXMLElement|SimpleXMLIterator, null returned\nreturnIterableBoundaryXml(): Return value must be of type Traversable|array, null returned\naliasedReturnBoundaryXml(): Return value must be of type SimpleXMLElement, null returned\nCannot assign null to property BoundaryXmlBox::$node of type SimpleXMLElement\nCannot assign null to property BoundaryXmlBox::$node of type SimpleXMLElement\nCannot assign null to property BoundaryXmlBox::$node of type SimpleXMLElement\nCannot assign null to property BoundaryXmlBox::$node of type SimpleXMLElement\npresent\nCannot assign null to property BoundaryXmlBox::$union of type SimpleXMLElement|int\nint(1)\nCannot assign null to property BoundaryXmlBox::$items of type Traversable|array\nCannot assign null to property BoundaryXmlBox::$staticItems of type Traversable|array\nNULL\nCannot assign null to property BoundaryXmlBox::$staticNode of type SimpleXMLElement\npresent\nCannot assign null to property BoundaryXmlStaticChild::$node of type SimpleXMLElement\npresent\n1:Cannot assign null to property BoundaryXmlBox::$staticNode of type SimpleXMLElement\n1:Cannot assign null to property BoundaryXmlBox::$staticNode of type SimpleXMLElement\nBoundaryXmlReturnBase::missingStatic(): Return value must be of type BoundaryXmlReturnChild, null returned\nBoundaryXmlReturnBase::missingSelf(): Return value must be of type BoundaryXmlReturnBase, null returned\nBoundaryXmlReturnBase::missingStaticCall(): Return value must be of type BoundaryXmlReturnChild, null returned\nBoundaryXmlConstructor::__construct(): Argument #1 ($node) must be of type SimpleXMLElement, null given\nBoundaryXmlMethods::__propset_node(): Argument #1 ($node) must be of type SimpleXMLElement, null given\nBoundaryXmlMethods::__propget_node(): Return value must be of type SimpleXMLElement, null returned\nBoundaryXmlMethods::accept(): Argument #1 ($node) must be of type SimpleXMLElement, null given\nBoundaryXmlMethods::acceptStatic(): Argument #1 ($node) must be of type SimpleXMLElement, null given\nBoundaryXmlElement::relativeVariadic(): Argument #1 must be of type BoundaryXmlElement, null given\nacceptBoundaryXml(): Argument #1 ($node) must be of type SimpleXMLElement, null given\nacceptBoundaryXml(): Argument #1 ($node) must be of type SimpleXMLElement, null given\nArgument #1 ($node) must be of type SimpleXMLElement, null given\nBoundaryXmlHook::$node::set(): Argument #1 ($value) must be of type SimpleXMLElement, null given\nNULL\nNULL\npresent\nentered|reference|peer|nullable|iterable|variadic|reference_variadic|done\n";

/// Makes the renamed fixture reach Magician with runtime-unknown source, without extra compiles.
fn opaque_fixture(source: &str) -> String {
    let body = source.strip_prefix("<?php").unwrap().replace('\\', "\\\\").replace('\'', "\\'");
    format!("<?php $source = $argc > 0 ? '{body}' : ''; eval($source);")
}

/// A return in one switch case cannot make its sibling read a raw nullable slot as an object cell.
#[test]
fn test_simplexml_reference_return_switch_siblings_keep_nullable_storage() {
    if skip_without_xml_native("test_simplexml_reference_return_switch_siblings_keep_nullable_storage") { return; }
    let out = compile_and_run_with_heap_debug(r#"<?php
function &switchReferenceBoundaryXml(SimpleXMLElement $root, int $choice): SimpleXMLElement {
    $node = $root->present;
    switch ($choice) {
        case 1: return $node;
        default:
            $node = $root->missing->child;
            var_dump($node);
            $node = $root->changed;
            return $node;
    }
}
$root = new SimpleXMLElement('<r><present>ok</present><changed>later</changed></r>');
$first =& switchReferenceBoundaryXml($root, $argc > 0 ? 1 : 0);
echo $first->getName(), '|';
$second =& switchReferenceBoundaryXml($root, $argc > 0 ? 2 : 0);
echo $second->getName(), '|';
$second['id'] = 'alias'; echo (string)$root->changed['id'];
unset($first, $second, $root);
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "present|NULL\nchanged|alias");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Both native class lineages and nullable reads preserve PHP results and strict heap cleanup.
#[test]
fn test_simplexml_review_type_boundaries_bundle() {
    if skip_without_xml_native("test_simplexml_review_type_boundaries_bundle") { return; }
    // Prefix incdec is intentionally covered only by compiled PHP until Magician supports that
    // syntax. The eval fixture retains every supported arithmetic and compound-update boundary.
    let arithmetic_eval = opaque_fixture(ARITHMETIC_EVAL);
    let boundaries_eval = opaque_fixture(&BOUNDARIES.replace("BoundaryXml", "EvalBoundaryXml"));
    let boundaries_eval_expected = BOUNDARIES_EXPECTED.replace("BoundaryXml", "EvalBoundaryXml");
    // Magician rejects reference-return declarations and yield; compiled PHP covers both.
    let boundaries_aot = format!("{BOUNDARIES}\n{}\n{}",
        REFERENCES.strip_prefix("<?php").unwrap(), GENERATORS.strip_prefix("<?php").unwrap());
    let boundaries_aot_expected = format!("{BOUNDARIES_EXPECTED}{REFERENCES_EXPECTED}{GENERATORS_EXPECTED}");
    assert_bundle_with(&[
        BundleCase { name: "lineage_aot", source: ARITHMETIC, expected: ARITHMETIC_EXPECTED },
        BundleCase { name: "lineage_eval", source: &arithmetic_eval, expected: ARITHMETIC_EVAL_EXPECTED },
        BundleCase { name: "boundaries_aot", source: &boundaries_aot, expected: &boundaries_aot_expected },
        BundleCase { name: "boundaries_eval", source: &boundaries_eval, expected: &boundaries_eval_expected },
    ], |source| {
        let out = compile_and_run_with_heap_debug(source);
        assert!(out.success, "status={} stdout={} stderr={}", out.status, out.stdout, out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
        assert!(!out.stderr.contains("Warning:"), "{}", out.stderr);
        out.stdout
    });
}

/// Retains the native boundary corpus independently of the shared program's later eval barrier.
#[test]
fn test_simplexml_type_boundaries_native_bundle() {
    if skip_without_xml_native("test_simplexml_type_boundaries_native_bundle") { return; }
    let source = format!("{BOUNDARIES}\n{}\n{}",
        REFERENCES.strip_prefix("<?php").unwrap(), GENERATORS.strip_prefix("<?php").unwrap());
    let expected = format!("{BOUNDARIES_EXPECTED}{REFERENCES_EXPECTED}{GENERATORS_EXPECTED}");
    assert_independent_boundary_program(&source, &expected);
}

/// Isolates eval boundary owners without dropping the original shared native/eval regression.
#[test]
fn test_simplexml_type_boundaries_eval_bundle() {
    if skip_without_xml_native("test_simplexml_type_boundaries_eval_bundle") { return; }
    let source = opaque_fixture(&BOUNDARIES.replace("BoundaryXml", "EvalBoundaryXml"));
    let source = format!("<?php class IndependentBoundaryXmlCapability extends SimpleXMLElement {{}}\n{}",
        source.strip_prefix("<?php").unwrap());
    assert_independent_boundary_program(&source, &BOUNDARIES_EXPECTED.replace("BoundaryXml", "EvalBoundaryXml"));
}

/// Keeps wrong-output assembly and strict per-program heap and warning checks for attribution.
fn assert_independent_boundary_program(source: &str, expected: &str) {
    let (runs, assembly) = compile_and_run_with_heap_debug_and_asm_per_argv(source, &[&[]]);
    preserve_stdout_mismatches(source, &assembly, &runs, &[expected]);
    drop(assembly);
    let output = &runs[0];
    assert!(output.success, "status={} stdout={} stderr={}", output.status, output.stdout, output.stderr);
    assert_eq!(output.stdout, expected);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
    assert!(!output.stderr.contains("Warning:"), "{}", output.stderr);
}
