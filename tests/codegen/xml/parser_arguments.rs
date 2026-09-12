//! Purpose:
//! Exercises boxed parser arguments at XML handler-setter boundaries.
//!
//! Called from:
//! - The XML codegen integration suite.
//!
//! Key details:
//! - Requires the managed XML native package, like the other XML fixtures.
//! - Declared array and Mixed parameters must validate before object dispatch.

use crate::support::*;

/// A declared-array unpack is evaluated once and retains the parser across named-handler effects.
#[test]
fn test_xml_declared_array_unpack_keeps_named_argument_order() {
    if skip_without_xml_native("test_xml_declared_array_unpack_keeps_named_argument_order") {
        return;
    }
    let out = compile_and_run(r#"<?php
function stagedParser(XMLParser $parser): array { echo "prefix|"; return [$parser]; }
function stagedNullHandler(): mixed { echo "handler|"; return null; }
$parser = xml_parser_create();
echo xml_set_element_handler(...stagedParser($parser),
    end_handler: stagedNullHandler(), start_handler: null), "|";
echo xml_parse($parser, "<root/>", true);
"#);
    assert_eq!(out, "prefix|handler|1|1");
}

/// All setters accept a parser read from a declared array without losing the original object.
#[test]
fn test_xml_boxed_parser_arguments_preserve_handler_dispatch() {
    if skip_without_xml_native("test_xml_boxed_parser_arguments_preserve_handler_dispatch") {
        return;
    }
    let out = compile_and_run_with_heap_debug(r#"<?php
function parserArguments(XMLParser $parser): array { return [$parser]; }
function clearParserHandlers(mixed $parser): void {
    echo xml_set_element_handler($parser, null, null);
    echo xml_set_character_data_handler($parser, null);
    echo xml_set_processing_instruction_handler($parser, null);
    echo xml_set_default_handler($parser, null);
    echo xml_set_unparsed_entity_decl_handler($parser, null);
    echo xml_set_notation_decl_handler($parser, null);
    echo xml_set_external_entity_ref_handler($parser, null);
    echo xml_set_start_namespace_decl_handler($parser, null);
    echo xml_set_end_namespace_decl_handler($parser, null), "|";
}
$parser = xml_parser_create();
clearParserHandlers(parserArguments($parser)[0]);
XML_SET_ELEMENT_HANDLER(...parserArguments($parser), start_handler: function ($p, $name, array $attributes) {
    echo $name, ":", $attributes["ID"], "|";
}, end_handler: null);
\xml_set_character_data_handler(parserArguments($parser)[0], function ($p, $data) { echo $data, "|"; });
echo xml_parse($parser, '<root id="x">text</root>', true), "|";
unset($parser);
echo "done";
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "111111111|ROOT:x|text|1|done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Runtime parser validation rejects wrong boxed values after later argument side effects.
#[test]
fn test_xml_boxed_parser_type_errors_preserve_evaluation_and_cleanup() {
    if skip_without_xml_native("test_xml_boxed_parser_type_errors_preserve_evaluation_and_cleanup") {
        return;
    }
    let out = compile_and_run_with_heap_debug(r#"<?php
class WrongXmlParser {}
function badParserArguments(mixed $value): array { echo "args|"; return [$value]; }
function nullXmlHandler(): mixed { echo "handler|"; return null; }
function rejectParserArgument(mixed $value): void {
    try {
        xml_set_element_handler(...badParserArguments($value), start_handler: nullXmlHandler(), end_handler: null);
        echo "unexpected|";
    } catch (TypeError $error) {
        echo $error->getMessage(), "|";
        unset($error);
    }
}
rejectParserArgument(42);
rejectParserArgument("not a parser");
rejectParserArgument(null);
rejectParserArgument([]);
rejectParserArgument(new WrongXmlParser());
echo "done";
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    let expected = ["int", "string", "null", "array", "WrongXmlParser"]
        .iter().map(|ty| format!(
            "args|handler|xml_set_element_handler(): Argument #1 ($parser) must be of type XMLParser, {ty} given|",
        )).collect::<String>() + "done";
    assert_eq!(out.stdout, expected, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
