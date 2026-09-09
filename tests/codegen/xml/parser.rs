//! Purpose:
//! End-to-end fixtures for the `ext/xml` SAX parser: object model, options, events with
//! positions, incremental parsing, error codes and namespace handling.
//!
//! Called from:
//! - `cargo test --test codegen_tests xml::parser` through Rust's test harness.
//!
//! Key details:
//! - Expected output is what PHP 8.5.10 prints for the same program.

use crate::support::*;

/// A handler-driven walk prints the event stream with libxml2's positions and chunking.
#[test]
fn test_xml_basic_event_stream_with_positions() {
    if skip_without_xml_native("test_xml_basic_event_stream_with_positions") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
$p = xml_parser_create();
xml_set_element_handler($p,
    function($p, $name, $attrs) { echo "START ", var_export($name, true), " ", json_encode($attrs), " @", xml_get_current_line_number($p), ":", xml_get_current_column_number($p), "/", xml_get_current_byte_index($p), "\n"; },
    function($p, $name) { echo "END ", var_export($name, true), " @", xml_get_current_line_number($p), ":", xml_get_current_column_number($p), "/", xml_get_current_byte_index($p), "\n"; });
xml_set_character_data_handler($p, function($p, $d) { echo "CDATA ", var_export($d, true), " @", xml_get_current_line_number($p), ":", xml_get_current_column_number($p), "/", xml_get_current_byte_index($p), "\n"; });
xml_set_processing_instruction_handler($p, function($p, $t, $d) { echo "PI ", var_export($t, true), " ", var_export($d, true), "\n"; });
xml_set_default_handler($p, function($p, $d) { echo "DEFAULT ", var_export($d, true), "\n"; });
$r = xml_parse($p, "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!-- c -->\n<root a=\"1\" b='two'>\n  <item>hello &amp; world &lt;x&gt;</item>\n  <e/>\n  <?pi some data?>\n  <![CDATA[raw <cdata> & stuff]]>\n  text\n</root>\n", true);
echo "RESULT $r code=", xml_get_error_code($p), " msg=", var_export(xml_error_string(xml_get_error_code($p)), true), " @", xml_get_current_line_number($p), ":", xml_get_current_column_number($p), "/", xml_get_current_byte_index($p), "\n";
"#,
    );
    assert_eq!(
        out,
        "DEFAULT '<!-- c -->'\nSTART 'ROOT' {\"A\":\"1\",\"B\":\"two\"} @3:20/69\nCDATA '\n  ' @4:3/73\nSTART 'ITEM' [] @4:8/78\nCDATA 'hello ' @4:15/85\nCDATA '&' @4:20/90\nCDATA ' world ' @4:27/97\nCDATA '<' @4:31/101\nCDATA 'x' @4:32/102\nCDATA '>' @4:36/106\nEND 'ITEM' @4:43/113\nCDATA '\n  ' @5:3/116\nSTART 'E' [] @5:5/118\nEND 'E' @5:7/120\nCDATA '\n  ' @6:3/123\nPI 'pi' 'some data'\nCDATA '\n  ' @7:3/142\nCDATA 'raw <cdata> & stuff' @7:34/173\nCDATA '\n  text\n' @9:1/181\nEND 'ROOT' @9:8/188\nRESULT 1 code=0 msg='No error' @10:1/189\n"
    );
}

