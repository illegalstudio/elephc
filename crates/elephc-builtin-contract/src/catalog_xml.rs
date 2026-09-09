//! Purpose:
//! Canonical contracts for PHP's `ext/xml` (`xml_*`, the expat-style SAX parser API) and
//! `ext/xmlwriter` (`xmlwriter_*`) function surfaces.
//!
//! Called from:
//! - `crate::registry` when assembling the complete shared contract catalog.
//!
//! Key details:
//! - Most entries are `BuiltinKind::PreludeProvided`: the compiler implements the names as
//!   elephc-PHP declarations injected by `elephc::xml_prelude`, which call the `elephc_xml`
//!   bridge through an `extern` block, and Magician reaches the same compiled declarations
//!   through its native-function bridge.
//! - Ten entries are `BuiltinKind::Function` registry builtins whose AOT homes compose the
//!   prelude's `__elephc_xml_*` twins: `xml_parse_into_struct` (its two write-only
//!   by-reference outputs may name undefined variables, which only a checker hook can
//!   accept) and the nine `xml_set_*_handler` setters (a checker hook types an
//!   unannotated handler closure's parameters from the event it will receive). See
//!   `elephc::builtins::xml`.
//! - Parameter types spell PHP 8.5's stubs: `XMLParser` / `XMLWriter` receivers and the
//!   `callable|string|null` handler unions are `Mixed` because the catalog has no class or
//!   union spelling; the prelude declares the class types and the parity gate accepts that.

use crate::{
    Area, BuiltinContract, BuiltinId, BuiltinKind, DefaultSpec, ParamSpec, PhpModule, TypeSpec,
};

/// Builds one by-value parameter, optionally with a PHP default.
macro_rules! param {
    ($name:literal, $ty:ident) => {
        ParamSpec {
            name: $name,
            ty: TypeSpec::$ty,
            default: None,
            by_ref: false,
        }
    };
    ($name:literal, $ty:ident = $default:expr) => {
        ParamSpec {
            name: $name,
            ty: TypeSpec::$ty,
            default: Some($default),
            by_ref: false,
        }
    };
    ($name:literal, ?$ty:ident = $default:expr) => {
        ParamSpec {
            name: $name,
            ty: TypeSpec::Nullable(&TypeSpec::$ty),
            default: Some($default),
            by_ref: false,
        }
    };
    ($name:literal, ?$ty:ident) => {
        ParamSpec {
            name: $name,
            ty: TypeSpec::Nullable(&TypeSpec::$ty),
            default: None,
            by_ref: false,
        }
    };
}

/// Builds one by-reference parameter, optionally with a PHP default.
macro_rules! by_ref_param {
    ($name:literal, $ty:ident) => {
        ParamSpec {
            name: $name,
            ty: TypeSpec::$ty,
            default: None,
            by_ref: true,
        }
    };
    ($name:literal, $ty:ident = $default:expr) => {
        ParamSpec {
            name: $name,
            ty: TypeSpec::$ty,
            default: Some($default),
            by_ref: true,
        }
    };
}

/// Builds one XML-family contract.
macro_rules! xml_contract {
    (
        $name:literal, $module:ident, $kind:ident,
        [$($param:expr),* $(,)?], $returns:expr, $summary:literal
        $(, deprecation: $deprecation:literal)? $(,)?
    ) => {
        BuiltinContract {
            id: BuiltinId::from_canonical_name($name),
            name: $name,
            area: Area::Xml,
            module: PhpModule::$module,
            since: None,
            kind: BuiltinKind::$kind,
            params: &[$($param),*],
            variadic: None,
            variadic_by_ref: false,
            min_args: None,
            max_args: None,
            arity_error: None,
            returns: $returns,
            by_ref_return: false,
            summary: $summary,
            examples: &[],
            php_manual: None,
            deprecation: xml_contract!(@deprecation $($deprecation)?),
            extension: false,
            internal: false,
            requirements: &[],
        }
    };
    (@deprecation $text:literal) => { Some($text) };
    (@deprecation) => { None };
}

