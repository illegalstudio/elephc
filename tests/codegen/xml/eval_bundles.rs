//! Purpose:
//! Shares expensive XML eval catalog compilation across independent process fixtures.
//!
//! Called from:
//! - Managed-libxml2 codegen CI on Linux and macOS.
//!
//! Key details:
//! - Parser callbacks, by-reference callable outputs and writer bytes retain original assertions.
//! - Eval-only handler rejection remains an explicit compiled/eval divergence from PHP.

use crate::support::*;

/// Shares one native catalog compile across two independent process fixtures.
/// Original eval forms, caller scopes, outputs and ownership assertions are retained.
#[test]
fn test_xml_eval_parser_and_callable_shapes_bundle() {
    if skip_without_xml_native("test_xml_eval_parser_and_callable_shapes_bundle") { return; }
    let runs = compile_and_run_capture_per_argv(r####"<?php
function on_start(XMLParser $p, string $n, mixed $a) { echo "S $n ", json_encode($a), " @", xml_get_current_line_number($p), "\n"; }
function on_end(XMLParser $p, string $n) { echo "E $n\n"; }
final class Sink {
    public function chars(XMLParser $p, string $d) { echo "C ", var_export($d, true), "\n"; }
}
if ($argc === 1) {
$sink = new Sink();
eval('
$p = xml_parser_create();
var_dump(get_class($p), $p instanceof XMLParser, extension_loaded("xml"), extension_loaded("xmlwriter"), function_exists("xml_parse"));
var_dump(xml_set_element_handler($p, "on_start", "on_end"), xml_set_character_data_handler($p, [$sink, "chars"]));
var_dump(xml_parse($p, "<a x=\"1\">hi<b/></a>", true), xml_get_error_code($p));
$s = xml_parser_create();
xml_parser_set_option($s, XML_OPTION_SKIP_WHITE, true);
var_dump(xml_parse_into_struct($s, "<r> <i>1</i> </r>", $values, $index));
echo json_encode($values), " ", json_encode($index), "\n";
$t = xml_parser_create();
var_dump(xml_parse_into_struct($t, "<k/>", $only));
echo json_encode($only), "\n";
$bad = xml_parser_create();
var_dump(xml_parse($bad, "<a></b>", true), xml_get_error_code($bad), xml_error_string(xml_get_error_code($bad)));
try { new XMLParser(); } catch (Error $e) { echo $e->getMessage(), "\n"; }
try { xml_set_default_handler("nope", "on_end"); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { xml_set_default_handler($p, "no_such_function"); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
');
} else {
$host = xml_parser_create();
eval('
$p = xml_parser_create();
$f = "xml_parse_into_struct";
var_dump($f($p, "<a><b/></a>", $v1, $i1));
echo json_encode($v1), " ", json_encode($i1), "\n";
$p = xml_parser_create();
$g = xml_parse_into_struct(...);
var_dump($g($p, "<c/>", $v2));
echo json_encode($v2), "\n";
$p = xml_parser_create();
var_dump(call_user_func_array("xml_parse_into_struct", [$p, "<d/>", &$v3, &$i3]));
echo json_encode($v3), " ", json_encode($i3), "\n";
$p = xml_parser_create();
var_dump(call_user_func_array("xml_parse_into_struct", ["parser" => $p, "data" => "<e/>", "values" => &$v4, "index" => &$i4]));
echo json_encode($v4), " ", json_encode($i4), "\n";
$p = xml_parser_create();
$v5 = null;
var_dump(call_user_func("xml_parse_into_struct", $p, "<f/>", $v5));
var_dump($v5);
');
}
"####, &[&[], &["second-case"]]);
    assert_eq!(runs.len(), 2);
    // test_xml_parser_inside_eval
    {
        let output = &runs[0];
        assert!(output.success, "case=test_xml_parser_inside_eval stdout={:?} stderr={}", output.stdout, output.stderr);
        let out = output.stdout.as_str();
        assert_eq!(
            out,
            "string(9) \"XMLParser\"\nbool(true)\nbool(true)\nbool(true)\nbool(true)\nbool(true)\nbool(true)\nS A {\"X\":\"1\"} @1\nC 'hi'\nS B [] @1\nE B\nE A\nint(1)\nint(0)\nint(1)\n[{\"tag\":\"R\",\"type\":\"open\",\"level\":1},{\"tag\":\"I\",\"type\":\"complete\",\"level\":2,\"value\":\"1\"},{\"tag\":\"R\",\"type\":\"close\",\"level\":1}] {\"R\":[0,2],\"I\":[1]}\nint(1)\n[{\"tag\":\"K\",\"type\":\"complete\",\"level\":1}]\nint(0)\nint(76)\nstring(14) \"Mismatched tag\"\nCannot directly construct XMLParser, use xml_parser_create() or xml_parser_create_ns() instead\nxml_set_default_handler(): Argument #1 ($parser) must be of type XMLParser, string given\nxml_set_default_handler(): Argument #2 ($handler) an object must be set via xml_set_object() to be able to lookup method\n"
        );
    }
    // test_xml_parse_into_struct_dynamic_callable_shapes_inside_eval
    {
        let output = &runs[1];
        assert!(output.success, "case=test_xml_parse_into_struct_dynamic_callable_shapes_inside_eval stdout={:?} stderr={}", output.stdout, output.stderr);
        let out = output;
        assert_eq!(
            out.stdout,
            "int(1)\n[{\"tag\":\"A\",\"type\":\"open\",\"level\":1},{\"tag\":\"B\",\"type\":\"complete\",\"level\":2},{\"tag\":\"A\",\"type\":\"close\",\"level\":1}] {\"A\":[0,2],\"B\":[1]}\nint(1)\n[{\"tag\":\"C\",\"type\":\"complete\",\"level\":1}]\nint(1)\n[{\"tag\":\"D\",\"type\":\"complete\",\"level\":1}] {\"D\":[0]}\nint(1)\n[{\"tag\":\"E\",\"type\":\"complete\",\"level\":1}] {\"E\":[0]}\nint(1)\nNULL\n"
        );
        assert_eq!(
            out.stderr.matches("must be passed by reference, value given").count(),
            1,
            "only the by-value call_user_func() shape may warn: {}",
            out.stderr
        );
        assert!(
            out.stderr.contains("xml_parse_into_struct(): Argument #3 ($values) must be passed by reference, value given"),
            "missing by-value warning: {}",
            out.stderr
        );
        assert!(
            !out.stderr.contains("Undefined variable"),
            "by-reference outputs must not read their variables first: {}",
            out.stderr
        );
    }
}

/// Shares one native catalog compile across two independent process fixtures.
/// Original eval forms, caller scopes, outputs and ownership assertions are retained.
#[test]
fn test_xml_eval_writer_and_handler_registration_bundle() {
    if skip_without_xml_native("test_xml_eval_writer_and_handler_registration_bundle") { return; }
    let runs = compile_and_run_capture_per_argv(r####"<?php
function on_start(XMLParser $p, string $n, mixed $a) { echo "compiled start $n\n"; }
function on_end(XMLParser $p, string $n) { echo "compiled end $n\n"; }
if ($argc === 1) {
$host = new XMLWriter();
$host->openMemory();
$host->startElement('host');
eval('
$w = new XMLWriter();
$w->openMemory();
$w->setIndent(true);
$w->startElement("root");
$w->writeAttribute("id", "1");
$w->writeElement("t", "a&b");
$w->endElement();
echo $w->outputMemory();
$f = xmlwriter_open_memory();
xmlwriter_start_element($f, "p");
xmlwriter_write_cdata($f, "x");
xmlwriter_end_element($f);
var_dump(xmlwriter_output_memory($f), xmlwriter_flush($f));
try { $f->startElement(""); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
$host->writeElement("from-eval", "yes");
$host->endElement();
');
echo $host->outputMemory(), "\n";
} else {
$hostClosure = function (XMLParser $p, string $n, mixed $a) { echo "host closure $n\n"; };
eval('
function ev_start($p, $n, $a) { echo "never\n"; }
class EvSink { function s($p, $n, $a) { echo "never\n"; } function c($p, $d) { echo "never\n"; } }
$p = xml_parser_create();
try { xml_set_element_handler($p, function ($p, $n, $a) { echo "never\n"; }, "on_end"); } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
try { xml_set_element_handler($p, "on_start", "ev_start"); } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
try { xml_set_character_data_handler($p, [new EvSink(), "c"]); } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
try { xml_set_default_handler($p, "EvSink::c"); } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
try { xml_set_object($p, new EvSink()); } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
try { xml_set_object($p, function () {}); } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
var_dump(xml_set_element_handler($p, $hostClosure, "on_end"));
var_dump(xml_parse($p, "<a>t</a>", true));
');
}
"####, &[&[], &["second-case"]]);
    assert_eq!(runs.len(), 2);
    // test_xmlwriter_inside_eval
    {
        let output = &runs[0];
        assert!(output.success, "case=test_xmlwriter_inside_eval stdout={:?} stderr={}", output.stdout, output.stderr);
        let out = output.stdout.as_str();
        assert_eq!(
            out,
            "<root id=\"1\">\n <t>a&amp;b</t>\n</root>\nstring(20) \"<p><![CDATA[x]]></p>\"\nstring(0) \"\"\nXMLWriter::startElement(): Argument #2 must be a valid element name, \"\" given\n<host><from-eval>yes</from-eval></host>\n"
        );
    }
    // test_xml_eval_declared_handlers_are_rejected_inside_eval
    {
        let output = &runs[1];
        assert!(output.success, "case=test_xml_eval_declared_handlers_are_rejected_inside_eval stdout={:?} stderr={}", output.stdout, output.stderr);
        let out = output.stdout.as_str();
        assert_eq!(
            out,
            "Error: xml_set_element_handler(): handlers declared inside eval() cannot be invoked by the compiled parser; declare the handler in compiled code\nError: xml_set_element_handler(): handlers declared inside eval() cannot be invoked by the compiled parser; declare the handler in compiled code\nError: xml_set_character_data_handler(): handlers declared inside eval() cannot be invoked by the compiled parser; declare the handler in compiled code\nError: xml_set_default_handler(): handlers declared inside eval() cannot be invoked by the compiled parser; declare the handler in compiled code\nError: xml_set_object(): objects of classes declared inside eval() cannot be bound as handler targets\nError: xml_set_object(): objects of classes declared inside eval() cannot be bound as handler targets\nbool(true)\nhost closure A\ncompiled end A\nint(1)\n"
        );
    }
}
