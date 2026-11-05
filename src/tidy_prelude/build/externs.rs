//! Purpose:
//! The `extern "elephc_tidy"` block of the tidy prelude: one declaration per
//! `elephc_tidy_*` entry point of the bridge's C ABI.
//!
//! Called from:
//! - `crate::tidy_prelude::build::tidy_declarations`.
//!
//! Key details:
//! - TRANSCRIBED from the PHP form kept in `crate::tidy_prelude::fragments` by
//!   `synthetic_class::transcribe`; the oracle test compares both node by node.
//! - Declaring these externs is what links `libelephc_tidy.a`; strings cross as
//!   NUL-terminated copies and every integer travels as a 64-bit register.

use crate::parser::ast::*;
use crate::synthetic_class::*;

/// `elephc_tidy_create` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_create() -> Stmt {
    extern_fn("elephc_tidy_create", "elephc_tidy")
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_release` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_release() -> Stmt {
    extern_fn("elephc_tidy_release", "elephc_tidy")
        .param("doc", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_set_encoding` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_set_encoding() -> Stmt {
    extern_fn("elephc_tidy_set_encoding", "elephc_tidy")
        .param("doc", CType::Int)
        .param("encoding", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_load_config` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_load_config() -> Stmt {
    extern_fn("elephc_tidy_load_config", "elephc_tidy")
        .param("doc", CType::Int)
        .param("path", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_parse` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_parse() -> Stmt {
    extern_fn("elephc_tidy_parse", "elephc_tidy")
        .param("doc", CType::Int)
        .param("data", CType::Str)
        .param("len", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_clean_repair` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_clean_repair() -> Stmt {
    extern_fn("elephc_tidy_clean_repair", "elephc_tidy")
        .param("doc", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_diagnose` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_diagnose() -> Stmt {
    extern_fn("elephc_tidy_diagnose", "elephc_tidy")
        .param("doc", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_initialized` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_initialized() -> Stmt {
    extern_fn("elephc_tidy_initialized", "elephc_tidy")
        .param("doc", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_output` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_output() -> Stmt {
    extern_fn("elephc_tidy_output", "elephc_tidy")
        .param("doc", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_tidy_output_present` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_output_present() -> Stmt {
    extern_fn("elephc_tidy_output_present", "elephc_tidy")
        .param("doc", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_error_buffer` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_error_buffer() -> Stmt {
    extern_fn("elephc_tidy_error_buffer", "elephc_tidy")
        .param("doc", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_tidy_error_buffer_present` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_error_buffer_present() -> Stmt {
    extern_fn("elephc_tidy_error_buffer_present", "elephc_tidy")
        .param("doc", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_status` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_status() -> Stmt {
    extern_fn("elephc_tidy_status", "elephc_tidy")
        .param("doc", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_html_version` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_html_version() -> Stmt {
    extern_fn("elephc_tidy_html_version", "elephc_tidy")
        .param("doc", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_is_xhtml` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_is_xhtml() -> Stmt {
    extern_fn("elephc_tidy_is_xhtml", "elephc_tidy")
        .param("doc", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_is_xml` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_is_xml() -> Stmt {
    extern_fn("elephc_tidy_is_xml", "elephc_tidy")
        .param("doc", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_error_count` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_error_count() -> Stmt {
    extern_fn("elephc_tidy_error_count", "elephc_tidy")
        .param("doc", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_warning_count` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_warning_count() -> Stmt {
    extern_fn("elephc_tidy_warning_count", "elephc_tidy")
        .param("doc", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_access_count` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_access_count() -> Stmt {
    extern_fn("elephc_tidy_access_count", "elephc_tidy")
        .param("doc", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_config_count` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_config_count() -> Stmt {
    extern_fn("elephc_tidy_config_count", "elephc_tidy")
        .param("doc", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_release_date` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_release_date() -> Stmt {
    extern_fn("elephc_tidy_release_date", "elephc_tidy")
        .returns(CType::Str)
        .build()
}

/// `elephc_tidy_option_count` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_option_count() -> Stmt {
    extern_fn("elephc_tidy_option_count", "elephc_tidy")
        .param("doc", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_option_index` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_option_index() -> Stmt {
    extern_fn("elephc_tidy_option_index", "elephc_tidy")
        .param("doc", CType::Int)
        .param("name", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_option_name` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_option_name() -> Stmt {
    extern_fn("elephc_tidy_option_name", "elephc_tidy")
        .param("doc", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_tidy_option_type` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_option_type() -> Stmt {
    extern_fn("elephc_tidy_option_type", "elephc_tidy")
        .param("doc", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_option_internal` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_option_internal() -> Stmt {
    extern_fn("elephc_tidy_option_internal", "elephc_tidy")
        .param("doc", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_option_value_str` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_option_value_str() -> Stmt {
    extern_fn("elephc_tidy_option_value_str", "elephc_tidy")
        .param("doc", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_tidy_option_value_int` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_option_value_int() -> Stmt {
    extern_fn("elephc_tidy_option_value_int", "elephc_tidy")
        .param("doc", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_option_value_bool` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_option_value_bool() -> Stmt {
    extern_fn("elephc_tidy_option_value_bool", "elephc_tidy")
        .param("doc", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_option_doc` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_option_doc() -> Stmt {
    extern_fn("elephc_tidy_option_doc", "elephc_tidy")
        .param("doc", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_tidy_option_doc_present` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_option_doc_present() -> Stmt {
    extern_fn("elephc_tidy_option_doc_present", "elephc_tidy")
        .param("doc", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_set_option_str` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_set_option_str() -> Stmt {
    extern_fn("elephc_tidy_set_option_str", "elephc_tidy")
        .param("doc", CType::Int)
        .param("name", CType::Str)
        .param("value", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_set_option_int` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_set_option_int() -> Stmt {
    extern_fn("elephc_tidy_set_option_int", "elephc_tidy")
        .param("doc", CType::Int)
        .param("name", CType::Str)
        .param("value", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_set_option_bool` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_set_option_bool() -> Stmt {
    extern_fn("elephc_tidy_set_option_bool", "elephc_tidy")
        .param("doc", CType::Int)
        .param("name", CType::Str)
        .param("value", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_root` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_root() -> Stmt {
    extern_fn("elephc_tidy_root", "elephc_tidy")
        .param("doc", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_html` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_html() -> Stmt {
    extern_fn("elephc_tidy_html", "elephc_tidy")
        .param("doc", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_head` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_head() -> Stmt {
    extern_fn("elephc_tidy_head", "elephc_tidy")
        .param("doc", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_body` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_body() -> Stmt {
    extern_fn("elephc_tidy_body", "elephc_tidy")
        .param("doc", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_node_release` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_node_release() -> Stmt {
    extern_fn("elephc_tidy_node_release", "elephc_tidy")
        .param("node", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_node_type` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_node_type() -> Stmt {
    extern_fn("elephc_tidy_node_type", "elephc_tidy")
        .param("node", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_node_name` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_node_name() -> Stmt {
    extern_fn("elephc_tidy_node_name", "elephc_tidy")
        .param("node", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_tidy_node_text` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_node_text() -> Stmt {
    extern_fn("elephc_tidy_node_text", "elephc_tidy")
        .param("node", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_tidy_node_line` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_node_line() -> Stmt {
    extern_fn("elephc_tidy_node_line", "elephc_tidy")
        .param("node", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_node_column` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_node_column() -> Stmt {
    extern_fn("elephc_tidy_node_column", "elephc_tidy")
        .param("node", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_node_proprietary` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_node_proprietary() -> Stmt {
    extern_fn("elephc_tidy_node_proprietary", "elephc_tidy")
        .param("node", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_node_id` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_node_id() -> Stmt {
    extern_fn("elephc_tidy_node_id", "elephc_tidy")
        .param("node", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_node_has_children` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_node_has_children() -> Stmt {
    extern_fn("elephc_tidy_node_has_children", "elephc_tidy")
        .param("node", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_node_has_siblings` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_node_has_siblings() -> Stmt {
    extern_fn("elephc_tidy_node_has_siblings", "elephc_tidy")
        .param("node", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_node_is_comment` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_node_is_comment() -> Stmt {
    extern_fn("elephc_tidy_node_is_comment", "elephc_tidy")
        .param("node", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_node_is_html` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_node_is_html() -> Stmt {
    extern_fn("elephc_tidy_node_is_html", "elephc_tidy")
        .param("node", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_node_is_text` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_node_is_text() -> Stmt {
    extern_fn("elephc_tidy_node_is_text", "elephc_tidy")
        .param("node", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_node_is_jste` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_node_is_jste() -> Stmt {
    extern_fn("elephc_tidy_node_is_jste", "elephc_tidy")
        .param("node", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_node_is_asp` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_node_is_asp() -> Stmt {
    extern_fn("elephc_tidy_node_is_asp", "elephc_tidy")
        .param("node", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_node_is_php` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_node_is_php() -> Stmt {
    extern_fn("elephc_tidy_node_is_php", "elephc_tidy")
        .param("node", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_node_parent` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_node_parent() -> Stmt {
    extern_fn("elephc_tidy_node_parent", "elephc_tidy")
        .param("node", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_node_previous` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_node_previous() -> Stmt {
    extern_fn("elephc_tidy_node_previous", "elephc_tidy")
        .param("node", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_node_next` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_node_next() -> Stmt {
    extern_fn("elephc_tidy_node_next", "elephc_tidy")
        .param("node", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_node_child_count` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_node_child_count() -> Stmt {
    extern_fn("elephc_tidy_node_child_count", "elephc_tidy")
        .param("node", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_node_child` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_node_child() -> Stmt {
    extern_fn("elephc_tidy_node_child", "elephc_tidy")
        .param("node", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_node_attr_count` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_node_attr_count() -> Stmt {
    extern_fn("elephc_tidy_node_attr_count", "elephc_tidy")
        .param("node", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_tidy_node_attr_name` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_node_attr_name() -> Stmt {
    extern_fn("elephc_tidy_node_attr_name", "elephc_tidy")
        .param("node", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_tidy_node_attr_value` — transcribed from the PHP form.
pub(super) fn decl_extern_elephc_tidy_node_attr_value() -> Stmt {
    extern_fn("elephc_tidy_node_attr_value", "elephc_tidy")
        .param("node", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Str)
        .build()
}