/// The `ext/xml` and `ext/xmlwriter` function contracts, in PHP's stub order.
pub(crate) static CONTRACTS: &[BuiltinContract] = &[
    // -- ext/xml --
    xml_contract!(
        "xml_parser_create", Xml, PreludeProvided,
        [param!("encoding", ?Str = DefaultSpec::Null)],
        TypeSpec::Mixed,
        "Creates a new XML parser object."
    ),
    xml_contract!(
        "xml_parser_create_ns", Xml, PreludeProvided,
        [
            param!("encoding", ?Str = DefaultSpec::Null),
            param!("separator", Str = DefaultSpec::Str(":")),
        ],
        TypeSpec::Mixed,
        "Creates a namespace-aware XML parser whose qualified names join the URI and local name with a separator."
    ),
    xml_contract!(
        "xml_set_object", Xml, PreludeProvided,
        [param!("parser", Mixed), param!("object", Mixed)],
        TypeSpec::Bool,
        "Binds an object whose methods are looked up for string handler names.",
        deprecation: "Deprecated since PHP 8.4; provide a proper method callable to the xml_set_*_handler() functions."
    ),
    xml_contract!(
        "xml_set_element_handler", Xml, Function,
        [
            param!("parser", Mixed),
            param!("start_handler", Mixed),
            param!("end_handler", Mixed),
        ],
        TypeSpec::Bool,
        "Sets the start and end element handlers."
    ),
    xml_contract!(
        "xml_set_character_data_handler", Xml, Function,
        [param!("parser", Mixed), param!("handler", Mixed)],
        TypeSpec::Bool,
        "Sets the character data handler."
    ),
    xml_contract!(
        "xml_set_processing_instruction_handler", Xml, Function,
        [param!("parser", Mixed), param!("handler", Mixed)],
        TypeSpec::Bool,
        "Sets the processing instruction handler."
    ),
    xml_contract!(
        "xml_set_default_handler", Xml, Function,
        [param!("parser", Mixed), param!("handler", Mixed)],
        TypeSpec::Bool,
        "Sets the default handler that receives everything no other handler claims."
    ),
    xml_contract!(
        "xml_set_unparsed_entity_decl_handler", Xml, Function,
        [param!("parser", Mixed), param!("handler", Mixed)],
        TypeSpec::Bool,
        "Sets the unparsed (NDATA) entity declaration handler."
    ),
    xml_contract!(
        "xml_set_notation_decl_handler", Xml, Function,
        [param!("parser", Mixed), param!("handler", Mixed)],
        TypeSpec::Bool,
        "Sets the notation declaration handler."
    ),
    xml_contract!(
        "xml_set_external_entity_ref_handler", Xml, Function,
        [param!("parser", Mixed), param!("handler", Mixed)],
        TypeSpec::Bool,
        "Sets the external entity reference handler."
    ),
    xml_contract!(
        "xml_set_start_namespace_decl_handler", Xml, Function,
        [param!("parser", Mixed), param!("handler", Mixed)],
        TypeSpec::Bool,
        "Sets the handler called when a namespace declaration starts."
    ),
    xml_contract!(
        "xml_set_end_namespace_decl_handler", Xml, Function,
        [param!("parser", Mixed), param!("handler", Mixed)],
        TypeSpec::Bool,
        "Sets the handler called when a namespace declaration ends."
    ),
    xml_contract!(
        "xml_parse", Xml, PreludeProvided,
        [
            param!("parser", Mixed),
            param!("data", Str),
            param!("is_final", Bool = DefaultSpec::Bool(false)),
        ],
        TypeSpec::Int,
        "Parses a chunk of XML data, dispatching the registered handlers."
    ),
    xml_contract!(
        "xml_parse_into_struct", Xml, Function,
        [
            param!("parser", Mixed),
            param!("data", Str),
            by_ref_param!("values", Mixed),
            by_ref_param!("index", Mixed = DefaultSpec::Null),
        ],
        TypeSpec::Mixed,
        "Parses a whole XML document into an array of tag structures and an index by tag name."
    ),
    xml_contract!(
        "xml_get_error_code", Xml, PreludeProvided,
        [param!("parser", Mixed)],
        TypeSpec::Int,
        "Returns the parser's last error code."
    ),
    xml_contract!(
        "xml_error_string", Xml, PreludeProvided,
        [param!("error_code", Int)],
        TypeSpec::Nullable(&TypeSpec::Str),
        "Returns the message for an XML parser error code."
    ),
    xml_contract!(
        "xml_get_current_line_number", Xml, PreludeProvided,
        [param!("parser", Mixed)],
        TypeSpec::Int,
        "Returns the current line number of the parser."
    ),
    xml_contract!(
        "xml_get_current_column_number", Xml, PreludeProvided,
        [param!("parser", Mixed)],
        TypeSpec::Int,
        "Returns the current column number of the parser."
    ),
    xml_contract!(
        "xml_get_current_byte_index", Xml, PreludeProvided,
        [param!("parser", Mixed)],
        TypeSpec::Int,
        "Returns the current byte index of the parser."
    ),
    xml_contract!(
        "xml_parser_free", Xml, PreludeProvided,
        [param!("parser", Mixed)],
        TypeSpec::Bool,
        "Frees an XML parser; a no-op kept for compatibility.",
        deprecation: "Deprecated since PHP 8.5, as it has no effect since PHP 8.0."
    ),
    xml_contract!(
        "xml_parser_set_option", Xml, PreludeProvided,
        [param!("parser", Mixed), param!("option", Int), param!("value", Mixed)],
        TypeSpec::Bool,
        "Sets an XML_OPTION_* parser option."
    ),
    xml_contract!(
        "xml_parser_get_option", Xml, PreludeProvided,
        [param!("parser", Mixed), param!("option", Int)],
        TypeSpec::Mixed,
        "Reads an XML_OPTION_* parser option."
    ),
    // -- ext/xmlwriter --
    xml_contract!(
        "xmlwriter_open_uri", Xmlwriter, PreludeProvided,
        [param!("uri", Str)],
        TypeSpec::Mixed,
        "Creates a writer that outputs to a URI or file path; throws ValueError when it cannot be opened."
    ),
    xml_contract!(
        "xmlwriter_open_memory", Xmlwriter, PreludeProvided,
        [],
        TypeSpec::Mixed,
        "Creates a writer that buffers its output in memory."
    ),
    xml_contract!(
        "xmlwriter_set_indent", Xmlwriter, PreludeProvided,
        [param!("writer", Mixed), param!("enable", Bool)],
        TypeSpec::Bool,
        "Toggles indentation of the output."
    ),
    xml_contract!(
        "xmlwriter_set_indent_string", Xmlwriter, PreludeProvided,
        [param!("writer", Mixed), param!("indentation", Str)],
        TypeSpec::Bool,
        "Sets the string used for one indentation level."
    ),
    xml_contract!(
        "xmlwriter_start_comment", Xmlwriter, PreludeProvided,
        [param!("writer", Mixed)],
        TypeSpec::Bool,
        "Starts a comment."
    ),
    xml_contract!(
        "xmlwriter_end_comment", Xmlwriter, PreludeProvided,
        [param!("writer", Mixed)],
        TypeSpec::Bool,
        "Ends the current comment."
    ),
    xml_contract!(
        "xmlwriter_start_attribute", Xmlwriter, PreludeProvided,
        [param!("writer", Mixed), param!("name", Str)],
        TypeSpec::Bool,
        "Starts an attribute."
    ),
    xml_contract!(
        "xmlwriter_end_attribute", Xmlwriter, PreludeProvided,
        [param!("writer", Mixed)],
        TypeSpec::Bool,
        "Ends the current attribute."
    ),
    xml_contract!(
        "xmlwriter_write_attribute", Xmlwriter, PreludeProvided,
        [param!("writer", Mixed), param!("name", Str), param!("value", Str)],
        TypeSpec::Bool,
        "Writes a complete attribute."
    ),
    xml_contract!(
        "xmlwriter_start_attribute_ns", Xmlwriter, PreludeProvided,
        [
            param!("writer", Mixed),
            param!("prefix", ?Str),
            param!("name", Str),
            param!("namespace", ?Str),
        ],
        TypeSpec::Bool,
        "Starts a namespaced attribute."
    ),
    xml_contract!(
        "xmlwriter_write_attribute_ns", Xmlwriter, PreludeProvided,
        [
            param!("writer", Mixed),
            param!("prefix", ?Str),
            param!("name", Str),
            param!("namespace", ?Str),
            param!("value", Str),
        ],
        TypeSpec::Bool,
        "Writes a complete namespaced attribute."
    ),
    xml_contract!(
        "xmlwriter_start_element", Xmlwriter, PreludeProvided,
        [param!("writer", Mixed), param!("name", Str)],
        TypeSpec::Bool,
        "Starts an element."
    ),
    xml_contract!(
        "xmlwriter_end_element", Xmlwriter, PreludeProvided,
        [param!("writer", Mixed)],
        TypeSpec::Bool,
        "Ends the current element, using the short form when it has no content."
    ),
    xml_contract!(
        "xmlwriter_full_end_element", Xmlwriter, PreludeProvided,
        [param!("writer", Mixed)],
        TypeSpec::Bool,
        "Ends the current element with an explicit end tag."
    ),
    xml_contract!(
        "xmlwriter_start_element_ns", Xmlwriter, PreludeProvided,
        [
            param!("writer", Mixed),
            param!("prefix", ?Str),
            param!("name", Str),
            param!("namespace", ?Str),
        ],
        TypeSpec::Bool,
        "Starts a namespaced element."
    ),
    xml_contract!(
        "xmlwriter_write_element", Xmlwriter, PreludeProvided,
        [
            param!("writer", Mixed),
            param!("name", Str),
            param!("content", ?Str = DefaultSpec::Null),
        ],
        TypeSpec::Bool,
        "Writes a complete element."
    ),
    xml_contract!(
        "xmlwriter_write_element_ns", Xmlwriter, PreludeProvided,
        [
            param!("writer", Mixed),
            param!("prefix", ?Str),
            param!("name", Str),
            param!("namespace", ?Str),
            param!("content", ?Str = DefaultSpec::Null),
        ],
        TypeSpec::Bool,
        "Writes a complete namespaced element."
    ),
    xml_contract!(
        "xmlwriter_start_pi", Xmlwriter, PreludeProvided,
        [param!("writer", Mixed), param!("target", Str)],
        TypeSpec::Bool,
        "Starts a processing instruction."
    ),
    xml_contract!(
        "xmlwriter_end_pi", Xmlwriter, PreludeProvided,
        [param!("writer", Mixed)],
        TypeSpec::Bool,
        "Ends the current processing instruction."
    ),
    xml_contract!(
        "xmlwriter_write_pi", Xmlwriter, PreludeProvided,
        [param!("writer", Mixed), param!("target", Str), param!("content", Str)],
        TypeSpec::Bool,
        "Writes a complete processing instruction."
    ),
    xml_contract!(
        "xmlwriter_start_cdata", Xmlwriter, PreludeProvided,
        [param!("writer", Mixed)],
        TypeSpec::Bool,
        "Starts a CDATA section."
    ),
    xml_contract!(
        "xmlwriter_end_cdata", Xmlwriter, PreludeProvided,
        [param!("writer", Mixed)],
        TypeSpec::Bool,
        "Ends the current CDATA section."
    ),
    xml_contract!(
        "xmlwriter_write_cdata", Xmlwriter, PreludeProvided,
        [param!("writer", Mixed), param!("content", Str)],
        TypeSpec::Bool,
        "Writes a complete CDATA section."
    ),
    xml_contract!(
        "xmlwriter_text", Xmlwriter, PreludeProvided,
        [param!("writer", Mixed), param!("content", Str)],
        TypeSpec::Bool,
        "Writes escaped text content."
    ),
    xml_contract!(
        "xmlwriter_write_raw", Xmlwriter, PreludeProvided,
        [param!("writer", Mixed), param!("content", Str)],
        TypeSpec::Bool,
        "Writes raw, unescaped content."
    ),
    xml_contract!(
        "xmlwriter_start_document", Xmlwriter, PreludeProvided,
        [
            param!("writer", Mixed),
            param!("version", ?Str = DefaultSpec::Str("1.0")),
            param!("encoding", ?Str = DefaultSpec::Null),
            param!("standalone", ?Str = DefaultSpec::Null),
        ],
        TypeSpec::Bool,
        "Writes the XML declaration."
    ),
    xml_contract!(
        "xmlwriter_end_document", Xmlwriter, PreludeProvided,
        [param!("writer", Mixed)],
        TypeSpec::Bool,
        "Ends the document, closing every open node."
    ),
    xml_contract!(
        "xmlwriter_write_comment", Xmlwriter, PreludeProvided,
        [param!("writer", Mixed), param!("content", Str)],
        TypeSpec::Bool,
        "Writes a complete comment."
    ),
    xml_contract!(
        "xmlwriter_start_dtd", Xmlwriter, PreludeProvided,
        [
            param!("writer", Mixed),
            param!("qualifiedName", Str),
            param!("publicId", ?Str = DefaultSpec::Null),
            param!("systemId", ?Str = DefaultSpec::Null),
        ],
        TypeSpec::Bool,
        "Starts a DTD."
    ),
    xml_contract!(
        "xmlwriter_end_dtd", Xmlwriter, PreludeProvided,
        [param!("writer", Mixed)],
        TypeSpec::Bool,
        "Ends the current DTD."
    ),
    xml_contract!(
        "xmlwriter_write_dtd", Xmlwriter, PreludeProvided,
        [
            param!("writer", Mixed),
            param!("name", Str),
            param!("publicId", ?Str = DefaultSpec::Null),
            param!("systemId", ?Str = DefaultSpec::Null),
            param!("content", ?Str = DefaultSpec::Null),
        ],
        TypeSpec::Bool,
        "Writes a complete DTD."
    ),
    xml_contract!(
        "xmlwriter_start_dtd_element", Xmlwriter, PreludeProvided,
        [param!("writer", Mixed), param!("qualifiedName", Str)],
        TypeSpec::Bool,
        "Starts a DTD element declaration."
    ),
    xml_contract!(
        "xmlwriter_end_dtd_element", Xmlwriter, PreludeProvided,
        [param!("writer", Mixed)],
        TypeSpec::Bool,
        "Ends the current DTD element declaration."
    ),
    xml_contract!(
        "xmlwriter_write_dtd_element", Xmlwriter, PreludeProvided,
        [param!("writer", Mixed), param!("name", Str), param!("content", Str)],
        TypeSpec::Bool,
        "Writes a complete DTD element declaration."
    ),
    xml_contract!(
        "xmlwriter_start_dtd_attlist", Xmlwriter, PreludeProvided,
        [param!("writer", Mixed), param!("name", Str)],
        TypeSpec::Bool,
        "Starts a DTD attribute list declaration."
    ),
    xml_contract!(
        "xmlwriter_end_dtd_attlist", Xmlwriter, PreludeProvided,
        [param!("writer", Mixed)],
        TypeSpec::Bool,
        "Ends the current DTD attribute list declaration."
    ),
    xml_contract!(
        "xmlwriter_write_dtd_attlist", Xmlwriter, PreludeProvided,
        [param!("writer", Mixed), param!("name", Str), param!("content", Str)],
        TypeSpec::Bool,
        "Writes a complete DTD attribute list declaration."
    ),
    xml_contract!(
        "xmlwriter_start_dtd_entity", Xmlwriter, PreludeProvided,
        [param!("writer", Mixed), param!("name", Str), param!("isParam", Bool)],
        TypeSpec::Bool,
        "Starts a DTD entity declaration."
    ),
    xml_contract!(
        "xmlwriter_end_dtd_entity", Xmlwriter, PreludeProvided,
        [param!("writer", Mixed)],
        TypeSpec::Bool,
        "Ends the current DTD entity declaration."
    ),
    xml_contract!(
        "xmlwriter_write_dtd_entity", Xmlwriter, PreludeProvided,
        [
            param!("writer", Mixed),
            param!("name", Str),
            param!("content", Str),
            param!("isParam", Bool = DefaultSpec::Bool(false)),
            param!("publicId", ?Str = DefaultSpec::Null),
            param!("systemId", ?Str = DefaultSpec::Null),
            param!("notationData", ?Str = DefaultSpec::Null),
        ],
        TypeSpec::Bool,
        "Writes a complete DTD entity declaration."
    ),
    xml_contract!(
        "xmlwriter_output_memory", Xmlwriter, PreludeProvided,
        [param!("writer", Mixed), param!("flush", Bool = DefaultSpec::Bool(true))],
        TypeSpec::Str,
        "Returns the buffered output of a memory writer."
    ),
    xml_contract!(
        "xmlwriter_flush", Xmlwriter, PreludeProvided,
        [param!("writer", Mixed), param!("empty", Bool = DefaultSpec::Bool(true))],
        TypeSpec::Mixed,
        "Flushes the buffer: the buffered string for a memory writer, the byte count written for a URI writer."
    ),
];

