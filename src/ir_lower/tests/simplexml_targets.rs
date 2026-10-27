//! Purpose:
//! SimpleXML compiler and ABI coverage for every supported target.
//!
//! Called from:
//! - AST-to-EIR unit tests in CI, without linking native target archives.
//!
//! Key details:
//! - Covers document factories, dimensions, casts, JSON, DOM import, and iteration.

/// Compiles the same complete SimpleXML fixture through one supported target's emitter.
fn verify_simplexml_target(target: &str) {
    let module = super::lower_source_at_for_target(r###"<?php
debug_backtrace();
$counterXml = new SimpleXMLElement('<r>2</r>');
echo -$counterXml; $previousXml = $counterXml++; echo $counterXml, (string)$previousXml;
$uriRoot = simplexml_load_file('data://text/plain;base64,PHIvPg==');
class MixedXmlChildHolder { public mixed $child = null; }
$holder = new MixedXmlChildHolder();
try { $holder?->child->run(); } catch (Error $error) { echo 'caught'; }
$xml = new SimpleXMLIterator('<r id="a"><item>42</item><item>3</item></r>');
$xml->item[0] = '7';
$xml['id'] = 'b';
$xml->item[] = 9;
function appendIntegerToXmlOrArray(mixed $container): void { $container[] = 9; }
appendIntegerToXmlOrArray([]);
$sax = xml_parser_create();
xml_set_element_handler($sax, function ($parser, $name, $attributes) {}, null);
$values = []; $index = [];
echo xml_parse_into_struct($sax, '<r/>', $values, $index);
$cloned = clone $xml;
$cloned->item[0] = 'copy';
echo (string)$xml->item[0], ':', (string)$cloned->item[0];
$xml->addChild('p:child', 'value', 'urn:p');
foreach ($xml->item as $key => $node) { echo $key, (string)$node, (int)$node, (float)$node; }
var_dump((bool)$xml->missing);
var_dump(empty($xml->item), empty($xml['id']), empty($xml?->missing));
var_dump(empty($xml?->item['id']), empty($xml?->item->missing));
function xmlUnionProbes(SimpleXMLElement|ArrayObject|null $node): void {
    var_dump(isset($node['id']), empty($node['id']));
    unset($node['id']);
}
xmlUnionProbes($xml);
xmlUnionProbes(new ArrayObject(['id' => 'ordinary']));
xmlUnionProbes(null);
function exposeXmlProbeGlobal(): void { global $probeGlobalXml; echo $probeGlobalXml->getName(); }
$probeGlobalXml = new SimpleXMLElement('<r id="global"/>');
exposeXmlProbeGlobal();
var_dump(isset($probeGlobalXml['id']));
unset($probeGlobalXml['id']);
class XmlEmptyScalarPeer { public string $text = '0'; }
$peer = new XmlEmptyScalarPeer();
var_dump(empty($peer?->text[0]));
function xmlEmptyObjectParameter(object $node): bool { return empty($node->item); }
var_dump(xmlEmptyObjectParameter($xml));
function xmlErasedStringParameter(object $node): string { return (string)$node; }
echo xmlErasedStringParameter($xml);
function xmlWriteOpaqueParameter(mixed $node): void { $node->other[0] = 'value'; }
xmlWriteOpaqueParameter($xml);
function xmlAppendOpaqueParameter(mixed $node): void { $node[] = 'appended'; }
xmlAppendOpaqueParameter($xml->item);
function xmlMixedArithmetic(mixed $node): mixed { return $node + 1; }
$otherNumber = new \BcMath\Number('1');
echo xmlMixedArithmetic($xml->item[0]), xmlMixedArithmetic($otherNumber);
class TargetZeroXml extends SimpleXMLElement {}
function xmlMixedNegation(mixed $node): mixed { return -$node; }
function xmlTypedNegation(SimpleXMLElement $node): mixed { return -$node; }
function xmlSubclassNegation(TargetZeroXml $node): mixed { return -$node; }
function xmlUnionNegation(SimpleXMLElement|null $node): mixed { return -$node; }
$zeroXml = new TargetZeroXml('<r>0.0</r>');
echo xmlMixedNegation($zeroXml), xmlTypedNegation($zeroXml), xmlSubclassNegation($zeroXml), xmlUnionNegation($zeroXml);
echo xmlMixedNegation($otherNumber);
function xmlNullablePropertyReads(SimpleXMLElement $node): void {
    var_dump($node->missing->child, $node->attributes()->missing);
    $node->created->child = 'written';
    $node->chain->middle->leaf = 'deep';
    $node->dimension->middle['at'] = 'attribute';
    $node->appended->middle[] = 'tail';
    $name = 'middle'; $node->dynamic->{$name}->leaf = 'runtime';
    $node->sum->middle->leaf += 1;
    $node->increment->middle->leaf++;
    var_dump($node->created->child, $node->missing->child?->getName());
}
xmlNullablePropertyReads($xml);
interface TargetXmlReceiver {}
class TargetGuardedXml extends SimpleXMLElement implements TargetXmlReceiver {}
function xmlGuardedInterfaceReads(TargetXmlReceiver $node, string $name): void {
    if ($node instanceof TargetGuardedXml) {
        echo $node->item, $node?->item, $node->$name ?? 'dynamic';
        var_dump(isset($node->item), empty($node->missing));
        echo $node->missing ?? 'fallback', $node?->missing ?? 'safe';
    }
}
xmlGuardedInterfaceReads(new TargetGuardedXml('<r><item>guarded</item></r>'), 'item');
echo (int)($xml->item[0] > 1), (int)($xml->item[0] == '7'), $xml <=> $xml->item;
$xml->item[0] += 1;
try { $xml->item[0][null] = 'invalid'; } catch (ValueError $error) { echo $error->getMessage(); }
$xml->xpath('count(//item)');
echo json_encode($xml), json_encode((array)$xml), $xml->asXML();
var_dump($xml); print_r($xml); echo print_r($xml, true), var_export($xml, true);
$doc = new DOMDocument(); $doc->loadXML('<r><v>shared</v></r>');
$imported = simplexml_import_dom($doc);
if ($imported instanceof SimpleXMLElement) { $imported->v = 'changed'; }
echo $doc->saveXML();
"###, std::path::Path::new("main.php"), std::path::Path::new("."),
        crate::codegen::platform::Target::parse(target).unwrap());
    let arithmetic = module.functions.iter().find(|function| function.name == "xmlMixedArithmetic")
        .expect("mixed arithmetic fixture must remain reachable");
    for class in ["SimpleXMLElement", "BcMath\\Number"] {
        let method = elephc_builtin_contract::lookup_class(class).unwrap().operators.unwrap()
            .arithmetic_method(elephc_builtin_contract::ObjectArithmetic::Add).unwrap();
        let expected = format!("{class}::{method}");
        assert!(arithmetic.instructions.iter().any(|inst| {
            inst.op == crate::ir::Op::StaticMethodCall
                && matches!(inst.immediate, Some(crate::ir::Immediate::Data(id))
                    if module.data.strings.get(id.as_raw() as usize) == Some(&expected))
        }), "{target}: missing arithmetic adapter {expected}");
    }
    for name in ["xmlMixedNegation", "xmlTypedNegation", "xmlSubclassNegation", "xmlUnionNegation"] {
        let function = module.functions.iter().find(|function| function.name == name).unwrap();
        let method = elephc_builtin_contract::lookup_class("SimpleXMLElement").unwrap().operators.unwrap()
            .arithmetic_method(elephc_builtin_contract::ObjectArithmetic::Mul).unwrap();
        let expected = format!("SimpleXMLElement::{method}");
        assert!(function.instructions.iter().any(|inst| {
            inst.op == crate::ir::Op::StaticMethodCall
                && matches!(inst.immediate, Some(crate::ir::Immediate::Data(id))
                    if module.data.strings.get(id.as_raw() as usize) == Some(&expected))
        }), "{target}: {name} must preserve Number-class negative zero through {expected}");
    }
    let negate = module.functions.iter().find(|function| function.name == "xmlMixedNegation").unwrap();
    let method = elephc_builtin_contract::lookup_class("BcMath\\Number").unwrap().operators.unwrap()
        .arithmetic_method(elephc_builtin_contract::ObjectArithmetic::Sub).unwrap();
    let expected = format!("BcMath\\Number::{method}");
    assert!(negate.instructions.iter().any(|inst| {
        inst.op == crate::ir::Op::StaticMethodCall && matches!(inst.immediate, Some(crate::ir::Immediate::Data(id))
            if module.data.strings.get(id.as_raw() as usize) == Some(&expected))
    }), "{target}: object-result classes retain their subtraction adapter");
    let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    assert!(assembly.contains("__rt_gc_collect_cycles_explicit"), "{target}: shutdown retires native document cycles");
    assert!(assembly.contains("__rt_sprintf_mixed_to_string"), "{target}: erased object string-context ABI");
    assert!(assembly.contains("elephc_xml_sx_load"), "{target}: document parsing boundary");
    assert!(assembly.contains("elephc_xml_sx_mutate"), "{target}: document mutation boundary");
    assert!(assembly.contains("elephc_xml_sx_diagnostics"), "{target}: native diagnostic transfer boundary");
    assert!(assembly.contains("elephc_xml_sx_load_dom"), "{target}: DOM structured parsing boundary");
    assert!(assembly.contains("elephc_runtime_simplexml_stream_close_mode"), "{target}: stream cleanup C ABI boundary");
    assert!(assembly.contains("elephc_xml_sx_load_stream"), "{target}: PHP stream parser boundary");
    assert!(assembly.contains("elephc_xml_sx_load_file"), "{target}: native file parser boundary");
    assert!(assembly.contains("elephc_xml_sx_compare"), "{target}: XML node comparison boundary");
    assert!(assembly.contains("elephc_xml_sx_iterator"), "{target}: linear native iterator boundary");
    assert!(assembly.contains(&crate::names::method_symbol("SimpleXMLElement", "__elephc_array")),
        "{target}: native property projection body");
    assert!(assembly.contains("_class_debug_projection_ptrs"), "{target}: native debug dispatch metadata");
    assert!(assembly.contains(&crate::names::method_symbol("SimpleXMLElement", "__elephc_debug_projection")),
        "{target}: owned native debug projection adapter");
    assert!(assembly.contains(&crate::names::method_symbol("SimpleXMLElement", "__elephc_debug_info")),
        "{target}: separate zero-argument native debug hook");
    assert!(assembly.contains(&crate::names::method_symbol("SimpleXMLElement", "__elephc_clone_initialize")),
        "{target}: native clone handler body");
}

/// Schedules target-sensitive SimpleXML fixtures independently under CI's ordinary timeout.
macro_rules! target_test {
    ($name:ident, $target:literal) => {
        /// Verifies SimpleXML lowering and emitted calls on the selected supported target.
        #[test]
        fn $name() { verify_simplexml_target($target); }
    };
}
target_test!(simplexml_target_macos, "macos-aarch64");
target_test!(simplexml_target_ios, "ios-arm64");
target_test!(simplexml_target_ios_sim, "ios-sim-arm64");
target_test!(simplexml_target_linux_arm, "linux-aarch64");
target_test!(simplexml_target_linux_x86, "linux-x86_64");
