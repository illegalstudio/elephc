<?php
// textContent / nodeValue assigned to a free-standing node reach the tree.
$d = new DOMDocument();
$e = $d->appendChild($d->createElement('item', 'hello'));
$e->appendChild($d->createElement('sub', ' world'));
echo $e->textContent, '|', $e->nodeValue, "\n";
$e->textContent = 'replaced';
echo $d->saveXML($e), $e->childNodes->length, "\n";
$e->nodeValue = 'again';
echo $d->saveXML($e), "\n";
$e->textContent = '';
echo $d->saveXML($e), $e->childNodes->length, "\n";
$e->textContent = 'a<b>&';
echo $d->saveXML($e), "\n";
$t = new DOMText('txt');
$t->textContent = 'viaText';
echo $t->nodeValue, "\n";
