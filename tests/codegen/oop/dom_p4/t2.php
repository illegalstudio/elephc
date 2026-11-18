<?php
function names($n) { $o = []; foreach ($n->childNodes as $k) { $o[] = $k->nodeName; } return implode(',', $o); }
$d = new DOMDocument();
$d->loadXML('<r xmlns="http://u" xmlns:p="http://v"><a id="1">x</a><p:b/>tail<c><d/></c></r>');
$r = $d->documentElement;
echo names($r), "\n";
$a = $r->firstChild; $c = $r->lastChild;
$r->insertBefore($c, $a);
echo names($r), ' ', $r->firstChild === $c ? 'same' : 'diff', ' ', $c->parentNode === $r ? 'p' : 'np', "\n";
$moved = $r->removeChild($a);
echo names($r), ' ', var_export($a->parentNode, true), ' ', $moved === $a ? 'same' : 'diff', "\n";
$r->appendChild($a);
echo names($r), ' ', $a->getAttribute('id'), ' ', $a->textContent, "\n";
$new = $d->createElement('n');
$new->appendChild(new DOMText('hi'));
$r->insertBefore($new, $a);
echo names($r), ' ', $new->parentNode === $r ? 'p' : 'np', ' ', $new->firstChild->nodeValue, ' ', $new->nextSibling === $a ? 'ns' : 'nns', "\n";
$old = $r->replaceChild($d->createElement('w'), $new);
echo names($r), ' ', $old === $new ? 'same' : 'diff', ' ', var_export($new->parentNode, true), "\n";
$tail = $r->childNodes->item(2);
$tail->nodeValue = 'abcdef';
$s = $tail->splitText(2);
echo $tail->nodeValue, '|', $s->nodeValue, '|', $tail->nextSibling === $s ? 'linked' : 'nolink', '|', $r->childNodes->length, "\n";
$s2 = $s->splitText(1);
echo $s->nodeValue, '|', $s2->nodeValue, '|', $s->nextSibling === $s2 ? 'linked' : 'nolink', '|', $r->childNodes->length, "\n";
$r->normalize();
echo names($r), ' ', $r->childNodes->length, "\n";
echo $r->namespaceURI, ' ', var_export($r->prefix, true), ' ', $r->localName, "\n";
$pb = $r->childNodes->item(array_search('p:b', explode(',', names($r))));
echo $pb->namespaceURI, ' ', $pb->prefix, ' ', $pb->localName, ' ', $pb->nodeName, "\n";
var_dump($r->lookupNamespaceURI(null), $r->lookupNamespaceURI('p'), $r->lookupNamespaceURI('q'), $r->lookupPrefix('http://v'), $r->lookupPrefix('http://u'), $r->isDefaultNamespace('http://u'), $c->lookupNamespaceURI('p'), $d->lookupNamespaceURI('p'));
echo $r->getLineNo() > 0 ? 'line' : 'noline', "\n";
echo $r->getNodePath(), ' ', $c->getNodePath(), ' ', $c->firstChild->getNodePath(), "\n";
$cl = $c->cloneNode(true);
echo $cl->nodeName, $cl->childNodes->length, ' ', var_export($cl->parentNode, true), ' ', $cl->ownerDocument === $d ? 'od' : 'nod', "\n";
$r->appendChild($cl);
echo names($r), "\n";
$frag = new DOMDocumentFragment();
$frag->appendChild($d->createElement('f1')); $frag->appendChild($d->createElement('f2'));
$r->insertBefore($frag, $c);
echo names($r), "\n";
$d2 = new DOMDocument(); $d2->loadXML('<q/>');
try { $r->appendChild($d2->documentElement); } catch (DOMException $e) { echo $e->getCode(), $e->getMessage(), "\n"; }
try { $r->removeChild($d2->documentElement); } catch (DOMException $e) { echo $e->getCode(), $e->getMessage(), "\n"; }
echo $d->saveXML();
