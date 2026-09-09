//! Purpose:
//! End-to-end fixtures for the `ext/xml` handler table: default-handler routing, entity
//! handling, `xml_set_object()` method binding, notation/entity declarations, external
//! entity references, exceptions thrown from handlers, first-class-callable handlers that
//! declare fewer parameters than the event, closures annotating the attribute map as a
//! bare `array`, and setters called with NAMED arguments (the handler typing follows the
//! parameter a closure is bound to, not the position it is written at).
//!
//! Called from:
//! - `cargo test --test codegen_tests xml::handlers` through Rust's test harness.
//!
//! Key details:
//! - The routing rules mirror php-src `ext/xml/compat.c`; expectations are PHP 8.5.10 output.

use crate::support::*;

/// The default handler receives what no installed handler claims, and entity references
/// follow php-src's `get_entity()` rules for every handler combination.
#[test]
fn test_xml_default_handler_and_entity_routing() {
    if skip_without_xml_native("test_xml_default_handler_and_entity_routing") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
function run(string $doc, bool $cdata, bool $default, bool $start, string $label) {
    $p = xml_parser_create();
    if ($cdata) xml_set_character_data_handler($p, function($p,$d){ echo "  C ", var_export($d,true), "\n"; });
    if ($default) xml_set_default_handler($p, function($p,$d){ echo "  D ", var_export($d,true), "\n"; });
    if ($start) xml_set_element_handler($p, function($p,$n,$a){ echo "  S $n ", json_encode($a), "\n"; }, function($p,$n){ echo "  E $n\n"; });
    $r = xml_parse($p, $doc, true);
    echo "[", $label, "] => $r code=", xml_get_error_code($p), "\n";
}
$d = '<!DOCTYPE r [<!ENTITY e "EE"><!ENTITY m "<b>bb</b>">]><r>a&e;b&m;c&amp;d&#65;</r>';
run($d, true, false, false, 'cdata');
run($d, true, true, false, 'cdata,default');
run($d, false, true, false, 'default');
run($d, false, true, true, 'start,default');
run($d, false, false, true, 'start');
run("<r x='1'>t<!-- c --><?pi d?><?pi2?><![CDATA[cd]]><b/></r>", false, true, false, 'default');
run("<r x='1'>t<b/></r>", false, true, true, 'default,start');
"#,
    );
    assert_eq!(
        out,
        "  C 'a'\n  C 'EE'\n  C 'b'\n  C '<b>bb</b>'\n  C 'c'\n  C '&'\n  C 'd'\n  C 'A'\n[cdata] => 1 code=0\n  D '<r>'\n  C 'a'\n  D '&e;'\n  C 'b'\n  D '&m;'\n  C 'c'\n  C '&'\n  C 'd'\n  C 'A'\n  D '</r>'\n[cdata,default] => 1 code=0\n  D '<r>'\n  D 'a'\n  D '&e;'\n  D 'b'\n  D '&m;'\n  D 'c'\n  D '&amp;'\n  D 'd'\n  D 'A'\n  D '</r>'\n[default] => 1 code=0\n  S R []\n  D 'a'\n  D '&e;'\n  D 'b'\n  D '&m;'\n  D 'c'\n  D '&amp;'\n  D 'd'\n  D 'A'\n  E R\n[start,default] => 1 code=0\n  S R []\n  E R\n[start] => 1 code=0\n  D '<r x=\\'1\\'>'\n  D 't'\n  D '<!-- c -->'\n  D '<?pi d?>'\n  D '<?pi2 (null)?>'\n  D 'cd'\n  D '<b>'\n  D '</b>'\n  D '</r>'\n[default] => 1 code=0\n  S R {\"X\":\"1\"}\n  D 't'\n  S B []\n  E B\n  E R\n[default,start] => 1 code=0\n"
    );
}

