//! Purpose:
//! End-to-end fixtures proving the xml surface behaves identically inside `eval()`: the
//! forwarded functions, real `XMLParser` / `XMLWriter` objects, by-reference outputs,
//! `extension_loaded()`, and the undefined-function error when the bridge is absent.
//!
//! Called from:
//! - `cargo test --test codegen_tests xml::eval` through Rust's test harness.
//!
//! Key details:
//! - Eval fragments run the compiled prelude through Magician's native-function bridge, so
//!   objects created in eval are host objects. Handlers must be callables the compiled
//!   dispatcher can invoke (compiled functions, methods and closures); a handler that exists
//!   only inside eval is rejected up front with a clear `Error`.
//! - Expectations are PHP 8.5.10 output, except the eval-only handler rejection, which is a
//!   documented divergence (PHP has no compiled/eval split).

use crate::support::*;

/// Rebased XML dispatch keeps argument leases through named binding and releases them on TypeError.
/// Registry validation runs without libxml2, so this ownership regression needs no native package.
#[test]
fn test_core_xml_eval_argument_owners_are_released_after_type_errors() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$source = '
for ($i = 0; $i < 2; $i++) {
    try { xml_set_element_handler(null, str_repeat("start", 2), str_repeat("end", 2)); }
    catch (TypeError $error) { echo "direct|"; unset($error); }
    try { xml_parse_into_struct(parser: 1, data: str_repeat("text", 2), values: $rows); }
    catch (TypeError $error) { echo "named|"; unset($error); }
}
unset($rows);
// ' . $argc;
eval($source);
unset($source);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "direct|named|direct|named|", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A parser driven entirely from eval prints the same event stream as native code, and
/// `xml_parse_into_struct()` writes its outputs back through eval's by-reference targets.
/// Handlers are compiled callables named from eval (a function name, and an
/// `[$object, 'method']` pair), the shapes the compiled dispatcher can invoke.
#[test]
fn test_xml_parser_inside_eval() {
    if skip_without_xml_native("test_xml_parser_inside_eval") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
function on_start(XMLParser $p, string $n, mixed $a) { echo "S $n ", json_encode($a), " @", xml_get_current_line_number($p), "\n"; }
function on_end(XMLParser $p, string $n) { echo "E $n\n"; }
final class Sink {
    public function chars(XMLParser $p, string $d) { echo "C ", var_export($d, true), "\n"; }
}
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
"#,
    );
    assert_eq!(
        out,
        "string(9) \"XMLParser\"\nbool(true)\nbool(true)\nbool(true)\nbool(true)\nbool(true)\nbool(true)\nS A {\"X\":\"1\"} @1\nC 'hi'\nS B [] @1\nE B\nE A\nint(1)\nint(0)\nint(1)\n[{\"tag\":\"R\",\"type\":\"open\",\"level\":1},{\"tag\":\"I\",\"type\":\"complete\",\"level\":2,\"value\":\"1\"},{\"tag\":\"R\",\"type\":\"close\",\"level\":1}] {\"R\":[0,2],\"I\":[1]}\nint(1)\n[{\"tag\":\"K\",\"type\":\"complete\",\"level\":1}]\nint(0)\nint(76)\nstring(14) \"Mismatched tag\"\nCannot directly construct XMLParser, use xml_parser_create() or xml_parser_create_ns() instead\nxml_set_default_handler(): Argument #1 ($parser) must be of type XMLParser, string given\nxml_set_default_handler(): Argument #2 ($handler) an object must be set via xml_set_object() to be able to lookup method\n"
    );
}

/// `XMLWriter` created and driven inside eval, through both the object and the procedural
/// API, produces PHP's bytes; a host-created writer can be continued from eval.
#[test]
fn test_xmlwriter_inside_eval() {
    if skip_without_xml_native("test_xmlwriter_inside_eval") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
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
"#,
    );
    assert_eq!(
        out,
        "<root id=\"1\">\n <t>a&amp;b</t>\n</root>\nstring(20) \"<p><![CDATA[x]]></p>\"\nstring(0) \"\"\nXMLWriter::startElement(): Argument #2 must be a valid element name, \"\" given\n<host><from-eval>yes</from-eval></host>\n"
    );
}

