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
