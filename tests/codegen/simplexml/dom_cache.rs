//! Purpose:
//! Verifies shared native DOM wrappers keep identity without retaining unused PHP owners.
//!
//! Called from:
//! - Managed-libxml2 codegen CI on Linux and macOS.
//!
//! Key details:
//! - Both XML and HTML cases run through AOT and opaque eval with strict heap checks.

use crate::support::*;

/// Repeated DOM queries and live additions reuse wrappers that still retire after their last use.
#[test]
fn test_simplexml_dom_cache_identity_and_weak_owners_bundle() {
    if skip_without_xml_native("test_simplexml_dom_cache_identity_and_weak_owners_bundle") { return; }
    let body = r###"foreach ([false, true] as $html) {
    $document = new DOMDocument();
    if ($html) { $document->loadHTML('<html><body><div><a></a><b></b></div></body></html>'); }
    else { $document->loadXML('<div><a/><b/></div>'); }
    $root = $document->getElementsByTagName('div')->item(0);
    $first = $root->childNodes->item(0);
    $last = $root->childNodes->item(1);
    echo $html ? 'html|' : 'xml|';
    var_dump($document->documentElement === $document->documentElement,
        $root === $document->getElementsByTagName('div')->item(0),
        $first === $root->childNodes->item(0), $root === $first->parentNode,
        $last === $first->nextSibling, $first === $last->previousSibling,
        $document === $first->ownerDocument);
    $xml = simplexml_import_dom($root);
    $xml->addChild('c', 'value');
    $added = $root->childNodes->item(2);
    var_dump($added === $last->nextSibling, $root === $added->parentNode);
    $weak = WeakReference::create($added);
    unset($added, $xml, $first, $last, $root, $document);
    var_dump($weak->get() === null);
    unset($weak);
}
"###;
    let quoted_body = body.replace('\\', "\\\\").replace('\'', "\\'");
    let source = format!("<?php\n{body}\n$source = $argc > 0 ? '{quoted_body}' : ''; eval($source);");
    let out = compile_and_run_with_heap_debug(&source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    let expected = r###"xml|bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
html|bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
"###;
    assert_eq!(out.stdout, expected.repeat(2));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
