<?php
$d = new DOMDocument();
$d->loadXML('<r>héllo<b/>tail</r>');
$r = $d->documentElement;
$t = $r->childNodes->item(0);
$s = $t->splitText(2);
echo get_class($s), "|", $t->data, "|", $s->data, "|", $r->childNodes->length, "\n";
echo $r->childNodes->item(1)->data, "|", $r->childNodes->item(2)->nodeName, "|", $r->childNodes->item(3)->data, "\n";
echo $t->wholeText, "|", $s->wholeText, "\n";
var_dump($t->splitText(99));
try { $t->splitText(-1); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
echo $d->saveXML($r), "\n";
$last = $r->childNodes->item(3);
$e = $last->splitText(4);
echo $r->childNodes->length, "|", var_export($e->data, true), "\n";
$n = new DOMText("loose");
$p = $n->splitText(2);
echo $n->data, "|", $p->data, "\n";
$c = new DOMCdataSection("abcd");
$q = $c->splitText(1);
echo get_class($q), "|", $c->data, "|", $q->data, "\n";
