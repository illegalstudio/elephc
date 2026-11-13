<?php
class EvalLineageXmlElement extends SimpleXMLElement {}
class EvalLineageXmlIterator extends SimpleXMLIterator {}
function negateEvalLineageXml(SimpleXMLElement|SimpleXMLIterator $node): int|float {
    return -$node;
}
$iterator = new SimpleXMLIterator('<r>2</r>');
$element = simplexml_load_string('<r>3</r>');
var_dump($iterator + $element, $element * $iterator, $iterator - $element,
    $element / $iterator, $element % $iterator, $element ** $iterator);
$derivedIterator = new EvalLineageXmlIterator('<r>4</r>');
$derivedElement = new EvalLineageXmlElement('<r>5</r>');
var_dump($derivedIterator + $element, $derivedElement + $iterator);
function evalLineageXmlUpdateSource(): mixed { return new EvalLineageXmlIterator('<r>2</r>'); }
$updated = evalLineageXmlUpdateSource();
$updated += $derivedElement;
var_dump($updated, negateEvalLineageXml($iterator), negateEvalLineageXml($derivedElement));
