//! Purpose:
//! Compile-time diagnostic tests for the `ext/xml` / `ext/xmlwriter` prelude surface:
//! `XMLParser` is `final`, the wrappers reject wrong argument counts and types, the
//! `xml_parse_into_struct()` registry builtin requires variables for its outputs, and the
//! handler setters reject handler shapes the run time could not serve (a misspelled
//! function name, a variadic closure, a closure with more required parameters than the
//! event supplies).
//!
//! Called from:
//! - `cargo test --test error_tests xml` through Rust's test harness.
//!
//! Key details:
//! - These need only the INJECTED PRELUDE, never a link, so they run on every machine.
//! - Every assertion also checks the message is a REAL diagnostic rather than an
//!   "Undefined function/class" one, which is what a silently broken injection would
//!   produce.

use super::*;

/// Asserts `src` fails to compile and the message contains `needle`, with a guard against
/// a missing-prelude error masquerading as the expected diagnostic.
fn expect_xml_error(src: &str, needle: &str) {
    let error = check_source(src).expect_err("program must fail to compile");
    assert!(
        !error.contains("Undefined function") && !error.contains("Undefined class"),
        "xml prelude was not injected; got: {error}"
    );
    assert!(
        error.contains(needle),
        "expected an error containing {needle:?}, got: {error}"
    );
}

/// PHP's `XMLParser` is `final`: a user class cannot extend it.
#[test]
fn xml_parser_cannot_be_extended() {
    expect_xml_error(
        "<?php class MyParser extends XMLParser {} $p = xml_parser_create();",
        "final",
    );
}

/// `xml_parse()` takes at most three arguments.
#[test]
fn xml_parse_rejects_extra_arguments() {
    expect_xml_error(
        "<?php $p = xml_parser_create(); xml_parse($p, '<a/>', true, 4);",
        "xml_parse",
    );
}

/// `xml_parser_set_option()` requires its option number.
#[test]
fn xml_parser_set_option_requires_the_option() {
    expect_xml_error(
        "<?php $p = xml_parser_create(); xml_parser_set_option($p);",
        "xml_parser_set_option",
    );
}

/// `xml_parse()`'s data must be a string, not an array.
#[test]
fn xml_parse_rejects_non_string_data() {
    expect_xml_error(
        "<?php $p = xml_parser_create(); xml_parse($p, ['<a/>'], true);",
        "xml_parse",
    );
}

/// `xml_parse_into_struct()`'s outputs must be plain variables.
#[test]
fn xml_parse_into_struct_requires_variable_outputs() {
    expect_xml_error(
        "<?php $p = xml_parser_create(); xml_parse_into_struct($p, '<a/>', []);",
        "could not be passed by reference",
    );
}

/// `xml_parse_into_struct()`'s first argument must be an `XMLParser`.
#[test]
fn xml_parse_into_struct_requires_a_parser() {
    expect_xml_error(
        "<?php xml_parse_into_struct('nope', '<a/>', $values);",
        "must be of type XMLParser",
    );
}

/// `xmlwriter_start_element()` needs the element name.
#[test]
fn xmlwriter_start_element_requires_a_name() {
    expect_xml_error(
        "<?php $w = xmlwriter_open_memory(); xmlwriter_start_element($w);",
        "xmlwriter_start_element",
    );
}

/// `XMLWriter::writeAttribute()` rejects a non-string value.
#[test]
fn xmlwriter_write_attribute_rejects_array_value() {
    expect_xml_error(
        "<?php $w = new XMLWriter(); $w->openMemory(); $w->startElement('a'); $w->writeAttribute('x', ['v']);",
        "writeAttribute",
    );
}

/// `xml_set_element_handler()`'s first argument must be an `XMLParser`; the handler
/// setters are checked at compile time like `xml_parse_into_struct()`.
#[test]
fn xml_set_element_handler_requires_a_parser() {
    expect_xml_error(
        "<?php xml_set_element_handler('nope', 'a', 'b');",
        "must be of type XMLParser",
    );
}

/// `xml_parse_into_struct()` cannot write into a property: PHP accepts one by reference,
/// but the lowering can only store into a variable, so the hook rejects it up front.
#[test]
fn xml_parse_into_struct_rejects_a_property_output() {
    expect_xml_error(
        "<?php class C { public $v; function run() { $p = xml_parser_create(); xml_parse_into_struct($p, '<a/>', $this->v); } }",
        "xml_parse_into_struct() parameter $values must be passed a variable",
    );
}

