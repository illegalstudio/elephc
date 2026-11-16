//! Purpose:
//! End-to-end tests of php 8.4's `Dom\HTMLDocument` surface (`crate::dom_html_prelude`): a
//! program parses HTML, walks and mutates the tree, and its output is php's.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - The prelude's PHP model is otherwise checked only by its parse-parity oracle, which proves
//!   the builders equal the model but not that the compiled model behaves: these fixtures
//!   compile it. The expected text is `php -n`'s output for the same program (php 8.5.10).
//! - One program covers many behaviours, because each compile of the parser is expensive.

use crate::support::*;

/// The behaviours the 2026-11 branch review measured against php: mutation errors are
/// `DOMException`s raised before the tree changes (a cycle used to hang), `isConnected`,
/// `length` in characters, numeric references with leading zeros, the UTF-8 byte order mark and
/// invalid bytes, CDATA in svg, U+0000, `createEmpty()`'s node type, case-sensitive svg tag
/// names, `namedItem()` in tree order, `title`, a comment cut after `--!`, script data escapes,
/// template contents kept out of the tree, select content parsed in body, and windows-1252
/// sniffed from `<meta charset>`.
#[test]
fn test_dom_html_document_matches_php() {
    let out = compile_and_run(
        r#"<?php
function dump(Dom\Node $n, int $depth): string
{
    $pad = str_repeat(' ', $depth);
    if ($n instanceof Dom\Element) {
        $out = $pad . $n->localName . "\n";
    } elseif ($n instanceof Dom\Text) {
        $out = $pad . '"' . $n->data . '"' . "\n";
    } elseif ($n instanceof Dom\Comment) {
        $out = $pad . '#' . $n->data . "\n";
    } else {
        $out = $pad . '!' . $n->nodeName . "\n";
    }
    foreach ($n->childNodes as $child) {
        $out .= dump($child, $depth + 1);
    }
    return $out;
}
function parse(string $html): Dom\HTMLDocument
{
    return Dom\HTMLDocument::createFromString($html, LIBXML_NOERROR);
}
$d = parse('<div><p>a</p></div>');
$div = $d->body->firstChild;
try { $div->firstChild->appendChild($div); } catch (DOMException $e) { echo get_class($e), ': ', $e->getMessage(), ' ', $e->getCode(), "\n"; }
$i = $d->createElement('i');
$p = $div->firstChild;
try { $d->body->insertBefore($p, $i); } catch (DOMException $e) { echo $e->getMessage(), ' ', $e->getCode(), "\n"; }
echo $p->parentNode === $div ? "kept\n" : "moved\n";
try { $div->removeChild($i); } catch (DOMException $e) { echo $e->getMessage(), "\n"; }
echo $d->body->isConnected ? 'c' : 'n', $i->isConnected ? 'c' : 'n', "\n";
$div->appendChild($i);
echo $i->isConnected ? 'c' : 'n', "\n";
$div->removeChild($i);
echo $i->isConnected ? 'c' : 'n', "\n";
function chars(?Dom\Node $n): string
{
    return $n instanceof Dom\CharacterData ? $n->length . '' : '?';
}
function data(?Dom\Node $n): string
{
    return $n instanceof Dom\CharacterData ? $n->data : '?';
}
echo chars(parse("<p>h\u{e9}llo \u{1F600}</p>")->body->firstChild->firstChild), "\n";
echo parse('<p>&#0000000065;&#x0000000043;</p>')->body->textContent, "\n";
$d = parse("\xEF\xBB\xBF<!DOCTYPE html><html><head><title> a  b </title></head><body><p>a</p></body></html>");
echo $d->doctype === null ? 'none' : $d->doctype->name, ' [', $d->title, "]\n";
echo bin2hex(parse("<p>a\xFFb</p>")->body->textContent), "\n";
echo dump(parse('<svg><![CDATA[a<b]]></svg>')->body, 0);
echo bin2hex(parse("<title>a\0b</title><p>c\0d</p>")->documentElement->textContent), "\n";
echo Dom\HTMLDocument::createEmpty()->nodeType, "\n";
$d = parse('<svg><linearGradient></linearGradient></svg><a name=x></a><b id=x></b>');
echo $d->getElementsByTagName('linearGradient')->length, ' ', $d->getElementsByTagName('*')->namedItem('x')->localName, "\n";
echo '[', data(parse('<p>x</p><!--a--!')->body->lastChild), "]\n";
echo dump(parse('<body><script><!--<script>a</script>b--></script>x')->body, 0);
$d = parse('<template><p>x</p></template><div>y</div>');
echo $d->getElementsByTagName('p')->length, ' ', $d->head->firstChild->childNodes->length, "\n";
echo dump(parse('<select><b>x</b><i>y</select>z<p><select><textarea></select></textarea>')->body, 0);
$d = parse("<meta charset=iso-8859-1><p>\xE9</p>");
echo $d->characterSet, ' ', bin2hex($d->body->textContent), "\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "DOMException: Hierarchy Request Error 3\n",
            "Not Found Error 8\n",
            "kept\n",
            "Not Found Error\n",
            "cn\n",
            "c\n",
            "n\n",
            "7\n",
            "AC\n",
            "html [a b]\n",
            "61efbfbd62\n",
            "body\n svg\n  \"a<b\"\n",
            "61efbfbd626364\n",
            "13\n",
            "1 a\n",
            "[a]\n",
            "body\n script\n  \"<!--<script>a</script>b-->\"\n \"x\"\n",
            "0 0\n",
            "body\n select\n  b\n   \"x\"\n  i\n   \"y\"\n i\n  \"z\"\n  p\n   select\n    textarea\n     \"</select>\"\n",
            "windows-1252 c3a9\n",
        )
    );
}

