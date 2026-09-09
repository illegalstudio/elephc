//! Purpose:
//! Builds the xml prelude as AST — the `extern "elephc_xml"` block, `XMLParser`,
//! `XMLWriter`, and every `xml_*` / `xmlwriter_*` procedural function — so the PHP
//! surface compiles through the ordinary class/function pipeline without tokenizing
//! PHP text at injection time.
//!
//! Called from:
//! - `crate::xml_prelude::inject_if_used`, after the hash and curl preludes and before
//!   name resolution.
//!
//! Key details:
//! - TRANSCRIBED, not rewritten: every declaration in the submodules was generated from
//!   the parse of the PHP form in `crate::xml_prelude::fragments` (`synthetic_class::
//!   transcribe`, driven by `ELEPHC_TRANSCRIBE_WHICH=file`), and
//!   `xml_prelude::oracle_tests::built_declarations_match_the_php` compares the built
//!   AST against that parse node by node. Edit the shape here only with that comparison
//!   in hand.
//! - Declaration ORDER follows the PHP source (externs, XMLParser, xml_* functions,
//!   XMLWriter, xmlwriter_* functions) so the oracle can zip the two programs.
//! - One helper per declaration, never one expression for the whole surface: a prelude
//!   built as a single nested builder expression overflows the stack.

use crate::parser::ast::Program;
use crate::synthetic_class::internal_declarations;

mod externs;
mod parser;
mod writer;

