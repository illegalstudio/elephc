<?php
class ReviewNumericZeroXml extends SimpleXMLElement {}
function reviewWriteNestedXml(mixed $node, string $name): void { $node->opaque->{$name}->leaf = 'boxed'; }
function reviewThrowWriteName(): string { throw new RuntimeException('write-name'); }
function reviewNegateXmlMixed(mixed $value): mixed { return -$value; }
function reviewNegateXmlSubclass(ReviewNumericZeroXml $value): mixed { return -$value; }
// REVIEW_BODY

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

