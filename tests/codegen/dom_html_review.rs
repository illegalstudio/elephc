//! Purpose:
//! Covers user-class ownership and tree construction regressions in the DOM HTML subset.
//!
//! Called from:
//! - The DOM HTML codegen integration tests.
//!
//! Key details:
//! - Closing tags match open element names; comments retain their own value only.
//! - User-owned DOM names must not trigger conflicting prelude declarations.
//! - Generic type references inject DOM before monomorphization, without stealing user names.

use crate::support::*;

/// Legacy PHP keeps content after a closed body beside it, not inside the selected body.
#[test]
fn dom_html_ninth_review_closed_body_following_content() {
    let out = compile_and_run(r#"<?php
$doc = new DOMDocument();
$doc->loadHTML('<body>A</body><div>B</div>', LIBXML_HTML_NODEFDTD | LIBXML_NOERROR);
$body = $doc->getElementsByTagName('body')->item(0);
$div = $doc->getElementsByTagName('div')->item(0);
$html = $doc->getElementsByTagName('html')->item(0);
echo $body->nodeValue, ':', $body->childNodes->length, ':';
echo $div->parentNode === $html && $body->nextSibling === $div ? 'siblings' : 'bad';
echo ':', $doc->saveXML($html);
"#);
    assert_eq!(out, "A:1:siblings:<html><body>A</body><div>B</div></html>");
}

/// Legacy libxml reopens an html child without duplicating body or moving the document instruction.
#[test]
fn dom_html_ninth_review_closed_document_following_content() {
    let out = compile_and_run(r#"<?php
$doc = new DOMDocument();
foreach ([0, LIBXML_NOBLANKS] as $extra) {
    $doc->loadHTML('<html><body>A</body></html><?worker data><div>B</div><!--inside--><div>C</div>', LIBXML_HTML_NODEFDTD | LIBXML_NOERROR | $extra);
    $html = $doc->getElementsByTagName('html')->item(0);
    $body = $doc->getElementsByTagName('body')->item(0);
    $reopened = $html->childNodes->item(1);
    $instruction = $doc->childNodes->item(1);
    echo $doc->getElementsByTagName('html')->length, ':', $doc->getElementsByTagName('body')->length, ':';
    echo $body->nodeValue, ':', $reopened->nodeName, ':', $reopened->childNodes->length, '|';
    echo $html->nextSibling === $instruction && $instruction->previousSibling === $html ? 'linked' : 'bad';
    echo ':', $instruction->parentNode === $doc ? 'document' : 'bad';
    echo ':', $doc->saveXML();
}
"#);
    assert_eq!(out, "2:1:A:html:3|linked:document:<html><body>A</body><html><div>B</div><!--inside--><div>C</div></html></html><?worker data?>".repeat(2));
}

/// Reopening a literal document tree keeps every temporary wrapper owner balanced.
#[test]
fn dom_html_ninth_review_reopened_tree_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function enableDom(DOMDocument $doc): void {}
for ($i = 0; $i < 16; $i++) {
    $text = __elephc_dom_new_node('text', '#text', 'A', [], []);
    $body = __elephc_dom_new_node('element', 'body', '', [], [$text]);
    $html = __elephc_dom_new_node('element', 'html', '', [], [$body]);
    $instruction = __elephc_dom_new_node('pi', 'worker', 'data', [], []);
    $later = __elephc_dom_new_node('element', 'div', '', [], [__elephc_dom_new_node('text', '#text', 'B', [], [])]);
    $tree = __elephc_dom_document_from_roots([$html, $instruction, $later], 0);
    echo count($tree['children']), ':', __elephc_dom_text_of($tree), ':';
    echo $tree['children'][0]['children'][1]['name'], ':', $tree['children'][1]['name'], '|';
    unset($tree);
}
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "2:AB:html:worker|".repeat(16), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Closed-body sibling behavior remains independent of EIR optimization.
#[test]
fn dom_html_ninth_review_closed_body_no_ir_opt() {
    without_ir_opt(dom_html_ninth_review_closed_body_following_content);
}

/// Reopened-document topology and instruction links remain independent of EIR optimization.
#[test]
fn dom_html_ninth_review_closed_document_no_ir_opt() {
    without_ir_opt(dom_html_ninth_review_closed_document_following_content);
}

/// Reopened-document temporary owners remain balanced without EIR optimization.
#[test]
fn dom_html_ninth_review_reopened_tree_no_ir_opt() {
    without_ir_opt(dom_html_ninth_review_reopened_tree_owners);
}

/// Trailing comments remain document siblings without introducing a second html/body wrapper.
#[test]
fn dom_html_eighth_review_explicit_document_tail_comments() {
    let out = compile_and_run(r#"<?php
$doc = new DOMDocument();
foreach ([0, LIBXML_NOBLANKS] as $extra) {
    $doc->loadHTML('<html><body>x</body></html><!--tail-->', LIBXML_HTML_NODEFDTD | LIBXML_NOERROR | $extra);
    $html = $doc->getElementsByTagName('html')->item(0);
    $body = $doc->getElementsByTagName('body')->item(0);
    echo $doc->getElementsByTagName('html')->length, ':', $doc->getElementsByTagName('body')->length, ':';
    echo $doc->childNodes->length, ':', $body->nodeValue, ':', $doc->saveXML($html), '|';
    $tail = $doc->childNodes->item(1);
    echo $tail instanceof DOMComment ? 'C' : 'bad';
    echo ':', $tail->nodeValue, ':', $doc->saveXML($tail), '|';
}
"#);
    assert_eq!(out, "1:1:2:x:<html><body>x</body></html>|C:tail:<!--tail-->|".repeat(2));
}

/// Explicit HTML document padding is ignored independently of fragment NOBLANKS filtering.
#[test]
fn dom_html_eighth_review_explicit_document_tail_whitespace() {
    let out = compile_and_run(r#"<?php
$doc = new DOMDocument();
foreach ([0, LIBXML_NOBLANKS] as $extra) {
    foreach (["<html><body>x</body></html> \n\t", "<html><body>x</body></html> <!--tail--> \n"] as $source) {
        $doc->loadHTML($source, LIBXML_HTML_NODEFDTD | LIBXML_NOERROR | $extra);
        $html = $doc->getElementsByTagName('html')->item(0);
        echo $doc->getElementsByTagName('html')->length, ':', $doc->getElementsByTagName('body')->length, ':';
        echo $html->childNodes->length, ':', $doc->saveXML($html), '|';
    }
}
"#);
    assert_eq!(out, "1:1:1:<html><body>x</body></html>|".repeat(4));
}

/// Leading and trailing document nodes preserve their order, parents and bidirectional links.
#[test]
fn dom_html_eighth_review_explicit_document_siblings() {
    let out = compile_and_run(r#"<?php
$doc = new DOMDocument();
$doc->loadHTML('<!--lead--><html><head></head><body>x</body></html><!--tail--><?Tail data?>', LIBXML_HTML_NODEFDTD | LIBXML_NOERROR);
$lead = $doc->childNodes->item(0); $html = $doc->childNodes->item(1);
$tail = $doc->childNodes->item(2); $pi = $doc->childNodes->item(3);
echo $doc->childNodes->length, ':', $html->nodeName, ':', $doc->getElementsByTagName('body')->item(0)->nodeValue, '|';
echo $lead->nextSibling === $html && $html->previousSibling === $lead ? 'L' : 'bad';
echo $html->nextSibling === $tail && $tail->previousSibling === $html ? 'T' : 'bad';
echo $tail->nextSibling === $pi && $pi->previousSibling === $tail ? 'P' : 'bad';
echo $tail->parentNode === $doc && $pi->parentNode === $doc ? 'D' : 'bad';
if ($pi instanceof DOMProcessingInstruction) {
    echo ':I:', $pi->target, ':', $pi->data;
} else { echo ':bad'; }
echo ':', $doc->saveXML($html);
"#);
    assert_eq!(out, "4:html:x|LTPD:I:Tail:data?:<html><head/><body>x</body></html>");
}

/// An explicit body remains the sole body while its trailing comment stays inside the html wrapper.
#[test]
fn dom_html_eighth_review_explicit_body_tail() {
    let out = compile_and_run(r#"<?php
$doc = new DOMDocument();
$doc->loadHTML('<body>x</body><!--tail-->', LIBXML_HTML_NODEFDTD | LIBXML_NOERROR);
$html = $doc->getElementsByTagName('html')->item(0);
$tail = $html->childNodes->item(1);
echo $doc->getElementsByTagName('body')->length, ':', $html->childNodes->length, ':';
echo $tail instanceof DOMComment ? 'C' : 'bad';
echo ':', $tail->parentNode === $html ? 'H' : 'bad';
echo ':', $doc->saveXML($html);
"#);
    assert_eq!(out, "1:2:C:H:<html><body>x</body><!--tail--></html>");
}

/// Wrapper selection retires trailing-node partitions without tokenizer or cyclic DOM owners.
#[test]
fn dom_html_eighth_review_trailing_tree_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function enableDom(DOMDocument $doc): void {}
for ($i = 0; $i < 16; $i++) {
    $text = __elephc_dom_new_node('text', '#text', 'x', [], []);
    $body = __elephc_dom_new_node('element', 'body', '', [], [$text]);
    $html = __elephc_dom_new_node('element', 'html', '', [], [$body]);
    $tail = __elephc_dom_new_node('comment', '#comment', 'tail', [], []);
    $padding = __elephc_dom_new_node('text', '#text', ' ', [], []);
    $tree = __elephc_dom_document_from_roots([$html, $tail, $padding], 0);
    echo count($tree['children']), ':', __elephc_dom_text_of($tree), ':';
    echo $tree['children'][1]['value'], '|';
    unset($tree);
}
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "2:x:tail|".repeat(16), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Wrapper selection and trailing document-node ownership do not depend on IR optimization.
#[test]
fn dom_html_eighth_review_without_ir_opt() {
    crate::support::without_ir_opt(|| {
        dom_html_eighth_review_explicit_document_tail_comments();
        dom_html_eighth_review_explicit_document_tail_whitespace();
        dom_html_eighth_review_explicit_document_siblings();
        dom_html_eighth_review_explicit_body_tail();
        dom_html_eighth_review_trailing_tree_owners();
    });
}

/// NOBLANKS filters html/head blanks while preserving title/body text and the no-flag control.
#[test]
fn dom_html_seventh_review_noblanks_wrappers() {
    let out = compile_and_run(r#"<?php
$flags = LIBXML_NOERROR | LIBXML_COMPACT | LIBXML_HTML_NODEFDTD | LIBXML_NOBLANKS | LIBXML_NOXMLDECL;
$doc = new DOMDocument();
foreach (["<html><body><div>Hi</div></body> </html>", "<html> <body><div>Hi</div></body></html>",
    "<html>\n<head></head>\n<body>x</body>\n</html>",
    "<html> <head> <title> A </title> </head> <body> <div>Hi</div> </body> </html>"] as $source) {
    $doc->loadHTML($source, $flags);
    $html = $doc->getElementsByTagName('html')->item(0);
    echo $html->childNodes->length, ':', json_encode($html->nodeValue), ':', $doc->saveXML($html), '|';
}
$doc->loadHTML('<html> <body>x</body> </html>', $flags & ~LIBXML_NOBLANKS);
$html = $doc->getElementsByTagName('html')->item(0);
echo $html->childNodes->length, ':', json_encode($html->nodeValue), ':', $doc->saveXML($html);
"#);
    assert_eq!(out, "1:\"Hi\":<html><body><div>Hi</div></body></html>|1:\"Hi\":<html><body><div>Hi</div></body></html>|2:\"x\":<html><head/><body>x</body></html>|2:\" A  Hi \":<html><head><title> A </title></head><body> <div>Hi</div> </body></html>|3:\" x \":<html> <body>x</body> </html>");
}

/// XML attributes use uppercase hexadecimal Unicode references while text remains UTF-8.
#[test]
fn dom_html_seventh_review_unicode_attributes() {
    let out = compile_and_run(r#"<?php
$doc = new DOMDocument();
$flags = LIBXML_NOERROR | LIBXML_COMPACT | LIBXML_HTML_NODEFDTD | LIBXML_NOBLANKS | LIBXML_NOXMLDECL;
foreach (['é', 'a b', '€😀', '&é"\''] as $value) {
    $doc->loadHTML('<?xml encoding="UTF-8"><div title="' . htmlspecialchars($value, ENT_COMPAT) . '">' . $value . '</div>', $flags);
    $div = $doc->getElementsByTagName('div')->item(0);
    echo $doc->saveXML($div), '|';
}
"#);
    assert_eq!(out, "<div title=\"&#xE9;\">é</div>|<div title=\"a&#xA0;b\">a b</div>|<div title=\"&#x20AC;&#x1F600;\">€😀</div>|<div title=\"&amp;&#xE9;&quot;'\">&amp;é\"'</div>|");
}

/// Unicode references are inserted after existing XML escapes and cannot escape their own ampersand.
#[test]
fn dom_html_seventh_review_unicode_attribute_controls() {
    let out = compile_and_run(r#"<?php
$doc = new DOMDocument();
$doc->loadHTML("<?xml encoding=\"UTF-8\"><div title=\"é&#13;&#10;&#9;&amp;#xE9;\">é</div>", LIBXML_NOERROR | LIBXML_HTML_NODEFDTD);
$div = $doc->getElementsByTagName('div')->item(0);
echo $doc->saveXML($div);
"#);
    assert_eq!(out, "<div title=\"&#xE9;&#13;&#10;&#9;&amp;#xE9;\">é</div>");
}

/// Repeated attribute serialization releases its string arguments and intermediate byte probes.
#[test]
fn dom_html_seventh_review_unicode_attribute_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function enableDom(DOMDocument $doc): void {}
for ($i = 0; $i < 16; $i++) { echo __elephc_dom_escape_attr("é€😀"); }
echo '|', __elephc_dom_escape_attr('plain'), '|', __elephc_dom_escape_attr('');
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, format!("{}|plain|", "&#xE9;&#x20AC;&#x1F600;".repeat(16)), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Both parser and serializer corrections remain observable with EIR optimization disabled.
#[test]
fn dom_html_seventh_review_without_ir_opt() {
    crate::support::without_ir_opt(|| {
        dom_html_seventh_review_noblanks_wrappers();
        dom_html_seventh_review_unicode_attributes();
        dom_html_seventh_review_unicode_attribute_controls();
        dom_html_seventh_review_unicode_attribute_owners();
    });
}

/// Termwind's NOBLANKS drops document padding, not interior or explicit-body spaces.
#[test]
fn dom_html_latest_review_document_padding() {
    let out = compile_and_run(r#"<?php
$doc = new DOMDocument();
$flags = LIBXML_NOERROR | LIBXML_COMPACT | LIBXML_HTML_NODEFDTD | LIBXML_NOBLANKS | LIBXML_NOXMLDECL;
foreach ([" <div>Hi</div> ", "\n\t<div>Hi</div>\n\t", "<div>A</div> <b>B</b> ", "<body> <div>Hi</div> </body> "] as $source) {
    $doc->loadHTML($source, $flags);
    $body = $doc->getElementsByTagName('body')->item(0);
    echo $body->childNodes->length, ':';
    foreach ($body->childNodes as $child) { echo $child->nodeName, ','; }
    echo $doc->saveXML($body), '|';
}
$doc->loadHTML(' <div>Hi</div> ', LIBXML_HTML_NODEFDTD | LIBXML_NOERROR);
$body = $doc->getElementsByTagName('body')->item(0);
echo $body->childNodes->length, ':', $doc->saveXML($body);
"#);
    assert_eq!(out, "1:div,<body><div>Hi</div></body>|1:div,<body><div>Hi</div></body>|3:div,#text,b,<body><div>A</div> <b>B</b></body>|3:#text,div,#text,<body> <div>Hi</div> </body>|2:<body><div>Hi</div> </body>");
}

/// Native direct and mixed count calls dispatch through the DOM list's Countable contract.
#[test]
fn dom_html_latest_review_countable_lists() {
    let out = compile_and_run(r#"<?php
function size(mixed $value): int { return count($value); }
$doc = new DOMDocument();
$doc->loadHTML('<div><span>A</span><b>B</b><span>C</span></div>', LIBXML_HTML_NODEFDTD | LIBXML_NOERROR);
$list = $doc->getElementsByTagName('span');
echo count($list), ':', size($list), ':', count($doc->childNodes), ':';
echo count($doc->getElementsByTagName('missing')), ':';
echo $list instanceof Countable ? 'Y' : 'bad';
"#);
    assert_eq!(out, "2:2:1:0:Y");
}

/// Eval observes the same registered Countable implementation as native code.
#[test]
fn dom_html_latest_review_eval_countable_list() {
    let out = compile_and_run(r#"<?php
function enableDom(DOMDocument $doc): void {}
echo eval('$doc = new DOMDocument(); $doc->loadHTML("<div>X</div>"); return count($doc->getElementsByTagName("div"));');
"#);
    assert_eq!(out, "1");
}

/// Counting does not retain extra list owners or consume its independent traversal state.
#[test]
fn dom_html_latest_review_countable_list_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function size(mixed $value): int { return count($value); }
$items = new DOMNodeList([1, 2]);
for ($i = 0; $i < 16; $i++) { echo size($items); }
foreach ($items as $item) { echo $item; }
unset($items);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "222222222222222212", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Repeated nested traversals, including early inner exits, release their fresh iterators.
#[test]
fn dom_html_sixth_review_iterator_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function walk(DOMNodeList $items): int {
    $count = 0;
    foreach ($items as $outer) {
        foreach ($items as $inner) { $count++; break; }
        $count++;
    }
    return $count;
}
$items = new DOMNodeList([1, 2]);
for ($i = 0; $i < 16; $i++) { echo walk($items) === 4 ? 'Y' : 'bad'; }
unset($items);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "YYYYYYYYYYYYYYYY", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Eval dispatch can construct and query the prelude that the host program registers.
#[test]
fn dom_html_sixth_review_eval_registered_surface() {
    let out = compile_and_run(r#"<?php
function enableDom(DOMDocument $doc): void {}
echo eval('$doc = new DOMDocument(); $doc->loadHTML("<div>X</div>"); return $doc->getElementsByTagName("div")->item(0)->nodeValue;');
"#);
    assert_eq!(out, "X");
}

/// The real CLI repeats DOM discovery after loading a namespaced Composer renderer.
#[test]
fn dom_html_sixth_review_autoload_cli() {
    let dir = make_cli_test_dir("elephc_dom_autoload_sixth");
    let result: std::io::Result<(std::process::Output, Option<std::process::Output>)> = (|| {
        std::fs::create_dir_all(dir.join("src"))?;
        std::fs::write(dir.join("composer.json"), r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#)?;
        std::fs::write(dir.join("src/Renderer.php"), r#"<?php
namespace App;
use DOMDocument as Document;
class Renderer {
    public function render(): string {
        $doc = new Document();
        $doc->loadHTML('<div>Hello</div>');
        return $doc->getElementsByTagName('div')->item(0)->nodeValue;
    }
}
"#)?;
        std::fs::write(dir.join("main.php"), "<?php echo (new App\\Renderer())->render();")?;
        let compiled = elephc_cli_command(&dir).arg("main.php").output()?;
        let output = if compiled.status.success() {
            Some(std::process::Command::new(dir.join("main")).output()?)
        } else { None };
        Ok((compiled, output))
    })();
    std::fs::remove_dir_all(&dir).expect("clean up CLI fixture on both success and failure");
    let (compiled, output) = result.expect("run CLI fixture");
    assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
    let output = output.expect("successful compilation must run");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "Hello");
}

/// Runtime existence probes verify the registry's native and eval class support declarations.
#[test]
fn dom_html_sixth_review_class_catalog_support() {
    let out = compile_and_run(r#"<?php
$doc = new DOMDocument();
echo class_exists('DOMCharacterData') ? '1' : '0';
echo class_exists('DOMComment') ? '1' : '0';
echo class_exists('DOMDocument') ? '1' : '0';
echo class_exists('DOMElement') ? '1' : '0';
echo class_exists('DOMNode') ? '1' : '0';
echo class_exists('DOMNodeList') ? '1' : '0';
echo class_exists('DOMProcessingInstruction') ? '1' : '0';
echo class_exists('DOMText') ? '1' : '0';
echo '|';
echo eval('$names = ["DOMCharacterData", "DOMComment", "DOMDocument", "DOMElement", "DOMNode", "DOMNodeList", "DOMProcessingInstruction", "DOMText"]; $bits = ""; foreach ($names as $name) { $bits .= class_exists($name, false) ? "1" : "0"; } return $bits;');
"#);
    assert_eq!(out, "11111111|11111111");
}

/// Descendant queries must not consume the iterator of an active child traversal.
#[test]
fn dom_html_sixth_review_query_during_child_walk() {
    let out = compile_and_run(r#"<?php
$doc = new DOMDocument();
$doc->loadHTML('<div><span>A</span><b>B</b><span>C</span></div>');
$div = $doc->getElementsByTagName('div')->item(0);
foreach ($div->childNodes as $key => $child) {
    echo $key, ':', $child->nodeName, ':', $div->getElementsByTagName('span')->length, ';';
}
"#);
    assert_eq!(out, "0:span:2;1:b:2;2:span:2;");
}

/// Every nested traversal of the same list owns an independent cursor.
#[test]
fn dom_html_sixth_review_nested_child_walks() {
    let out = compile_and_run(r#"<?php
$doc = new DOMDocument();
$doc->loadHTML('<div><span>A</span><b>B</b></div>');
$list = $doc->getElementsByTagName('div')->item(0)->childNodes;
foreach ($list as $outerKey => $outer) {
    echo $outerKey, ':', $outer->nodeName, '[';
    foreach ($list as $innerKey => $inner) { echo $innerKey, ':', $inner->nodeName, ';'; }
    echo ']';
}
"#);
    assert_eq!(out, "0:span[0:span;1:b;]1:b[0:span;1:b;]");
}

/// A renderer loaded through Composer can be the only source of DOM references.
#[test]
fn dom_html_sixth_review_autoload_renderer() {
    let out = compile_and_run_files(&[
        ("composer.json", r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#),
        ("src/Renderer.php", r#"<?php
namespace App;
use DOMDocument as Document;
class Renderer {
    public function render(): string {
        $doc = new Document();
        $doc->loadHTML('<div>Hello</div>');
        return $doc->getElementsByTagName('div')->item(0)->nodeValue;
    }
}
"#),
        ("main.php", "<?php echo (new App\\Renderer())->render();"),
    ], "main.php");
    assert_eq!(out, "Hello");
}

/// Leading comments and instructions remain document siblings, not Termwind body children.
#[test]
fn dom_html_fifth_review_document_leading_nodes() {
    let out = compile_and_run_capture(r#"<?php
$dom = new DOMDocument();
$flags = LIBXML_HTML_NODEFDTD | LIBXML_NOERROR;
$dom->loadHTML("<!--lead--> \n<?worker data?><div>X</div>", $flags);
echo $dom->childNodes->length, '|';
foreach ($dom->childNodes as $child) {
    echo $child->nodeName, ':';
    echo $child->parentNode === $dom ? 'P' : 'bad';
    echo $child->ownerDocument === $dom ? 'O' : 'bad';
    echo '|';
}
$body = $dom->getElementsByTagName('body')->item(0);
echo $body->childNodes->length, ':', $body->childNodes->item(0)->nodeName, '|';
$first = $dom->childNodes->item(0);
$pi = $first->nextSibling;
echo $pi instanceof DOMProcessingInstruction ? 'I' : 'bad';
echo $pi->nextSibling->nodeName, ':', $pi->previousSibling === $first ? 'S' : 'bad', '|';
$dom->loadHTML('<!--only-->', $flags);
echo $dom->childNodes->length, ':', $dom->getElementsByTagName('body')->length, ':';
echo $dom->childNodes->item(0)->nodeName, '|';
$dom->loadHTML('<?worker data?>', $flags);
echo $dom->childNodes->length, ':', $dom->getElementsByTagName('html')->length, ':';
echo $dom->childNodes->item(0)->nodeName;
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "3|#comment:PO|worker:PO|html:PO|1:div|Ihtml:S|1:0:#comment|1:0:worker", "{}", out.stderr);
    assert!(out.stderr.is_empty(), "{}", out.stderr);
}

/// Discarded unmatched closing tags do not split adjacent text node identities.
#[test]
fn dom_html_fifth_review_ignored_end_tag_text_coalescing() {
    let out = compile_and_run_capture(r#"<?php
$dom = new DOMDocument();
$flags = LIBXML_HTML_NODEFDTD | LIBXML_NOERROR;
$dom->loadHTML('<div>A</not>B</missing>C</div>', $flags);
$div = $dom->getElementsByTagName('div')->item(0);
$text = $div->childNodes->item(0);
echo $div->childNodes->length, ':', $text->nodeName, ':', $text->nodeValue, ':';
echo $text->nextSibling === null ? 'N' : 'bad', '|', $dom->saveXML($div), '|';
$dom->loadHTML('<div>A<span>B</span>C</div>', $flags);
$div = $dom->getElementsByTagName('div')->item(0);
echo $div->childNodes->length, ':', $div->childNodes->item(1)->nodeName;
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "1:#text:ABC:N|<div>ABC</div>|3:span", "{}", out.stderr);
    assert!(out.stderr.is_empty(), "{}", out.stderr);
}

/// Legacy HTML parsing drops an unfinished comment tail without creating a comment node.
#[test]
fn dom_html_fifth_review_unterminated_comment() {
    let out = compile_and_run_capture(r#"<?php
$dom = new DOMDocument();
$flags = LIBXML_HTML_NODEFDTD | LIBXML_NOERROR;
$dom->loadHTML('<div>a<!--b</div>c', $flags);
$div = $dom->getElementsByTagName('div')->item(0);
echo $div->childNodes->length, ':', $div->nodeValue, '|', $dom->saveXML($div), '|';
$dom->loadHTML('<!--unfinished', $flags);
echo $dom->childNodes->length, ':', $dom->getElementsByTagName('body')->length;
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "1:a|<div>a</div>|0:0", "{}", out.stderr);
    assert!(out.stderr.is_empty(), "{}", out.stderr);
}

/// Consecutive instructions preserve typed PI fields and links across every sibling kind.
#[test]
fn dom_html_fourth_review_processing_instruction_fields() {
    let out = compile_and_run_capture(r#"<?php
$dom = new DOMDocument();
$dom->loadHTML('<div><?One a&b><?Two?><span>x</span><!--c--><?Three></div>');
$div = $dom->getElementsByTagName('div')->item(0);
echo $div->childNodes->length, ':', $div->nodeValue, '|';
foreach ($div->childNodes as $child) {
    if ($child instanceof DOMProcessingInstruction) {
        echo $child->target, ':', $child->data, ':';
        echo $child->previousSibling === null ? 'N' : $child->previousSibling->nodeName, ':';
        echo $child->nextSibling === null ? 'N' : $child->nextSibling->nodeName, '|';
        echo $child instanceof DOMNode ? 'B' : 'bad';
        echo $child instanceof DOMCharacterData ? 'bad' : 'P';
    }
}
echo '|', $dom->saveXML($div);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "5:x|One:a&b:N:Two|BPTwo:?:One:span|BPThree::#comment:N|BP|<div><?One a&b?><?Two ??><span>x</span><!--c--><?Three?></div>", "{}", out.stderr);
    assert!(out.stderr.is_empty(), "{}", out.stderr);
}

/// A global user-owned PI name, even case-folded, must not inject a conflicting prelude.
#[test]
fn dom_html_fourth_review_user_processing_instruction() {
    let out = compile_and_run(r#"<?php
class DOMProcessingInstruction { public string $marker = 'user'; }
$pi = new \domprocessinginstruction();
echo $pi->marker;
"#);
    assert_eq!(out, "user");
}

/// Closed HTML processing instructions retain their own value and sibling position, not parent text.
#[test]
fn dom_html_fourth_review_processing_instructions() {
    let out = compile_and_run_capture(r#"<?php
$dom = new DOMDocument();
$dom->loadHTML('<div>a<?php echo 1; ?>b</div>');
$div = $dom->getElementsByTagName('div')->item(0);
echo $div->childNodes->length, '|', $div->nodeValue, '|', $dom->saveXML($div), '|';
$pi = $div->childNodes->item(1);
echo get_class($pi), ':', $pi->nodeName, ':', $pi->nodeValue, '|';
echo $pi->previousSibling->nodeValue, $pi->nextSibling->nodeValue, '|';
echo $pi->parentNode === $div ? 'P' : 'bad', $pi->ownerDocument === $dom ? 'O' : 'bad';
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "3|ab|<div>a<?php echo 1; ??>b</div>|DOMProcessingInstruction:php:echo 1; ?|ab|PO", "{}", out.stderr);
    assert!(out.stderr.is_empty(), "{}", out.stderr);
}

/// PI targets preserve case and legacy HTML recovery retains unterminated target text.
#[test]
fn dom_html_fourth_review_processing_instruction_recovery() {
    let out = compile_and_run_capture(r#"<?php
$dom = new DOMDocument();
foreach (['<div>a<?Target Data>b</div>', '<div>a<?foo>b</div>', '<div>a<?foo?>b</div>', '<div>a<?unclosed', '<div>a<?foo bar', '<div>a<?>b</div>', '<div>a<?1 bad>b</div>'] as $html) {
    $dom->loadHTML($html);
    $div = $dom->getElementsByTagName('div')->item(0);
    echo $div->nodeValue, '|', $div->childNodes->length, '|', $dom->saveXML($div), ';';
}
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "ab|3|<div>a<?Target Data?>b</div>;ab|3|<div>a<?foo?>b</div>;ab|3|<div>a<?foo ??>b</div>;aunclosed|1|<div>aunclosed</div>;a|1|<div>a</div>;a>b|1|<div>a&gt;b</div>;a1 bad>b|1|<div>a1 bad&gt;b</div>;", "{}", out.stderr);
    assert!(out.stderr.is_empty(), "{}", out.stderr);
}

/// HTML parse normalizes names, while the caller's tag and attribute queries remain literal.
#[test]
fn dom_html_fourth_review_literal_lookups() {
    let out = compile_and_run_capture(r#"<?php
$dom = new DOMDocument();
$dom->loadHTML('<DIV CLASS="X"><SPAN TITLE="Y">Hi</SPAN></DIV>');
$div = $dom->getElementsByTagName('div')->item(0);
echo $div->nodeName, '|', $div->getAttribute('class'), ':', $div->getAttribute('CLASS'), '|';
echo $dom->getElementsByTagName('DIV')->length, ':', $dom->getElementsByTagName('div')->length, '|';
echo $div->getElementsByTagName('SPAN')->length, ':', $div->getElementsByTagName('span')->length, ':', $div->getElementsByTagName('*')->length;
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "div|X:|0:1|0:1:1", "{}", out.stderr);
    assert!(out.stderr.is_empty(), "{}", out.stderr);
}

/// Duplicate parsed attributes keep the first value, including empty and boolean attributes.
#[test]
fn dom_html_fourth_review_duplicate_attributes() {
    let out = compile_and_run_capture(r#"<?php
$dom = new DOMDocument();
$dom->loadHTML('<div class="a" CLASS="b" id="1" id="2" title="" title="later">x</div><input disabled="" DISABLED checked checked="no">');
$div = $dom->getElementsByTagName('div')->item(0);
$input = $dom->getElementsByTagName('input')->item(0);
echo $dom->saveXML($div), '|', $div->getAttribute('class'), '|', $div->getAttribute('title'), '|';
echo $input->getAttribute('disabled'), ':', $input->getAttribute('checked'), '|', $dom->saveXML($input);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "<div class=\"a\" id=\"1\" title=\"\">x</div>|a||:checked|<input disabled=\"\" checked=\"checked\"/>", "{}", out.stderr);
    assert!(out.stderr.is_empty(), "{}", out.stderr);
}

/// Entity helper temporaries and the static named table release without constructing a DOM tree.
#[test]
fn dom_html_third_review_entity_decoder_heap_clean() {
    for probe in [
        "echo __elephc_dom_named_entity_codepoint('AElig');",
        "echo __elephc_dom_entity_utf8(128512);",
        "echo __elephc_dom_decode_entities('plain');",
        "echo __elephc_dom_decode_entities('&AElig;');",
        "echo __elephc_dom_decode_entities('&#65');",
    ] {
        let source = format!("<?php function enableDomPrelude(DOMDocument $unused): void {{}} {probe}");
        let result = compile_and_run_with_heap_debug(&source);
        assert!(result.success, "{probe}: {}", result.stderr);
        assert!(result.stderr.contains("HEAP DEBUG: leak summary: clean"), "{probe}: {}", result.stderr);
    }
    let out = compile_and_run_with_heap_debug(r###"<?php
function enableDomPrelude(DOMDocument $unused): void {}
for ($i = 0; $i < 3; $i++) {
    echo __elephc_dom_decode_entities('&AElig;&#x1F600;&amp;lt; &unknown; &#65'), '|';
}
"###);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "Æ😀&lt; &unknown; A|Æ😀&lt; &unknown; A|Æ😀&lt; &unknown; A|", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Text, quoted and unquoted attributes decode legacy named and numeric HTML references.
#[test]
fn dom_html_third_review_entities() {
    let out = compile_and_run_capture(r###"<?php
$dom = new DOMDocument();
$dom->loadHTML('<div title="&euro;&#x1F600;&#60;&apos;" data=&nbsp;&#65;>a&nbsp;b&#60;c&apos;d</div>');
$div = $dom->getElementsByTagName('div')->item(0);
echo $div->nodeValue, '|', $div->getAttribute('title'), '|', $div->getAttribute('data'), '|';
$dom->loadHTML('<div>a&nbsp;b&#60;c&apos;d</div>');
echo $dom->saveXML($dom->getElementsByTagName('div')->item(0));
"###);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "a b<c'd|€😀<'| A|<div>a b&lt;c'd</div>", "{}", out.stderr);
}

/// Script and style retain raw text and serialize it as safe CDATA.
#[test]
fn dom_html_third_review_raw_text() {
    let out = compile_and_run_capture(r###"<?php
foreach (['script', 'style'] as $tag) {
    $dom = new DOMDocument();
    $dom->loadHTML('<div><' . $tag . '>a<b&amp;</' . strtoupper($tag) . '>c</div>');
    $div = $dom->getElementsByTagName('div')->item(0);
    echo $div->nodeValue, '|', $div->childNodes->length, '|', $dom->saveXML($div), '|';
}
$dom->loadHTML('<div><script>a]]>b</script>c</div>');
$div = $dom->getElementsByTagName('div')->item(0);
echo $dom->nodeValue === null ? 'N' : 'bad', '|', $div->nodeValue, '|', $dom->saveXML($div);
"###);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "a<b&amp;c|2|<div><script><![CDATA[a<b&amp;]]></script>c</div>|a<b&amp;c|2|<div><style><![CDATA[a<b&amp;]]></style>c</div>|N|a]]>bc|<div><script><![CDATA[a]]]]><![CDATA[>b]]></script>c</div>", "{}", out.stderr);
}

/// Invalid closing-name starts preserve the remaining text in one text node.
#[test]
fn dom_html_third_review_spaced_end_tag() {
    let out = compile_and_run_capture(r###"<?php
$dom = new DOMDocument();
foreach (['</ div>', '</1>', '</>'] as $end) {
    $dom->loadHTML('<div>a' . $end . 'b</div>');
    $div = $dom->getElementsByTagName('div')->item(0);
    echo $div->nodeValue, '|', $div->childNodes->length, '|', $dom->saveXML($div), ';';
}
"###);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "a div>b|1|<div>a div&gt;b</div>;a1>b|1|<div>a1&gt;b</div>;a>b|1|<div>a&gt;b</div>;", "{}", out.stderr);
}

/// XML serialization escapes CR everywhere and LF/TAB inside attributes.
#[test]
fn dom_html_third_review_carriage_returns() {
    let out = compile_and_run_capture(r###"<?php
$dom = new DOMDocument();
$dom->loadHTML("<div title=\"a\r\nb\tc\">a\r\nb\tc</div>");
$div = $dom->getElementsByTagName('div')->item(0);
echo json_encode($div->nodeValue), '|', json_encode($div->getAttribute('title')), '|', $dom->saveXML($div);
"###);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "\"a\\r\\nb\\tc\"|\"a\\r\\nb\\tc\"|<div title=\"a&#13;&#10;b&#9;c\">a&#13;\nb\tc</div>", "{}", out.stderr);
}

/// The document has no owner while its current element and text children retain the document.
#[test]
fn dom_html_third_review_document_owner() {
    let out = compile_and_run_capture(r###"<?php
$dom = new DOMDocument();
echo $dom->ownerDocument === null ? 'N' : 'bad', '|';
$dom->loadHTML('<div>Hi</div>');
$div = $dom->getElementsByTagName('div')->item(0);
$text = $div->childNodes->item(0);
echo $dom->ownerDocument === null ? 'N' : 'bad', '|';
echo $div->ownerDocument === $dom ? 'O' : 'bad', $text->ownerDocument === $dom ? 'O' : 'bad', '|';
$dom->loadHTML('<span>Bye</span>');
$span = $dom->getElementsByTagName('span')->item(0);
echo $dom->ownerDocument === null ? 'N' : 'bad', '|', $span->ownerDocument === $dom ? 'O' : 'bad';
"###);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "N|N|OO|N|O", "{}", out.stderr);
}

/// Entity decoding is nonrecursive and preserves unknown or unterminated legacy names.
#[test]
fn dom_html_third_review_entity_decode_once() {
    let out = compile_and_run_capture(r###"<?php
$dom = new DOMDocument();
$dom->loadHTML('<div>&amp;lt; &notAnEntity; &nbsp &AMP; &#65 &#x41 &#0; &#xD800;</div>');
$div = $dom->getElementsByTagName('div')->item(0);
echo $div->nodeValue, '|', $dom->saveXML($div);
"###);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "&lt; &notAnEntity; &nbsp &AMP; A A  |<div>&amp;lt; &amp;notAnEntity; &amp;nbsp &amp;AMP; A A  </div>", "{}", out.stderr);
}

/// Every legacy HTML4 entity plus apos matches the independent PHP/libxml 2.9 oracle.
#[test]
fn dom_html_third_review_all_named_entities() {
    let entries = r#"
AElig=198 Aacute=193 Acirc=194 Agrave=192 Alpha=913 Aring=197 Atilde=195 Auml=196 Beta=914
Ccedil=199 Chi=935 Dagger=8225 Delta=916 ETH=208 Eacute=201 Ecirc=202 Egrave=200 Epsilon=917
Eta=919 Euml=203 Gamma=915 Iacute=205 Icirc=206 Igrave=204 Iota=921 Iuml=207 Kappa=922
Lambda=923 Mu=924 Ntilde=209 Nu=925 OElig=338 Oacute=211 Ocirc=212 Ograve=210 Omega=937
Omicron=927 Oslash=216 Otilde=213 Ouml=214 Phi=934 Pi=928 Prime=8243 Psi=936 Rho=929 Scaron=352
Sigma=931 THORN=222 Tau=932 Theta=920 Uacute=218 Ucirc=219 Ugrave=217 Upsilon=933 Uuml=220
Xi=926 Yacute=221 Yuml=376 Zeta=918 aacute=225 acirc=226 acute=180 aelig=230 agrave=224
alefsym=8501 alpha=945 amp=38 and=8743 ang=8736 apos=39 aring=229 asymp=8776 atilde=227
auml=228 bdquo=8222 beta=946 brvbar=166 bull=8226 cap=8745 ccedil=231 cedil=184 cent=162
chi=967 circ=710 clubs=9827 cong=8773 copy=169 crarr=8629 cup=8746 curren=164 dArr=8659
dagger=8224 darr=8595 deg=176 delta=948 diams=9830 divide=247 eacute=233 ecirc=234 egrave=232
empty=8709 emsp=8195 ensp=8194 epsilon=949 equiv=8801 eta=951 eth=240 euml=235 euro=8364
exist=8707 fnof=402 forall=8704 frac12=189 frac14=188 frac34=190 frasl=8260 gamma=947 ge=8805
gt=62 hArr=8660 harr=8596 hearts=9829 hellip=8230 iacute=237 icirc=238 iexcl=161 igrave=236
image=8465 infin=8734 int=8747 iota=953 iquest=191 isin=8712 iuml=239 kappa=954 lArr=8656
lambda=955 lang=9001 laquo=171 larr=8592 lceil=8968 ldquo=8220 le=8804 lfloor=8970 lowast=8727
loz=9674 lrm=8206 lsaquo=8249 lsquo=8216 lt=60 macr=175 mdash=8212 micro=181 middot=183
minus=8722 mu=956 nabla=8711 nbsp=160 ndash=8211 ne=8800 ni=8715 not=172 notin=8713 nsub=8836
ntilde=241 nu=957 oacute=243 ocirc=244 oelig=339 ograve=242 oline=8254 omega=969 omicron=959
oplus=8853 or=8744 ordf=170 ordm=186 oslash=248 otilde=245 otimes=8855 ouml=246 para=182
part=8706 permil=8240 perp=8869 phi=966 pi=960 piv=982 plusmn=177 pound=163 prime=8242
prod=8719 prop=8733 psi=968 quot=34 rArr=8658 radic=8730 rang=9002 raquo=187 rarr=8594
rceil=8969 rdquo=8221 real=8476 reg=174 rfloor=8971 rho=961 rlm=8207 rsaquo=8250 rsquo=8217
sbquo=8218 scaron=353 sdot=8901 sect=167 shy=173 sigma=963 sigmaf=962 sim=8764 spades=9824
sub=8834 sube=8838 sum=8721 sup=8835 sup1=185 sup2=178 sup3=179 supe=8839 szlig=223 tau=964
there4=8756 theta=952 thetasym=977 thinsp=8201 thorn=254 tilde=732 times=215 trade=8482
uArr=8657 uacute=250 uarr=8593 ucirc=251 ugrave=249 uml=168 upsih=978 upsilon=965 uuml=252
weierp=8472 xi=958 yacute=253 yen=165 yuml=255 zeta=950 zwj=8205 zwnj=8204
"#;
    let mut html = String::from("<div>");
    let mut expected = String::new();
    for entry in entries.split_whitespace() {
        let (name, codepoint) = entry.split_once('=').unwrap();
        html.push_str(&format!("&{name};"));
        expected.push(char::from_u32(codepoint.parse().unwrap()).unwrap());
    }
    html.push_str("</div>");
    let source = format!("<?php $dom = new DOMDocument(); $dom->loadHTML('{html}'); echo $dom->getElementsByTagName('div')->item(0)->nodeValue;");
    let out = compile_and_run_capture(&source);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, expected, "{}", out.stderr);
}

/// HTML parsing retains whitespace-only siblings even when NOBLANKS is requested.
#[test]
fn dom_html_second_review_noblanks_preserves_whitespace() {
    let out = compile_and_run(r#"<?php
$dom = new DOMDocument();
$dom->loadHTML('<span>a</span> <span>b</span>', LIBXML_NOERROR | LIBXML_NOBLANKS);
$body = $dom->getElementsByTagName('body')->item(0);
echo $body->nodeValue, '|', $body->childNodes->length, '|';
$dom->loadHTML("<div>\n  <span>x</span>\n</div>", LIBXML_NOERROR | LIBXML_NOBLANKS);
$div = $dom->getElementsByTagName('div')->item(0);
echo json_encode($div->nodeValue), '|', $div->childNodes->length;
"#);
    assert_eq!(out, "a b|3|\"\\n  x\\n\"|3");
}

/// Legacy libxml declaration handling preserves unknown bang markup as one text run.
#[test]
fn dom_html_second_review_unknown_declaration_is_text() {
    let out = compile_and_run(r#"<?php
$dom = new DOMDocument();
$dom->loadHTML('<!DOCTYPE html><div>a<!not>b</div>');
$div = $dom->getElementsByTagName('div')->item(0);
echo $div->nodeValue, '|', $div->childNodes->length, '|', $dom->saveXML($div);
"#);
    assert_eq!(out, "a<!not>b|1|<div>a&lt;!not&gt;b</div>");
}

/// XML escaping differs between text and double-quoted attribute values.
#[test]
fn dom_html_second_review_contextual_xml_escaping() {
    let out = compile_and_run(r#"<?php
$dom = new DOMDocument();
$dom->loadHTML("<div title=\"it's &quot;ok&quot; &amp; &lt;&gt;\">say \"hi\" &amp; &lt;&gt; 'yes'</div>");
$div = $dom->getElementsByTagName('div')->item(0);
echo $dom->saveXML($div);
"#);
    assert_eq!(out, "<div title=\"it's &quot;ok&quot; &amp; &lt;&gt;\">say \"hi\" &amp; &lt;&gt; 'yes'</div>");
}

/// Loading or reloading a document never changes its null nodeValue.
#[test]
fn dom_html_second_review_document_value_is_null() {
    let out = compile_and_run(r#"<?php
$dom = new DOMDocument();
echo $dom->nodeValue === null ? 'N' : 'bad';
$dom->loadHTML('<div>Hi</div>');
echo $dom->nodeValue === null ? 'N' : 'bad';
echo '|', $dom->getElementsByTagName('html')->item(0)->nodeValue;
$dom->loadHTML('<span>Bye</span>');
echo '|', $dom->nodeValue === null ? 'N' : 'bad';
echo '|', $dom->getElementsByTagName('html')->item(0)->nodeValue;
"#);
    assert_eq!(out, "NN|Hi|N|Bye");
}

/// Slashes in unquoted values remain data, while a standalone closing slash ends the tag.
#[test]
fn dom_html_token_review_unquoted_slashes() {
    let out = compile_and_run(r#"<?php
$dom = new DOMDocument();
$dom->loadHTML('<a href=http://example.com title=ok>x</a><div class=foo/bar>z</div><span / title=kept>t</span><b data=x />');
$a = $dom->getElementsByTagName('a')->item(0);
$div = $dom->getElementsByTagName('div')->item(0);
$span = $dom->getElementsByTagName('span')->item(0);
$b = $dom->getElementsByTagName('b')->item(0);
echo $a->getAttribute('href'), '|', $a->getAttribute('title'), '|', $a->nodeValue, '|',
    $div->getAttribute('class'), '|', $div->nodeValue, '|', $span->getAttribute('title'), '|',
    $span->nodeValue, '|', $dom->saveXML($b);
"#);
    assert_eq!(out, "http://example.com|ok|x|foo/bar|z|kept|t|<b data=\"x\"/>");
}

/// A less-than sign followed by a non-letter stays text rather than becoming a tag.
#[test]
fn dom_html_token_review_nonletter_start() {
    let out = compile_and_run(r#"<?php
$dom = new DOMDocument();
$dom->loadHTML('<div>1<2</div><span>x<_y</span><b>3<-4</b>');
foreach (['div', 'span', 'b'] as $name) {
    $node = $dom->getElementsByTagName($name)->item(0);
    echo $node->nodeValue, '|', $node->childNodes->length, '|', $dom->saveXML($node), ';';
}
"#);
    assert_eq!(out, "1<2|1|<div>1&lt;2</div>;x<_y|1|<span>x&lt;_y</span>;3<-4|1|<b>3&lt;-4</b>;");
}

/// Libxml's HTML boolean names use their name for an omitted value, unlike other attributes.
#[test]
fn dom_html_token_review_boolean_attributes() {
    let out = compile_and_run(r#"<?php
$dom = new DOMDocument();
$dom->loadHTML('<input CHECKED compact declare defer disabled ismap multiple nohref noresize noshade nowrap readonly selected hidden required title><input disabled="" checked=no>');
$input = $dom->getElementsByTagName('input')->item(0);
foreach (['checked', 'compact', 'declare', 'defer', 'disabled', 'ismap', 'multiple', 'nohref', 'noresize', 'noshade', 'nowrap', 'readonly', 'selected', 'hidden', 'required', 'title'] as $name) {
    echo $name, '=', $input->getAttribute($name), ';';
}
$explicit = $dom->getElementsByTagName('input')->item(1);
echo '|', $explicit->getAttribute('disabled'), '|', $explicit->getAttribute('checked');
"#);
    assert_eq!(out, "checked=checked;compact=compact;declare=declare;defer=defer;disabled=disabled;ismap=ismap;multiple=multiple;nohref=nohref;noresize=noresize;noshade=noshade;nowrap=nowrap;readonly=readonly;selected=selected;hidden=;required=;title=;||no");
}

/// XML serialization uses empty-element syntax for ordinary elements as well as void tags.
#[test]
fn dom_html_token_review_empty_elements() {
    let out = compile_and_run(r#"<?php
$dom = new DOMDocument();
$dom->loadHTML('<div class="a"></div><span></span><p> </p><i><!--comment--></i><br>');
$body = $dom->getElementsByTagName('body')->item(0);
foreach ($body->childNodes as $node) { echo $dom->saveXML($node); }
"#);
    assert_eq!(out, "<div class=\"a\"/><span/><p> </p><i><!--comment--></i><br/>");
}

/// A generic function's bound keeps the namespace ownership of a user-defined DOM class.
#[test]
fn dom_html_review_generic_function_bound_uses_namespace() {
    let out = compile_and_run(r#"<?php
namespace App;
class DOMDocument {}
function identity<T: DOMDocument>(T $value): T { return $value; }
$document = identity(new DOMDocument());
echo get_class($document);
"#);
    assert_eq!(out, "App\\DOMDocument");
}

/// A DOM reference carried only by inherited generic arguments still enables dynamic creation.
#[test]
fn dom_html_review_generic_inheritance_injects_prelude() {
    let out = compile_and_run(r#"<?php
class Box<T> {}
class Documents extends Box<DOMDocument> {}
$name = 'DOMDocument';
$document = new $name();
echo $document->nodeName;
"#);
    assert_eq!(out, "#document");
}

/// A user's generic DOM class remains user-owned through explicit instantiation.
#[test]
fn dom_html_review_user_generic_class_is_not_redeclared() {
    let out = compile_and_run(r#"<?php
class DOMDocument<T> {
    public function __construct(public T $value) {}
}
$document = new DOMDocument<int>(42);
echo $document->value;
"#);
    assert_eq!(out, "42");
}

/// The user-facing example exercises sibling stack reuse and comment-free aggregate text.
#[test]
fn dom_html_review_example() {
    let out = compile_and_run(include_str!("../../examples/dom-html/main.php"));
    assert_eq!(out, "div class=text-green-500 text=Hi there!\n");
}

/// A global user-owned DOM class keeps its definition even under case-folded references.
#[test]
fn dom_html_review_user_class_is_not_redeclared() {
    let out = compile_and_run(r#"<?php
class DOMDocument { public function marker(): string { return 'user'; } }
$doc = new \domdocument();
echo $doc->marker();
"#);
    assert_eq!(out, "user");
}

/// A namespaced user class and the real global DOM surface can coexist through imports.
#[test]
fn dom_html_review_namespaced_user_class_and_global_prelude() {
    let out = compile_and_run(r#"<?php
namespace App {
    class DOMDocument { public function marker(): string { return 'app'; } }
}
namespace Client {
    use App\DOMDocument as UserDoc;
    use \DOMDocument as HtmlDoc;
    $user = new UserDoc();
    $html = new HtmlDoc();
    $html->loadHTML('<div>html</div>');
    echo $user->marker(), ':', $html->getElementsByTagName('div')->item(0)->nodeValue;
}
"#);
    assert_eq!(out, "app:html");
}

/// Unmatched closing tags are ignored and a matching ancestor closes its descendants.
#[test]
fn dom_html_review_closing_tags_match_open_elements() {
    let out = compile_and_run_capture(r#"<?php
$dom = new DOMDocument();
$dom->loadHTML('<div>A</missing>B<span>C</div>D<b>E</b></span>');
$body = $dom->getElementsByTagName('body')->item(0);
$div = $dom->getElementsByTagName('div')->item(0);
echo $dom->saveXML($body), '|', $div->nodeValue, '|', $body->childNodes->length;
"#);
    assert!(out.success, "{}", out.stderr.lines().take(4).collect::<Vec<_>>().join("\n"));
    assert_eq!(out.stdout, "<body><div>AB<span>C</span></div>D<b>E</b></body>|ABC|3");
    assert!(out.stderr.is_empty(), "{}", out.stderr.lines().take(4).collect::<Vec<_>>().join("\n"));
}

/// Comments remain visible as nodes and in XML but not in ancestor descendant text.
#[test]
fn dom_html_review_comments_do_not_aggregate_into_element_text() {
    let out = compile_and_run(r#"<?php
$dom = new DOMDocument();
$dom->loadHTML('<div>A<!--hidden--><span>C<!--nested--></span>B</div>');
$div = $dom->getElementsByTagName('div')->item(0);
$span = $dom->getElementsByTagName('span')->item(0);
$comment = $div->childNodes->item(1);
echo $div->nodeValue, '|', $span->nodeValue, '|', $comment->nodeValue, '|', $dom->saveXML($div);
"#);
    assert_eq!(out, "ACB|C|hidden|<div>A<!--hidden--><span>C<!--nested--></span>B</div>");
}

/// A dynamic NodeList loop stops after its two nodes without reading past the end.
#[test]
fn dom_html_review_iterator_stops_at_the_end() {
    let out = compile_and_run_capture(r#"<?php
$dom = new DOMDocument();
$dom->loadHTML('<div><span>A</span><b>B</b></div>');
$div = $dom->getElementsByTagName('div')->item(0);
$seen = 0;
foreach ($div->childNodes as $key => $child) {
    echo $key, ':', $child->nodeName, ':', $child->nodeValue, ';';
    $seen++;
    if ($seen > 3) { break; }
}

echo $seen;
"#);
    assert!(out.success, "{}", out.stderr.lines().take(4).collect::<Vec<_>>().join("\n"));
    assert_eq!(out.stdout, "0:span:A;1:b:B;2");
    assert!(out.stderr.is_empty(), "{}", out.stderr.lines().take(4).collect::<Vec<_>>().join("\n"));
}

/// The tree-builder stack can append back into the integer slot removed by pop.
#[test]
fn dom_html_review_stack_pop_reuses_tail_index() {
    let out = compile_and_run(r#"<?php
function replaceStackTop(array $stack): array {
    $removed = array_pop($stack);
    $stack[] = ['name' => 'b'];
    return $stack;
}
$stack = [['name' => 'div'], ['name' => 'span']];
$stack = replaceStackTop($stack);
echo count($stack), ':';
foreach ($stack as $key => $node) { echo $key, '=', $node['name'], ';'; }
"#);
    assert_eq!(out, "2:0=div;1=b;");
}
