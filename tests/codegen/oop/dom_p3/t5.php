<?php
// elephc-only: php cannot build a DOMNamedNodeMap from nodes or give a fragment an owner by hand.
$x = new DOMAttr('x', '1');
$y = new DOMAttr('p:y', '2');
$y->__elephc_dom_ns = 'urn:q';
$m = new DOMNamedNodeMap(['x' => $x, 'p:y' => $y]);
echo $m->length, '|', count($m), '|', $m->item(1)->value, '|', var_export($m->item(2), true), '|', var_export($m->item(-1), true), "\n";
echo $m->getNamedItem('p:y')->value, '|', var_export($m->getNamedItem('zz'), true), '|', $m->getNamedItem('x') === $x ? 'same' : 'other', "\n";
echo $m->getNamedItemNS(null, 'x')->value, '|', $m->getNamedItemNS('urn:q', 'y')->value, '|', var_export($m->getNamedItemNS(null, 'y'), true), '|', var_export($m->getNamedItemNS('urn:q', 'x'), true), "\n";
foreach ($m as $k => $v) {
    echo $k, '=', $v->value, ',';
}
echo "\n";

$t = new DOMDocumentType('r', 'pub', 'sys');
echo $t->name, '|', $t->nodeName, '|', $t->publicId, '|', $t->systemId, '|', var_export($t->internalSubset, true), '|', $t->entities->length, '|', $t->notations->length, "\n";

$d = new DOMDocument();
$f = new DOMDocumentFragment();
echo $f->nodeName, '|', $f->childNodes->length, "\n";
try {
    $f->appendXML('<a/>');
} catch (DOMException $e) {
    echo $e->getMessage(), '|', $e->getCode(), "\n";
}
$f->ownerDocument = $d;
var_dump($f->appendXML(''));
var_dump($f->appendXML('<a>x</a>t<!--c-->'));
echo $f->childNodes->length, '|', $f->childNodes->item(0)->nodeName, '|', $f->childNodes->item(1)->nodeValue, '|', $f->childNodes->item(2)->nodeName, "\n";
var_dump(@$f->appendXML('<a>'));
echo $f->childNodes->length, "\n";