/// `xml_set_object()` binds string handler names to methods, rebinding on a new object
/// and rejecting names the object cannot serve, with PHP's `ValueError` messages.
#[test]
fn test_xml_set_object_method_handlers() {
    if skip_without_xml_native("test_xml_set_object_method_handlers") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
class H {
    public $log = [];
    function s($p, $n, $a) { $this->log[] = "s:$n"; }
    function e($p, $n) { $this->log[] = "e:$n"; }
    function c($p, $d) { $this->log[] = "c:$d"; }
}
class O2 {}
$h = new H;
$p = xml_parser_create();
xml_set_object($p, $h);
xml_set_element_handler($p, "s", "e");
xml_set_character_data_handler($p, "c");
xml_parse($p, "<a>x<b>y</b></a>", true);
echo implode(",", $h->log), "\n";
$p = xml_parser_create();
try { xml_set_element_handler($p, 'nope_fn', 'nope2'); } catch (ValueError $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
xml_set_object($p, new H);
try { xml_set_element_handler($p, 'nope', null); } catch (ValueError $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
xml_set_element_handler($p, 's', null);
try { xml_set_object($p, new O2); } catch (ValueError $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
function gs($p, $n, $a) { echo "gs:$n\n"; }
$p = xml_parser_create();
xml_set_element_handler($p, "gs", null);
xml_parse($p, "<a>x</a>", true);
xml_set_element_handler($p, '', null);
$p = xml_parser_create();
xml_set_element_handler($p, function($p,$n,$a){ echo "closure\n"; }, function($p,$n){ echo "E\n"; });
xml_set_element_handler($p, '', null);
var_dump(xml_parse($p, "<a>x</a>", true));
"#,
    );
    assert_eq!(
        out,
        "s:A,c:x,s:B,c:y,e:B,e:A\nValueError: xml_set_element_handler(): Argument #2 ($start_handler) an object must be set via xml_set_object() to be able to lookup method\nValueError: xml_set_element_handler(): Argument #2 ($start_handler) method H::nope() does not exist\nValueError: xml_set_object(): Argument #2 ($object) cannot safely swap to object of class O2 as method \"s\" does not exist, which was set via xml_set_element_handler()\ngs:A\nint(1)\n"
    );
}

/// Notation and unparsed-entity declarations, external entity references (including a
/// `false` answer that stops the parse with `XML_ERROR_EXTERNAL_ENTITY_HANDLING`) and
/// processing instructions reach their handlers with PHP's argument shapes.
#[test]
fn test_xml_declaration_and_entity_ref_handlers() {
    if skip_without_xml_native("test_xml_declaration_and_entity_ref_handlers") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
$p = xml_parser_create();
xml_set_notation_decl_handler($p, function($p, $n, $b, $s, $pub) { echo "NOTATION ", json_encode([$n,$b,$s,$pub]), "\n"; });
xml_set_unparsed_entity_decl_handler($p, function($p, $n, $b, $s, $pub, $not) { echo "UNPARSED ", json_encode([$n,$b,$s,$pub,$not]), "\n"; });
xml_set_external_entity_ref_handler($p, function($p, $names, $b, $s, $pub) { echo "EXTREF ", json_encode([$names,$b,$s,$pub]), "\n"; return true; });
xml_set_character_data_handler($p, function($p,$d){ echo "C ", var_export($d,true), "\n"; });
xml_set_processing_instruction_handler($p, function($p, $t, $d) { echo "PI ", var_export($t, true), " ", var_export($d, true), "\n"; });
$r = xml_parse($p, "<!DOCTYPE r [<!NOTATION n SYSTEM \"nsys\"><!NOTATION m PUBLIC \"pub\"><!ENTITY u SYSTEM \"usys\" NDATA n><!ENTITY x PUBLIC \"xpub\" \"xsys\">]><r><?pi?><?p2 data?>a&x;b</r>", true);
echo "=> $r code=", xml_get_error_code($p), "\n";
$p = xml_parser_create();
xml_set_external_entity_ref_handler($p, function($p, $names, $b, $s, $pub) { echo "EXTREF $names\n"; return false; });
xml_set_character_data_handler($p, function($p,$d){ echo "C ", var_export($d,true), "\n"; });
$r = xml_parse($p, "<!DOCTYPE r [<!ENTITY x SYSTEM \"xsys\">]><r>a&x;b</r>", true);
echo "=> $r code=", xml_get_error_code($p), " ", xml_error_string(xml_get_error_code($p)), "\n";
"#,
    );
    assert_eq!(
        out,
        "NOTATION [\"n\",false,\"nsys\",false]\nNOTATION [\"m\",false,false,\"pub\"]\nUNPARSED [\"u\",false,\"usys\",false,\"n\"]\nPI 'pi' false\nPI 'p2' 'data'\nC 'a'\nEXTREF [\"x\",\"\",\"xsys\",\"xpub\"]\nC 'b'\n=> 1 code=0\nC 'a'\nEXTREF x\n=> 0 code=21 PEReference: forbidden within markup decl in internal subset\n"
    );
}

/// An exception thrown by a handler propagates out of `xml_parse()`, stops the parser
/// without recording an error, and later parses report the document as ended; a parser
/// re-entered from a handler raises PHP's recursion `Error`.
#[test]
fn test_xml_handler_exceptions_and_recursion() {
    if skip_without_xml_native("test_xml_handler_exceptions_and_recursion") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
$p = xml_parser_create();
xml_set_element_handler($p, function($p,$n,$a){ echo "S $n\n"; if ($n === 'B') throw new RuntimeException("boom"); }, function($p,$n){ echo "E $n\n"; });
try { $r = xml_parse($p, "<a><b/><c/></a>", true); var_dump($r); } catch (RuntimeException $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
var_dump(xml_get_error_code($p), xml_get_current_byte_index($p));
var_dump(xml_parse($p, "<z/>", true), xml_get_error_code($p));
$p = xml_parser_create();
xml_set_element_handler($p, function($p,$n,$a){ try { xml_parse($p, "<x/>", true); } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; } }, null);
xml_parse($p, "<a/>", true);
"#,
    );
    assert_eq!(
        out,
        "S A\nS B\nRuntimeException: boom\nint(0)\nint(15)\nint(0)\nint(5)\nError: Parser must not be called recursively\n"
    );
}

