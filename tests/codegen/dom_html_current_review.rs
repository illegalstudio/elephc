//! Purpose:
//! Regressions for legacy HTML implied bodies, reopened roots and non-void tags.
//!
//! Called from:
//! - The DOM HTML codegen integration test module.
//!
//! Key details:
//! - Expected trees follow PHP with libxml 2.9.14, not modern HTML5 parsing.

use crate::support::*;

/// An implied body excludes head and roots following the closed outer html.
#[test]
fn dom_html_current_review_implied_body() {
    let out = compile_and_run(r#"<?php
$doc = new DOMDocument();
$doc->loadHTML('<html><head></head><div>B</div></html><span>C</span>', LIBXML_HTML_NODEFDTD | LIBXML_NOERROR);
$html = $doc->getElementsByTagName('html')->item(0);
$body = $doc->getElementsByTagName('body')->item(0);
echo $doc->saveXML($html), '|', $body->childNodes->item(0)->nodeName, '|';
echo $body->nodeValue, ':', $html->childNodes->item(0)->nodeName;
"#);
    assert_eq!(out, "<html><head/><body><div>B</div></body><html><span>C</span></html></html>|div|B:head");
}

/// A later explicit html root is adopted rather than nested in another wrapper.
#[test]
fn dom_html_current_review_adopt_reopened_html() {
    let out = compile_and_run(r#"<?php
$doc = new DOMDocument();
$doc->loadHTML('<html><head></head><div>B</div></html><html><span>C</span></html>', LIBXML_HTML_NODEFDTD | LIBXML_NOERROR);
echo $doc->saveXML($doc->getElementsByTagName('html')->item(0));
"#);
    assert_eq!(out, "<html><head/><body><div>B</div></body><html><span>C</span></html></html>");
}

/// A head-only document does not acquire an empty body containing its head.
#[test]
fn dom_html_current_review_head_only() {
    let out = compile_and_run(r#"<?php
$doc = new DOMDocument();
$doc->loadHTML('<html><head></head></html>', LIBXML_HTML_NODEFDTD | LIBXML_NOERROR);
echo $doc->saveXML($doc->getElementsByTagName('html')->item(0)), ':', $doc->getElementsByTagName('body')->length;
"#);
    assert_eq!(out, "<html><head/></html>:0");
}

/// Legacy libxml retains text children in embed, source, track and wbr elements.
#[test]
fn dom_html_current_review_legacy_non_void_elements() {
    let out = compile_and_run(r#"<?php
$doc = new DOMDocument();
foreach (['embed', 'source', 'track', 'wbr'] as $tag) {
    $doc->loadHTML('<div>a<' . $tag . '>b</div>', LIBXML_HTML_NODEFDTD | LIBXML_NOERROR);
    $node = $doc->getElementsByTagName($tag)->item(0);
    echo $doc->saveXML($doc->getElementsByTagName('div')->item(0)), ':', $node->nodeValue, '|';
}
"#);
    assert_eq!(out, "<div>a<embed>b</embed></div>:b|<div>a<source>b</source></div>:b|<div>a<track>b</track></div>:b|<div>a<wbr>b</wbr></div>:b|");
}

/// Implied-body construction preserves the same tree without EIR optimization.
#[test]
fn dom_html_current_review_implied_body_no_ir_opt() {
    without_ir_opt(dom_html_current_review_implied_body);
}

/// Explicit reopened wrappers remain single nodes without EIR optimization.
#[test]
fn dom_html_current_review_adopt_reopened_html_no_ir_opt() {
    without_ir_opt(dom_html_current_review_adopt_reopened_html);
}

/// Legacy tag classification is independent of EIR optimization.
#[test]
fn dom_html_current_review_legacy_non_void_elements_no_ir_opt() {
    without_ir_opt(dom_html_current_review_legacy_non_void_elements);
}

/// Literal wrapper partitioning retires its intermediate arrays without DOM cycles.
#[test]
fn dom_html_current_review_wrapper_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function enableDom(DOMDocument $doc): void {}
for ($i = 0; $i < 16; $i++) {
    $head = __elephc_dom_new_node('element', 'head', '', [], []);
    $text = __elephc_dom_new_node('text', '#text', 'B', [], []);
    $div = __elephc_dom_new_node('element', 'div', '', [], [$text]);
    $html = __elephc_dom_new_node('element', 'html', '', [], [$head, $div]);
    $later = __elephc_dom_new_node('element', 'html', '', [], [$div]);
    $roots = [$html, $later];
    $tree = __elephc_dom_wrap_html($roots);
    echo __elephc_dom_text_of($tree), '|';
    unset($tree, $roots);
}
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "BB|".repeat(16));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Wrapper temporary owners remain balanced without EIR optimization.
#[test]
fn dom_html_current_review_wrapper_owners_no_ir_opt() {
    without_ir_opt(dom_html_current_review_wrapper_owners);
}

/// Head-only wrapper selection is independent of EIR optimization.
#[test]
fn dom_html_current_review_head_only_no_ir_opt() {
    without_ir_opt(dom_html_current_review_head_only);
}