/// `xml_parser_create()` returns a final `XMLParser`; direct construction throws, options
/// round-trip with PHP's defaults, and invalid options raise PHP's `ValueError`s.
#[test]
fn test_xml_parser_object_model_and_options() {
    if skip_without_xml_native("test_xml_parser_object_model_and_options") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
$p = xml_parser_create();
var_dump(get_class($p), $p instanceof XMLParser);
var_dump(xml_parser_get_option($p, XML_OPTION_CASE_FOLDING), xml_parser_get_option($p, XML_OPTION_TARGET_ENCODING), xml_parser_get_option($p, XML_OPTION_SKIP_TAGSTART), xml_parser_get_option($p, XML_OPTION_SKIP_WHITE), xml_parser_get_option($p, XML_OPTION_PARSE_HUGE));
var_dump(xml_parser_set_option($p, XML_OPTION_SKIP_TAGSTART, 3), xml_parser_get_option($p, XML_OPTION_SKIP_TAGSTART));
try { xml_parser_set_option($p, 99, 1); } catch (ValueError $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
try { xml_parser_get_option($p, 99); } catch (ValueError $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
try { xml_parser_set_option($p, XML_OPTION_TARGET_ENCODING, "bogus"); } catch (ValueError $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
$p2 = xml_parser_create("ISO-8859-1");
var_dump(xml_parser_get_option($p2, XML_OPTION_TARGET_ENCODING));
try { xml_parser_create("bogus"); } catch (ValueError $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
try { new XMLParser(); } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
var_dump(xml_parser_free($p), XML_SAX_IMPL, XML_ERROR_TAG_MISMATCH, XML_OPTION_PARSE_HUGE);
var_dump(extension_loaded('xml'), extension_loaded('xmlwriter'));
"#,
    );
    assert_eq!(
        out,
        "string(9) \"XMLParser\"\nbool(true)\nbool(true)\nstring(5) \"UTF-8\"\nint(0)\nbool(false)\nbool(false)\nbool(true)\nint(3)\nValueError: xml_parser_set_option(): Argument #2 ($option) must be a XML_OPTION_* constant\nValueError: xml_parser_get_option(): Argument #2 ($option) must be a XML_OPTION_* constant\nValueError: xml_parser_set_option(): Argument #3 ($value) is not a supported target encoding\nstring(10) \"ISO-8859-1\"\nValueError: xml_parser_create(): Argument #1 ($encoding) is not a supported source encoding\nError: Cannot directly construct XMLParser, use xml_parser_create() or xml_parser_create_ns() instead\nbool(true)\nstring(6) \"libxml\"\nint(7)\nint(5)\nbool(true)\nbool(true)\n"
    );
}

/// Errors carry libxml2's codes and PHP's messages; the parser stops at the first fatal
/// one and later chunks keep answering 0.
#[test]
fn test_xml_error_codes_and_strings() {
    if skip_without_xml_native("test_xml_error_codes_and_strings") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
foreach (["<root><a></b></root>", "<root/><x/>", "", "<root><a>", "<a b>x</a>", "<a b='1' b='2'/>", "<a>]]></a>", "<!-- -- --><a/>", "<r>&nope;</r>"] as $doc) {
    $p = xml_parser_create();
    $r = xml_parse($p, $doc, true);
    echo var_export($doc, true), " => $r code=", xml_get_error_code($p), " '", xml_error_string(xml_get_error_code($p)), "' @", xml_get_current_line_number($p), ":", xml_get_current_column_number($p), "/", xml_get_current_byte_index($p), "\n";
}
for ($i = 0; $i < 6; $i++) { echo $i, ": ", xml_error_string($i), "\n"; }
var_dump(xml_error_string(76), xml_error_string(114), xml_error_string(-1));
$p = xml_parser_create();
var_dump(xml_parse($p, "<a><b>", false));
var_dump(xml_parse($p, "</a>", false), xml_get_error_code($p));
var_dump(xml_parse($p, "", true), xml_get_error_code($p));
$p = xml_parser_create();
var_dump(xml_parse($p, "<a/>"), xml_parse($p, "", true), xml_parse($p, "<b/>", true), xml_get_error_code($p));
"#,
    );
    assert_eq!(
        out,
        "'<root><a></b></root>' => 0 code=76 'Mismatched tag' @1:14/13\n'<root/><x/>' => 0 code=5 'Invalid document end' @1:8/7\n'' => 0 code=4 'Not well-formed (invalid token)' @1:1/0\n'<root><a>' => 0 code=77 'Tag not finished' @1:10/9\n'<a b>x</a>' => 0 code=41 'Attribute without value' @1:6/5\n'<a b=\\'1\\' b=\\'2\\'/>' => 0 code=42 'Attribute redefined' @1:17/16\n'<a>]]></a>' => 0 code=62 'Sequence ']]>' not allowed in content' @1:7/6\n'<!-- -- --><a/>' => 0 code=80 'Comment must not contain '--' (double-hyphen)' @1:12/11\n'<r>&nope;</r>' => 0 code=26 'Undeclared entity error' @1:10/9\n0: No error\n1: No memory\n2: Invalid document start\n3: Empty document\n4: Not well-formed (invalid token)\n5: Invalid document end\nstring(14) \"Mismatched tag\"\nstring(7) \"Unknown\"\nstring(7) \"Unknown\"\nint(1)\nint(0)\nint(76)\nint(0)\nint(76)\nint(1)\nint(1)\nint(0)\nint(5)\n"
    );
}

/// Incremental parsing dispatches events as soon as their item is complete, with the
/// byte index reflecting the consumed input after each chunk.
#[test]
fn test_xml_incremental_parsing_positions() {
    if skip_without_xml_native("test_xml_incremental_parsing_positions") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
$p = xml_parser_create();
xml_set_element_handler($p, function($p,$n,$a){ echo "S $n ", json_encode($a), " @", xml_get_current_byte_index($p), "\n"; }, function($p,$n){ echo "E $n @", xml_get_current_byte_index($p), "\n"; });
xml_set_character_data_handler($p, function($p,$d){ echo "C ", var_export($d,true), " @", xml_get_current_byte_index($p), "\n"; });
$doc = "<root><item a='12'>hello world</item><x/></root>";
foreach (str_split($doc, 7) as $chunk) { $r = xml_parse($p, $chunk, false); echo "chunk ", var_export($chunk,true), " => $r line=", xml_get_current_line_number($p), " col=", xml_get_current_column_number($p), " byte=", xml_get_current_byte_index($p), "\n"; }
var_dump(xml_parse($p, "", true));
"#,
    );
    assert_eq!(
        out,
        "S ROOT [] @5\nchunk '<root><' => 1 line=1 col=7 byte=6\nchunk 'item a=' => 1 line=1 col=7 byte=6\nS ITEM {\"A\":\"12\"} @18\nchunk '\\'12\\'>he' => 1 line=1 col=20 byte=19\nchunk 'llo wor' => 1 line=1 col=20 byte=19\nC 'hello world' @30\nchunk 'ld</ite' => 1 line=1 col=31 byte=30\nE ITEM @37\nS X [] @39\nE X @41\nchunk 'm><x/><' => 1 line=1 col=42 byte=41\nE ROOT @48\nchunk '/root>' => 1 line=1 col=49 byte=48\nint(1)\n"
    );
}

/// Namespace-aware parsing qualifies names with the separator, reports declarations to
/// the start-namespace handler and drops `xmlns` attributes, like PHP.
#[test]
fn test_xml_namespace_parsing() {
    if skip_without_xml_native("test_xml_namespace_parsing") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
$p = xml_parser_create_ns(null, "|");
xml_set_element_handler($p, function($p,$n,$a){ echo "S $n ", json_encode($a), "\n"; }, function($p,$n){ echo "E $n\n"; });
xml_set_start_namespace_decl_handler($p, function($p,$pre,$uri){ echo "NS ", var_export($pre,true), " ", var_export($uri,true), "\n"; });
xml_set_end_namespace_decl_handler($p, function($p,$pre){ echo "NSE ", var_export($pre,true), "\n"; });
$r = xml_parse($p, "<r:root xmlns:r=\"urn:r\" xmlns=\"urn:d\" r:a=\"1\" b=\"2\"><child/><r:c/></r:root>", true);
echo "=> $r code=", xml_get_error_code($p), "\n";
$p = xml_parser_create_ns();
xml_set_element_handler($p, function($p,$n,$a){ echo "S $n ", json_encode($a), "\n"; }, function($p,$n){ echo "E $n\n"; });
$r = xml_parse($p, "<a><p:b/></a>", true);
echo "=> $r code=", xml_get_error_code($p), "\n";
$p = xml_parser_create();
xml_set_element_handler($p, function($p,$n,$a){ echo "S $n ", json_encode($a), "\n"; }, null);
xml_parse($p, "<r xmlns='u' xmlns:p='v' p:a='1' xml:lang='en'/>", true);
"#,
    );
    assert_eq!(
        out,
        "NS 'r' 'urn:r'\nNS false 'urn:d'\nS URN:R|ROOT {\"URN:R|A\":\"1\",\"B\":\"2\"}\nS URN:D|CHILD []\nE URN:D|CHILD\nS URN:R|C []\nE URN:R|C\nE URN:R|ROOT\n=> 1 code=0\nS A []\nS B []\nE B\nE A\n=> 0 code=201\nS R {\"XMLNS\":\"u\",\"XMLNS:P\":\"v\",\"P:A\":\"1\",\"XML:LANG\":\"en\"}\n"
    );
}