/// `createFromString()` resolves lexbor's encoding labels: an unknown `$overrideEncoding` throws
/// php's `ValueError`, and a legacy encoding (override, `<meta>` or byte order mark) is decoded.
#[test]
fn test_dom_html_document_decodes_the_legacy_encodings() {
    let out = compile_and_run(
        r#"<?php
function show($src, $enc = null) {
    try {
        $d = $enc === null
            ? Dom\HTMLDocument::createFromString($src, LIBXML_NOERROR)
            : Dom\HTMLDocument::createFromString($src, LIBXML_NOERROR, $enc);
        echo $d->charset, " ", bin2hex($d->body->textContent), "\n";
    } catch (ValueError $e) {
        echo get_class($e), ": ", $e->getMessage(), "\n";
    }
}
show("<p>x</p>", 'bogus-enc');
show("<p>x</p>", ' utf-8');
show("<p>\x82\xa0</p>", 'SJIS');
show("<meta charset=shift_jis><p>\x82\xa0</p>");
show("<p>\xb1</p>", 'latin2');
show("<p>\xc1</p>", 'KOI8-R');
show("<p>\xe0</p>", 'cp1251');
show("<p>\xa4\xa2</p>", 'EUC-JP');
show("<p>\x80\xff</p>", 'x-user-defined');
show("\xff\xfe<\0p\0>\0x\0");
show("\xef\xbb\xbf<p>\xc3\xa9</p>", 'koi8-r');
show("<p>\xe9</p>", 'LATIN1');
"#,
    );
    let refusal = "ValueError: Dom\\HTMLDocument::createFromString(): Argument #3 ($overrideEncoding) must be a valid document encoding\n";
    assert_eq!(
        out,
        [
            refusal,
            refusal,
            "Shift_JIS e38182\n",
            "Shift_JIS e38182\n",
            "ISO-8859-2 c485\n",
            "KOI8-R d0b0\n",
            "windows-1251 d0b0\n",
            "EUC-JP e38182\n",
            "x-user-defined ef9e80ef9fbf\n",
            "UTF-16LE 78\n",
            "KOI8-R d09ee295a9c2a9d186e29598\n",
            "windows-1252 c3a9\n",
        ]
        .concat()
    );
}

/// `firstChild` is typed `?Dom\Node`: a property only `Dom\Element` or `Dom\Text` declares is
/// read from the run-time node, as php does.
#[test]
fn test_dom_html_document_reads_a_subclass_property_through_a_node() {
    let out = compile_and_run(
        r#"<?php
$h = Dom\HTMLDocument::createFromString('<p>hi</p>', LIBXML_NOERROR);
echo $h->body->firstChild->tagName, "\n";
echo $h->body->firstChild->firstChild->length, "\n";
"#,
    );
    assert_eq!(out, "P\n2\n");
}
