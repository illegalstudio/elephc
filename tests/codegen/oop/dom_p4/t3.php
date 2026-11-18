<?php
$d = new DOMDocument();
$p = $d->createElement('p');
$c1 = $d->createElement('c1');
$c2 = $d->createElement('c2');
$p->appendChild($c1);
$p->appendChild($c2);
$c1->appendChild($d->createElement('deep'));
$text = new DOMText('abcd');
$p->appendChild($text);
$tail = $text->splitText(2);
echo $p->childNodes->length, $text->nodeValue, $tail->nodeValue, $text->nextSibling === $tail ? 'L' : 'N', $tail->previousSibling === $text ? 'L' : 'N', "\n";
$weak = [WeakReference::create($p), WeakReference::create($c1), WeakReference::create($c2), WeakReference::create($text)];
unset($p, $c1, $c2, $text, $tail);
foreach ($weak as $w) {
    echo $w->get() === null ? 'freed ' : 'alive ';
}
echo "\n";
$q = $d->createElement('q');
$k = $d->createElement('k');
$q->appendChild($k);
unset($q);
echo $k->parentNode === null ? 'orphan-null' : 'orphan-kept', "\n";
