//! Purpose:
//! Interpreter-only tests for Magician's `ext/xml` / `ext/xmlwriter` forwarding homes: the
//! names are registered, a fragment that reaches a prelude function without a host that
//! linked the xml bridge raises PHP's undefined-function Error instead of an interpreter
//! fault, the ten registry builtins still run PHP's `$parser` check in every call shape,
//! handlers that exist only inside eval are rejected before the host is reached, and
//! `get_loaded_extensions()` follows the host's bridge registration.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - The real forwarding path (host-registered prelude functions and objects) is covered by
//!   the compiled `tests/codegen/xml` eval fixtures; this harness has no host program, so a
//!   fake `XMLParser` object is pre-bound into the scope where a parser is needed and the
//!   assertions stop at the point where the host would be called.

use super::super::*;
use super::support::*;

/// Every xml contract has a registered eval home reachable by name.
#[test]
fn xml_surface_is_registered() {
    for name in [
        "xml_parser_create",
        "xml_parse",
        "xml_parse_into_struct",
        "xml_error_string",
        "xmlwriter_open_memory",
        "xmlwriter_flush",
    ] {
        assert!(
            eval_php_visible_builtin_exists(name),
            "{name} must be a registered eval builtin"
        );
        assert!(eval_xml_builtin_name(name), "{name} must route to the xml dispatcher");
    }
    assert!(!eval_xml_builtin_name("xml_helper"));
    assert!(!eval_xml_builtin_name("strlen"));
}

