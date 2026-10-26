//! Purpose:
//! Checks PHP-equivalent SimpleXML projections for whitespace, CDATA, and named lists.
//!
//! Called from:
//! - Managed-libxml2 codegen CI on Linux and macOS.
//!
//! Key details:
//! - Runs the same fixture through AOT and opaque eval with strict heap ownership checks.

use crate::support::*;

/// Array, JSON, and debug projections retain object-valued nodes and PHP's list shape.
#[test]
fn test_simplexml_projection_whitespace_cdata_and_named_lists_bundle() {
    if skip_without_xml_native("test_simplexml_projection_whitespace_cdata_and_named_lists_bundle") { return; }
    let body = r###"$catalog = new SimpleXMLElement("<catalog>\n  <product sku=\"A1\">\n    <name>Notebook</name>\n  </product>\n  <product sku=\"B2\">\n    <name>Pen</name>\n  </product>\n</catalog>");
echo json_encode($catalog), "\n", json_encode((array)$catalog), "\n";
echo json_encode($catalog->product), "\n", json_encode((array)$catalog->product), "\n";
foreach ([
    '<r><i>a<b>z</b></i><i>c</i></r>',
    '<r><i>a</i><i><x>q</x></i></r>',
    '<r><i><![CDATA[a]]></i><i>c</i></r>',
    '<r><i>  </i><i>c</i></r>',
    '<r><i/><i>c</i></r>',
    '<r><i>a</i></r>',
    '<r><i>a</i><!--c--><i>b</i></r>'
] as $source) {
    $xml = new SimpleXMLElement($source);
    echo json_encode($xml), '|', json_encode((array)$xml), '|';
    echo json_encode($xml->i), '|', json_encode((array)$xml->i), '|', json_encode($xml->i->__debugInfo()), "\n";
}
foreach ([
    '<r>  </r>', "<r>\n</r>", '<r><![CDATA[x]]></r>', '<r>x</r>', '<r>0</r>',
    '<r><a><![CDATA[x]]></a><b>  </b><c>x</c></r>',
    '<r><!--c--></r>', '<r><?pi x?></r>',
    '<r xmlns:p="urn:p"><!--c--><?pi x?><p:a>hidden</p:a><a>visible</a></r>'
] as $source) {
    $xml = new SimpleXMLElement($source);
    echo json_encode($xml), '|', json_encode((array)$xml), '|', json_encode($xml->__debugInfo()), '|';
    echo json_encode($xml->children()), '|', json_encode((array)$xml->children()), "\n";
}
"###;
    let quoted_body = body.replace('\\', "\\\\").replace('\'', "\\'");
    let source = format!("<?php\n{body}\n$source = $argc > 0 ? '{quoted_body}' : '';\neval($source);");
    let out = compile_and_run_with_heap_debug(&source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    let expected = r###"{"product":[{"@attributes":{"sku":"A1"},"name":"Notebook"},{"@attributes":{"sku":"B2"},"name":"Pen"}]}
{"product":[{"@attributes":{"sku":"A1"},"name":"Notebook"},{"@attributes":{"sku":"B2"},"name":"Pen"}]}
{"@attributes":{"sku":"A1"},"name":"Notebook"}
{"@attributes":{"sku":"A1"},"name":"Notebook"}
{"i":["a","c"]}|{"i":["a","c"]}|{"b":"z"}|{"b":"z"}|{"b":"z"}
{"i":["a",{"x":"q"}]}|{"i":["a",{"x":"q"}]}|{"0":"a","1":{"x":"q"}}|["a",{"x":"q"}]|["a",{"x":"q"}]
{"i":[{},"c"]}|{"i":[{},"c"]}|{"0":{},"1":"c"}|[{},"c"]|[{},"c"]
{"i":[{},"c"]}|{"i":[{},"c"]}|{"0":{},"1":"c"}|[{},"c"]|[{},"c"]
{"i":[{},"c"]}|{"i":[{},"c"]}|{}|[]|[]
{"i":"a"}|{"i":"a"}|{"0":"a"}|["a"]|["a"]
{"i":["a","b"],"comment":{}}|{"i":["a","b"],"comment":{}}|{"0":"a","1":"b"}|["a","b"]|["a","b"]
{}|[]|[]|{}|[]
{}|[]|[]|{}|[]
{}|[]|[]|{}|[]
{"0":"x"}|["x"]|["x"]|{}|[]
{"0":"0"}|["0"]|["0"]|{}|[]
{"a":{},"b":{},"c":"x"}|{"a":{},"b":{},"c":"x"}|{"a":{},"b":{},"c":"x"}|{"a":{},"b":{},"c":"x"}|{"a":{},"b":{},"c":"x"}
{"comment":{}}|{"comment":{}}|{"comment":{}}|{}|[]
{"pi":{}}|{"pi":{}}|{"pi":{}}|{}|[]
{"comment":{},"pi":{},"a":"visible"}|{"comment":{},"pi":{},"a":"visible"}|{"comment":{},"pi":{},"a":"visible"}|{"a":"visible"}|{"a":"visible"}
"###;
    assert_eq!(out.stdout, expected.repeat(2));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
