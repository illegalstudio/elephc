//! Purpose:
//! Integration tests for the libxml2 2.9.13 HTML4 parser behind `DOMDocument::loadHTML` /
//! `loadHTMLFile`: implied html/head/body/p, auto-closing, entities, Latin-1 and meta charset,
//! doctype, comments and processing instructions, NODEFDTD / NOIMPLIED / NOBLANKS.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every expectation is the output of `php -n` (8.5, libxml 2.9.13) for the same program.
//! - The tree is dumped through `childNodes` / `getAttribute`, so the tests do not depend on a
//!   serializer.

use crate::support::*;

/// A thirty-document differential corpus against php's own parser.
#[test]
fn test_load_html_matches_php_libxml_tree() {
    let out = compile_and_run(r####"<?php
libxml_use_internal_errors(true);
function dump(DOMNode $n, int $d): string {
    $pad = str_repeat(' ', $d);
    $o = '';
    if ($n instanceof DOMDocumentType) {
        $o = $pad . 'D ' . $n->name . '|' . $n->publicId . '|' . $n->systemId . "\n";
    } elseif ($n instanceof DOMElement) {
        $o = $pad . 'E ' . $n->nodeName . ' [' . $n->getAttribute('a') . ',' . $n->getAttribute('class') . ',' . $n->getAttribute('href') . ',' . $n->getAttribute('id') . "]\n";
    } elseif ($n instanceof DOMCdataSection) {
        $o = $pad . 'C ' . bin2hex((string) $n->nodeValue) . "\n";
    } elseif ($n instanceof DOMText) {
        $o = $pad . 'T ' . bin2hex((string) $n->nodeValue) . "\n";
    } elseif ($n instanceof DOMComment) {
        $o = $pad . 'M ' . bin2hex((string) $n->nodeValue) . "\n";
    } elseif ($n instanceof DOMProcessingInstruction) {
        $o = $pad . 'P ' . $n->target . ' ' . bin2hex($n->data) . "\n";
    }
    foreach ($n->childNodes as $c) {
        $o .= dump($c, $d + 1);
    }
    return $o;
}
$cases = [
    [base64_decode('PHA+YTxwPmI='), 0],
    [base64_decode('PGI+PGk+eDwvYj48L2k+'), 0],
    [base64_decode('PGRpdj48dGFibGU+PHRyPjx0ZD54PC90YWJsZT4='), 0],
    [base64_decode('PHRpdGxlPnQ8L3RpdGxlPjxib2R5Png='), 0],
    [base64_decode('cGxhaW4gdGV4dA=='), 0],
    [base64_decode('PCFbQ0RBVEFbeF1dPg=='), 0],
    [base64_decode('PHA+YSAmZm9vOyAmYW1wOyAmIzIzMzsgJmVhY3V0ZTs='), 0],
    [base64_decode('PHNjcmlwdD5pZihhPGIpe308L3NjcmlwdD4='), 0],
    [base64_decode('PHVsPjxsaT5hPGxpPmI8L3VsPg=='), 0],
    [base64_decode('PGh0bWw+PGJvZHk+PHA+eDwvcD48L2JvZHk+PC9odG1sPjxwPnk='), 0],
    [base64_decode('PCEtLSBjIC0tPjwhRE9DVFlQRSBodG1sPjxodG1sPjxoZWFkPjwvaGVhZD48Ym9keT54PC9ib2R5PjwvaHRtbD4='), 0],
    [base64_decode('PGRpdj4KICA8c3Bhbj54PC9zcGFuPgo8L2Rpdj4='), 256],
    [base64_decode('PGEgaHJlZj14IGhpZGRlbiBjbGFzcz55PC9hPnRleHQ='), 0],
    [base64_decode('Y2Fm6SA8Yj54PC9iPg=='), 0],
    [base64_decode('PG1ldGEgY2hhcnNldD11dGYtOD5jYWbDqQ=='), 0],
    [base64_decode('PD9waHAgZWNobyAxID8+PHA+YQ=='), 0],
    [base64_decode('PGZyYW1lc2V0PjxmcmFtZT48L2ZyYW1lc2V0Pg=='), 0],
    [base64_decode('PHNlbGVjdD48b3B0aW9uPmE8b3B0aW9uPmI8L3NlbGVjdD4='), 0],
    [base64_decode('PGJyLz48aHI+PGltZyBzcmM9YT4='), 0],
    [base64_decode('PGRpdj54PC9kaXY+'), 4],
    [base64_decode('PGRpdj54PC9kaXY+'), 8196],
    [base64_decode('PHA+eDwhZG9jdHlwZSBodG1sPjwvaHRtbD48ZGw+eQ=='), 0],
    [base64_decode('PCFET0NUWVBFIGh0bWwgUFVCTElDICItLy9XM0MvL0RURCBIVE1MIDQuMDEvL0VOIiAiaHR0cDovL3d3dy53My5vcmcvVFIvaHRtbDQvc3RyaWN0LmR0ZCI+PGh0bWw+PGJvZHk+ICA8cD54PC9wPiAgPC9ib2R5PjwvaHRtbD4='), 256],
    [base64_decode('PGh0bWw+PGhlYWQ+PC9oZWFkPjxkaXY+QjwvZGl2PjwvaHRtbD48aHRtbD48c3Bhbj5DPC9zcGFuPjwvaHRtbD4='), 0],
    [base64_decode('PGJvZHk+QTxib2R5PkI8L2JvZHk+QzwvYm9keT48ZGl2PkQ8L2Rpdj4='), 0],
    [base64_decode('PGZyYW1lc2V0PkY8Ym9keSBpZD14PkM8L2JvZHk+PC9mcmFtZXNldD4='), 0],
    [base64_decode('PERJViBDTEFTUz0iZmlyc3QiIGNsYXNzPSJpZ25vcmVkIj5hPD9PbmUgeD48P1R3bz8+YjwvRElWPg=='), 0],
    [base64_decode('PGRpdj5hPCEtLXVuZmluaXNoZWQ='), 0],
    [base64_decode('PHRleHRhcmVhPjxiPng8L2I+PC90ZXh0YXJlYT48c3R5bGU+YT5ie308L3N0eWxlPg=='), 0],
    [base64_decode('PHVsPgo8bGk+YTwvbGk+CjxsaT5iPC9saT4KPC91bD4K'), 256],
];
foreach ($cases as $i => $case) {
    $doc = new DOMDocument();
    $doc->loadHTML($case[0], $case[1]);
    echo '#', $i, "\n", dump($doc, 0);
}
"####);
    assert_eq!(out, r####"#0
 D html|-//W3C//DTD HTML 4.0 Transitional//EN|http://www.w3.org/TR/REC-html40/loose.dtd
 E html [,,,]
  E body [,,,]
   E p [,,,]
    T 61
   E p [,,,]
    T 62
#1
 D html|-//W3C//DTD HTML 4.0 Transitional//EN|http://www.w3.org/TR/REC-html40/loose.dtd
 E html [,,,]
  E body [,,,]
   E b [,,,]
    E i [,,,]
     T 78
#2
 D html|-//W3C//DTD HTML 4.0 Transitional//EN|http://www.w3.org/TR/REC-html40/loose.dtd
 E html [,,,]
  E body [,,,]
   E div [,,,]
    E table [,,,]
     E tr [,,,]
      E td [,,,]
       T 78
#3
 D html|-//W3C//DTD HTML 4.0 Transitional//EN|http://www.w3.org/TR/REC-html40/loose.dtd
 E html [,,,]
  E head [,,,]
   E title [,,,]
    T 74
  E body [,,,]
   T 78
#4
 D html|-//W3C//DTD HTML 4.0 Transitional//EN|http://www.w3.org/TR/REC-html40/loose.dtd
 E html [,,,]
  E body [,,,]
   E p [,,,]
    T 706c61696e2074657874
#5
 D html|-//W3C//DTD HTML 4.0 Transitional//EN|http://www.w3.org/TR/REC-html40/loose.dtd
#6
 D html|-//W3C//DTD HTML 4.0 Transitional//EN|http://www.w3.org/TR/REC-html40/loose.dtd
 E html [,,,]
  E body [,,,]
   E p [,,,]
    T 612026666f6f3b202620c3a920c3a9
#7
 D html|-//W3C//DTD HTML 4.0 Transitional//EN|http://www.w3.org/TR/REC-html40/loose.dtd
 E html [,,,]
  E head [,,,]
   E script [,,,]
    C 696628613c62297b7d
#8
 D html|-//W3C//DTD HTML 4.0 Transitional//EN|http://www.w3.org/TR/REC-html40/loose.dtd
 E html [,,,]
  E body [,,,]
   E ul [,,,]
    E li [,,,]
     T 61
    E li [,,,]
     T 62
#9
 D html|-//W3C//DTD HTML 4.0 Transitional//EN|http://www.w3.org/TR/REC-html40/loose.dtd
 E html [,,,]
  E body [,,,]
   E p [,,,]
    T 78
  E html [,,,]
   E p [,,,]
    T 79
#10
 D html||
 M 206320
 E html [,,,]
  E head [,,,]
  E body [,,,]
   T 78
#11
 D html|-//W3C//DTD HTML 4.0 Transitional//EN|http://www.w3.org/TR/REC-html40/loose.dtd
 E html [,,,]
  E body [,,,]
   E div [,,,]
    T 0a2020
    E span [,,,]
     T 78
    T 0a
#12
 D html|-//W3C//DTD HTML 4.0 Transitional//EN|http://www.w3.org/TR/REC-html40/loose.dtd
 E html [,,,]
  E body [,,,]
   E a [,,x,]
    T 79
   T 74657874
#13
 D html|-//W3C//DTD HTML 4.0 Transitional//EN|http://www.w3.org/TR/REC-html40/loose.dtd
 E html [,,,]
  E body [,,,]
   E p [,,,]
    T 636166c3a920
    E b [,,,]
     T 78
#14
 D html|-//W3C//DTD HTML 4.0 Transitional//EN|http://www.w3.org/TR/REC-html40/loose.dtd
 E html [,,,]
  E head [,,,]
   E meta [,,,]
  E body [,,,]
   E p [,,,]
    T 636166c3a9
#15
 D html|-//W3C//DTD HTML 4.0 Transitional//EN|http://www.w3.org/TR/REC-html40/loose.dtd
 P php 6563686f2031203f
 E html [,,,]
  E body [,,,]
   E p [,,,]
    T 61
#16
 D html|-//W3C//DTD HTML 4.0 Transitional//EN|http://www.w3.org/TR/REC-html40/loose.dtd
 E html [,,,]
  E frameset [,,,]
   E frame [,,,]
#17
 D html|-//W3C//DTD HTML 4.0 Transitional//EN|http://www.w3.org/TR/REC-html40/loose.dtd
 E html [,,,]
  E body [,,,]
   E select [,,,]
    E option [,,,]
     T 61
    E option [,,,]
     T 62
#18
 D html|-//W3C//DTD HTML 4.0 Transitional//EN|http://www.w3.org/TR/REC-html40/loose.dtd
 E html [,,,]
  E body [,,,]
   E br [,,,]
   E hr [,,,]
   E img [,,,]
#19
 E html [,,,]
  E body [,,,]
   E div [,,,]
    T 78
#20
 E div [,,,]
  T 78
#21
 D html||
 E html [,,,]
  E body [,,,]
   E p [,,,]
    T 78
   E dl [,,,]
    T 79
#22
 D html|-//W3C//DTD HTML 4.01//EN|http://www.w3.org/TR/html4/strict.dtd
 E html [,,,]
  E body [,,,]
   E p [,,,]
    T 78
#23
 D html|-//W3C//DTD HTML 4.0 Transitional//EN|http://www.w3.org/TR/REC-html40/loose.dtd
 E html [,,,]
  E head [,,,]
  E body [,,,]
   E div [,,,]
    T 42
  E html [,,,]
   E span [,,,]
    T 43
#24
 D html|-//W3C//DTD HTML 4.0 Transitional//EN|http://www.w3.org/TR/REC-html40/loose.dtd
 E html [,,,]
  E body [,,,]
   T 414243
  E div [,,,]
   T 44
#25
 D html|-//W3C//DTD HTML 4.0 Transitional//EN|http://www.w3.org/TR/REC-html40/loose.dtd
 E html [,,,]
  E frameset [,,,]
   T 46
   E body [,,,x]
    T 43
#26
 D html|-//W3C//DTD HTML 4.0 Transitional//EN|http://www.w3.org/TR/REC-html40/loose.dtd
 E html [,,,]
  E body [,,,]
   E div [,first,,]
    T 61
    P One 78
    P Two 3f
    T 62
#27
 D html|-//W3C//DTD HTML 4.0 Transitional//EN|http://www.w3.org/TR/REC-html40/loose.dtd
 E html [,,,]
  E body [,,,]
   E div [,,,]
    T 61
#28
 D html|-//W3C//DTD HTML 4.0 Transitional//EN|http://www.w3.org/TR/REC-html40/loose.dtd
 E html [,,,]
  E body [,,,]
   E textarea [,,,]
    E b [,,,]
     T 78
   E style [,,,]
    C 613e627b7d
#29
 D html|-//W3C//DTD HTML 4.0 Transitional//EN|http://www.w3.org/TR/REC-html40/loose.dtd
 E html [,,,]
  E body [,,,]
   E ul [,,,]
    E li [,,,]
     T 61
    T 0a
    E li [,,,]
     T 62
    T 0a
"####);
}

/// `loadHTML` / `loadHTMLFile` contract: empty arguments, files, the default doctype node.
#[test]
fn test_load_html_file_and_default_doctype() {
    let out = compile_and_run(r####"<?php
$d = new DOMDocument();
try { $d->loadHTML(''); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
try { $d->loadHTMLFile(''); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
$path = sys_get_temp_dir() . '/elephc_dom_html_parser_' . getmypid() . '.html';
file_put_contents($path, '<html><body><p class="k">file</p></body></html>');
var_dump($d->loadHTMLFile($path));
echo $d->getElementsByTagName('p')->item(0)->getAttribute('class'), $d->getElementsByTagName('p')->item(0)->nodeValue, "\n";
unlink($path);
var_dump(@$d->loadHTMLFile($path));
$d = new DOMDocument();
var_dump($d->loadHTML('<p>x</p>'));
echo $d->childNodes->length, $d->firstChild instanceof DOMDocumentType ? 'D' : 'x', $d->firstChild->nextSibling->nodeName, "\n";
echo $d->childNodes->item(0)->nextSibling->previousSibling === $d->childNodes->item(0) ? 'linked' : 'broken', "\n";
$d->loadHTML('<p>y</p>', LIBXML_HTML_NODEFDTD);
echo $d->childNodes->length, $d->firstChild->nodeName, "\n";"####);
    assert_eq!(out, r####"DOMDocument::loadHTML(): Argument #1 ($source) must not be empty
DOMDocument::loadHTMLFile(): Argument #1 ($filename) must not be empty
bool(true)
kfile
bool(false)
bool(true)
2Dhtml
linked
1html
"####);
}