/// A first-class callable handler may declare fewer parameters than the event supplies —
/// PHP ignores the surplus — down to none at all.
#[test]
fn test_xml_first_class_callable_handlers_with_fewer_parameters() {
    if skip_without_xml_native("test_xml_first_class_callable_handlers_with_fewer_parameters") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
function s1($p) { echo "s1\n"; }
function e1($p, $n) { echo "e1 $n\n"; }
function c0() { echo "c0\n"; }
$p = xml_parser_create();
xml_set_element_handler($p, s1(...), e1(...));
xml_set_character_data_handler($p, c0(...));
xml_parse($p, "<a>x<b/></a>", true);
"#,
    );
    assert_eq!(out, "s1\nc0\ns1\ne1 B\ne1 A\n");
}

/// A start handler annotating its attribute parameter as a bare `array` receives the
/// event's attribute map exactly like an unannotated one.
#[test]
fn test_xml_start_handler_with_array_annotated_attributes() {
    if skip_without_xml_native("test_xml_start_handler_with_array_annotated_attributes") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
$p = xml_parser_create();
xml_set_element_handler($p, function ($p, $n, array $a) { echo "S $n ", json_encode($a), " ", count($a), "\n"; foreach ($a as $k => $v) echo " $k=$v\n"; }, function ($p, $n) { echo "E $n\n"; });
xml_parse($p, "<a x='1' y='2'><b/></a>", true);
"#,
    );
    assert_eq!(
        out,
        "S A {\"X\":\"1\",\"Y\":\"2\"} 2\n X=1\n Y=2\nS B [] 0\nE B\nE A\n"
    );
}

/// A setter called with named arguments types its closure handlers exactly like the
/// positional call: a bare-`array` attribute parameter receives the whole attribute map.
#[test]
fn test_xml_named_start_handler_observes_the_attribute_map() {
    if skip_without_xml_native("test_xml_named_start_handler_observes_the_attribute_map") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
$p = xml_parser_create();
xml_set_element_handler(parser: $p, start_handler: function ($parser, $name, array $attributes) { var_dump($attributes); }, end_handler: null);
xml_parse($p, "<a id='x'/>", true);
"#,
    );
    assert_eq!(out, "array(1) {\n  [\"ID\"]=>\n  string(1) \"x\"\n}\n");
}