/// Case folding, tag-start skipping and the target encoding shape every string handed
/// to a handler; namespaced calls resolve case-insensitively like every builtin.
#[test]
fn test_xml_options_shape_handler_strings() {
    if skip_without_xml_native("test_xml_options_shape_handler_strings") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
namespace App;
$p = \XML_PARSER_CREATE();
xml_parser_set_option($p, \XML_OPTION_CASE_FOLDING, 0);
xml_parser_set_option($p, \XML_OPTION_SKIP_TAGSTART, 1);
xml_set_element_handler($p, function($p,$n,$a){ echo "S ", var_export($n,true), " ", json_encode($a), "\n"; }, function($p,$n){ echo "E ", var_export($n,true), "\n"; });
xml_parse($p, "<Root><Item id='x'>t</Item><ab/></Root>", true);
$p = xml_parser_create();
xml_parser_set_option($p, \XML_OPTION_TARGET_ENCODING, "ISO-8859-1");
xml_set_character_data_handler($p, function($p,$d){ echo "C ", bin2hex($d), "\n"; });
xml_set_element_handler($p, function($p,$n,$a){ echo "S ", bin2hex($n), " ", json_encode(array_map(fn($v) => bin2hex($v), $a)), "\n"; }, null);
var_dump(xml_parse($p, "<\xc3\xa9l\xc3\xa9 a='\xc3\xa9'>caf\xc3\xa9 \xe2\x82\xac</\xc3\xa9l\xc3\xa9>", true));
"#,
    );
    assert_eq!(
        out,
        "S 'oot' []\nS 'tem' {\"id\":\"x\"}\nE 'tem'\nS 'b' []\nE 'b'\nE 'oot'\nS e94ce9 {\"A\":\"e9\"}\nC 636166\nC e9203f\nint(1)\n"
    );
}

