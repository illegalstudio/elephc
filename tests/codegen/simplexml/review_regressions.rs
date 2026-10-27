//! Purpose:
//! Covers PHP compatibility and operand ownership found in the SimpleXML review.
//!
//! Called from:
//! - Codegen CI with managed libxml2 and the PHP reference interpreter.
//!
//! Key details:
//! - Distinguishes container empty tests, boolean casts, and XPath wrapper kinds.

use crate::support::*;

/// XPath attributes are filtered lists, whereas offset attributes remain stable scalar nodes.
#[test]
fn test_simplexml_xpath_attribute_views() {
    if skip_without_xml_native("test_simplexml_xpath_attribute_views") { return; }
    let out = compile_and_run(r###"<?php
$xml = new SimpleXMLElement('<r xmlns:p="urn:p" id="7" p:id="8"/>');
foreach ($xml->xpath('//@*') as $attribute) {
    echo $attribute->getName(), ':', (string)$attribute, ':', count($attribute), ':', json_encode($attribute), '|';
    foreach ($attribute as $key => $value) { echo $key, '=', (string)$value, '|'; }
}
$offset = $xml['id'];
echo count($offset), ':', json_encode($offset), '|';
"###);
    assert_eq!(out, "id:7:1:{\"@attributes\":{\"id\":\"7\"}}|id=7|id:8:1:{\"@attributes\":{\"id\":\"8\"}}|id=8|0:{\"0\":\"7\"}|");
}

/// XPath keeps comments and processing instructions with their own content and serialization.
#[test]
fn test_simplexml_xpath_comment_and_processing_instruction() {
    if skip_without_xml_native("test_simplexml_xpath_comment_and_processing_instruction") { return; }
    let out = compile_and_run(r###"<?php
$xml = new SimpleXMLElement('<r><!--note--><?work data?></r>');
foreach ($xml->xpath('comment() | processing-instruction()') as $node) {
    echo $node->getName(), ':', (string)$node, ':', $node->asXML(), '|';
}
"###);
    assert_eq!(out, "comment:note:<!--note-->|work:data:<?work data?>|");
}

/// Scalar XPath results warn with E_WARNING before returning false, including handler throws.
#[test]
fn test_simplexml_xpath_scalar_warning() {
    if skip_without_xml_native("test_simplexml_xpath_scalar_warning") { return; }
    let out = compile_and_run(r###"<?php
$xml = new SimpleXMLElement('<r><a/></r>');
set_error_handler(function(int $level, string $message): bool { echo $level, ':', $message, '|'; return true; });
var_dump($xml->xpath('count(//a)'), $xml->xpath('true()'), $xml->xpath('string(//a)'));
restore_error_handler();
set_error_handler(function(int $level, string $message): bool { throw new RuntimeException('caught'); });
try { $xml->xpath('count(//a)'); } catch (RuntimeException $exception) { echo $exception->getMessage(); }
restore_error_handler();
"###);
    assert_eq!(out, "2:SimpleXMLElement::xpath(): XPath expression must return a node set, number returned|2:SimpleXMLElement::xpath(): XPath expression must return a node set, bool returned|2:SimpleXMLElement::xpath(): XPath expression must return a node set, string returned|bool(false)\nbool(false)\nbool(false)\ncaught");
}

/// Native empty tests bypass subclass hooks, preserve ordinary slots, and differ from bool casts.
#[test]
fn test_simplexml_empty_native_container_content() {
    if skip_without_xml_native("test_simplexml_empty_native_container_content") { return; }
    let out = compile_and_run(r###"<?php
class EmptyNativeXml extends SimpleXMLElement {
    public string $zero = 'declared';
    public function __isset(string $name): bool { echo 'wrong'; return false; }
    public function offsetExists(mixed $key): bool { echo 'wrong'; return false; }
}
$xml = new EmptyNativeXml('<r z="0" n="00"><blank/><zero>0</zero><number>00</number><nested><a/></nested><cdata><![CDATA[0]]></cdata></r>');
echo (int)empty($xml->blank), (int)empty($xml->zero), (int)empty($xml->number), (int)empty($xml->nested), (int)empty($xml->cdata), (int)empty($xml->absent), '|';
echo (int)empty($xml['z']), (int)empty($xml['n']), (int)empty($xml['absent']), (int)empty($xml->zero[0]), '|';
$selected = $xml->zero; $attribute = $xml['z'];
echo (int)empty($selected), (int)empty($attribute), (int)(bool)$selected, (int)(bool)$attribute, '|';
function emptyNativeXmlBoxed(mixed $node, string $key): void {
    echo (int)empty($node->{$key}), (int)empty($node?->{$key}), '|';
}
emptyNativeXmlBoxed($xml, 'zero');
$ordinary = new stdClass(); $ordinary->zero = $selected;
emptyNativeXmlBoxed($ordinary, 'zero');
function emptyNativeXmlDimension(mixed $node): void { echo (int)empty($node['z']); }
emptyNativeXmlDimension($xml);
emptyNativeXmlDimension(['z' => '0']);
"###);
    assert_eq!(out, "110001|1011|0011|11|00|11");
}

/// Bundled AOT and eval probes preserve receiver rebinding, name order and nullsafe laziness.
#[test]
fn test_simplexml_empty_operand_order_and_eval_bundle() {
    if skip_without_xml_native("test_simplexml_empty_operand_order_and_eval_bundle") { return; }
    let out = compile_and_run_with_heap_debug(r###"<?php
$xml = new SimpleXMLElement('<r><zero>1</zero></r>');
function replaceEmptyXmlReceiver(): string {
    global $xml;
    echo 'key|';
    $xml = new SimpleXMLElement('<r z="0"><zero>0</zero></r>');
    return 'zero';
}
echo (int)empty($xml->{replaceEmptyXmlReceiver()}), '|';
function emptyXmlNullsafe(?SimpleXMLElement $node): void {
    echo (int)empty($node?->{replaceEmptyXmlReceiver()}), '|';
}
emptyXmlNullsafe(null);
$code = $argc > 0 ? 'echo (int)empty($xml->zero), (int)empty($xml["z"]), "|"; $selection = $xml->zero; echo (int)empty($selection);' : '';
eval($code);
unset($selection, $xml);
"###);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "key|1|1|11|0");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Attribute views add to their containing element and existing namespace URIs keep their prefix.
#[test]
fn test_simplexml_add_attribute_parent_and_namespace_reuse() {
    if skip_without_xml_native("test_simplexml_add_attribute_parent_and_namespace_reuse") { return; }
    let out = compile_and_run(r###"<?php
$xml = new SimpleXMLElement('<r xmlns:p="urn:p" id="1"/>');
$xml['id']->addAttribute('offset', '2');
$xml->attributes()->addAttribute('list', '3');
$xml->addChild('q:child', 'v', 'urn:p');
$xml->addAttribute('q:a', '4', 'urn:p');
echo $xml->asXML();
"###);
    assert_eq!(out, "<?xml version=\"1.0\"?>\n<r xmlns:p=\"urn:p\" id=\"1\" offset=\"2\" list=\"3\" p:a=\"4\"><p:child>v</p:child></r>\n");
}

/// Explicit empty attribute namespaces preserve bindings and distinguish omitted namespaces.
#[test]
fn test_simplexml_add_attribute_explicit_empty_namespace() {
    if skip_without_xml_native("test_simplexml_add_attribute_explicit_empty_namespace") { return; }
    let out = compile_and_run(r#"<?php
set_error_handler(function(int $level, string $message): bool { echo 'warning|'; return true; });
$plain = new SimpleXMLElement('<a/>');
$plain->addAttribute('b', 'c', '');
echo $plain->asXML();
$prefixed = new SimpleXMLElement('<a xmlns="urn:x"/>');
$prefixed->addAttribute('p:b', '2', '');
echo $prefixed->asXML();
$existing = new SimpleXMLElement('<a b="old"/>');
$existing->addAttribute('b', 'new', '');
echo $existing->asXML();
$duplicate = new SimpleXMLElement('<a/>');
$duplicate->addAttribute('b', 'first', '');
$duplicate->addAttribute('b', 'second', '');
echo $duplicate->asXML();
$omitted = new SimpleXMLElement('<a/>');
$omitted->addAttribute('b', 'first', '');
$omitted->addAttribute('b', 'second');
echo $omitted->asXML();
restore_error_handler();
"#);
    assert_eq!(out, concat!(
        "<?xml version=\"1.0\"?>\n<a xmlns=\"\" b=\"c\"/>\n",
        "<?xml version=\"1.0\"?>\n<a xmlns=\"urn:x\" xmlns:p=\"\" p:b=\"2\"/>\n",
        "<?xml version=\"1.0\"?>\n<a xmlns=\"\" b=\"old\" b=\"new\"/>\n",
        "warning|<?xml version=\"1.0\"?>\n<a xmlns=\"\" b=\"first\"/>\n",
        "<?xml version=\"1.0\"?>\n<a xmlns=\"\" b=\"first\" b=\"second\"/>\n",
    ));
}

/// Write and add failures use E_WARNING, preserve appending, and validate names with ValueError.
#[test]
fn test_simplexml_mutation_warning_contract() {
    if skip_without_xml_native("test_simplexml_mutation_warning_contract") { return; }
    let out = compile_and_run(r###"<?php
$xml = new SimpleXMLElement('<r id="1"><item>a</item><item>b</item></r>');
set_error_handler(function(int $level, string $message): bool { echo $level, ':', $message, '|'; return true; });
$xml->item = 'wrong';
$xml->attributes()->addChild('blocked');
$xml->missing->addChild('blocked');
$xml->missing->addAttribute('blocked', 'v');
$xml->addAttribute('bare', 'v', 'urn:x');
$xml->addAttribute('id', '2');
$xml->item[5] = 'c';
echo count($xml->item), ':', (string)$xml->item[2], '|';
restore_error_handler();
try { $xml->addChild(''); } catch (ValueError $error) { echo $error->getMessage(), '|'; }
try { $xml->addAttribute('', 'v'); } catch (ValueError $error) { echo $error->getMessage(), '|'; }
set_error_handler(function(int $level, string $message): bool { throw new RuntimeException('stopped'); });
try { $xml->item[9] = 'not-added'; } catch (RuntimeException $error) { echo $error->getMessage(), ':', count($xml->item); }
restore_error_handler();
"###);
    assert_eq!(out, concat!(
        "2:main(): Cannot assign to an array of nodes (duplicate subnodes or attr detected)|",
        "2:SimpleXMLElement::addChild(): Cannot add element to attributes|",
        "2:SimpleXMLElement::addChild(): Cannot add child. Parent is not a permanent member of the XML tree|",
        "2:SimpleXMLElement::addAttribute(): Unable to locate parent Element|",
        "2:SimpleXMLElement::addAttribute(): Attribute requires prefix for namespace|",
        "2:SimpleXMLElement::addAttribute(): Attribute already exists|",
        "2:main(): Cannot add element item number 5 when only 2 such elements exist|3:c|",
        "SimpleXMLElement::addChild(): Argument #1 ($qualifiedName) must not be empty|",
        "SimpleXMLElement::addAttribute(): Argument #1 ($qualifiedName) must not be empty|stopped:3",
    ));
}

/// Ordinary eval ArrayAccess empty keeps a fresh key through both offsetExists and offsetGet.
#[test]
fn test_eval_arrayaccess_probe_key_owners() {
    let out = compile_and_run_with_heap_debug(r###"<?php
class ProbeIndexLeaseArray implements ArrayAccess {
    public function offsetExists(mixed $offset): bool { return strlen((string)$offset) > 0; }
    public function offsetGet(mixed $offset): mixed { return substr((string)$offset, 0, 1); }
    public function offsetSet(mixed $offset, mixed $value): void {}
    public function offsetUnset(mixed $offset): void {}
}
$object = new ProbeIndexLeaseArray();
$source = $argc > 0 ? '
$checks = 0;
for ($i = 0; $i < 32; $i++) {
    $checks += (int)isset($object["key" . str_repeat("x", 40)]);
    $checks += (int)empty($object["0key" . str_repeat("x", 40)]);
}
echo $checks;
' : '';
eval($source);
"###);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "64");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Review regressions share one catalog compile and independent AOT/eval process heaps.
#[test]
fn test_simplexml_review_names_and_filter_close_bundle() {
    if skip_without_xml_native("test_simplexml_review_names_and_filter_close_bundle") { return; }
    let names = r####"
$xml = new SimpleXMLElement('<r xmlns:p="urn:p" at="base"/>');
$xml->{'p:q'} = 'value';
$xml['p:at'] = 'literal';
$xml->{'a:b'}->c = 'nested';
$xml->{'p:q'}[] = 'second';
echo $xml->asXML();
echo (int)isset($xml->{'p:q'}), ':', (string)$xml->{'p:q'}[0], ':', (string)$xml->{'p:q'}[1], ':', count($xml->children('urn:p')), '|';
echo (string)$xml['at'], ':', (string)$xml['p:at'], ':', $xml->{'a:b'}->getName(), '|';
$added = $xml->addChild('p:method', 'qualified', 'urn:p');
echo $added->getName(), ':', (string)$xml->children('urn:p')->method, '|';
$leading = $xml->addChild(':a', 'leading');
echo $leading->getName(), '|';
set_error_handler(function(int $level, string $message): bool { echo $message, '|'; return true; });
$xml->addAttribute(':b', '1', 'urn:q');
$xml->addAttribute(':c', '1');
$xml->addAttribute(':c', '2');
restore_error_handler();
echo $xml->asXML();
unset($added, $leading, $xml);
"####;
    let filters = r####"
$uri = 'php://filter/read=xml.review.throw/resource=data://text/plain,%3Cr%2F%3E';
try {
    if ($constructor) { new SimpleXMLElement($uri, 0, true); }
    else { simplexml_load_file($uri); }
} catch (Throwable $error) {
    echo get_class($error), ':', $error->getMessage(), '|';
    if ($constructor) { echo get_class($error->getPrevious()), '|'; }
}
unset($error, $uri);
"####;
    let semantics = r####"
$prefixes = new SimpleXMLElement('<r xmlns:p="urn:a" xmlns:q="urn:b"><p:c>A</p:c><q:c>B</q:c></r>');
$prefixes->registerXPathNamespace('p', 'urn:b');
$prefixes->registerXPathNamespace('alias', 'urn:b');
echo (string)$prefixes->xpath('//p:c')[0], ':', (string)$prefixes->xpath('//alias:c')[0], '|';
$declarationsXml = new SimpleXMLElement('<r xmlns:a="urn:a"><c xmlns:b="urn:b"/><empty xmlns=""/></r>');
echo json_encode($declarationsXml->getDocNamespaces(true)), '|';
echo json_encode($declarationsXml->c->getDocNamespaces(false, false)), '|';
echo json_encode($declarationsXml->children()->getDocNamespaces(false, false)), '|';
echo json_encode($declarationsXml->missing->getDocNamespaces(false, false)), '|';
echo json_encode($declarationsXml->c[0]->getDocNamespaces(false, false)), '|';
echo json_encode($declarationsXml->getNamespaces(true)), '|';
foreach (['MissingXmlReviewClass', 'stdClass'] as $className) {
    try { simplexml_load_string('<r/>', $className); }
    catch (TypeError $error) { echo $error->getMessage(), '|'; }
    try { simplexml_load_file('no-such-review.xml', $className); }
    catch (TypeError $error) { echo $error->getMessage(), '|'; }
    try { simplexml_import_dom(new DOMDocument(), $className); }
    catch (TypeError $error) { echo $error->getMessage(), '|'; }
}
$zero = new SimpleXMLElement('<r>0.0</r>');
$negativeZero = new ReviewNumericZeroXml('<r>-0.0</r>');
var_dump(-$zero, reviewNegateXmlMixed($zero), -$negativeZero, reviewNegateXmlSubclass($negativeZero));
$maybeZero = $argc > 0 ? $zero : null;
var_dump(-$maybeZero);
$document = new SimpleXMLElement('<r><a><b>original</b></a></r>');
$node = $document->a[0]; $copied = clone $node;
echo count($copied->xpath('..')), '|';
$copied->b = 'copy'; echo (string)$document->a->b, ':', (string)$copied->b, '|';
$group = clone $document->a; echo count($group->xpath('..')), '|';
unset($node, $group, $document);
echo $copied->asXML(), '|';
$reads = new SimpleXMLElement('<r known="yes"><a/></r>');
var_dump($reads->missing->child, $reads->a->c->d, $reads->attributes()->q);
$dynamicName = 'child'; var_dump($reads->missing->{$dynamicName});
echo (int)is_null($reads->missing->child), ':', (int)is_object($reads->a->children()->foo), '|';
$reads->created->child = 'written'; echo (string)$reads->created->child, '|';
$reads->chain->middle->leaf = 'deep'; echo (string)$reads->chain->middle->leaf, '|';
$writes = new SimpleXMLElement('<r/>');
$writes->one->two->three->leaf = 'four'; echo (string)$writes->one->two->three->leaf, '|';
$writes->dimension->child['at'] = 'attribute'; echo (string)$writes->dimension->child['at'], '|';
$writes->appended->child[] = 'tail'; echo (string)$writes->appended->child, '|';
$writeName = 'middle'; $writes->runtime->{$writeName}->leaf = 'dynamic';
echo (string)$writes->runtime->middle->leaf, '|';
reviewWriteNestedXml($writes, $writeName);
echo (string)$writes->opaque->middle->leaf, '|';
$writes->sum->deep->leaf += 1; echo $writes->sum->asXML(), '|';
echo (int)isset($writes->sum->deep->leaf), '|';
$writes->increment->deep->leaf++; echo (int)isset($writes->increment->deep->leaf), '|';
try { $writes->throwing->{reviewThrowWriteName()}->leaf = 'unused'; }
catch (RuntimeException $writeError) { echo $writeError->getMessage(), ':', (int)isset($writes->throwing), '|'; }
unset($writes, $writeName, $writeError);
$emptyAttributes = new SimpleXMLElement('<r/>');
var_dump($emptyAttributes->attributes()->q, $reads->missing->child?->getName());
for ($iteration = 0; $iteration < 32; $iteration++) {
    $temporaryClone = clone $reads->a[0];
    $temporaryClone->newChild = 'value';
}
unset($temporaryClone, $reads, $emptyAttributes, $dynamicName, $iteration);
unset($prefixes, $declarationsXml, $className, $error, $zero, $negativeZero, $maybeZero, $copied);
"####;
    let update_expressions = r####"
$counter = new SimpleXMLElement('<r><c>5</c></r>');
var_dump(++$counter->c, ++$counter->c === 7);
$previousCounter = $counter->c++;
var_dump(get_class($previousCounter), (string)$previousCounter, (string)$counter->c);
var_dump($counter->c += 2, --$counter->c);
$missingCounter = new SimpleXMLElement('<r/>');
var_dump($missingCounter->c += 1, ++$missingCounter->d);
echo $missingCounter->asXML(), '|';
$throwingUpdate = new SimpleXMLElement('<r/>');
try { $throwingUpdate->absent->leaf += throw new RuntimeException('rhs-throw'); }
catch (RuntimeException $updateError) { echo $updateError->getMessage(), ':', (int)isset($throwingUpdate->absent), '|'; }
unset($counter, $previousCounter, $missingCounter, $throwingUpdate, $updateError);
"####;
    let updates = r####"
set_error_handler(function(int $level, string $message): bool { throw new RuntimeException($message); });
$updates = new SimpleXMLElement('<r/>');
$updates->a->b->c += 1;
$updates->d->e->f++;
$updates->text .= 'suffix';
echo $updates->asXML(), '|';
$existingUpdate = new SimpleXMLElement('<r><c/></r>');
$existingUpdate->c += 1; echo $existingUpdate->asXML(), '|';
$attributeUpdate = new SimpleXMLElement('<r a="5"/>');
$attributeUpdate->attributes()->a += 1;
$attributeUpdate->attributes()->b += 1;
echo $attributeUpdate->asXML(), '|';
$liveUpdate = new SimpleXMLElement('<r/>');
$liveUpdate->a->b->c += reviewCreateUpdateNode();
echo (string)$liveUpdate->a->b->c, '|';
$offsetUpdate = new SimpleXMLElement('<r/>');
$offsetUpdate->a[0]->b = 1;
$offsetUpdate->a[1]->b = 2;
echo $offsetUpdate->asXML(), '|';
$hookUpdate = new ReviewHookOuter();
reviewWriteHookLabel($hookUpdate); echo $hookUpdate->child->label, '|';
var_dump(reviewBumpHookCount($hookUpdate));
$hookUpdate->child->label = 'direct'; echo $hookUpdate->child->label, '|';
$hookUpdate->count += 1; echo $hookUpdate->count, '|';
$lateName = 'old'; $lateUpdate = new SimpleXMLElement('<r/>');
$lateUpdate->{$lateName} += reviewChangeWriteName();
echo $lateUpdate->asXML(), '|';
$lateName = 'old'; $lateUpdate = new SimpleXMLElement('<r/>');
$lateUpdate->{$lateName}->leaf = reviewChangeWriteName();
echo $lateUpdate->asXML(), '|';
$nameObject = new ReviewWriteName();
$lateUpdate->{$nameObject} = reviewChangeWriteName();
echo $lateUpdate->asXML(), '|';
unset($lateName, $lateUpdate, $nameObject);
__EXPRESSION_UPDATES__
restore_error_handler();
set_error_handler(function(int $level, string $message): bool { echo $level, ':', $message, '|'; return true; });
$badOffset = new SimpleXMLElement('<r/>');
try { $badOffset->a[1]->b = 1; } catch (Error $offsetError) { echo $offsetError->getMessage(), '|'; }
echo $badOffset->asXML(), '|';
$appendedOffset = new SimpleXMLElement('<r><a/></r>');
$appendedOffset->a[3]->b = 3;
echo $appendedOffset->asXML(), '|';
restore_error_handler();
unset($updates, $existingUpdate, $attributeUpdate, $liveUpdate, $offsetUpdate, $hookUpdate, $badOffset, $appendedOffset, $offsetError);
"####;
    let native_updates = updates.replace("__EXPRESSION_UPDATES__", update_expressions);
    let runtime_updates = updates.replace("__EXPRESSION_UPDATES__", "");
    let declarations = r####"<?php
class ReviewNumericZeroXml extends SimpleXMLElement {}
function reviewWriteNestedXml(mixed $node, string $name): void { $node->opaque->{$name}->leaf = 'boxed'; }
function reviewThrowWriteName(): string { throw new RuntimeException('write-name'); }
function reviewCreateUpdateNode(): int {
    global $liveUpdate;
    echo (int)isset($liveUpdate->a->b->c), '|';
    $liveUpdate->a->b->c = 7;
    return 1;
}
class ReviewWriteName {
    public function __toString(): string { echo 'name|'; return 'converted'; }
}
function reviewChangeWriteName(): int {
    global $lateName;
    echo 'rhs|'; $lateName = 'new'; return 1;
}
class ReviewHookInner { public string $label = ''; }
class ReviewHookOuter {
    private ReviewHookInner $inner;
    public ReviewHookInner $child { get { echo 'get|'; return $this->inner; } }
    public int $count = 1 { get { echo 'count|'; return $this->count * 10; } }
    public function __construct() { $this->inner = new ReviewHookInner(); }
}
function reviewWriteHookLabel(mixed $node): void { $node->child->label = 'set'; }
function reviewBumpHookCount(mixed $node): int { return $node->count += 1; }
function reviewNegateXmlMixed(mixed $value): mixed { return -$value; }
function reviewNegateXmlSubclass(ReviewNumericZeroXml $value): mixed { return -$value; }
class XmlReviewOtherFilter extends php_user_filter {
    public function onClose(): void { echo 'other-close|'; }
}
class XmlReviewThrowFilter extends php_user_filter {
    public function onCreate(): bool { echo 'create|'; return true; }
    public function filter($in, $out, &$consumed, bool $closing): int { throw new RuntimeException('filter'); }
    public function onClose(): void { echo 'unexpected-close|'; }
    public function __destruct() {
        $other = fopen('php://temp', 'w+');
        stream_filter_append($other, 'xml.review.other', STREAM_FILTER_READ);
        fclose($other);
        echo 'drop|';
    }
}
stream_filter_register('xml.review.other', 'XmlReviewOtherFilter');
stream_filter_register('xml.review.throw', 'XmlReviewThrowFilter');
"####;
    let eval_names = names.replace('\\', "\\\\").replace('\'', "\\'");
    let eval_filters = filters.replace('\\', "\\\\").replace('\'', "\\'");
    let eval_semantics = semantics.replace('\\', "\\\\").replace('\'', "\\'");
    let eval_updates = runtime_updates.replace('\\', "\\\\").replace('\'', "\\'");
    let source = format!("{declarations}
        if ($argc === 1) {{ {names} }}
        elseif ($argc === 2) {{ $source = $argc > 0 ? '{eval_names}' : ''; eval($source); }}
        elseif ($argc === 7) {{ {semantics} }}
        elseif ($argc === 8) {{ $source = $argc > 0 ? '{eval_semantics}' : ''; eval($source); }}
        elseif ($argc === 9) {{ {native_updates} }}
        elseif ($argc === 10) {{ $source = $argc > 0 ? '{eval_updates}' : ''; eval($source); }}
        else {{
            set_error_handler(function(int $level, string $message): bool {{ return true; }});
            $constructor = $argc === 4 || $argc === 6;
            if ($argc <= 4) {{ {filters} }}
            else {{ $source = $argc > 0 ? '{eval_filters}' : ''; eval($source); }}
            restore_error_handler();
        }}");
    let args: Vec<_> = (0..10).map(|count| vec!["x"; count]).collect();
    let argvs: Vec<_> = args.iter().map(Vec::as_slice).collect();
    let runs = compile_and_run_with_heap_debug_per_argv(&source, &argvs);
    let names_expected = "<?xml version=\"1.0\"?>\n<r xmlns:p=\"urn:p\" at=\"base\" p:at=\"literal\"><p:q>value</p:q><a:b><c>nested</c></a:b><p:q>second</p:q></r>\n1:value:second:0|base:literal:a:b|method:qualified|:a|SimpleXMLElement::addAttribute(): Attribute requires prefix for namespace|SimpleXMLElement::addAttribute(): Attribute already exists|<?xml version=\"1.0\"?>\n<r xmlns:p=\"urn:p\" at=\"base\" p:at=\"literal\" :c=\"1\"><p:q>value</p:q><a:b><c>nested</c></a:b><p:q>second</p:q><p:method>qualified</p:method><:a>leading</:a></r>\n";
    let factory = "create|other-close|drop|RuntimeException:filter|";
    let constructor = "create|other-close|drop|Exception:String could not be parsed as XML|RuntimeException|";
    let semantics_expected = "A:B|{\"a\":\"urn:a\",\"b\":\"urn:b\",\"\":\"\"}|{\"a\":\"urn:a\"}|{\"a\":\"urn:a\"}|{\"a\":\"urn:a\"}|{\"b\":\"urn:b\"}|[]|simplexml_load_string(): Argument #2 ($class_name) must be a class name derived from SimpleXMLElement or null, MissingXmlReviewClass given|simplexml_load_file(): Argument #2 ($class_name) must be a class name derived from SimpleXMLElement or null, MissingXmlReviewClass given|simplexml_import_dom(): Argument #2 ($class_name) must be a class name derived from SimpleXMLElement or null, MissingXmlReviewClass given|simplexml_load_string(): Argument #2 ($class_name) must be a class name derived from SimpleXMLElement or null, stdClass given|simplexml_load_file(): Argument #2 ($class_name) must be a class name derived from SimpleXMLElement or null, stdClass given|simplexml_import_dom(): Argument #2 ($class_name) must be a class name derived from SimpleXMLElement or null, stdClass given|float(-0)\nfloat(-0)\nfloat(0)\nfloat(0)\nfloat(-0)\n0|original:copy|1|<a><b>copy</b></a>|NULL\nNULL\nNULL\nNULL\n1:1|written|deep|four|attribute|tail|dynamic|boxed|<sum><deep><leaf/></deep></sum>|1|1|write-name:0|NULL\nNULL\n";
    let updates_expected = "<?xml version=\"1.0\"?>\n<r><a><b><c/></b></a><d><e><f/></e></d><text/></r>\n|<?xml version=\"1.0\"?>\n<r><c>1</c></r>\n|<?xml version=\"1.0\"?>\n<r a=\"5\" b=\"\"/>\n|0|8|<?xml version=\"1.0\"?>\n<r><a><b>1</b></a><a><b>2</b></a></r>\n|get|get|set|count|int(11)\nget|get|direct|count|count|1110|rhs|<?xml version=\"1.0\"?>\n<r><new/></r>\n|rhs|<?xml version=\"1.0\"?>\n<r><new><leaf>1</leaf></new></r>\n|rhs|name|<?xml version=\"1.0\"?>\n<r><new><leaf>1</leaf></new><converted>1</converted></r>\n|int(6)\nbool(true)\nstring(16) \"SimpleXMLElement\"\nstring(1) \"8\"\nstring(1) \"8\"\nint(10)\nint(9)\nint(1)\nint(1)\n<?xml version=\"1.0\"?>\n<r><c/><d/></r>\n|rhs-throw:0|2:main(): Cannot add element a number 1 when only 0 such elements exist|8:Indirect modification of overloaded element of SimpleXMLElement has no effect|Attempt to assign property \"b\" on null|<?xml version=\"1.0\"?>\n<r><a/></r>\n|2:main(): Cannot add element a number 3 when only 1 such elements exist|<?xml version=\"1.0\"?>\n<r><a/><a><b>3</b></a></r>\n|";
    let eval_updates_expected = "<?xml version=\"1.0\"?>\n<r><a><b><c/></b></a><d><e><f/></e></d><text/></r>\n|<?xml version=\"1.0\"?>\n<r><c>1</c></r>\n|<?xml version=\"1.0\"?>\n<r a=\"5\" b=\"\"/>\n|0|8|<?xml version=\"1.0\"?>\n<r><a><b>1</b></a><a><b>2</b></a></r>\n|get|get|set|count|int(11)\nget|get|direct|count|count|1110|rhs|<?xml version=\"1.0\"?>\n<r><new/></r>\n|rhs|<?xml version=\"1.0\"?>\n<r><new><leaf>1</leaf></new></r>\n|rhs|name|<?xml version=\"1.0\"?>\n<r><new><leaf>1</leaf></new><converted>1</converted></r>\n|2:Unknown: Cannot add element a number 1 when only 0 such elements exist|8:Indirect modification of overloaded element of SimpleXMLElement has no effect|Attempt to assign property \"b\" on null|<?xml version=\"1.0\"?>\n<r><a/></r>\n|2:Unknown: Cannot add element a number 3 when only 1 such elements exist|<?xml version=\"1.0\"?>\n<r><a/><a><b>3</b></a></r>\n|";
    let expected = [names_expected, names_expected, factory, constructor, factory, constructor, semantics_expected, semantics_expected, updates_expected, eval_updates_expected];
    assert_eq!(runs.len(), expected.len());
    for (mode, (out, expected)) in runs.iter().zip(expected).enumerate() {
        assert!(out.success, "mode={mode} stdout={:?} stderr={}", out.stdout, out.stderr);
        assert_eq!(out.stdout, expected, "mode={mode}: {}", out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "mode={mode}: {}", out.stderr);
        assert!(!out.stderr.contains("Warning:"), "mode={mode}: {}", out.stderr);
    }
}