/// Builds the whole xml surface, one declaration per helper.
pub(crate) fn xml_declarations() -> Program {
    internal_declarations(|| {
        vec![
            externs::decl_extern_elephc_xml_parser_create(),
            externs::decl_extern_elephc_xml_parser_free(),
            externs::decl_extern_elephc_xml_parser_set_option(),
            externs::decl_extern_elephc_xml_parser_get_option(),
            externs::decl_extern_elephc_xml_parser_set_target_encoding(),
            externs::decl_extern_elephc_xml_encoding_supported(),
            externs::decl_extern_elephc_xml_parser_target_encoding(),
            externs::decl_extern_elephc_xml_parser_feed(),
            externs::decl_extern_elephc_xml_parser_next(),
            externs::decl_extern_elephc_xml_parser_event_string(),
            externs::decl_extern_elephc_xml_parser_event_int(),
            externs::decl_extern_elephc_xml_parser_event_attr_name(),
            externs::decl_extern_elephc_xml_parser_event_attr_value(),
            externs::decl_extern_elephc_xml_parser_event_ns_has_prefix(),
            externs::decl_extern_elephc_xml_parser_event_ns_prefix(),
            externs::decl_extern_elephc_xml_parser_event_ns_uri(),
            externs::decl_extern_elephc_xml_parser_error_code(),
            externs::decl_extern_elephc_xml_parser_well_formed(),
            externs::decl_extern_elephc_xml_parser_line(),
            externs::decl_extern_elephc_xml_parser_column(),
            externs::decl_extern_elephc_xml_parser_byte_index(),
            externs::decl_extern_elephc_xml_parser_stop(),
            externs::decl_extern_elephc_xml_error_string(),
            externs::decl_extern_elephc_xml_writer_create(),
            externs::decl_extern_elephc_xml_writer_free(),
            externs::decl_extern_elephc_xml_writer_valid_name(),
            externs::decl_extern_elephc_xml_writer_set_indent(),
            externs::decl_extern_elephc_xml_writer_set_indent_string(),
            externs::decl_extern_elephc_xml_writer_start_document(),
            externs::decl_extern_elephc_xml_writer_end_document(),
            externs::decl_extern_elephc_xml_writer_start_comment(),
            externs::decl_extern_elephc_xml_writer_end_comment(),
            externs::decl_extern_elephc_xml_writer_write_comment(),
            externs::decl_extern_elephc_xml_writer_start_element(),
            externs::decl_extern_elephc_xml_writer_start_element_ns(),
            externs::decl_extern_elephc_xml_writer_end_element(),
            externs::decl_extern_elephc_xml_writer_full_end_element(),
            externs::decl_extern_elephc_xml_writer_write_element(),
            externs::decl_extern_elephc_xml_writer_write_element_ns(),
            externs::decl_extern_elephc_xml_writer_start_attribute(),
            externs::decl_extern_elephc_xml_writer_start_attribute_ns(),
            externs::decl_extern_elephc_xml_writer_end_attribute(),
            externs::decl_extern_elephc_xml_writer_write_attribute(),
            externs::decl_extern_elephc_xml_writer_write_attribute_ns(),
            externs::decl_extern_elephc_xml_writer_start_pi(),
            externs::decl_extern_elephc_xml_writer_end_pi(),
            externs::decl_extern_elephc_xml_writer_write_pi(),
            externs::decl_extern_elephc_xml_writer_start_cdata(),
            externs::decl_extern_elephc_xml_writer_end_cdata(),
            externs::decl_extern_elephc_xml_writer_write_cdata(),
            externs::decl_extern_elephc_xml_writer_text(),
            externs::decl_extern_elephc_xml_writer_write_raw(),
            externs::decl_extern_elephc_xml_writer_start_dtd(),
            externs::decl_extern_elephc_xml_writer_end_dtd(),
            externs::decl_extern_elephc_xml_writer_write_dtd(),
            externs::decl_extern_elephc_xml_writer_start_dtd_element(),
            externs::decl_extern_elephc_xml_writer_end_dtd_element(),
            externs::decl_extern_elephc_xml_writer_write_dtd_element(),
            externs::decl_extern_elephc_xml_writer_start_dtd_attlist(),
            externs::decl_extern_elephc_xml_writer_end_dtd_attlist(),
            externs::decl_extern_elephc_xml_writer_write_dtd_attlist(),
            externs::decl_extern_elephc_xml_writer_start_dtd_entity(),
            externs::decl_extern_elephc_xml_writer_end_dtd_entity(),
            externs::decl_extern_elephc_xml_writer_write_dtd_entity(),
            externs::decl_extern_elephc_xml_writer_output(),
            externs::decl_extern_elephc_xml_writer_take_output(),
            externs::decl_extern_elephc_xml_writer_output_len(),
            externs::decl_extern_elephc_xml_writer_output_has_nul(),
            externs::decl_extern_elephc_xml_writer_output_hex(),
            parser::decl_class_xmlparser(),
            parser::decl_fn_xml_parser_create(),
            parser::decl_fn_xml_parser_create_ns(),
            parser::decl_fn_xml_set_object(),
            parser::decl_fn_elephc_xml_set_element_handler(),
            parser::decl_fn_elephc_xml_set_character_data_handler(),
            parser::decl_fn_elephc_xml_set_processing_instruction_handler(),
            parser::decl_fn_elephc_xml_set_default_handler(),
            parser::decl_fn_elephc_xml_set_unparsed_entity_decl_handler(),
            parser::decl_fn_elephc_xml_set_notation_decl_handler(),
            parser::decl_fn_elephc_xml_set_external_entity_ref_handler(),
            parser::decl_fn_elephc_xml_set_start_namespace_decl_handler(),
            parser::decl_fn_elephc_xml_set_end_namespace_decl_handler(),
            parser::decl_fn_xml_parse(),
            parser::decl_fn_elephc_xml_parse_into_struct(),
            parser::decl_fn_elephc_xml_struct_values(),
            parser::decl_fn_elephc_xml_struct_index(),
            parser::decl_fn_xml_get_error_code(),
            parser::decl_fn_xml_error_string(),
            parser::decl_fn_xml_get_current_line_number(),
            parser::decl_fn_xml_get_current_column_number(),
            parser::decl_fn_xml_get_current_byte_index(),
            parser::decl_fn_xml_parser_free(),
            parser::decl_fn_xml_parser_set_option(),
            parser::decl_fn_xml_parser_get_option(),
            writer::decl_class_xmlwriter(),
            writer::decl_fn_xmlwriter_open_uri(),
            writer::decl_fn_xmlwriter_open_memory(),
            writer::decl_fn_xmlwriter_set_indent(),
            writer::decl_fn_xmlwriter_set_indent_string(),
            writer::decl_fn_xmlwriter_start_comment(),
            writer::decl_fn_xmlwriter_end_comment(),
            writer::decl_fn_xmlwriter_start_attribute(),
            writer::decl_fn_xmlwriter_end_attribute(),
            writer::decl_fn_xmlwriter_write_attribute(),
            writer::decl_fn_xmlwriter_start_attribute_ns(),
            writer::decl_fn_xmlwriter_write_attribute_ns(),
            writer::decl_fn_xmlwriter_start_element(),
            writer::decl_fn_xmlwriter_end_element(),
            writer::decl_fn_xmlwriter_full_end_element(),
            writer::decl_fn_xmlwriter_start_element_ns(),
            writer::decl_fn_xmlwriter_write_element(),
            writer::decl_fn_xmlwriter_write_element_ns(),
            writer::decl_fn_xmlwriter_start_pi(),
            writer::decl_fn_xmlwriter_end_pi(),
            writer::decl_fn_xmlwriter_write_pi(),
            writer::decl_fn_xmlwriter_start_cdata(),
            writer::decl_fn_xmlwriter_end_cdata(),
            writer::decl_fn_xmlwriter_write_cdata(),
            writer::decl_fn_xmlwriter_text(),
            writer::decl_fn_xmlwriter_write_raw(),
            writer::decl_fn_xmlwriter_start_document(),
            writer::decl_fn_xmlwriter_end_document(),
            writer::decl_fn_xmlwriter_write_comment(),
            writer::decl_fn_xmlwriter_start_dtd(),
            writer::decl_fn_xmlwriter_end_dtd(),
            writer::decl_fn_xmlwriter_write_dtd(),
            writer::decl_fn_xmlwriter_start_dtd_element(),
            writer::decl_fn_xmlwriter_end_dtd_element(),
            writer::decl_fn_xmlwriter_write_dtd_element(),
            writer::decl_fn_xmlwriter_start_dtd_attlist(),
            writer::decl_fn_xmlwriter_end_dtd_attlist(),
            writer::decl_fn_xmlwriter_write_dtd_attlist(),
            writer::decl_fn_xmlwriter_start_dtd_entity(),
            writer::decl_fn_xmlwriter_end_dtd_entity(),
            writer::decl_fn_xmlwriter_write_dtd_entity(),
            writer::decl_fn_xmlwriter_output_memory(),
            writer::decl_fn_xmlwriter_flush(),
        ]
    })
}