/// Handler binding edges pinned against php-src: an external-entity-ref handler that
/// throws stops the parser with error 21 before the exception reaches the caller,
/// `xml_parser_free()` answers false from inside a handler (PHP also warns; elephc has no
/// warning channel), scalar handler names are coerced like PHP's coercive parse, the
/// `xml_set_object()` swap error names the method as declared, and error codes wrap to a
/// C `int`.
#[test]
fn test_xml_handler_binding_edges() {
    if skip_without_xml_native("test_xml_handler_binding_edges") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
$p = xml_parser_create();
xml_set_external_entity_ref_handler($p, function ($p, $names, $base, $sys, $pub) { throw new RuntimeException("ext " . $names); });
try {
    xml_parse($p, "<!DOCTYPE r [<!ENTITY e SYSTEM \"e.xml\">]><r>&e;</r>", true);
} catch (RuntimeException $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
var_dump(xml_get_error_code($p), xml_error_string(xml_get_error_code($p)));
$q = xml_parser_create();
xml_set_element_handler($q, function ($q, $n, $a) { var_dump(xml_parser_free($q)); }, null);
var_dump(xml_parse($q, "<a/>", true));
var_dump(xml_parser_free($q));
$r = xml_parser_create();
try { xml_set_default_handler($r, 42); } catch (ValueError $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
try { xml_set_default_handler($r, 4.5); } catch (ValueError $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
var_dump(xml_set_default_handler($r, false));
final class Sink { public function CData($p, $d) { echo "C:$d\n"; } }
final class Other { public function nothing() {} }
$s = xml_parser_create();
xml_set_object($s, new Sink());
xml_set_character_data_handler($s, "CData");
try { xml_set_object($s, new Other()); } catch (ValueError $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
var_dump(xml_error_string(4294967296 + 76));
"#,
    );
    assert_eq!(out, "RuntimeException: ext e\nint(21)\nstring(60) \"PEReference: forbidden within markup decl in internal subset\"\nbool(false)\nint(1)\nbool(true)\nValueError: xml_set_default_handler(): Argument #2 ($handler) an object must be set via xml_set_object() to be able to lookup method\nValueError: xml_set_default_handler(): Argument #2 ($handler) an object must be set via xml_set_object() to be able to lookup method\nbool(true)\nValueError: xml_set_object(): Argument #2 ($object) cannot safely swap to object of class Other as method \"CData\" does not exist, which was set via xml_set_character_data_handler()\nstring(14) \"Mismatched tag\"\n");
}

/// `xml_get_error_code()` inside a handler answers the parser's state at that event, not
/// the chunk's final state: both start handlers read 0 although the same chunk then
/// fails with a mismatched tag, which only the read after `xml_parse()` sees.
#[test]
fn test_xml_error_code_inside_handler_is_the_event_state() {
    if skip_without_xml_native("test_xml_error_code_inside_handler_is_the_event_state") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
$p = xml_parser_create();
xml_set_element_handler($p, function ($p, $n, $a) { echo $n, ":", xml_get_error_code($p), "\n"; }, null);
var_dump(xml_parse($p, "<root><ok/></wrong>", true));
echo "final:", xml_get_error_code($p), "\n";
"#,
    );
    assert_eq!(out, "ROOT:0\nOK:0\nint(0)\nfinal:76\n");
}

/// The same document split before the bad end tag: the first chunk succeeds with code 0
/// inside and after it, the final chunk dispatches nothing and reports 76.
#[test]
fn test_xml_error_code_inside_handler_split_chunks() {
    if skip_without_xml_native("test_xml_error_code_inside_handler_split_chunks") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
$p = xml_parser_create();
xml_set_element_handler($p, function ($p, $n, $a) { echo $n, ":", xml_get_error_code($p), "\n"; }, null);
var_dump(xml_parse($p, "<root><ok/>", false));
echo "mid:", xml_get_error_code($p), "\n";
var_dump(xml_parse($p, "</wrong>", true));
echo "final:", xml_get_error_code($p), "\n";
"#,
    );
    assert_eq!(out, "ROOT:0\nOK:0\nint(1)\nmid:0\nint(0)\nfinal:76\n");
}

/// Reads after an earlier error in the same chunk: a non-fatal namespace error (201,
/// 203) is set from the offending tag on, so the character-data handler sees it for the
/// text that follows and 0 for the text before; a fatal error (undeclared entity, 26)
/// dispatches nothing further, so no handler ever reads it; and an external-entity-ref
/// handler answering false reads 0 itself while every read after the stop answers 21.
#[test]
fn test_xml_error_code_in_cdata_handler_after_earlier_error() {
    if skip_without_xml_native("test_xml_error_code_in_cdata_handler_after_earlier_error") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
$p = xml_parser_create_ns(null, "|");
xml_set_element_handler($p, function ($p, $n, $a) { echo "S ", $n, ":", xml_get_error_code($p), "\n"; }, function ($p, $n) { echo "E ", $n, ":", xml_get_error_code($p), "\n"; });
xml_set_character_data_handler($p, function ($p, $d) { echo "C ", var_export($d, true), ":", xml_get_error_code($p), "\n"; });
var_dump(xml_parse($p, "<r>a<p:b/>b<c/>c</r>", true));
echo "final:", xml_get_error_code($p), "\n";
$p = xml_parser_create_ns(null, "|");
xml_set_element_handler($p, function ($p, $n, $a) { echo "S ", $n, " ", json_encode($a), ":", xml_get_error_code($p), "\n"; }, function ($p, $n) { echo "E ", $n, ":", xml_get_error_code($p), "\n"; });
xml_set_character_data_handler($p, function ($p, $d) { echo "C ", var_export($d, true), ":", xml_get_error_code($p), "\n"; });
var_dump(xml_parse($p, "<r>one<a xmlns:p='u1' p:x='1' xmlns:q='u1' q:x='2'/>two</r>", true));
echo "final:", xml_get_error_code($p), "\n";
$p = xml_parser_create();
xml_set_element_handler($p, function ($p, $n, $a) { echo "S ", $n, ":", xml_get_error_code($p), "\n"; }, function ($p, $n) { echo "E ", $n, ":", xml_get_error_code($p), "\n"; });
xml_set_character_data_handler($p, function ($p, $d) { echo "C ", var_export($d, true), ":", xml_get_error_code($p), "\n"; });
xml_set_default_handler($p, function ($p, $d) { echo "D ", var_export($d, true), ":", xml_get_error_code($p), "\n"; });
var_dump(xml_parse($p, "<r>a&nope;b<c/>d</r>", true));
echo "final:", xml_get_error_code($p), "\n";
$p = xml_parser_create();
xml_set_external_entity_ref_handler($p, function ($p, $names, $base, $sys, $pub) { echo "EXTREF ", $names, ":", xml_get_error_code($p), "\n"; return false; });
xml_set_character_data_handler($p, function ($p, $d) { echo "C ", var_export($d, true), ":", xml_get_error_code($p), "\n"; });
var_dump(xml_parse($p, "<!DOCTYPE r [<!ENTITY x SYSTEM \"xsys\">]><r>a&x;b</wrong>", true));
echo "final:", xml_get_error_code($p), "\n";
var_dump(xml_parse($p, "<z/>", true));
echo "after:", xml_get_error_code($p), "\n";
"#,
    );
    assert_eq!(
        out,
        "S R:0\nC 'a':0\nS B:201\nE B:201\nC 'b':201\nS C:201\nE C:201\nC 'c':201\nE R:201\nint(0)\nfinal:201\nS R []:0\nC 'one':0\nS A {\"U1|X\":\"2\"}:203\nE A:203\nC 'two':203\nE R:203\nint(0)\nfinal:203\nS R:0\nC 'a':0\nD '&nope;':0\nint(0)\nfinal:26\nC 'a':0\nEXTREF x:0\nint(0)\nfinal:21\nint(0)\nafter:21\n"
    );
}
