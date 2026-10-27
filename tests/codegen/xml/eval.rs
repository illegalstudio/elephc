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
