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
    let source = [r###"<?php
debug_backtrace();
$counterXml = new SimpleXMLElement('<r>2</r>');
$targetLiveDocument = new DOMDocument();
$targetLiveDocument->loadXML('<r><a/><b/></r>');
$targetLiveRoot = $targetLiveDocument->documentElement;
$targetLiveA = $targetLiveRoot->childNodes->item(0);
$targetLiveXml = simplexml_import_dom($targetLiveDocument);
unset($targetLiveXml->b);
var_dump($targetLiveA->nextSibling);
$targetLiveXml->addChild('c', 'new');
echo $targetLiveA->nextSibling->nodeName;
var_dump($targetLiveRoot === $targetLiveDocument->documentElement, $targetLiveA === $targetLiveRoot->childNodes->item(0));
$targetRawDocument = new DOMDocument();
$targetRawDocument->loadHTML('<body><script>a<b</script></body>');
$targetRawXml = simplexml_import_dom($targetRawDocument);
$targetRawScript = $targetRawDocument->getElementsByTagName('script')->item(0);
echo $targetRawScript->childNodes->item(0)->nodeName;
$targetRawXml->body->script = 'c<d';
echo $targetRawDocument->saveXML($targetRawScript), $targetRawScript->childNodes->item(0)->nodeName;
$targetRawScript->nodeValue = 'e<f';
$targetRawScript->textContent = 'g<h';
$targetXPathXml = new SimpleXMLElement('<r xmlns:p="urn:p" a="1"><p:c>yes</p:c><!--r--></r>');
echo count($targetXPathXml['a']->xpath('../p:c'));
echo count($targetXPathXml->xpath('//comment()')[0]->xpath('../p:c'));
var_dump($targetXPathXml->missing->registerXPathNamespace('q', 'urn:p'));
$targetXPathXml->registerXPathNamespace('q', 'urn:p');
$targetXPathClone = clone $targetXPathXml;
echo count($targetXPathClone->xpath('//p:c'));
class TargetXmlSaveDocument extends DOMDocument {
    public function saveXML(mixed $node = null, int $options = 0): string { return parent::saveXML($node, $options); }
}
$saveDocument = new TargetXmlSaveDocument();
$saveDocument->loadHTML('<html><body>target</body></html>');
echo $saveDocument->saveXML();
$targetHtmlBody = $saveDocument->getElementsByTagName('body')->item(0);
$targetHtmlBody->setAttribute('title', 'é');
$targetHtmlBefore = $saveDocument->saveXML();
var_dump($targetHtmlBody->setAttribute('xmlns', 'urn:target'), $targetHtmlBody->setAttribute('xmlns', 'ignored'));
$targetHtmlXml = simplexml_import_dom($saveDocument);
var_dump($targetHtmlBefore === $saveDocument->saveXML(), $targetHtmlBody === $saveDocument->getElementsByTagName('body')->item(0));
echo $targetHtmlBody->getAttribute('xmlns');
echo count($targetHtmlXml->xpath('//body'));
$targetXmlChildren = $saveDocument->documentElement->childNodes;
echo $targetXmlChildren->item(0)->nodeName;
echo $saveDocument->saveXML();
$targetMetadataDocument = new DOMDocument();
$targetMetadataDocument->loadXML('<r><![CDATA[q]]><?work data?></r>');
$targetMetadataChildren = $targetMetadataDocument->documentElement->childNodes;
echo $targetMetadataChildren->item(0)->nodeName;
$targetMetadataPi = $targetMetadataChildren->item(1);
$targetMetadataPi->data = 'changed';
echo $targetMetadataPi->data;
$targetMetadataDocument->loadXML('<!--before--><?work data?><r/><!---->');
echo $targetMetadataDocument->childNodes->length, $targetMetadataDocument->documentElement->previousSibling->nodeName;
echo $targetMetadataDocument->saveXML($targetMetadataDocument);
echo $targetMetadataDocument->textContent;
$targetTextRoot = $targetMetadataDocument->documentElement;
$targetTextRoot->nodeValue = 'A&amp;B';
$targetTextRoot->textContent = 'A&amp;B';
echo $targetTextRoot->nodeValue;
$targetMetadataDocument->loadXML('<p:r xmlns:p="urn:p" p:a="1" xml:lang="it"><p:c/></p:r>');
$targetQualifiedRoot = $targetMetadataDocument->documentElement;
echo $targetQualifiedRoot->nodeName, $targetQualifiedRoot->childNodes->item(0)->nodeName;
$targetQualifiedRoot->setAttribute('p:a', '3');
$targetQualifiedRoot->setAttribute('xml:lang', 'en');
echo $targetQualifiedRoot->getAttribute('p:a'), $targetQualifiedRoot->getAttribute('xml:lang');
echo $targetMetadataDocument->getElementsByTagName('c')->length;
try { $targetQualifiedRoot->setAttribute('', 'bad'); } catch (ValueError $error) { echo $error->getMessage(); }
try { $targetQualifiedRoot->setAttribute('1bad', 'bad'); } catch (DOMException $error) { echo $error->getCode(); }
interface TargetGuardedXmlView {}
class TargetGuardedXml extends SimpleXMLElement implements TargetGuardedXmlView {
    public string $label = 'before';
}
function targetGuardedXmlWrites(TargetGuardedXmlView $node): void {
    if ($node instanceof TargetGuardedXml) {
        $node->label = 'after';
        $name = 'label'; $node->{$name} = 'dynamic';
        unset($node->label); var_dump(isset($node->label));
        $node->value = 'changed';
        $name = 'value'; unset($node->{$name});
        echo count($node->value);
    }
}
targetGuardedXmlWrites(new TargetGuardedXml('<r><value>old</value></r>'));
$targetNamespaceXml = new SimpleXMLElement('<r><a><b xmlns:p="urn:p"><p:c p:k="v"/></b></a></r>');
$targetNamespaceXml->registerXPathNamespace('q', 'urn:p');
$targetNamespaceChild = $targetNamespaceXml->xpath('//q:c')[0];
$targetNamespaceXml->a->b = 't';
$targetNamespaceXml->a = 't2';
echo $targetNamespaceChild->asXML(), json_encode($targetNamespaceChild->getNamespaces());
$targetBitwiseXml = new SimpleXMLElement('<r><n>3</n></r>');
$targetBitwiseXml->addChild('tail:', 'v');
$targetBitwiseXml->addChild('p:e', 'w', '');
echo count($targetBitwiseXml->xpath('//e'));
$targetBitwiseXml->n |= 4;
$targetBitwiseXml->n >>= 1;
echo $targetBitwiseXml->n & 6, 8 | $targetBitwiseXml->n, $targetBitwiseXml->n << 1;
echo (new SimpleXMLElement('<r>3</r>')) | 4;
$targetBinaryName = 'k' . chr($argc > 0 ? 233 : 234);
$targetBitwiseXml[$targetBinaryName] = chr($argc > 0 ? 255 : 254);
echo bin2hex((string)$targetBitwiseXml[$targetBinaryName]);
try { $targetBitwiseXml->saveXML("out.xml\0.txt"); } catch (ValueError $error) { echo $error->getMessage(); }
class TargetHookedCoalesceXml extends SimpleXMLElement {
    public function __isset(string $name): bool { return true; }
    public function __get(string $name): mixed { return 'wrong'; }
    public function selfProbe(): mixed { return $this->missing ?? 'self-fallback'; }
}
function xmlNullableHookCoalesce(?TargetHookedCoalesceXml $node): mixed { return $node->missing ?? 'fallback'; }
$hookedQuietXml = new TargetHookedCoalesceXml('<r/>');
$hookedQuietXml->added ??= 'value';
echo $hookedQuietXml->selfProbe(), xmlNullableHookCoalesce($hookedQuietXml);
$quietPropertyName = 'added';
var_dump(isset($hookedQuietXml->{$quietPropertyName}), isset($hookedQuietXml['id']));
function xmlQuietScalarProbes(int $number): void {
    var_dump(isset($number->a), empty($number->{xmlComputedTargetName()}));
}
xmlQuietScalarProbes($argc);
class TargetXmlPresencePeer {
    public function __isset(string $name): bool { return true; }
    public function __get(string $name): mixed { return null; }
}
function xmlQuietUnionProbe(SimpleXMLElement|TargetXmlPresencePeer $node): bool { return isset($node->a); }
var_dump(xmlQuietUnionProbe($counterXml), xmlQuietUnionProbe(new TargetXmlPresencePeer()));
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
class TargetScopedWriteName {
    public function __toString(): string { return 'scoped'; }
    public function __destruct() {}
}
function xmlScopedWriteName(): TargetScopedWriteName { return new TargetScopedWriteName(); }
function xmlScopedLogicFallback(): bool { return true; }
function xmlScopedWriteCaptures(mixed $node): void {
    $node->{xmlScopedWriteName()}->leaf = 1;
    $node->{xmlScopedWriteName()}->leaf += 1;
    $node->indexed[xmlComputedTargetIndex()]->compound += 1 or xmlScopedLogicFallback();
    for ($iteration = 0; $iteration < 2; $iteration++) { $node->{xmlScopedWriteName()}->leaf++; }
    try { $node->{xmlScopedWriteName()}->leaf += throw new RuntimeException('scope'); }
    catch (RuntimeException $error) { echo $error->getMessage(); }
}
xmlScopedWriteCaptures($xml);
function xmlComputedTargetName(): string { return 'a'; }
function xmlComputedTargetIndex(): int { return 0; }
function xmlNullablePropertyReads(SimpleXMLElement $node): void {
    var_dump($node->missing->child, $node->attributes()->missing);
    $node->created->child = 'written';
    $node->chain->middle->leaf = 'deep';
    $node->dimension->middle['at'] = 'attribute';
    $node->appended->middle[] = 'tail';
    $name = 'middle'; $node->dynamic->{$name}->leaf = 'runtime';
    $node->sum->middle->leaf += 1;
    $node->increment->middle->leaf++;
    $node->{xmlComputedTargetName()}->computed = 1;
    $node->indexed[xmlComputedTargetIndex()]->compound += 1;
    $node->indexed[xmlComputedTargetIndex()]->stepped++;
    $node->indexed[0]->leaf = 1;
    $node->indexed[1]->leaf = 2;
    var_dump(++$node->counter, $node->counter++);
    $node->attributes()->temporary += 1;
    try { $node->thrown->leaf += throw new RuntimeException('rhs'); }
    catch (RuntimeException $error) { echo $error->getMessage(); }
    var_dump($node->created->child, $node->missing->child?->getName());
}
xmlNullablePropertyReads($xml);
interface TargetXmlReceiver {}
class TargetGuardedReadXml extends SimpleXMLElement implements TargetXmlReceiver {}
function xmlGuardedInterfaceReads(TargetXmlReceiver $node, string $name): void {
    if ($node instanceof TargetGuardedReadXml) {
        echo $node->item, $node?->item, $node->$name ?? 'dynamic';
        var_dump(isset($node->item), empty($node->missing));
        echo $node->missing ?? 'fallback', $node?->missing ?? 'safe';
    }
}
xmlGuardedInterfaceReads(new TargetGuardedReadXml('<r><item>guarded</item></r>'), 'item');
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
"###,
        include_str!("../../../tests/fixtures/simplexml/iterator_arithmetic.php").trim_start_matches("<?php"),
        include_str!("../../../tests/fixtures/simplexml/typed_object_boundaries.php").trim_start_matches("<?php"),
        include_str!("../../../tests/fixtures/simplexml/generator_object_boundaries.php").trim_start_matches("<?php"),
    ].concat();
    let module = super::lower_source_at_for_target(&source, std::path::Path::new("main.php"), std::path::Path::new("."),
        crate::codegen::platform::Target::parse(target).unwrap());
    let cache = module.functions.iter().find(|function| function.name == "__elephc_dom_remember_in")
        .expect("DOM cache writes must use a DOMDocument-typed helper");
    assert!(cache.instructions.iter().any(|instruction| instruction.op == crate::ir::Op::PropGetForWrite),
        "{target}: cache insertion must target the document's attached array property");
    assert!(!cache.instructions.iter().any(|instruction| instruction.op == crate::ir::Op::RuntimeCall
        && matches!(instruction.immediate, Some(crate::ir::Immediate::Data(id))
            if module.data.strings.get(id.as_raw() as usize).is_some_and(|name| name == "__elephc_dom_nodes"))),
        "{target}: cache insertion must not use the stdClass-only mixed property fallback");
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
    let captures = module.functions.iter().find(|function| function.name == "xmlScopedWriteCaptures").unwrap();
    let scoped_slots: Vec<_> = captures.locals.iter().filter(|slot| {
        slot.name.as_deref().is_some_and(|name| name.starts_with(crate::parser::ast::SCOPED_WRITE_TEMP_PREFIX))
            && captures.instructions.iter().any(|inst| inst.op == crate::ir::Op::PushCallOperandOwner
                && inst.immediate == Some(crate::ir::Immediate::LocalSlot(slot.id)))
    }).collect();
    assert!(!scoped_slots.is_empty(), "{target}: scoped captures need live unwind roots");
    for slot in scoped_slots {
        assert!(crate::ir::Ownership::php_type_needs_lifetime_tracking(&slot.php_type), "{target}");
        for op in [crate::ir::Op::PopCallOperandOwner, crate::ir::Op::ReleaseLocalSlot] {
            assert!(captures.instructions.iter().any(|inst| inst.op == op
                && inst.immediate == Some(crate::ir::Immediate::LocalSlot(slot.id))), "{target}: missing {op:?} for scoped capture");
        }
    }
    let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    let guard_name = crate::names::generator_parameter_guard_name("generatorBoundaryXml");
    let guard = module.closures.iter().find(|function| function.name == guard_name)
        .expect("generator parameters must have an ordinary eager EIR validator");
    assert!(!guard.flags.is_generator && guard.flags.is_synthetic);
    assert!(guard.instructions.iter().any(|instruction| instruction.op == crate::ir::Op::IsNull));
    let constructor = &assembly[assembly.find(&format!("{}:", crate::names::function_symbol("generatorBoundaryXml"))).unwrap()..];
    let validation = constructor.find(&crate::names::function_symbol(&guard_name)).unwrap();
    let allocation = constructor.find("__rt_fiber_construct").unwrap();
    assert!(validation < allocation, "{target}: null validation must precede coroutine allocation");
    for message in [
        "acceptBoundaryXml(): Argument #1 ($node) must be of type SimpleXMLElement, null given",
        "returnBoundaryXml(): Return value must be of type SimpleXMLElement, null returned",
        "Cannot assign null to property BoundaryXmlBox::$node of type SimpleXMLElement",
    ] {
        assert!(assembly.contains(message), "{target}: missing native object boundary: {message}");
    }
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
