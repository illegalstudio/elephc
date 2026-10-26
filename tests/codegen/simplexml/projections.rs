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
foreach ([
    '<r a="1"><!--x--><a>1</a><!--c--><?pi y?><b>2</b>text</r>',
    '<r a="1"><!--only--></r>',
    '<r xmlns:p="urn:p" a="1" p:b="2"><p:z>0</p:z><a>1</a><!--c--></r>'
] as $source) {
    $xml = new SimpleXMLElement($source);
    $children = $xml->children();
    echo json_encode($children), '|', json_encode((array)$children), '|', json_encode(get_object_vars($children)), '|', json_encode($children->__debugInfo()), "\n";
    $children = $xml->children('urn:p');
    echo json_encode($children), '|', json_encode($children->__debugInfo()), "\n";
}
$xml = new SimpleXMLElement('<!DOCTYPE r [<!ENTITY e "x"><!ENTITY f "y">]><r>&e;</r>');
echo json_encode($xml), "\n";
foreach (['<r><![CDATA[x]]></r>', '<r><!--c--></r>', '<r><?pi x?></r>', '<r> </r>', '<r/>', '<r xmlns:p="urn:p" p:a="1"/>', '<r xmlns:p="urn:p"><p:c/></r>'] as $source) {
    $xml = new SimpleXMLElement($source);
    echo (int)(bool)$xml, (int)(bool)new SimpleXMLElement($source, 0, false, 'urn:p'), '|';
}
$xml = new SimpleXMLElement('<r><!--c--><?pi x?></r>');
echo (int)(bool)$xml->xpath('//comment()')[0], (int)(bool)$xml->xpath('//processing-instruction()')[0], "\n";
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
{"a":"1","comment":{},"pi":{},"b":"2"}|{"a":"1","comment":{},"pi":{},"b":"2"}|{"a":"1","comment":{},"pi":{},"b":"2"}|{"@attributes":{"a":"1"},"a":"1","comment":{},"pi":{},"b":"2"}
{}|[]
{}|[]|[]|{"@attributes":{"a":"1"}}
{}|[]
{"a":"1","comment":{}}|{"a":"1","comment":{}}|{"a":"1","comment":{}}|{"@attributes":{"a":"1"},"a":"1","comment":{}}
{"z":"0","comment":{}}|{"@attributes":{"b":"2"},"z":"0","comment":{}}
{"e":{"e":"x"}}
00|11|11|11|00|01|01|00
"###;
    assert_eq!(out.stdout, expected.repeat(2));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
