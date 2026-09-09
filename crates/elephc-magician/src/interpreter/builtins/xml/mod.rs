//! Purpose:
//! Eval homes for PHP's `ext/xml` (`xml_*`) and `ext/xmlwriter` (`xmlwriter_*`). None of
//! them re-implements the surface: every home forwards to the compiled xml prelude the
//! HOST program registered into the eval context, so `eval()` code runs the very same
//! `XMLParser` / `XMLWriter` declarations as compiled code.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks::{EvalDirectHook, EvalValuesHook}::Xml`.
//! - `crate::interpreter::expressions::calls::eval_call` (source-level interception that
//!   keeps by-reference outputs and named arguments intact).
//! - `crate::interpreter::builtins::registry::dynamic_mutation` (dynamic-callable shapes —
//!   `$f(...)`, first-class callables, `call_user_func_array()` — with their captured
//!   by-reference targets).
//!
//! Key details:
//! - The shared C ABI is reached only through the compiled prelude; this crate declares no
//!   `elephc_xml_*` extern, so an `eval()`-using program that never links the bridge pays
//!   nothing for it, and a fragment that calls a prelude-provided function without the host
//!   having linked it (`--with-xml`, or a compiled use) raises PHP's `Call to undefined
//!   function` Error. The ten registry builtins (the `xml_set_*_handler()` setters and
//!   `xml_parse_into_struct()`) exist on both backends regardless and still run PHP's
//!   `$parser` `TypeError` without the bridge.
//! - Objects created in eval are real host `XMLParser` / `XMLWriter` objects: `new
//!   XMLWriter()` and method calls go through the native-class bridge like any host class.

use super::super::*;

mod dispatch;
mod xml_parser_create;
mod xml_parser_create_ns;
mod xml_set_object;
mod xml_set_element_handler;
mod xml_set_character_data_handler;
mod xml_set_processing_instruction_handler;
mod xml_set_default_handler;
mod xml_set_unparsed_entity_decl_handler;
mod xml_set_notation_decl_handler;
mod xml_set_external_entity_ref_handler;
mod xml_set_start_namespace_decl_handler;
mod xml_set_end_namespace_decl_handler;
mod xml_parse;
mod xml_parse_into_struct;
mod xml_get_error_code;
mod xml_error_string;
mod xml_get_current_line_number;
mod xml_get_current_column_number;
mod xml_get_current_byte_index;
mod xml_parser_free;
mod xml_parser_set_option;
mod xml_parser_get_option;
mod xmlwriter_open_uri;
mod xmlwriter_open_memory;
mod xmlwriter_set_indent;
mod xmlwriter_set_indent_string;
mod xmlwriter_start_comment;
mod xmlwriter_end_comment;
mod xmlwriter_start_attribute;
mod xmlwriter_end_attribute;
mod xmlwriter_write_attribute;
mod xmlwriter_start_attribute_ns;
mod xmlwriter_write_attribute_ns;
mod xmlwriter_start_element;
mod xmlwriter_end_element;
mod xmlwriter_full_end_element;
mod xmlwriter_start_element_ns;
mod xmlwriter_write_element;
mod xmlwriter_write_element_ns;
mod xmlwriter_start_pi;
mod xmlwriter_end_pi;
mod xmlwriter_write_pi;
mod xmlwriter_start_cdata;
mod xmlwriter_end_cdata;
mod xmlwriter_write_cdata;
mod xmlwriter_text;
mod xmlwriter_write_raw;
mod xmlwriter_start_document;
mod xmlwriter_end_document;
mod xmlwriter_write_comment;
mod xmlwriter_start_dtd;
mod xmlwriter_end_dtd;
mod xmlwriter_write_dtd;
mod xmlwriter_start_dtd_element;
mod xmlwriter_end_dtd_element;
mod xmlwriter_write_dtd_element;
mod xmlwriter_start_dtd_attlist;
mod xmlwriter_end_dtd_attlist;
mod xmlwriter_write_dtd_attlist;
mod xmlwriter_start_dtd_entity;
mod xmlwriter_end_dtd_entity;
mod xmlwriter_write_dtd_entity;
mod xmlwriter_output_memory;
mod xmlwriter_flush;

pub(in crate::interpreter) use dispatch::*;
