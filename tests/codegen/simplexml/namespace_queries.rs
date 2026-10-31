//! Purpose:
//! Checks one-pass SimpleXML namespace queries and independent returned snapshots.
//!
//! Called from:
//! - Managed-libxml2 codegen CI on Linux and macOS.
//!
//! Key details:
//! - Both fixtures cover AOT and opaque eval; ownership checks use a small document.

use crate::support::*;

/// Namespace snapshots preserve first-prefix order and refresh after live document mutations.
#[test]
fn test_simplexml_namespace_snapshot_order_and_mutation_bundle() {
    if skip_without_xml_native("test_simplexml_namespace_snapshot_order_and_mutation_bundle") { return; }
    let body = r###"$xml = new SimpleXMLElement('<r xmlns="" xmlns:p="urn:first"><p:a/><b xmlns:p="urn:shadow"><p:c/></b></r>');
echo json_encode($xml->getNamespaces(true)), ':', json_encode($xml->getDocNamespaces(true)), "\n";
$before = $xml->getDocNamespaces(true);
$xml->addChild('q:d', 'value', 'urn:q');
echo json_encode($before), ':', json_encode($xml->getNamespaces(true)), ':', json_encode($xml->getDocNamespaces(true)), "\n";
echo json_encode($xml->b->getNamespaces(true)), ':', json_encode($xml->b->getDocNamespaces(true, false)), "\n";
$empty = new SimpleXMLElement('<r/>');
echo json_encode($empty->getNamespaces(true)), ':', json_encode($empty->getDocNamespaces(true)), "\n";
unset($before, $xml, $empty);
"###;
    let quoted_body = body.replace('\\', "\\\\").replace('\'', "\\'");
    let source = format!("<?php\n{body}\n$source = $argc > 0 ? '{quoted_body}' : ''; eval($source);");
    let out = compile_and_run_with_heap_debug(&source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    let expected = r###"{"p":"urn:first"}:{"":"","p":"urn:first"}
{"":"","p":"urn:first"}:{"p":"urn:first","q":"urn:q"}:{"":"","p":"urn:first","q":"urn:q"}
{"p":"urn:shadow"}:{"":"","p":"urn:first","q":"urn:q"}
[]:[]
"###;
    assert_eq!(out.stdout, expected.repeat(2));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A feed with many namespace prefixes needs one traversal per public query within CI's timeout.
#[test]
fn test_simplexml_large_namespace_query_bundle() {
    if skip_without_xml_native("test_simplexml_large_namespace_query_bundle") { return; }
    let body = r###"$opening = '<r xmlns=""'; $tile = '';
for ($n = 0; $n < 50; $n++) {
    $opening .= ' xmlns:p' . $n . '="urn:' . $n . '"';
    $tile .= '<p' . $n . ':e/>';
}
$xml = new SimpleXMLElement($opening . '>' . str_repeat($tile, 4000) . '</r>');
unset($opening, $tile);
$used = $xml->getNamespaces(true); $declared = $xml->getDocNamespaces(true);
if (!is_array($declared)) { throw new Error('namespace snapshot must be an array'); }
$firstUsed = null; $lastUsed = null;
foreach ($used as $prefix => $uri) {
    if ($firstUsed === null) { $firstUsed = $prefix; }
    $lastUsed = $prefix;
}
$firstDeclared = null; $lastDeclared = null;
foreach ($declared as $prefix => $uri) {
    if ($firstDeclared === null) { $firstDeclared = $prefix; }
    $lastDeclared = $prefix;
}
echo count($used), ':', count($declared), ':', $firstUsed, ':', $lastUsed, ':', json_encode($firstDeclared), ':', $lastDeclared, "\n";
unset($used, $declared, $xml, $firstUsed, $lastUsed, $firstDeclared, $lastDeclared, $prefix, $uri);
"###;
    let quoted_body = body.replace('\\', "\\\\").replace('\'', "\\'");
    let source = format!("<?php\n{body}\n$source = $argc > 0 ? '{quoted_body}' : ''; eval($source);");
    let out = compile_and_run_with_heap_size(&source, 134_217_728);
    assert_eq!(out, r###"50:51:p0:p49:"":p49
"###.repeat(2));
}