/// The `xml_*` and `xmlwriter_*` names, for requirement and support tables.
pub(crate) fn contract_names() -> impl Iterator<Item = &'static str> {
    CONTRACTS.iter().map(|contract| contract.name)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The PHP 8.5 baseline attributes 22 functions to `xml` and 42 to `xmlwriter`.
    #[test]
    fn the_catalog_covers_both_modules() {
        let xml = CONTRACTS
            .iter()
            .filter(|contract| contract.module == PhpModule::Xml)
            .count();
        let xmlwriter = CONTRACTS
            .iter()
            .filter(|contract| contract.module == PhpModule::Xmlwriter)
            .count();
        assert_eq!(xml, 22);
        assert_eq!(xmlwriter, 42);
        assert!(CONTRACTS
            .iter()
            .all(|contract| contract.area == Area::Xml && !contract.internal));
    }

    /// `xml_parse_into_struct` and the nine handler setters are registry builtins; the rest
    /// are prelude declarations.
    #[test]
    fn only_struct_parsing_and_handler_setters_are_registry_builtins() {
        let registry: Vec<_> = CONTRACTS
            .iter()
            .filter(|contract| contract.kind == BuiltinKind::Function)
            .map(|contract| contract.name)
            .collect();
        assert_eq!(
            registry,
            vec![
                "xml_set_element_handler",
                "xml_set_character_data_handler",
                "xml_set_processing_instruction_handler",
                "xml_set_default_handler",
                "xml_set_unparsed_entity_decl_handler",
                "xml_set_notation_decl_handler",
                "xml_set_external_entity_ref_handler",
                "xml_set_start_namespace_decl_handler",
                "xml_set_end_namespace_decl_handler",
                "xml_parse_into_struct",
            ]
        );
        assert!(CONTRACTS
            .iter()
            .filter(|contract| contract.kind == BuiltinKind::PreludeProvided)
            .all(|contract| contract.params.iter().all(|param| !param.by_ref)));
    }
}
