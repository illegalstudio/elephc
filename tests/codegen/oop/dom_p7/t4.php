<?php
// saveHTML of a built tree: HTML 4 rules (void elements, boolean attributes, URI escaping, entities, raw script), node form, formatOutput, saveHTMLFile.
$d = new DOMDocument();
$html = $d->appendChild($d->createElement('html'));
$body = $html->appendChild($d->createElement('body'));
$p = $body->appendChild($d->createElement('p'));
$p->setAttribute('class', 'a b');
$p->setAttribute('data-x', '1');
$p->setAttribute('href', 'a b');
$p->appendChild(new DOMText('x'));
$p->appendChild($d->createElement('br'));
$p->appendChild(new DOMText('& < >'));
$p->appendChild($d->createElement('b', 't'));
$body->appendChild(new DOMComment('c'));
$script = $body->appendChild($d->createElement('script'));
$script->appendChild(new DOMText('if (a<b) {}'));
$div = $body->appendChild($d->createElement('div'));
$input = $div->appendChild($d->createElement('input'));
$input->setAttribute('disabled', 'disabled');
$input->setAttribute('name', 'n');
$a = $div->appendChild($d->createElement('a', 'l'));
$a->setAttribute('href', 'x y?z=1&w=2');
$img = $div->appendChild($d->createElement('img'));
$img->setAttribute('src', 'a.png');
$img->setAttribute('alt', '');
$q = $body->appendChild($d->createElement('p'));
$q->appendChild(new DOMText('é & <'));
$q->setAttribute('href', 'é y');
echo $d->saveHTML();
echo $d->saveHTML($p), "|\n";
echo $d->saveHTML($div), "|\n";
$d->formatOutput = true;
echo $d->saveHTML();
echo $d->saveHTMLFile('/tmp/elephc_p7_t4.html'), "\n";
echo file_get_contents('/tmp/elephc_p7_t4.html');
unlink('/tmp/elephc_p7_t4.html');
if ($argc > 99) { simplexml_import_dom($d); }
