//! Purpose:
//! Integration tests for the Termwind-facing DOM HTML subset: `loadHTML`,
//! `getElementsByTagName('body')`, child walks, attributes, node types, and
//! sibling pointers.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Fixtures compile PHP to a native binary and assert stdout. They cover the
//!   `HtmlRenderer::parse` tree walk for a simple styled `div`, not tables or
//!   code renderers.

use crate::support::*;

/// Verifies Termwind's `LIBXML_*` flag integers match php-src / libxml2.
#[test]
fn test_libxml_termwind_flag_values() {
    let out = compile_and_run(
        "<?php echo LIBXML_NOXMLDECL, ',', LIBXML_HTML_NODEFDTD, ',', LIBXML_NOERROR, ',', LIBXML_NOBLANKS, ',', LIBXML_COMPACT;",
    );
    assert_eq!(out, "2,4,32,256,65536");
}

/// Verifies `loadHTML` wraps a fragment in html/body and `item(0)` returns body.
#[test]
fn test_load_html_body_item() {
    let out = compile_and_run(
        r#"<?php
$dom = new DOMDocument();
$dom->loadHTML('<div class="text-green-500">Hi</div>', LIBXML_NOERROR | LIBXML_COMPACT | LIBXML_HTML_NODEFDTD | LIBXML_NOBLANKS | LIBXML_NOXMLDECL);
$body = $dom->getElementsByTagName('body')->item(0);
echo $body->nodeName;
"#,
    );
    assert_eq!(out, "body");
}

/// Verifies the Termwind-style walk: body child is a `div` with class and text.
#[test]
fn test_termwind_div_class_walk() {
    let out = compile_and_run(
        r#"<?php
$dom = new DOMDocument();
$html = ' <div class="text-green-500">Hi</div>';
$dom->loadHTML($html, LIBXML_NOERROR | LIBXML_COMPACT | LIBXML_HTML_NODEFDTD | LIBXML_NOBLANKS | LIBXML_NOXMLDECL);
$body = $dom->getElementsByTagName('body')->item(0);
$div = $body->childNodes->item(0);
echo $div instanceof DOMElement ? 'E' : 'x';
echo ':';
echo $div->nodeName;
echo ':';
echo $div->getAttribute('class');
echo ':';
$text = $div->childNodes->item(0);
echo $text instanceof DOMText ? 'T' : 'x';
echo ':';
echo $text->nodeValue;
"#,
    );
    assert_eq!(out, "E:div:text-green-500:T:Hi");
}

/// Verifies `foreach` over `childNodes` matches Termwind's `Node::getChildNodes`.
#[test]
fn test_child_nodes_foreach() {
    let out = compile_and_run(
        r#"<?php
$dom = new DOMDocument();
$dom->loadHTML('<div><span>A</span><b>B</b></div>', LIBXML_NOERROR | LIBXML_NOBLANKS);
$body = $dom->getElementsByTagName('body')->item(0);
$div = $body->childNodes->item(0);
foreach ($div->childNodes as $child) {
    echo $child->nodeName, ':', $child->nodeValue, ';';
}
"#,
    );
    assert_eq!(out, "span:A;b:B;");
}

/// Verifies `previousSibling` / `nextSibling` and comment node identity.
#[test]
fn test_siblings_and_comment() {
    let out = compile_and_run(
        r#"<?php
$dom = new DOMDocument();
$dom->loadHTML('<div>A<!--c-->B</div>', LIBXML_NOERROR | LIBXML_NOBLANKS);
$div = $dom->getElementsByTagName('div')->item(0);
$first = $div->childNodes->item(0);
$comment = $first->nextSibling;
$last = $comment->nextSibling;
echo $first->nodeValue;
echo $comment instanceof DOMComment ? 'C' : 'x';
echo $comment->nodeValue;
echo $last->nodeValue;
echo $last->previousSibling === $comment ? 'P' : 'x';
echo $first->previousSibling === null ? 'N' : 'x';
"#,
    );
    assert_eq!(out, "ACcBPN");
}

/// Verifies `LIBXML_NOBLANKS` drops whitespace-only text nodes between elements.
#[test]
fn test_noblanks_drops_whitespace_text() {
    let out = compile_and_run(
        r#"<?php
$dom = new DOMDocument();
$dom->loadHTML("<div>\n  <span>x</span>\n</div>", LIBXML_NOERROR | LIBXML_NOBLANKS);
$div = $dom->getElementsByTagName('div')->item(0);
echo $div->childNodes->length;
echo ':';
echo $div->childNodes->item(0)->nodeName;
"#,
    );
    assert_eq!(out, "1:span");
}

/// Verifies `saveXML` on a child and `ownerDocument` identity for Termwind `getHtml`.
#[test]
fn test_save_xml_and_owner_document() {
    let out = compile_and_run(
        r#"<?php
$dom = new DOMDocument();
$dom->loadHTML('<div class="text-green-500">Hi</div>', LIBXML_NOERROR | LIBXML_NOBLANKS | LIBXML_NOXMLDECL);
$div = $dom->getElementsByTagName('div')->item(0);
echo $div->ownerDocument instanceof DOMDocument ? 'D' : 'x';
echo ':';
echo $div->ownerDocument->saveXML($div);
"#,
    );
    assert_eq!(out, "D:<div class=\"text-green-500\">Hi</div>");
}

/// Verifies namespaced unqualified `LIBXML_*` and `\DOMDocument` as Termwind spells them.
#[test]
fn test_namespaced_termwind_call_shape() {
    let out = compile_and_run(
        r#"<?php
namespace Termwind;
$dom = new \DOMDocument();
$dom->loadHTML('<span class="font-bold">Ok</span>', LIBXML_NOERROR | LIBXML_NOBLANKS);
$body = $dom->getElementsByTagName('body')->item(0);
$el = $body->childNodes->item(0);
echo $el->nodeName, ':', $el->getAttribute('class'), ':', $el->nodeValue;
"#,
    );
    assert_eq!(out, "span:font-bold:Ok");
}
