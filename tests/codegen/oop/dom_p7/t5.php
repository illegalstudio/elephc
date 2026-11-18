<?php
// saveHTML of a parsed document (needs the libxml2 HTML4 parser package P6 for the tree itself).
$h = new DOMDocument();
$h->loadHTML('<p class="a b" data-x=1 href="a b">x<br>&eacute; &amp; <b>t</b></p><!--c--><script>if (a<b) {}</script>', LIBXML_HTML_NODEFDTD);
echo $h->saveHTML();
echo $h->saveHTML($h->getElementsByTagName('p')->item(0)), "|\n";
echo $h->saveHTML($h->getElementsByTagName('b')->item(0)), "|\n";
$h->formatOutput = true;
echo $h->saveHTML();
$b = new DOMDocument();
$b->loadHTML('<div><input disabled name=n><a href="x y?z=1&amp;w=1">l</a><img src="a.png" alt=""></div>', LIBXML_HTML_NOIMPLIED | LIBXML_HTML_NODEFDTD);
echo $b->saveHTML();
echo $b->saveHTML($b->documentElement), "|\n";
if ($argc > 99) { simplexml_import_dom($h); }