/// The same holds for an array element, at the `$index` position too.
#[test]
fn xml_parse_into_struct_rejects_an_array_element_output() {
    expect_xml_error(
        "<?php $a = []; $p = xml_parser_create(); xml_parse_into_struct($p, '<a/>', $vals, $a[0]);",
        "xml_parse_into_struct() parameter $index must be passed a variable",
    );
}

/// A function-name handler must be spelled as declared: the run-time dynamic callable
/// lookup is case-sensitive, so a case-insensitive compile-time match would only defer
/// the failure to an unrelated `ValueError`.
#[test]
fn xml_set_element_handler_rejects_a_misspelled_function_name() {
    expect_xml_error(
        "<?php function laterstart($p, $n, $a) {} $p = xml_parser_create(); xml_set_element_handler($p, 'LaterStart', null);",
        "xml_set_element_handler(): handler 'LaterStart' must match the declared spelling 'laterstart' (dynamic callables resolve case-sensitively)",
    );
}

/// A handler closure cannot declare a variadic parameter.
#[test]
fn xml_set_element_handler_rejects_a_variadic_closure() {
    expect_xml_error(
        "<?php $p = xml_parser_create(); xml_set_element_handler($p, function ($p, ...$rest) { echo count($rest); }, null);",
        "xml_set_element_handler(): handler closures cannot declare a variadic parameter: declare the parameters the event supplies",
    );
}

/// A handler closure with more required parameters than the event supplies is rejected
/// at compile time; PHP would throw `ArgumentCountError` from inside the parse.
#[test]
fn xml_set_element_handler_rejects_extra_required_parameters() {
    expect_xml_error(
        "<?php $p = xml_parser_create(); xml_set_element_handler($p, function ($p, $n, $a, $extra) { echo 1; }, null);",
        "xml_set_element_handler(): start handler declares 4 required parameters but the element event supplies 3",
    );
    expect_xml_error(
        "<?php $p = xml_parser_create(); xml_set_character_data_handler($p, function ($p, $d, $more, $evenmore) { echo 1; });",
        "xml_set_character_data_handler(): handler declares 4 required parameters but the character data event supplies 2",
    );
}

/// Optional parameters beyond the event's arity are fine: only REQUIRED ones count.
#[test]
fn xml_set_element_handler_accepts_optional_extra_parameters() {
    check_source(
        "<?php $p = xml_parser_create(); xml_set_element_handler($p, function ($p, $n, $a, $extra = 'dflt') { echo $extra; }, null);",
    )
    .expect("optional surplus parameters compile");
}

/// The surplus-parameter check follows the parameter a NAMED argument binds, whatever
/// position the closure is written at.
#[test]
fn xml_set_element_handler_rejects_extra_required_parameters_in_a_named_call() {
    expect_xml_error(
        "<?php $p = xml_parser_create(); xml_set_element_handler(end_handler: null, start_handler: function ($p, $n, $a, $extra) { echo 1; }, parser: $p);",
        "xml_set_element_handler(): start handler declares 4 required parameters but the element event supplies 3",
    );
}

/// `XMLParser::__clone()` and `XMLWriter::__clone()` are the `final` hooks that make the
/// classes uncloneable; a subclass overriding one could otherwise re-enable a shallow copy
/// that shares the native handle. PHP compiles the override and still refuses the clone at
/// run time; elephc rejects it at compile time (documented divergence).
#[test]
fn xmlwriter_subclass_cannot_override_the_uncloneable_hook() {
    expect_xml_error(
        "<?php class Over extends XMLWriter { public function __clone(): void {} } $w = new Over(); $w->openMemory();",
        "Cannot override final method XMLWriter::__clone",
    );
}

/// The surplus-parameter diagnostic also fires when the parser is unpacked before the
/// named handlers (the checker resolves the closure's slot through the call plan).
#[test]
fn xml_set_element_handler_rejects_extra_required_parameters_after_an_unpack() {
    expect_xml_error(
        "<?php $p = xml_parser_create(); $args = [$p]; xml_set_element_handler(...$args, start_handler: function ($p, $n, $a, $extra) { echo 1; }, end_handler: null);",
        "xml_set_element_handler(): start handler declares 4 required parameters but the element event supplies 3",
    );
}
