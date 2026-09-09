//! Purpose:
//! The `extern "elephc_xml"` block of the xml prelude: one declaration per `elephc_xml_*`
//! entry point of the bridge's C ABI (`crates/elephc-xml/src/abi.rs`).
//!
//! Called from:
//! - `crate::xml_prelude::build::xml_declarations`.
//!
//! Key details:
//! - TRANSCRIBED from the PHP form kept in `crate::xml_prelude::fragments` by
//!   `synthetic_class::transcribe`; the oracle test compares both node by node.
//! - Declaring these externs is what links `libelephc_xml.a`; strings cross as
//!   NUL-terminated `char*` copies, and every integer travels as a 64-bit register.

use crate::parser::ast::{CType, Stmt};
use crate::synthetic_class::{extern_fn};

/// `elephc_xml_parser_create` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_parser_create() -> Stmt {
    extern_fn("elephc_xml_parser_create", "elephc_xml")
        .param("namespaces", CType::Int)
        .param("separator", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_parser_free` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_parser_free() -> Stmt {
    extern_fn("elephc_xml_parser_free", "elephc_xml")
        .param("parser", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_parser_set_option` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_parser_set_option() -> Stmt {
    extern_fn("elephc_xml_parser_set_option", "elephc_xml")
        .param("parser", CType::Int)
        .param("option", CType::Int)
        .param("value", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_parser_get_option` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_parser_get_option() -> Stmt {
    extern_fn("elephc_xml_parser_get_option", "elephc_xml")
        .param("parser", CType::Int)
        .param("option", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_parser_set_target_encoding` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_parser_set_target_encoding() -> Stmt {
    extern_fn("elephc_xml_parser_set_target_encoding", "elephc_xml")
        .param("parser", CType::Int)
        .param("name", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_encoding_supported` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_encoding_supported() -> Stmt {
    extern_fn("elephc_xml_encoding_supported", "elephc_xml")
        .param("name", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_parser_target_encoding` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_parser_target_encoding() -> Stmt {
    extern_fn("elephc_xml_parser_target_encoding", "elephc_xml")
        .param("parser", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_xml_parser_feed` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_parser_feed() -> Stmt {
    extern_fn("elephc_xml_parser_feed", "elephc_xml")
        .param("parser", CType::Int)
        .param("data", CType::Str)
        .param("len", CType::Int)
        .param("is_final", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_parser_next` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_parser_next() -> Stmt {
    extern_fn("elephc_xml_parser_next", "elephc_xml")
        .param("parser", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_parser_event_string` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_parser_event_string() -> Stmt {
    extern_fn("elephc_xml_parser_event_string", "elephc_xml")
        .param("parser", CType::Int)
        .param("field", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_xml_parser_event_int` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_parser_event_int() -> Stmt {
    extern_fn("elephc_xml_parser_event_int", "elephc_xml")
        .param("parser", CType::Int)
        .param("field", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_parser_event_attr_name` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_parser_event_attr_name() -> Stmt {
    extern_fn("elephc_xml_parser_event_attr_name", "elephc_xml")
        .param("parser", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_xml_parser_event_attr_value` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_parser_event_attr_value() -> Stmt {
    extern_fn("elephc_xml_parser_event_attr_value", "elephc_xml")
        .param("parser", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_xml_parser_event_ns_has_prefix` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_parser_event_ns_has_prefix() -> Stmt {
    extern_fn("elephc_xml_parser_event_ns_has_prefix", "elephc_xml")
        .param("parser", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_parser_event_ns_prefix` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_parser_event_ns_prefix() -> Stmt {
    extern_fn("elephc_xml_parser_event_ns_prefix", "elephc_xml")
        .param("parser", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_xml_parser_event_ns_uri` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_parser_event_ns_uri() -> Stmt {
    extern_fn("elephc_xml_parser_event_ns_uri", "elephc_xml")
        .param("parser", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_xml_parser_error_code` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_parser_error_code() -> Stmt {
    extern_fn("elephc_xml_parser_error_code", "elephc_xml")
        .param("parser", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_parser_well_formed` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_parser_well_formed() -> Stmt {
    extern_fn("elephc_xml_parser_well_formed", "elephc_xml")
        .param("parser", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_parser_line` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_parser_line() -> Stmt {
    extern_fn("elephc_xml_parser_line", "elephc_xml")
        .param("parser", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_parser_column` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_parser_column() -> Stmt {
    extern_fn("elephc_xml_parser_column", "elephc_xml")
        .param("parser", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_parser_byte_index` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_parser_byte_index() -> Stmt {
    extern_fn("elephc_xml_parser_byte_index", "elephc_xml")
        .param("parser", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_parser_stop` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_parser_stop() -> Stmt {
    extern_fn("elephc_xml_parser_stop", "elephc_xml")
        .param("parser", CType::Int)
        .param("code", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_error_string` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_error_string() -> Stmt {
    extern_fn("elephc_xml_error_string", "elephc_xml")
        .param("code", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_xml_writer_create` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_writer_create() -> Stmt {
    extern_fn("elephc_xml_writer_create", "elephc_xml")
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_writer_free` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_writer_free() -> Stmt {
    extern_fn("elephc_xml_writer_free", "elephc_xml")
        .param("writer", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_writer_valid_name` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_writer_valid_name() -> Stmt {
    extern_fn("elephc_xml_writer_valid_name", "elephc_xml")
        .param("name", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_writer_set_indent` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_writer_set_indent() -> Stmt {
    extern_fn("elephc_xml_writer_set_indent", "elephc_xml")
        .param("writer", CType::Int)
        .param("enable", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_writer_set_indent_string` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_writer_set_indent_string() -> Stmt {
    extern_fn("elephc_xml_writer_set_indent_string", "elephc_xml")
        .param("writer", CType::Int)
        .param("indent", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_writer_start_document` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_writer_start_document() -> Stmt {
    extern_fn("elephc_xml_writer_start_document", "elephc_xml")
        .param("writer", CType::Int)
        .param("has_version", CType::Int)
        .param("version", CType::Str)
        .param("has_encoding", CType::Int)
        .param("encoding", CType::Str)
        .param("has_standalone", CType::Int)
        .param("standalone", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_writer_end_document` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_writer_end_document() -> Stmt {
    extern_fn("elephc_xml_writer_end_document", "elephc_xml")
        .param("writer", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_writer_start_comment` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_writer_start_comment() -> Stmt {
    extern_fn("elephc_xml_writer_start_comment", "elephc_xml")
        .param("writer", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_writer_end_comment` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_writer_end_comment() -> Stmt {
    extern_fn("elephc_xml_writer_end_comment", "elephc_xml")
        .param("writer", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_writer_write_comment` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_writer_write_comment() -> Stmt {
    extern_fn("elephc_xml_writer_write_comment", "elephc_xml")
        .param("writer", CType::Int)
        .param("content", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_writer_start_element` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_writer_start_element() -> Stmt {
    extern_fn("elephc_xml_writer_start_element", "elephc_xml")
        .param("writer", CType::Int)
        .param("name", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_writer_start_element_ns` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_writer_start_element_ns() -> Stmt {
    extern_fn("elephc_xml_writer_start_element_ns", "elephc_xml")
        .param("writer", CType::Int)
        .param("has_prefix", CType::Int)
        .param("prefix", CType::Str)
        .param("name", CType::Str)
        .param("has_uri", CType::Int)
        .param("uri", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_writer_end_element` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_writer_end_element() -> Stmt {
    extern_fn("elephc_xml_writer_end_element", "elephc_xml")
        .param("writer", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_writer_full_end_element` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_writer_full_end_element() -> Stmt {
    extern_fn("elephc_xml_writer_full_end_element", "elephc_xml")
        .param("writer", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_writer_write_element` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_writer_write_element() -> Stmt {
    extern_fn("elephc_xml_writer_write_element", "elephc_xml")
        .param("writer", CType::Int)
        .param("name", CType::Str)
        .param("has_content", CType::Int)
        .param("content", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_writer_write_element_ns` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_writer_write_element_ns() -> Stmt {
    extern_fn("elephc_xml_writer_write_element_ns", "elephc_xml")
        .param("writer", CType::Int)
        .param("has_prefix", CType::Int)
        .param("prefix", CType::Str)
        .param("name", CType::Str)
        .param("has_uri", CType::Int)
        .param("uri", CType::Str)
        .param("has_content", CType::Int)
        .param("content", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_writer_start_attribute` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_writer_start_attribute() -> Stmt {
    extern_fn("elephc_xml_writer_start_attribute", "elephc_xml")
        .param("writer", CType::Int)
        .param("name", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_writer_start_attribute_ns` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_writer_start_attribute_ns() -> Stmt {
    extern_fn("elephc_xml_writer_start_attribute_ns", "elephc_xml")
        .param("writer", CType::Int)
        .param("has_prefix", CType::Int)
        .param("prefix", CType::Str)
        .param("name", CType::Str)
        .param("has_uri", CType::Int)
        .param("uri", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_writer_end_attribute` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_writer_end_attribute() -> Stmt {
    extern_fn("elephc_xml_writer_end_attribute", "elephc_xml")
        .param("writer", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_writer_write_attribute` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_writer_write_attribute() -> Stmt {
    extern_fn("elephc_xml_writer_write_attribute", "elephc_xml")
        .param("writer", CType::Int)
        .param("name", CType::Str)
        .param("value", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_writer_write_attribute_ns` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_writer_write_attribute_ns() -> Stmt {
    extern_fn("elephc_xml_writer_write_attribute_ns", "elephc_xml")
        .param("writer", CType::Int)
        .param("has_prefix", CType::Int)
        .param("prefix", CType::Str)
        .param("name", CType::Str)
        .param("has_uri", CType::Int)
        .param("uri", CType::Str)
        .param("value", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_writer_start_pi` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_writer_start_pi() -> Stmt {
    extern_fn("elephc_xml_writer_start_pi", "elephc_xml")
        .param("writer", CType::Int)
        .param("target", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_writer_end_pi` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_writer_end_pi() -> Stmt {
    extern_fn("elephc_xml_writer_end_pi", "elephc_xml")
        .param("writer", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_writer_write_pi` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_writer_write_pi() -> Stmt {
    extern_fn("elephc_xml_writer_write_pi", "elephc_xml")
        .param("writer", CType::Int)
        .param("target", CType::Str)
        .param("content", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_writer_start_cdata` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_writer_start_cdata() -> Stmt {
    extern_fn("elephc_xml_writer_start_cdata", "elephc_xml")
        .param("writer", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_writer_end_cdata` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_writer_end_cdata() -> Stmt {
    extern_fn("elephc_xml_writer_end_cdata", "elephc_xml")
        .param("writer", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_writer_write_cdata` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_writer_write_cdata() -> Stmt {
    extern_fn("elephc_xml_writer_write_cdata", "elephc_xml")
        .param("writer", CType::Int)
        .param("content", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_writer_text` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_writer_text() -> Stmt {
    extern_fn("elephc_xml_writer_text", "elephc_xml")
        .param("writer", CType::Int)
        .param("content", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_writer_write_raw` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_writer_write_raw() -> Stmt {
    extern_fn("elephc_xml_writer_write_raw", "elephc_xml")
        .param("writer", CType::Int)
        .param("content", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_writer_start_dtd` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_writer_start_dtd() -> Stmt {
    extern_fn("elephc_xml_writer_start_dtd", "elephc_xml")
        .param("writer", CType::Int)
        .param("name", CType::Str)
        .param("has_public_id", CType::Int)
        .param("public_id", CType::Str)
        .param("has_system_id", CType::Int)
        .param("system_id", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_writer_end_dtd` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_writer_end_dtd() -> Stmt {
    extern_fn("elephc_xml_writer_end_dtd", "elephc_xml")
        .param("writer", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_writer_write_dtd` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_writer_write_dtd() -> Stmt {
    extern_fn("elephc_xml_writer_write_dtd", "elephc_xml")
        .param("writer", CType::Int)
        .param("name", CType::Str)
        .param("has_public_id", CType::Int)
        .param("public_id", CType::Str)
        .param("has_system_id", CType::Int)
        .param("system_id", CType::Str)
        .param("has_content", CType::Int)
        .param("content", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_writer_start_dtd_element` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_writer_start_dtd_element() -> Stmt {
    extern_fn("elephc_xml_writer_start_dtd_element", "elephc_xml")
        .param("writer", CType::Int)
        .param("name", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_writer_end_dtd_element` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_writer_end_dtd_element() -> Stmt {
    extern_fn("elephc_xml_writer_end_dtd_element", "elephc_xml")
        .param("writer", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_writer_write_dtd_element` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_writer_write_dtd_element() -> Stmt {
    extern_fn("elephc_xml_writer_write_dtd_element", "elephc_xml")
        .param("writer", CType::Int)
        .param("name", CType::Str)
        .param("content", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_writer_start_dtd_attlist` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_writer_start_dtd_attlist() -> Stmt {
    extern_fn("elephc_xml_writer_start_dtd_attlist", "elephc_xml")
        .param("writer", CType::Int)
        .param("name", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_writer_end_dtd_attlist` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_writer_end_dtd_attlist() -> Stmt {
    extern_fn("elephc_xml_writer_end_dtd_attlist", "elephc_xml")
        .param("writer", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_writer_write_dtd_attlist` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_writer_write_dtd_attlist() -> Stmt {
    extern_fn("elephc_xml_writer_write_dtd_attlist", "elephc_xml")
        .param("writer", CType::Int)
        .param("name", CType::Str)
        .param("content", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_writer_start_dtd_entity` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_writer_start_dtd_entity() -> Stmt {
    extern_fn("elephc_xml_writer_start_dtd_entity", "elephc_xml")
        .param("writer", CType::Int)
        .param("name", CType::Str)
        .param("is_param", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_writer_end_dtd_entity` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_writer_end_dtd_entity() -> Stmt {
    extern_fn("elephc_xml_writer_end_dtd_entity", "elephc_xml")
        .param("writer", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_writer_write_dtd_entity` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_writer_write_dtd_entity() -> Stmt {
    extern_fn("elephc_xml_writer_write_dtd_entity", "elephc_xml")
        .param("writer", CType::Int)
        .param("name", CType::Str)
        .param("content", CType::Str)
        .param("is_param", CType::Int)
        .param("flags", CType::Int)
        .param("public_id", CType::Str)
        .param("system_id", CType::Str)
        .param("notation", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_writer_output` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_writer_output() -> Stmt {
    extern_fn("elephc_xml_writer_output", "elephc_xml")
        .param("writer", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_xml_writer_take_output` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_writer_take_output() -> Stmt {
    extern_fn("elephc_xml_writer_take_output", "elephc_xml")
        .param("writer", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_xml_writer_output_len` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_writer_output_len() -> Stmt {
    extern_fn("elephc_xml_writer_output_len", "elephc_xml")
        .param("writer", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_writer_output_has_nul` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_writer_output_has_nul() -> Stmt {
    extern_fn("elephc_xml_writer_output_has_nul", "elephc_xml")
        .param("writer", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_writer_output_hex` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_xml_writer_output_hex() -> Stmt {
    extern_fn("elephc_xml_writer_output_hex", "elephc_xml")
        .param("writer", CType::Int)
        .param("take", CType::Int)
        .returns(CType::Str)
        .build()
}