/// Without a host that registered the compiled prelude, calling the surface raises PHP's
/// catchable `Call to undefined function` Error, whichever call shape is used.
#[test]
fn missing_host_prelude_raises_undefined_function() {
    let program = parse_fragment(
        br#"try { xml_parser_create(); echo "unreachable"; }
catch (Error $e) { echo $e->getMessage(); }
echo "|";
try { $f = "xmlwriter_open_memory"; $f(); echo "unreachable"; }
catch (Error $e) { echo $e->getMessage(); }"#,
    )
    .expect("parse xml fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("run xml fragment");
    assert_eq!(
        values.output,
        "Call to undefined function xml_parser_create()|Call to undefined function xmlwriter_open_memory()"
    );
}

/// `extension_loaded('xml')` inside eval answers false while no host registered the prelude.
#[test]
fn extension_loaded_follows_the_host_registration() {
    let context = ElephcEvalContext::new();
    assert!(!eval_extension_is_loaded_in("xml", &context));
    assert!(!eval_extension_is_loaded_in("XMLWriter", &context));
    assert!(eval_extension_is_loaded_in("json", &context));
}

/// Without a host that linked the bridge, `function_exists()` splits the surface the way
/// the compiler does: the prelude functions are absent, the ten registry builtins exist.
#[test]
fn function_exists_splits_registry_builtins_from_prelude_functions() {
    let context = ElephcEvalContext::new();
    assert!(eval_xml_builtin_linked(&context, "xml_parse_into_struct"));
    assert!(eval_xml_builtin_linked(&context, "xml_set_element_handler"));
    assert!(!eval_xml_builtin_linked(&context, "xml_parse"));
    assert!(!eval_xml_builtin_linked(&context, "xmlwriter_open_memory"));
    assert!(eval_xml_builtin_linked(&context, "strlen"));
}

/// The ten registry builtins exist on both backends whether or not the bridge is linked, so
/// without a host they still reach PHP's `$parser` `TypeError` — spelled with PHP's own
/// value names (`int`, `float`, `true` / `false`, ...), never `gettype()`'s — in every call
/// shape: direct, variable function, `call_user_func()` and `call_user_func_array()`.
#[test]
fn registry_builtins_check_their_parser_argument_without_the_bridge() {
    let program = parse_fragment(
        br#"try { xml_set_element_handler(null, "a", "b"); } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { xml_parse_into_struct(1, "x", $v); } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
$f = "xml_set_default_handler";
try { $f(true, "a"); } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { call_user_func("xml_parse_into_struct", 1.5, "x", null); } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { call_user_func_array("xml_set_character_data_handler", [[], "a"]); } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { xml_set_notation_decl_handler(false, "a"); } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { xml_set_processing_instruction_handler("p", "a"); } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { xml_parse(1, "x"); } catch (Error $e) { echo $e->getMessage(); }"#,
    )
    .expect("parse xml fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("run xml fragment");
    assert_eq!(
        values.output,
        "xml_set_element_handler(): Argument #1 ($parser) must be of type XMLParser, null given\
|xml_parse_into_struct(): Argument #1 ($parser) must be of type XMLParser, int given\
|xml_set_default_handler(): Argument #1 ($parser) must be of type XMLParser, true given\
|xml_parse_into_struct(): Argument #1 ($parser) must be of type XMLParser, float given\
|xml_set_character_data_handler(): Argument #1 ($parser) must be of type XMLParser, array given\
|xml_set_notation_decl_handler(): Argument #1 ($parser) must be of type XMLParser, false given\
|xml_set_processing_instruction_handler(): Argument #1 ($parser) must be of type XMLParser, string given\
|Call to undefined function xml_parse()"
    );
}

/// A handler that exists only inside eval — a closure, an eval-declared function named as a
/// string, an `[$object, 'method']` pair or `Class::method` over an eval-declared class, or
/// an eval-declared invokable object — is rejected with a clear catchable `Error` before
/// the parser's `__elephc_set_*` method is ever called; each handler slot is checked, and a
/// compiled name in the other slot does not mask it.
#[test]
fn eval_declared_handlers_are_rejected_before_reaching_the_host() {
    let program = parse_fragment(
        br#"function ev_start($p, $n, $a) {}
class EvSink { function s($p, $n) {} }
try { xml_set_element_handler($p, function ($p, $n, $a) {}, null); } catch (Error $e) { echo $e->getMessage(); }
echo "|";
try { xml_set_element_handler($p, "on_start", "ev_start"); } catch (Error $e) { echo $e->getMessage(); }
echo "|";
try { xml_set_character_data_handler($p, [new EvSink(), "s"]); } catch (Error $e) { echo $e->getMessage(); }
echo "|";
try { xml_set_default_handler($p, "EvSink::s"); } catch (Error $e) { echo $e->getMessage(); }
echo "|";
try { xml_set_default_handler($p, new EvSink()); } catch (Error $e) { echo $e->getMessage(); }
echo "|";
$f = "xml_set_end_namespace_decl_handler";
try { $f($p, "\\ev_start"); } catch (Error $e) { echo $e->getMessage(); }"#,
    )
    .expect("parse xml fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let parser = values.new_object("XMLParser").expect("allocate fake parser");
    scope.set("p", parser, ScopeCellOwnership::Borrowed);
    execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("run xml fragment");
    let rejected = |function: &str| {
        format!(
            "{function}(): handlers declared inside eval() cannot be invoked by the compiled parser; declare the handler in compiled code"
        )
    };
    assert_eq!(
        values.output,
        [
            rejected("xml_set_element_handler"),
            rejected("xml_set_element_handler"),
            rejected("xml_set_character_data_handler"),
            rejected("xml_set_default_handler"),
            rejected("xml_set_default_handler"),
            rejected("xml_set_end_namespace_decl_handler"),
        ]
        .join("|")
    );
}

/// `xml_set_object()` is a prelude function, so it needs the host's registration; once the
/// host registered it, an object of an eval-declared class (or an eval closure) is rejected
/// before the host is called, and a bad `$parser` still gets PHP's `TypeError` first.
#[test]
fn xml_set_object_rejects_eval_declared_objects() {
    let program = parse_fragment(
        br#"class EvSink { function s($p, $n, $a) {} }
try { xml_set_object($p, new EvSink()); } catch (Error $e) { echo $e->getMessage(); }
echo "|";
try { xml_set_object($p, function () {}); } catch (Error $e) { echo $e->getMessage(); }
echo "|";
try { xml_set_object("nope", new EvSink()); } catch (TypeError $e) { echo $e->getMessage(); }"#,
    )
    .expect("parse xml fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let unreachable = values.int(0).expect("allocate fake result");
    let create =
        NativeFunction::new(unreachable.as_ptr().cast(), fake_native_return_descriptor, 0);
    assert!(context.define_native_function("xml_parser_create", create).is_ok());
    let mut set_object =
        NativeFunction::new(unreachable.as_ptr().cast(), fake_native_return_descriptor, 2);
    assert!(set_object.set_param_name(0, "parser"));
    assert!(set_object.set_param_name(1, "object"));
    assert!(context.define_native_function("xml_set_object", set_object).is_ok());
    let parser = values.new_object("XMLParser").expect("allocate fake parser");
    scope.set("p", parser, ScopeCellOwnership::Borrowed);
    execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("run xml fragment");
    assert_eq!(
        values.output,
        "xml_set_object(): objects of classes declared inside eval() cannot be bound as handler targets\
|xml_set_object(): objects of classes declared inside eval() cannot be bound as handler targets\
|xml_set_object(): Argument #1 ($parser) must be of type XMLParser, string given"
    );
}

/// `get_loaded_extensions()` lists `xml` and `xmlwriter` exactly when the host registered
/// the prelude — the same condition `extension_loaded('xml')` answers — and never in the
/// Zend list.
#[test]
fn get_loaded_extensions_lists_xml_only_when_the_host_linked_the_bridge() {
    let source = br#"$ext = get_loaded_extensions();
echo count($ext), ":", in_array("xml", $ext) ? "xml" : "no-xml", ":", in_array("xmlwriter", $ext) ? "xmlwriter" : "no-xmlwriter";
echo ":", extension_loaded("xml") ? "loaded" : "absent", ":", count(get_loaded_extensions(true));"#;
    let base = if cfg!(feature = "curl") { 12 } else { 11 };

    let program = parse_fragment(source).expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("run without the bridge");
    assert_eq!(values.output, format!("{base}:no-xml:no-xmlwriter:absent:1"));

    let program = parse_fragment(source).expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let unreachable = values.int(0).expect("allocate fake result");
    let create =
        NativeFunction::new(unreachable.as_ptr().cast(), fake_native_return_descriptor, 0);
    assert!(context.define_native_function("xml_parser_create", create).is_ok());
    execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("run with the bridge registered");
    assert_eq!(
        values.output,
        format!("{}:xml:xmlwriter:loaded:1", base + 2)
    );
}
