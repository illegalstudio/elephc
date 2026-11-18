<?php
// A free-standing tree: header, escaping, empty elements, node kinds, formatOutput, options, node form.
$d = new DOMDocument();
echo $d->saveXML(), "|\n";
$r = $d->appendChild($d->createElement('r'));
echo $d->saveXML();
$r->appendChild($d->createElement('empty'));
$r->appendChild($d->createElement('t', 'plain'));
$r->appendChild(new DOMText('a<b>&"c\''));
$r->appendChild(new DOMText("x\ry"));
$r->appendChild(new DOMComment(' c '));
$r->appendChild(new DOMCdataSection('1 < 2'));
$r->appendChild(new DOMProcessingInstruction('pi', 'v'));
$r->appendChild($d->createElement('u', 'é日'));
echo $d->saveXML();
echo $d->saveXML($r->childNodes->item(1)), "\n";
echo $d->saveXML($r->childNodes->item(2)), "\n";
echo $d->saveXML($r->lastChild), "\n";
echo $d->saveXML($r, LIBXML_NOEMPTYTAG), "\n";
echo $d->saveXML(null, LIBXML_NOXMLDECL);
$utf = new DOMDocument('1.0', 'UTF-8');
$utf->appendChild($utf->createElement('é', 'é日'));
echo $utf->saveXML();
$latin = new DOMDocument('1.0', 'ISO-8859-1');
$latin->appendChild($latin->createElement('l', 'é'));
echo bin2hex($latin->saveXML()), "\n";
$d->formatOutput = true;
echo $d->saveXML();
echo $d->saveXML($r), "\n";
$f = new DOMDocument();
$f->formatOutput = true;
$f->appendChild($root = $f->createElement('root'));
$root->appendChild($a = $f->createElement('a'));
$a->appendChild($f->createElement('b'));
$a->appendChild($c = $f->createElement('c'));
$c->appendChild($f->createElement('d', 'text'));
$m = $root->appendChild($f->createElement('mixed'));
$m->appendChild(new DOMText('t'));
$m->appendChild($f->createElement('inner'));
$root->appendChild(new DOMComment('trailing'));
$root->setAttribute('k', "v\"<&>\n\t");
$root->setAttribute('é', 'é');
echo $f->saveXML();
echo $f->saveXML($a), "\n";
$f->formatOutput = false;
echo $f->saveXML();
echo $f->saveXML($f), "\n";
echo $f->save('/tmp/elephc_p7_t1.xml'), "\n";
echo file_get_contents('/tmp/elephc_p7_t1.xml');
unlink('/tmp/elephc_p7_t1.xml');
try {
    $other = new DOMDocument();
    echo $f->saveXML($other->createElement('x'));
} catch (DOMException $e) {
    echo $e->getMessage(), ' ', $e->getCode(), "\n";
}