/// Named arguments written in a different order than the signature (`end_handler` first,
/// `parser` last) still bind each closure to its own event's parameter types: the
/// bare-`array` attribute parameter receives the attribute map, the end handler its name.
#[test]
fn test_xml_named_handlers_in_signature_independent_order() {
    if skip_without_xml_native("test_xml_named_handlers_in_signature_independent_order") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
$p = xml_parser_create();
xml_set_element_handler(end_handler: function ($parser, $name) { echo "END $name\n"; }, start_handler: function ($parser, $name, array $attributes) { echo "START $name ", json_encode($attributes), " ", count($attributes), "\n"; foreach ($attributes as $k => $v) echo " $k=$v\n"; }, parser: $p);
xml_parse($p, "<a x='1' y='2'><b/></a>", true);
"#,
    );
    assert_eq!(
        out,
        "START A {\"X\":\"1\",\"Y\":\"2\"} 2\n X=1\n Y=2\nSTART B [] 0\nEND B\nEND A\n"
    );
}

/// Named character-data and default handlers, with `parser:` first in one call and last
/// in the other, get the event hints like positional calls.
#[test]
fn test_xml_named_character_data_and_default_handlers() {
    if skip_without_xml_native("test_xml_named_character_data_and_default_handlers") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
$p = xml_parser_create();
xml_set_character_data_handler(parser: $p, handler: function ($parser, $data) { echo "C ", var_export($data, true), " ", strlen($data), "\n"; });
xml_set_default_handler(handler: function ($parser, $data) { echo "D ", var_export($data, true), "\n"; }, parser: $p);
xml_parse($p, "<r><!-- c -->hi<?pi d?></r>", true);
"#,
    );
    assert_eq!(
        out,
        "D '<r>'\nD '<!-- c -->'\nC 'hi' 2\nD '<?pi d?>'\nD '</r>'\n"
    );
}

/// A positional `$parser` followed by named handlers (in either order) binds each closure
/// to the slot its name selects.
#[test]
fn test_xml_positional_parser_with_named_handlers() {
    if skip_without_xml_native("test_xml_positional_parser_with_named_handlers") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
$p = xml_parser_create();
xml_set_element_handler($p, end_handler: function ($parser, $name) { echo "E $name\n"; }, start_handler: function ($parser, $name, $attributes) { echo "S $name ", json_encode($attributes), "\n"; });
xml_set_processing_instruction_handler($p, handler: function ($parser, $target, $data) { echo "PI ", var_export($target, true), " ", var_export($data, true), "\n"; });
xml_parse($p, "<a k='v'><?pi?><?p2 data?><b/></a>", true);
"#,
    );
    assert_eq!(
        out,
        "S A {\"K\":\"v\"}\nPI 'pi' false\nPI 'p2' 'data'\nS B []\nE B\nE A\n"
    );
}

/// Named calls passing a function name and first-class callables keep working: the
/// non-closure handlers lower through the registry signature as before.
#[test]
fn test_xml_named_function_name_and_first_class_callable_handlers() {
    if skip_without_xml_native("test_xml_named_function_name_and_first_class_callable_handlers") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
function s1($p, $n, $a) { echo "s1 $n ", json_encode($a), "\n"; }
function e1($p, $n) { echo "e1 $n\n"; }
function c0($p, $d) { echo "c0 ", var_export($d, true), "\n"; }
$p = xml_parser_create();
xml_set_element_handler(parser: $p, end_handler: e1(...), start_handler: 's1');
xml_set_character_data_handler(handler: c0(...), parser: $p);
xml_parse($p, "<a q='1'>x<b/></a>", true);
"#,
    );
    assert_eq!(out, "s1 A {\"Q\":\"1\"}\nc0 'x'\ns1 B []\ne1 B\ne1 A\n");
}

