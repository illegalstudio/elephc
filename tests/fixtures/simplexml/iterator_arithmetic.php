<?php
class LineageXmlElement extends SimpleXMLElement {}
class LineageXmlIterator extends SimpleXMLIterator {}
function negateLineageXml(SimpleXMLElement|SimpleXMLIterator $node): int|float {
    return -$node;
}
function incrementLineageXml(SimpleXMLElement|SimpleXMLIterator $node): int|float {
    return ++$node;
}
$iterator = new SimpleXMLIterator('<r>2</r>');
$element = simplexml_load_string('<r>3</r>');
var_dump($iterator + $element, $element * $iterator, $iterator - $element,
    $element / $iterator, $element % $iterator, $element ** $iterator);
$derivedIterator = new LineageXmlIterator('<r>4</r>');
$derivedElement = new LineageXmlElement('<r>5</r>');
var_dump($derivedIterator + $element, $derivedElement + $iterator);
$updated = new LineageXmlIterator('<r>2</r>');
$updated += $derivedElement;
var_dump($updated, negateLineageXml($iterator), negateLineageXml($derivedElement),
    incrementLineageXml($iterator), incrementLineageXml($derivedElement));
