//! Purpose:
//! Regressions for legacy HTML implied bodies, reopened roots and non-void tags.
//!
//! Called from:
//! - The DOM HTML codegen integration test module.
//!
//! Key details:
//! - Expected trees follow PHP with libxml 2.9.14, not modern HTML5 parsing.

use crate::support::*;

/// An old implied-root flag must not trim whitespace inside a later unclosed element.
/// The expected output is php 8.5's own (libxml 2.9.13).
#[test]
fn dom_html_body_boundary_review_later_element_blanks() {
    let out = compile_and_run(r#"<?php
$doc = new DOMDocument();
foreach (['<div>B</div></body><span>   ', '<div>B</div></body><div>   ', '<span>   ', '<div><span>   '] as $source) {
    $doc->loadHTML($source, LIBXML_HTML_NODEFDTD | LIBXML_NOERROR | LIBXML_NOBLANKS);
    echo $doc->saveXML($doc->getElementsByTagName('html')->item(0)), '|';
}
"#);
    assert_eq!(out, "<html><body><div>B</div></body><span/></html>|<html><body><div>B</div></body><div/></html>|<html><body><span/></body></html>|<html><body><div><span/></div></body></html>|");
}

/// Element whitespace after a closed implied body is stable without EIR optimization.
#[test]
fn dom_html_body_boundary_review_later_element_blanks_no_ir_opt() {
    without_ir_opt(dom_html_body_boundary_review_later_element_blanks);
}

/// Closing an implied body preserves the boundary before a later explicit body or text.
/// The expected output is php 8.5's own (libxml 2.9.13).
#[test]
fn dom_html_body_boundary_review_closed_implied_body() {
    let out = compile_and_run(r#"<?php
$doc = new DOMDocument();
foreach (['<div>B</div></body><body id=x>C</body>', '<html><head></head><div>B</div></body><body id=x>C</body></html>', '<p>B</p></body><body class=z>C</body>', '<div>B</div></body>C'] as $source) {
    $doc->loadHTML($source, LIBXML_HTML_NODEFDTD | LIBXML_NOERROR);
    echo $doc->saveXML($doc->getElementsByTagName('html')->item(0)), '|';
}
"#);
    assert_eq!(out, "<html><body><div>B</div></body><body id=\"x\">C</body></html>|<html><head/><body><div>B</div></body><body id=\"x\">C</body></html>|<html><body><p>B</p></body><body class=\"z\">C</body></html>|<html><body><div>B</div></body><p>C</p></html>|");
}

/// A discarded self-closing body closes the current body without consuming a later end tag.
/// The expected output is php 8.5's own (libxml 2.9.13).
#[test]
fn dom_html_body_boundary_review_discarded_self_closing_body() {
    let out = compile_and_run(r#"<?php
$doc = new DOMDocument();
foreach (['<body>A<body/>B</body><div>C</div>', '<html><head></head><div>B</div><body/>C</html>', '<body>A<div>B<body/>C</div>D</body>E', '<html><div>B<body/>C</html>D', '<html><body>A<body>B</html>C</body>D', '<html><body>A<div>B<body/>C</head>D</body>E'] as $source) {
    $doc->loadHTML($source, LIBXML_HTML_NODEFDTD | LIBXML_NOERROR);
    echo $doc->saveXML($doc->getElementsByTagName('html')->item(0)), '|';
}
"#);
    assert_eq!(out, "<html><body>A</body><p>B</p><div>C</div></html>|<html><head/><body><div>B</div></body><p>C</p></html>|<html><body>A<div>B</div>CDE</body></html>|<html><body><div>B</div>CD</body></html>|<html><body>ABC</body><p>D</p></html>|<html><body>A<div>B</div>CD</body><p>E</p></html>|");
}

/// Legacy frame containers do not cause a later real body to be discarded as redundant.
#[test]
fn dom_html_body_boundary_review_frame_exclusions() {
    let out = compile_and_run(r#"<?php
$doc = new DOMDocument();
foreach (['<frame><body id=x>C</body>', '<frameset><body id=x>C</body></frameset>', '<frame>', '<frameset><div>B</div></frameset>', '<noframes><body id=x>C</body></noframes>', '<frameset>F<body id=x>C</body></frameset>', '<noframes>F<body id=x>C</body></noframes>'] as $source) {
    $doc->loadHTML($source, LIBXML_HTML_NODEFDTD | LIBXML_NOERROR);
    echo $doc->saveXML($doc->getElementsByTagName('html')->item(0)), '|';
}
"#);
    // The pinned libxml2.9.14 treats noframes as a normal element, unlike libxml2.15 CDATA.
    assert_eq!(out, "<html><frame/><body id=\"x\">C</body></html>|<html><frameset><body id=\"x\">C</body></frameset></html>|<html><frame/></html>|<html><frameset><body><div>B</div></body></frameset></html>|<html><noframes><body id=\"x\">C</body></noframes></html>|<html><frameset>F<body id=\"x\">C</body></frameset></html>|<html><noframes>F<body id=\"x\">C</body></noframes></html>|");
}

/// Closed implied-body boundaries do not depend on EIR optimization.
#[test]
fn dom_html_body_boundary_review_closed_implied_body_no_ir_opt() {
    without_ir_opt(dom_html_body_boundary_review_closed_implied_body);
}

/// Discarded self-closing body boundaries do not depend on EIR optimization.
#[test]
fn dom_html_body_boundary_review_discarded_self_closing_body_no_ir_opt() {
    without_ir_opt(dom_html_body_boundary_review_discarded_self_closing_body);
}

/// Frame exclusions do not depend on EIR optimization.
#[test]
fn dom_html_body_boundary_review_frame_exclusions_no_ir_opt() {
    without_ir_opt(dom_html_body_boundary_review_frame_exclusions);
}

/// Closed-body boundary recovery retires popped nodes and wrapper arrays without leaking.
/// Rewritten from the removed `__elephc_dom_*` builder helpers onto the public DOM API; the
/// expected output is php 8.5's own (libxml 2.9.13).
#[test]
fn dom_html_body_boundary_review_literal_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
for ($i = 0; $i < 16; $i++) {
    foreach (['<div>B</div></body><body>C</body>', '<body>A<body/>B</body><div>C</div>'] as $source) {
        $doc = new DOMDocument();
        $doc->loadHTML($source, LIBXML_HTML_NODEFDTD | LIBXML_NOERROR);
        $html = $doc->getElementsByTagName('html')->item(0);
        echo $html->childNodes->length, ':', $html->nodeValue, '|';
        unset($html, $doc);
    }
}
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "2:BC|3:ABC|".repeat(16));
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}

/// Boundary recovery keeps the same owner balance without EIR optimization.
#[test]
fn dom_html_body_boundary_review_literal_owners_no_ir_opt() {
    without_ir_opt(dom_html_body_boundary_review_literal_owners);
}

/// Body tags after implied content are ignored without nesting or displacing that content.
/// The expected output is php 8.5's own (libxml 2.9.13).
#[test]
fn dom_html_grok_current_late_body() {
    let out = compile_and_run(r#"<?php
$doc = new DOMDocument();
foreach (['<div>B</div><body id=x>C</body>', '<html><head></head><div>B</div><body>C</body></html>', '<div>B<body>C</body>D</div>', '<html><head></head>T<body>C</body></html>'] as $source) {
    $doc->loadHTML($source, LIBXML_HTML_NODEFDTD | LIBXML_NOERROR);
    echo $doc->saveXML($doc->getElementsByTagName('html')->item(0)), ':', $doc->getElementsByTagName('body')->length, '|';
}
"#);
    assert_eq!(out, "<html><body><div>B</div>C</body></html>:1|<html><head/><body><div>B</div>C</body></html>:1|<html><body><div>BCD</div></body></html>:1|<html><head/><body><p>T</p>C</body></html>:1|");
}

/// Nested repeated body tags discard their paired end, while separate bodies remain separate.
#[test]
fn dom_html_grok_current_body_controls() {
    let out = compile_and_run(r#"<?php
$doc = new DOMDocument();
foreach (['<body>A<body id=x>B</body>C</body><div>D</div>', '<body>A</body><body>B</body>', '<html><head></head><script>X</script><body id=x>C</body></html>'] as $source) {
    $doc->loadHTML($source, LIBXML_HTML_NODEFDTD | LIBXML_NOERROR);
    echo $doc->saveXML($doc->getElementsByTagName('html')->item(0)), '|';
}
"#);
    assert_eq!(out, "<html><body>ABC</body><div>D</div></html>|<html><body>A</body><body>B</body></html>|<html><head/><script><![CDATA[X]]></script><body id=\"x\">C</body></html>|");
}

/// Late-body repair does not depend on EIR optimization.
#[test]
fn dom_html_grok_current_late_body_no_ir_opt() {
    without_ir_opt(dom_html_grok_current_late_body);
}

/// Explicit-body boundaries remain distinct without EIR optimization.
#[test]
fn dom_html_grok_current_body_controls_no_ir_opt() {
    without_ir_opt(dom_html_grok_current_body_controls);
}

/// Compares late-body ownership for literal and runtime-owned tag names, without leaks.
/// Rewritten from the removed `__elephc_dom_*` builder helpers onto the public DOM API; the
/// expected output is php 8.5's own (libxml 2.9.13).
fn verify_late_body_owners(owned_names: bool) {
    let names = if owned_names {
        "$html_name = str_repeat('html', $argc); $head_name = str_repeat('head', $argc); $div_name = str_repeat('div', $argc); $body_name = str_repeat('body', $argc);"
    } else {
        "$html_name = 'html'; $head_name = 'head'; $div_name = 'div'; $body_name = 'body';"
    };
    let iterations = if owned_names { 8 } else { 16 };
    let source = r#"<?php
TAG_NAMES
for ($i = 0; $i < ITERATIONS; $i++) {
    $doc = new DOMDocument();
    $doc->loadHTML('<' . $html_name . '><' . $head_name . '></' . $head_name . '><' . $div_name . '>B</' . $div_name . '><' . $body_name . '>C</' . $body_name . '></' . $html_name . '>', LIBXML_HTML_NODEFDTD | LIBXML_NOERROR);
    $body = $doc->getElementsByTagName('body')->item(0);
    echo $body->childNodes->length, ':', $body->nodeValue, '|';
    unset($body, $doc);
}
"#.replace("TAG_NAMES", names).replace("ITERATIONS", &iterations.to_string());
    let out = compile_and_run_with_heap_debug(&source);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "2:BC|".repeat(iterations));
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}

/// Literal tags keep the late-body builder and wrapper paths heap-clean.
#[test]
fn dom_html_grok_current_body_owners() {
    verify_late_body_owners(false);
}

/// Runtime-owned tag names add no retention beyond the unchanged published builder baseline.
#[test]
fn dom_html_grok_current_owned_names_do_not_add_retention() {
    verify_late_body_owners(true);
}

/// The inherited owned-name bound does not depend on EIR optimization.
#[test]
fn dom_html_grok_current_owned_names_do_not_add_retention_no_ir_opt() {
    without_ir_opt(dom_html_grok_current_owned_names_do_not_add_retention);
}

/// Raw late-body forest ownership is independent of EIR optimization.
#[test]
fn dom_html_grok_current_body_owners_no_ir_opt() {
    without_ir_opt(dom_html_grok_current_body_owners);
}

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

/// Wrapper partitioning of a reopened html root retires its intermediate arrays without leaks.
/// Rewritten from the removed `__elephc_dom_*` builder helpers onto the public DOM API; the
/// expected output is php 8.5's own (libxml 2.9.13).
#[test]
fn dom_html_current_review_wrapper_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
for ($i = 0; $i < 16; $i++) {
    $doc = new DOMDocument();
    $doc->loadHTML('<html><head></head><div>B</div></html><html><div>B</div></html>', LIBXML_HTML_NODEFDTD | LIBXML_NOERROR);
    echo $doc->getElementsByTagName('html')->item(0)->nodeValue, '|';
    unset($doc);
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

/// A reopened document creates its first body when the original html contained only a head.
#[test]
fn dom_html_latest_review_head_only_reopened_body() {
    let out = compile_and_run(r#"<?php
$doc = new DOMDocument();
foreach (['<html><head></head></html><div>B</div>', '<html><head></head></html><html lang="en"><div>B</div></html>'] as $source) {
    $doc->loadHTML($source, LIBXML_HTML_NODEFDTD | LIBXML_NOERROR);
    $body = $doc->getElementsByTagName('body')->item(0);
    echo $doc->saveXML($doc->getElementsByTagName('html')->item(0)), ':', $body->nodeValue, '|';
}
"#);
    assert_eq!(out, "<html><head/><html><body><div>B</div></body></html></html>:B|<html><head/><html lang=\"en\"><body><div>B</div></body></html></html>:B|");
}

/// An html start tag inside an open html is ignored, including its attributes.
#[test]
fn dom_html_latest_review_redundant_html() {
    let out = compile_and_run(r#"<?php
$doc = new DOMDocument();
foreach (['<html id=a><html id=b class=c><div>B</div></html></html>', '<html><body>A</body><html lang=en><span>B</span></html></html>'] as $source) {
    $doc->loadHTML($source, LIBXML_HTML_NODEFDTD | LIBXML_NOERROR);
    echo $doc->getElementsByTagName('html')->length, ':', $doc->saveXML($doc->getElementsByTagName('html')->item(0)), '|';
}
"#);
    assert_eq!(out, "1:<html id=\"a\"><body><div>B</div></body></html>|1:<html><body>A</body><span>B</span></html>|");
}

/// Head-compatible metadata after head stays outside the subsequently implied body.
#[test]
fn dom_html_latest_review_post_head_metadata() {
    let out = compile_and_run(r#"<?php
$doc = new DOMDocument();
$doc->loadHTML('<html><head></head><script>X</script><style>Y</style><base href=x><link href=y><meta name=z><title>T</title><div>B</div></html>', LIBXML_HTML_NODEFDTD | LIBXML_NOERROR);
echo $doc->saveXML($doc->getElementsByTagName('html')->item(0)), ':', $doc->getElementsByTagName('body')->item(0)->nodeValue;
"#);
    assert_eq!(out, "<html><head/><script><![CDATA[X]]></script><style><![CDATA[Y]]></style><base href=\"x\"/><link href=\"y\"/><meta name=\"z\"/><title>T</title><body><div>B</div></body></html>:B");
}

/// NOBLANKS removes wrapper padding only after the implied body's boundaries are known.
#[test]
fn dom_html_latest_review_implied_body_blanks() {
    let out = compile_and_run(r#"<?php
$doc = new DOMDocument();
foreach ([0, LIBXML_NOBLANKS] as $extra) {
    $doc->loadHTML('<html><head></head> <div>A</div> <b>B</b> </html>', LIBXML_HTML_NODEFDTD | LIBXML_NOERROR | $extra);
    echo $doc->saveXML($doc->getElementsByTagName('html')->item(0)), ':', $doc->getElementsByTagName('body')->item(0)->childNodes->length, '|';
}
"#);
    assert_eq!(out, "<html><head/> <body><div>A</div> <b>B</b> </body></html>:4|<html><head/><body><div>A</div> <b>B</b> </body></html>:4|");
}

/// Legacy frame and isindex tags are void and leave following text beside them.
#[test]
fn dom_html_latest_review_legacy_void_elements() {
    let out = compile_and_run(r#"<?php
$doc = new DOMDocument();
$doc->loadHTML('<div>A<frame>B<isindex>C</div>', LIBXML_HTML_NODEFDTD | LIBXML_NOERROR);
$div = $doc->getElementsByTagName('div')->item(0);
echo $doc->saveXML($div), ':', $div->childNodes->length, ':', $div->nodeValue;
"#);
    assert_eq!(out, "<div>A<frame/>B<isindex/>C</div>:5:ABC");
}

/// A stray slash consumes its malformed attribute run but does not self-close a tag.
#[test]
fn dom_html_latest_review_stray_attribute_slash() {
    let out = compile_and_run(r#"<?php
$doc = new DOMDocument();
foreach (['<div /class=x /id=y>A</div><b>Z</b>', '<div a/b=c d=e>A</div><b>Z</b>', '<div / / >A</div><b>Z</b>', '<div a=x/ b=y>A</div><b>Z</b>'] as $source) {
    $doc->loadHTML($source, LIBXML_HTML_NODEFDTD | LIBXML_NOERROR);
    echo $doc->saveXML($doc->getElementsByTagName('body')->item(0)), '|';
}
"#);
    assert_eq!(out, "<body><div>A</div><b>Z</b></body>|<body><div a=\"\" d=\"e\">A</div><b>Z</b></body>|<body><div>A</div><b>Z</b></body>|<body><div a=\"x/\" b=\"y\">A</div><b>Z</b></body>|");
}

/// Reopened-body construction is independent of EIR optimization.
#[test]
fn dom_html_latest_review_head_only_reopened_body_no_ir_opt() {
    without_ir_opt(dom_html_latest_review_head_only_reopened_body);
}

/// Ignoring redundant html tags is independent of EIR optimization.
#[test]
fn dom_html_latest_review_redundant_html_no_ir_opt() {
    without_ir_opt(dom_html_latest_review_redundant_html);
}

/// Post-head metadata placement is independent of EIR optimization.
#[test]
fn dom_html_latest_review_post_head_metadata_no_ir_opt() {
    without_ir_opt(dom_html_latest_review_post_head_metadata);
}

/// Implied-body whitespace is preserved without EIR optimization.
#[test]
fn dom_html_latest_review_implied_body_blanks_no_ir_opt() {
    without_ir_opt(dom_html_latest_review_implied_body_blanks);
}

/// Legacy void-element boundaries are independent of EIR optimization.
#[test]
fn dom_html_latest_review_legacy_void_elements_no_ir_opt() {
    without_ir_opt(dom_html_latest_review_legacy_void_elements);
}

/// Attribute slash recovery is independent of EIR optimization.
#[test]
fn dom_html_latest_review_stray_attribute_slash_no_ir_opt() {
    without_ir_opt(dom_html_latest_review_stray_attribute_slash);
}

/// Isolates one wrapper ownership path (a metadata/whitespace tree, or a reopened html root).
/// Rewritten from the removed `__elephc_dom_*` builder helpers onto the public DOM API; the
/// expected output is php 8.5's own (libxml 2.9.13).
fn verify_literal_tree_owners(reopened: bool, flags: u32) {
    let source = if reopened {
        r#"<?php
for ($i = 0; $i < 8; $i++) {
    $doc = new DOMDocument();
    $doc->loadHTML('<html><head></head></html><html><div>A</div></html>', LIBXML_HTML_NODEFDTD | LIBXML_NOERROR | FLAGS);
    echo $doc->getElementsByTagName('body')->item(0)->nodeValue, '|';
    unset($doc);
}
"#
    } else {
        r#"<?php
for ($i = 0; $i < 8; $i++) {
    $doc = new DOMDocument();
    $doc->loadHTML('<html><head></head> <title>T</title><div>A</div> </html>', LIBXML_HTML_NODEFDTD | LIBXML_NOERROR | FLAGS);
    echo $doc->getElementsByTagName('html')->item(0)->nodeValue, '|';
    unset($doc);
}
"#
    }
    .replace("FLAGS", &flags.to_string());
    let out = compile_and_run_with_heap_debug(&source);
    assert!(out.success, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    let expected = match (reopened, flags) {
        (true, _) => "A|",
        (false, 0) => " TA |",
        (false, _) => "TA |",
    };
    assert_eq!(out.stdout, expected.repeat(8));
}

/// Literal metadata and whitespace trees retire the new body and filter owners.
#[test]
fn dom_html_latest_review_tree_owners() {
    verify_literal_tree_owners(false, 256);
}

/// Body and whitespace temporary owners stay balanced without EIR optimization.
#[test]
fn dom_html_latest_review_tree_owners_no_ir_opt() {
    without_ir_opt(dom_html_latest_review_tree_owners);
}

/// A head-only document's reopened body retires all temporary wrapper owners.
#[test]
fn dom_html_latest_review_reopened_tree_owners() {
    verify_literal_tree_owners(true, 256);
}

/// Reopened-body temporary owners stay balanced without EIR optimization.
#[test]
fn dom_html_latest_review_reopened_tree_owners_no_ir_opt() {
    without_ir_opt(dom_html_latest_review_reopened_tree_owners);
}

/// The reopened-body ownership control also covers parsing without blank filtering.
#[test]
fn dom_html_latest_review_reopened_tree_owners_no_flags() {
    verify_literal_tree_owners(true, 0);
}

/// Reopened-body ownership without flags is independent of EIR optimization.
#[test]
fn dom_html_latest_review_reopened_tree_owners_no_flags_no_ir_opt() {
    without_ir_opt(dom_html_latest_review_reopened_tree_owners_no_flags);
}