/// A handler closure written BEFORE the `parser:` named argument, with a body that only
/// type-checks under the event hints (`strlen($data)`, `strlen($name)`, a bare
/// `array $attributes`): the checker's eager pass maps the named argument to its parameter
/// slot before deciding to skip it, and the lowering applies the hints by slot.
#[test]
fn test_xml_named_handlers_written_before_the_parser_argument() {
    if skip_without_xml_native("test_xml_named_handlers_written_before_the_parser_argument") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
$p = xml_parser_create();
xml_set_character_data_handler(handler: function ($parser, $data) { echo "C ", strlen($data), "\n"; }, parser: $p);
xml_set_element_handler(end_handler: function ($parser, $name) { echo "E ", strlen($name), "\n"; }, start_handler: function ($parser, $name, array $attributes) { echo "S ", $name, " ", json_encode($attributes), "\n"; }, parser: $p);
xml_parse($p, '<r id="1">hi</r>', true);
"#,
    );
    // PHP 8.5.10 prints the same.
    assert_eq!(out, "S R {\"ID\":\"1\"}\nC 2\nE 1\n");
}

/// Unpacking the parser (`...[$p]`, `...['parser' => $p]`, `...$args`) next to named
/// handler closures keeps the per-slot handler typing: the static spreads are flattened
/// before the setter lowering, the dynamic one goes through the shared spread plan with
/// the hints applied per slot, and the unpacked expression is evaluated exactly once.
#[test]
fn test_xml_unpacked_parser_with_named_handlers() {
    if skip_without_xml_native("test_xml_unpacked_parser_with_named_handlers") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
$p = xml_parser_create();
xml_set_element_handler(...[$p], start_handler: function ($parser, $name, array $attributes) { echo "A "; var_dump($attributes); }, end_handler: null);
xml_parse($p, '<root id="x"/>', true);
$q = xml_parser_create();
xml_set_element_handler(...['parser' => $q], start_handler: function ($parser, $name, array $attributes) { echo "B "; var_dump($attributes); }, end_handler: null);
xml_parse($q, '<root id="y"/>', true);
$r = xml_parser_create(); $args = [$r];
xml_set_element_handler(...$args, end_handler: function ($parser, $name) { echo "E ", strlen($name), "\n"; }, start_handler: function ($parser, $name, array $attributes) { echo "S $name "; var_dump($attributes); });
xml_set_character_data_handler(...$args, handler: function ($parser, $data) { echo "C ", strlen($data), "\n"; });
xml_parse($r, '<root id="z">hi</root>', true);
function make(): array { echo "made\n"; return [xml_parser_create()]; }
xml_set_element_handler(...make(), start_handler: function ($parser, $name, array $attributes) { var_dump($attributes); }, end_handler: null);
echo "set\n";
"#,
    );
    // PHP 8.5.10 prints the same.
    assert_eq!(
        out,
        "A array(1) {\n  [\"ID\"]=>\n  string(1) \"x\"\n}\nB array(1) {\n  [\"ID\"]=>\n  string(1) \"y\"\n}\nS ROOT array(1) {\n  [\"ID\"]=>\n  string(1) \"z\"\n}\nC 2\nE 4\nmade\nset\n"
    );
}

/// An associative array unpacked at run time (`...$named` with a `parser` key) next to
/// named handler closures: the shared spread plan declines string-keyed dynamic unpacks,
/// so the setter lowers the plan's normalized form (each parameter read from the array
/// by key) with the same per-slot handler typing.
#[test]
fn test_xml_dynamic_assoc_unpack_with_named_handlers() {
    if skip_without_xml_native("test_xml_dynamic_assoc_unpack_with_named_handlers") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
$p = xml_parser_create(); $a = ['parser' => $p];
xml_set_element_handler(...$a, end_handler: function ($parser, $name) { echo "E ", strlen($name), "\n"; }, start_handler: function ($parser, $name, array $attributes) { echo "S $name "; var_dump($attributes); });
xml_set_character_data_handler(...$a, handler: function ($parser, $data) { echo "C ", strlen($data), "\n"; });
xml_parse($p, '<root id="x">hi</root>', true);
"#,
    );
    // PHP 8.5.10 prints the same.
    assert_eq!(out, "S ROOT array(1) {\n  [\"ID\"]=>\n  string(1) \"x\"\n}\nC 2\nE 4\n");
}
