<?php
$d = new DOMDocument();
$e = $d->createElement('r');
$t = new DOMText('abcdef');
$e->appendChild($t);
$e->appendChild($d->createElement('b'));
$s = $t->splitText(2);
echo get_class($s), "|", $t->data, "|", $s->data, "|", $e->childNodes->length, "\n";
echo $e->childNodes->item(0)->data, "|", $e->childNodes->item(1)->data, "|", $e->childNodes->item(2)->nodeName, "\n";
echo $t->wholeText, "|", $s->wholeText, "\n";
var_dump($t->splitText(99));
try { $t->splitText(-1); } catch (ValueError $ex) { echo $ex->getMessage(), "\n"; }
$last = $s->splitText(4);
echo $e->childNodes->length, "|", var_export($last->data, true), "|", $t->wholeText, "\n";
$n = new DOMText("loose");
$p = $n->splitText(2);
echo $n->data, "|", $p->data, "\n";
$c = new DOMCdataSection("abcd");
$q = $c->splitText(1);
echo get_class($q), "|", $c->data, "|", $q->data, "\n";
$u = new DOMText("héé");
$w = $u->splitText(1);
echo $u->data, "|", $w->data, "|", $w->length, "\n";
