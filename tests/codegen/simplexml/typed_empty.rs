//! Purpose:
//! Covers typed scalar, object, and interface continuations in native empty lowering.
//!
//! Called from:
//! - Codegen CI with managed libxml2.
//!
//! Key details:
//! - Ordinary scalar receivers must never produce a native XML method call.

use crate::support::*;

/// Linking SimpleXML preserves the ordinary magic presence protocol for opaque receivers.
#[test]
fn test_simplexml_named_isset_preserves_ordinary_magic_hooks() {
    if skip_without_xml_native("test_simplexml_named_isset_preserves_ordinary_magic_hooks") { return; }
    let out = compile_and_run_with_heap_debug(r#"<?php
interface XmlPresencePeer {}
class XmlPresenceGetOnly implements XmlPresencePeer {
    public function __get(string $name): mixed { echo 'get|'; return 'present'; }
}
class XmlPresenceBoth implements XmlPresencePeer {
    public function __isset(string $name): bool { echo 'isset|'; return true; }
    public function __get(string $name): mixed { echo 'get|'; return null; }
}
class XmlPresenceElement extends SimpleXMLElement implements XmlPresencePeer {}
function xmlPresenceMixed(mixed $node): void {
    $name = 'a';
    echo (int)isset($node->a), ':', (int)isset($node->{$name}), ':';
    echo $node->a ?? 'fallback', ':', (int)empty($node->a), '|';
}
function xmlPresenceUnion(XmlPresenceGetOnly|XmlPresenceBoth|SimpleXMLElement $node): void {
    $name = 'a';
    echo (int)isset($node->a), ':', (int)isset($node->{$name}), ':';
    echo $node->a ?? 'fallback', ':', (int)empty($node->a), '|';
}
function xmlPresenceObject(object $node): void {
    $name = 'a';
    echo (int)isset($node->a), ':', (int)isset($node->{$name}), ':';
    echo $node->a ?? 'fallback', ':', (int)empty($node->a), '|';
}
function xmlPresenceInterface(XmlPresencePeer $node): void {
    $name = 'a';
    echo (int)isset($node->a), ':', (int)isset($node->{$name}), ':';
    echo $node->a ?? 'fallback', ':', (int)empty($node->a), '|';
}
$nodes = [new XmlPresenceGetOnly(), new XmlPresenceBoth(), new XmlPresenceElement('<r><a>1</a></r>')];
foreach ($nodes as $node) { xmlPresenceMixed($node); }
foreach ($nodes as $node) {
    if ($node instanceof XmlPresenceGetOnly) { xmlPresenceUnion($node); }
    elseif ($node instanceof XmlPresenceBoth) { xmlPresenceUnion($node); }
    elseif ($node instanceof XmlPresenceElement) { xmlPresenceUnion($node); }
    else { throw new Error('unexpected presence receiver'); }
}
foreach ($nodes as $node) {
    if ($node instanceof XmlPresenceGetOnly) { xmlPresenceObject($node); }
    elseif ($node instanceof XmlPresenceBoth) { xmlPresenceObject($node); }
    elseif ($node instanceof XmlPresenceElement) { xmlPresenceObject($node); }
    else { throw new Error('expected object receiver'); }
}
foreach ($nodes as $node) {
    if ($node instanceof XmlPresencePeer) { xmlPresenceInterface($node); }
    else { throw new Error('expected presence peer'); }
}
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    let expected = "0:0:get|present:1|isset|1:isset|1:isset|get|fallback:isset|get|1|1:1:1:0|";
    assert_eq!(out.stdout, expected.repeat(4));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Scalar continuations preserve ordinary empty semantics and evaluate the final key.
#[test]
fn test_simplexml_empty_typed_scalar_continuations() {
    if skip_without_xml_native("test_simplexml_empty_typed_scalar_continuations") { return; }
    let source = r###"<?php
class EmptyScalarPeer { public int $child = 0; public string $text = '0'; }
function emptyScalarName(): string { echo 'name|'; return 'x'; }
$xml = new SimpleXMLElement('<r/>');
$peer = new EmptyScalarPeer();
echo (int)empty($peer?->child->{emptyScalarName()}), '|';
echo (int)empty($peer?->child[0]), '|', (int)empty($peer?->text[0]);
"###;
    assert_eq!(compile_and_run(source), "name|1|1|1");
    assert_eq!(without_ir_opt(|| compile_and_run(source)), "name|1|1|1");
}

/// XML content checks survive bare-object and interface hints, including borrowed boxed receivers.
#[test]
fn test_simplexml_empty_object_and_interface_parameters() {
    if skip_without_xml_native("test_simplexml_empty_object_and_interface_parameters") { return; }
    let out = compile_and_run_with_heap_debug(r###"<?php
function emptyXmlObject(object $node): bool { return empty($node->zero); }
function emptyXmlCountable(Countable $node): bool { return empty($node->zero); }
function emptyXmlRecursive(RecursiveIterator $node): bool { return empty($node->zero); }
function emptyXmlIterator(Iterator $node): bool { return empty($node->zero); }
function emptyXmlTraversable(Traversable $node): bool { return empty($node->zero); }
function emptyXmlStringable(Stringable $node): bool { return empty($node->zero); }
$xml = new SimpleXMLElement('<r><zero>0</zero></r>');
echo (int)emptyXmlObject($xml), (int)emptyXmlCountable($xml), (int)emptyXmlRecursive($xml), (int)emptyXmlIterator($xml), (int)emptyXmlTraversable($xml), (int)emptyXmlStringable($xml), '|';
$ordinary = new stdClass(); $ordinary->zero = 'kept';
echo (int)emptyXmlObject($ordinary);
"###);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "111111|0");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Native unions use internal adapters for isset, empty and unset, including real null receivers.
#[test]
fn test_simplexml_native_union_dimension_probes() {
    if skip_without_xml_native("test_simplexml_native_union_dimension_probes") { return; }
    let out = compile_and_run_with_heap_debug(r#"<?php
function nativeUnionProbe(SimpleXMLElement|SimpleXMLIterator|null $node): void {
    echo (int)isset($node['id']), ':', (int)empty($node['id']), '|';
    unset($node['id']);
    echo (int)isset($node['id']), '|';
}
nativeUnionProbe(null);
nativeUnionProbe(new SimpleXMLElement('<r id="x"/>'));
nativeUnionProbe(new SimpleXMLIterator('<r id="0"/>'));
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "0:1|0|1:0|0|1:1|0|");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Native XML and public ArrayAccess arms preserve their distinct isset and unset protocols.
#[test]
fn test_simplexml_arrayaccess_union_dimension_probes() {
    if skip_without_xml_native("test_simplexml_arrayaccess_union_dimension_probes") { return; }
    let out = compile_and_run_with_heap_debug(r#"<?php
class XmlDimensionProbePeer implements ArrayAccess {
    private bool $present = true;
    public function offsetExists(mixed $offset): bool { echo 'exists|'; return $this->present; }
    public function offsetGet(mixed $offset): mixed { echo 'get|'; return '1'; }
    public function offsetSet(mixed $offset, mixed $value): void {}
    public function offsetUnset(mixed $offset): void { echo 'unset|'; $this->present = false; }
}
function xmlDimensionProbeKey(): string { echo 'key|'; return 'id'; }
function probeXmlOrArrayAccess(SimpleXMLElement|XmlDimensionProbePeer|null $node): void {
    echo (int)isset($node[xmlDimensionProbeKey()]), ':', (int)empty($node['id']), '|';
    unset($node[xmlDimensionProbeKey()]);
    echo (int)isset($node['id']), '|';
}
probeXmlOrArrayAccess(null);
probeXmlOrArrayAccess(new SimpleXMLElement('<r id="1"/>'));
probeXmlOrArrayAccess(new XmlDimensionProbePeer());
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, concat!(
        "key|0:1|key|0|key|1:0|key|0|",
        "key|exists|1:exists|get|0|key|unset|exists|0|",
    ));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A known XML source keeps its native probe protocol when a main local uses boxed global storage.
#[test]
fn test_simplexml_main_global_dimension_probe_protocol() {
    if skip_without_xml_native("test_simplexml_main_global_dimension_probe_protocol") { return; }
    let out = compile_and_run_with_heap_debug(r#"<?php
function exposeXmlProbeGlobal(): void { global $probeGlobalXml; echo $probeGlobalXml->getName(), '|'; }
$probeGlobalXml = new SimpleXMLElement('<r id="global"/>');
exposeXmlProbeGlobal();
echo (int)isset($probeGlobalXml['id']), ':', (int)empty($probeGlobalXml['id']), '|';
unset($probeGlobalXml['id']);
echo (int)isset($probeGlobalXml['id']);
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "r|1:0|0");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
/// Erased variables dispatch unset to XML, public ArrayAccess, arrays, or PHP scalar diagnostics.
#[test]
fn test_simplexml_mixed_unset_selects_current_receiver() {
    if skip_without_xml_native("test_simplexml_mixed_unset_selects_current_receiver") { return; }
    let out = compile_and_run_with_heap_debug(r#"<?php
function removeMixedXmlOffset(mixed &$value): void { unset($value['a']); }
$xml = new SimpleXMLElement('<r a="old"/>');
removeMixedXmlOffset($xml); echo (int)isset($xml['a']), '|';
$ordinary = new ArrayObject(['a' => 'old']);
removeMixedXmlOffset($ordinary); echo count($ordinary), '|';
$array = ['a' => 'old']; $alias = $array;
removeMixedXmlOffset($array); echo count($array), ':', count($alias), '|';
set_error_handler(function(int $level, string $message): bool { echo $level, '|'; return true; });
$scalar = false; removeMixedXmlOffset($scalar); var_dump($scalar);
restore_error_handler();
$scalar = null; removeMixedXmlOffset($scalar); var_dump($scalar);
foreach ([true, 1, 1.0, 'abc'] as $scalar) {
    try { removeMixedXmlOffset($scalar); }
    catch (Error $error) { echo $error->getMessage(), '|'; }
}
function replaceUnsetXml(mixed &$value): string { $value = new SimpleXMLElement('<new a="value"/>'); return 'a'; }
$current = new SimpleXMLElement('<old a="value"/>');
$old = $current;
unset($current[replaceUnsetXml($current)]);
echo $current->getName(), ':', (int)isset($current['a']), ':', (int)isset($old['a']);
function erasedUnsetXml(SimpleXMLElement $value): mixed { echo '|receiver'; return $value; }
function computedUnsetKey(): string { echo '|key'; return 'a'; }
unset(erasedUnsetXml($old)[computedUnsetKey()]);
echo ':', (int)isset($old['a']);
unset(simplexml_load_string('<r a="temporary"/>')['a']);
function replaceProbeXml(mixed &$value): string { $value = new SimpleXMLElement('<probe/>'); return 'a'; }
$probe = new SimpleXMLElement('<r a="value"/>');
echo '|', (int)isset($probe[replaceProbeXml($probe)]), ':', $probe->getName();
$probe = new SimpleXMLElement('<r a="value"/>');
echo '|', (int)empty($probe[replaceProbeXml($probe)]), ':', $probe->getName();
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, concat!("0|0|0:1|8192|bool(false)\nNULL\n",
        "Cannot unset offset in a non-array variable|Cannot unset offset in a non-array variable|",
        "Cannot unset offset in a non-array variable|Cannot unset string offsets|new:0:1|receiver|key:0|0:probe|1:probe"));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
/// Opaque object receiver boxes stay rooted when a native empty key conversion throws.
#[test]
fn test_simplexml_object_empty_box_owners_survive_key_throws() {
    if skip_without_xml_native("test_simplexml_object_empty_box_owners_survive_key_throws") { return; }
    let out = compile_and_run_with_heap_debug(r#"<?php
class ThrowingNativeEmptyKey {
    public function __toString(): string { throw new RuntimeException('caught'); }
}
function emptyObjectXmlDimension(object $value, mixed $key): bool { return empty($value[$key]); }
$xml = new SimpleXMLElement('<r/>');
for ($i = 0; $i < 4; $i++) {
    echo (int)emptyObjectXmlDimension($xml, 'missing'), '|';
    try { emptyObjectXmlDimension($xml, new ThrowingNativeEmptyKey()); }
    catch (RuntimeException $error) { echo $error->getMessage(), '|'; }
}
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "1|caught|".repeat(4));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// XML capability must not turn unsupported erased storage unsets into discarded-copy mutations.
#[test]
fn test_simplexml_erased_unset_storage_is_rejected() {
    if skip_without_xml_native("test_simplexml_erased_unset_storage_is_rejected") { return; }
    for body in [
        "class ErasedStaticUnset { public static mixed $data = ['a' => 1]; } unset(ErasedStaticUnset::$data['a']);",
        "class ErasedDynamicUnset { public mixed $data = ['a' => 1]; } $h = new ErasedDynamicUnset(); $name = $argc > 0 ? 'data' : 'other'; unset($h->{$name}['a']);",
        "function erasedRefSeed(): mixed { return ['a' => 1, 'b' => 2]; } function &erasedRefUnset(mixed &$data): mixed { return $data; } $data = erasedRefSeed(); unset(erasedRefUnset($data)['a']);",
        "class ErasedMethodRefUnset { public mixed $data = ['a' => 1, 'b' => 2]; public function &bag(): mixed { return $this->data; } } $h = new ErasedMethodRefUnset(); unset($h->bag()['a']);",
        "class ErasedStaticRefUnset { public static mixed $data = ['a' => 1, 'b' => 2]; public static function &bag(): mixed { return self::$data; } } unset(ErasedStaticRefUnset::bag()['a']);",
    ] {
        let source = format!("<?php $xml = simplexml_load_string('<r/>'); echo $xml->getName(); {body}");
        let error = compile_source_expect_backend_error(&source);
        assert!(error.contains("unset target shape"), "{error}");
    }
}

/// An erased array element is refused at runtime while the original parent remains unchanged.
#[test]
fn test_simplexml_erased_nested_array_unset_preserves_unsupported_storage() {
    if skip_without_xml_native("test_simplexml_erased_nested_array_unset_preserves_unsupported_storage") { return; }
    let out = compile_and_run_with_heap_debug(r#"<?php
$xml = new SimpleXMLElement('<r/>');
$data = json_decode('{"a":{"b":1,"c":2}}', true);
try { unset($data['a']['b']); }
catch (Error $error) { echo $error->getMessage(), '|'; }
echo json_encode($data);
unset($xml, $data, $error);
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "Unsupported unset(): erased array storage has no attached removal path|{\"a\":{\"b\":1,\"c\":2}}");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// New property removal retains native XML protocols and mutates ordinary property storage in place.
#[test]
fn test_simplexml_property_unset_main_integration_bundle() {
    if skip_without_xml_native("test_simplexml_property_unset_main_integration_bundle") { return; }
    let source = r###"<?php
class XmlUnsetIntegrationHolder {
    public array $items = [];
    public mixed $bag;
    public SimpleXMLElement $xml;
    public readonly mixed $locked;
    public static array $shared = [];
    public function __construct(mixed $value) { $this->locked = $value; }
}
function XmlUnsetIntegrationKey(XmlUnsetIntegrationHolder $holder): int {
    echo 'key|'; $holder->bag = [8, 9]; return 0;
}
$doc = new SimpleXMLElement('<r><a>x</a><a>y</a><b id="keep"/></r>');
$holder = new XmlUnsetIntegrationHolder($doc->b);
$holder->xml = $doc->a; $holder->bag = $doc->a;
unset($holder->xml[0]); echo $doc->asXML();
unset($holder->bag[0]); echo $doc->asXML();
$holder->bag = $doc->b;
unset($holder->bag[XmlUnsetIntegrationKey($holder)]);
echo json_encode($holder->bag), '|', $doc->asXML();
$nested = ['xml' => $doc->b]; unset($nested['xml']['id']); echo $doc->asXML();
unset($holder->locked[0]); echo $doc->asXML();
$empty = new XmlUnsetIntegrationHolder(null); unset($empty->xml[0]); echo 'quiet|';
$holder->items = [4, 5, 6]; $copy = $holder->items;
unset($holder->items[1]); echo json_encode($holder->items), ':', json_encode($copy), '|';
$holder->bag = ['k' => 1, 'j' => 2]; $mixedCopy = $holder->bag;
unset($holder->bag['k']); echo json_encode($holder->bag), ':', json_encode($mixedCopy), '|';
XmlUnsetIntegrationHolder::$shared = [4, 5]; $staticCopy = XmlUnsetIntegrationHolder::$shared;
unset(XmlUnsetIntegrationHolder::$shared[0]); echo json_encode(XmlUnsetIntegrationHolder::$shared), ':', json_encode($staticCopy), '|';
XmlUnsetIntegrationHolder::$shared = [];
unset($holder, $empty, $nested, $doc, $copy, $mixedCopy, $staticCopy);
"###;
    let renamed = source.replace("XmlUnsetIntegration", "EvalXmlUnsetIntegration");
    let body = renamed.strip_prefix("<?php").unwrap().replace('\\', "\\\\").replace('\'', "\\'");
    let eval = format!("<?php $source = $argc > 0 ? '{body}' : ''; eval($source);");
    let expected = "<?xml version=\"1.0\"?>\n<r><a>y</a><b id=\"keep\"/></r>\n<?xml version=\"1.0\"?>\n<r><b id=\"keep\"/></r>\nkey|{\"1\":9}|<?xml version=\"1.0\"?>\n<r><b id=\"keep\"/></r>\n<?xml version=\"1.0\"?>\n<r><b/></r>\n<?xml version=\"1.0\"?>\n<r/>\nquiet|{\"0\":4,\"2\":6}:[4,5,6]|{\"j\":2}:{\"k\":1,\"j\":2}|{\"1\":5}:[4,5]|";
    assert_bundle_with(&[
        BundleCase { name: "property_unset_aot", source, expected },
        BundleCase { name: "property_unset_eval", source: &eval, expected },
    ], |source| {
        let out = compile_and_run_with_heap_debug(source);
        assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
        assert!(!out.stderr.contains("Warning:") && !out.stderr.contains("Notice:"), "{}", out.stderr);
        out.stdout
    });
}