/// A program that never links the bridge sees PHP's undefined-function `Error` in eval for
/// the prelude-provided functions, `extension_loaded('xml')` / `get_loaded_extensions()`
/// answer false on both sides, and `function_exists()` splits the surface the way the
/// compiler does: prelude functions are absent, the registry builtins (the handler setters,
/// `xml_parse_into_struct()`) exist regardless — and, existing, still run PHP's `$parser`
/// `TypeError` in every call shape instead of claiming to be undefined.
///
/// Deliberately NOT gated on the managed libxml2 artifact: this fixture's whole point is
/// that `elephc_xml` (and therefore libxml2) is never linked, so it runs everywhere.
#[test]
fn test_xml_absent_bridge_inside_eval() {
    let out = compile_and_run(
        r#"<?php
var_dump(extension_loaded('xml'));
eval('
var_dump(extension_loaded("xml"), function_exists("xml_parse"), function_exists("xml_set_element_handler"), in_array("xml", get_loaded_extensions()));
try { xml_parser_create(); } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
try { xml_set_element_handler(null, "a", "b"); } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
try { xml_parse_into_struct(1, "x", $v); } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
$f = "xml_set_default_handler";
try { $f(true, "a"); } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
try { call_user_func("xml_parse_into_struct", 1.5, "x", null); } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
try { xml_set_object(2.5, null); } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
try { xml_parse(1, "x"); } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
');
"#,
    );
    assert_eq!(
        out,
        "bool(false)\nbool(false)\nbool(false)\nbool(true)\nbool(false)\nError: Call to undefined function xml_parser_create()\nTypeError: xml_set_element_handler(): Argument #1 ($parser) must be of type XMLParser, null given\nTypeError: xml_parse_into_struct(): Argument #1 ($parser) must be of type XMLParser, int given\nTypeError: xml_set_default_handler(): Argument #1 ($parser) must be of type XMLParser, true given\nTypeError: xml_parse_into_struct(): Argument #1 ($parser) must be of type XMLParser, float given\nError: Call to undefined function xml_set_object()\nError: Call to undefined function xml_parse()\n"
    );
}

/// `xml_parse_into_struct()` writes `$values` / `$index` back in every dynamic-callable
/// shape — a variable function name, a first-class callable, `call_user_func_array()` with
/// positional and named `&$refs` — and only a genuine by-value `call_user_func()` warns
/// "must be passed by reference, value given" (to stderr) and leaves the variable alone.
#[test]
fn test_xml_parse_into_struct_dynamic_callable_shapes_inside_eval() {
    if skip_without_xml_native("test_xml_parse_into_struct_dynamic_callable_shapes_inside_eval") {
        return;
    }
    let out = compile_and_run_capture(
        r#"<?php
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
"#,
    );
    assert!(out.success, "program failed: stdout={:?} stderr={}", out.stdout, out.stderr);
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

/// The `$parser` `TypeError` names the given value the way PHP does — `int`, `float`,
/// `true` / `false`, `string`, `array`, `null`, the class of a host or eval-declared object —
/// for every registry builtin and for `xml_set_object()`; `get_loaded_extensions()` lists
/// `xml` / `xmlwriter` in eval once the host linked the bridge.
#[test]
fn test_xml_type_error_spells_php_type_names_inside_eval() {
    if skip_without_xml_native("test_xml_type_error_spells_php_type_names_inside_eval") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
function on_start(XMLParser $p, string $n, mixed $a) {}
function on_end(XMLParser $p, string $n) {}
eval('
class EvBad {}
foreach ([1, 1.5, true, false, "s", [], null, new stdClass, new ArrayObject([]), new EvBad] as $bad) {
    try { xml_set_element_handler($bad, "on_start", "on_end"); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
}
try { xml_parse_into_struct(1, "x", $vv); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { xml_set_object("nope", new stdClass); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
var_dump(in_array("xml", get_loaded_extensions()), in_array("xmlwriter", get_loaded_extensions()), in_array("xml", get_loaded_extensions(true)));
');
"#,
    );
    assert_eq!(
        out,
        "xml_set_element_handler(): Argument #1 ($parser) must be of type XMLParser, int given\nxml_set_element_handler(): Argument #1 ($parser) must be of type XMLParser, float given\nxml_set_element_handler(): Argument #1 ($parser) must be of type XMLParser, true given\nxml_set_element_handler(): Argument #1 ($parser) must be of type XMLParser, false given\nxml_set_element_handler(): Argument #1 ($parser) must be of type XMLParser, string given\nxml_set_element_handler(): Argument #1 ($parser) must be of type XMLParser, array given\nxml_set_element_handler(): Argument #1 ($parser) must be of type XMLParser, null given\nxml_set_element_handler(): Argument #1 ($parser) must be of type XMLParser, stdClass given\nxml_set_element_handler(): Argument #1 ($parser) must be of type XMLParser, ArrayObject given\nxml_set_element_handler(): Argument #1 ($parser) must be of type XMLParser, EvBad given\nxml_parse_into_struct(): Argument #1 ($parser) must be of type XMLParser, int given\nxml_set_object(): Argument #1 ($parser) must be of type XMLParser, string given\nbool(true)\nbool(true)\nbool(false)\n"
    );
}

/// Handlers that exist only inside eval — a closure, an eval-declared function named as a
/// string, an `[$object, 'method']` pair or `Class::method` over an eval-declared class —
/// are rejected by the setters with a clear catchable `Error`, and `xml_set_object()`
/// rejects an eval-declared object (or closure) the same way, all BEFORE the parser is
/// touched: the parser stays usable, and a compiled closure handed in from the host, like a
/// compiled function name, is dispatched normally.
#[test]
fn test_xml_eval_declared_handlers_are_rejected_inside_eval() {
    if skip_without_xml_native("test_xml_eval_declared_handlers_are_rejected_inside_eval") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
function on_start(XMLParser $p, string $n, mixed $a) { echo "compiled start $n\n"; }
function on_end(XMLParser $p, string $n) { echo "compiled end $n\n"; }
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
"#,
    );
    assert_eq!(
        out,
        "Error: xml_set_element_handler(): handlers declared inside eval() cannot be invoked by the compiled parser; declare the handler in compiled code\nError: xml_set_element_handler(): handlers declared inside eval() cannot be invoked by the compiled parser; declare the handler in compiled code\nError: xml_set_character_data_handler(): handlers declared inside eval() cannot be invoked by the compiled parser; declare the handler in compiled code\nError: xml_set_default_handler(): handlers declared inside eval() cannot be invoked by the compiled parser; declare the handler in compiled code\nError: xml_set_object(): objects of classes declared inside eval() cannot be bound as handler targets\nError: xml_set_object(): objects of classes declared inside eval() cannot be bound as handler targets\nbool(true)\nhost closure A\ncompiled end A\nint(1)\n"
    );
}
