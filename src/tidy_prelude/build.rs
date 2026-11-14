//! Purpose:
//! Builds the tidy prelude as AST — the `extern "elephc_tidy"` block, the `tidy` and
//! `tidyNode` classes, the 24 `tidy_*` functions, and `ob_tidyhandler` — so the PHP
//! surface compiles through the ordinary class/function pipeline without tokenizing PHP
//! text at injection time.
//!
//! Called from:
//! - `crate::tidy_prelude::inject_if_used`, after the xml prelude and before name
//!   resolution.
//!
//! Key details:
//! - TRANSCRIBED, not rewritten: every declaration below was generated from the parse of
//!   the PHP form in `crate::tidy_prelude::fragments` by `synthetic_class::transcribe`, and
//!   `tidy_prelude::oracle_tests::built_declarations_match_the_php` compares the built AST
//!   against that parse node by node. Edit the shape here only with that comparison in hand.
//! - One helper per declaration, never one expression for the whole surface: a prelude
//!   built as a single nested builder expression overflows the stack.
//! - Declaration order follows the PHP source (externs, internal helpers, classes,
//!   public functions, output callback) so the oracle can zip the two programs.

mod classes;
mod externs;
mod functions;
mod helpers;

use crate::parser::ast::Program;
use crate::synthetic_class::internal_declarations;

/// Builds the whole tidy surface, one declaration per helper.
pub(crate) fn tidy_declarations() -> Program {
    let mut declarations = internal_declarations(|| {
        vec![
            externs::decl_extern_elephc_tidy_create(),
            externs::decl_extern_elephc_tidy_release(),
            externs::decl_extern_elephc_tidy_set_encoding(),
            externs::decl_extern_elephc_tidy_load_config(),
            externs::decl_extern_elephc_tidy_parse(),
            externs::decl_extern_elephc_tidy_clean_repair(),
            externs::decl_extern_elephc_tidy_diagnose(),
            externs::decl_extern_elephc_tidy_initialized(),
            externs::decl_extern_elephc_tidy_output(),
            externs::decl_extern_elephc_tidy_output_present(),
            externs::decl_extern_elephc_tidy_error_buffer(),
            externs::decl_extern_elephc_tidy_error_buffer_present(),
            externs::decl_extern_elephc_tidy_status(),
            externs::decl_extern_elephc_tidy_html_version(),
            externs::decl_extern_elephc_tidy_is_xhtml(),
            externs::decl_extern_elephc_tidy_is_xml(),
            externs::decl_extern_elephc_tidy_error_count(),
            externs::decl_extern_elephc_tidy_warning_count(),
            externs::decl_extern_elephc_tidy_access_count(),
            externs::decl_extern_elephc_tidy_config_count(),
            externs::decl_extern_elephc_tidy_release_date(),
            externs::decl_extern_elephc_tidy_option_count(),
            externs::decl_extern_elephc_tidy_option_index(),
            externs::decl_extern_elephc_tidy_option_name(),
            externs::decl_extern_elephc_tidy_option_type(),
            externs::decl_extern_elephc_tidy_option_internal(),
            externs::decl_extern_elephc_tidy_option_value_str(),
            externs::decl_extern_elephc_tidy_option_value_int(),
            externs::decl_extern_elephc_tidy_option_value_bool(),
            externs::decl_extern_elephc_tidy_option_doc(),
            externs::decl_extern_elephc_tidy_option_doc_present(),
            externs::decl_extern_elephc_tidy_set_option_str(),
            externs::decl_extern_elephc_tidy_set_option_int(),
            externs::decl_extern_elephc_tidy_set_option_bool(),
            externs::decl_extern_elephc_tidy_root(),
            externs::decl_extern_elephc_tidy_html(),
            externs::decl_extern_elephc_tidy_head(),
            externs::decl_extern_elephc_tidy_body(),
            externs::decl_extern_elephc_tidy_node_release(),
            externs::decl_extern_elephc_tidy_node_type(),
            externs::decl_extern_elephc_tidy_node_name(),
            externs::decl_extern_elephc_tidy_node_text(),
            externs::decl_extern_elephc_tidy_node_line(),
            externs::decl_extern_elephc_tidy_node_column(),
            externs::decl_extern_elephc_tidy_node_proprietary(),
            externs::decl_extern_elephc_tidy_node_id(),
            externs::decl_extern_elephc_tidy_node_has_children(),
            externs::decl_extern_elephc_tidy_node_has_siblings(),
            externs::decl_extern_elephc_tidy_node_is_comment(),
            externs::decl_extern_elephc_tidy_node_is_html(),
            externs::decl_extern_elephc_tidy_node_is_text(),
            externs::decl_extern_elephc_tidy_node_is_jste(),
            externs::decl_extern_elephc_tidy_node_is_asp(),
            externs::decl_extern_elephc_tidy_node_is_php(),
            externs::decl_extern_elephc_tidy_node_parent(),
            externs::decl_extern_elephc_tidy_node_previous(),
            externs::decl_extern_elephc_tidy_node_next(),
            externs::decl_extern_elephc_tidy_node_child_count(),
            externs::decl_extern_elephc_tidy_node_child(),
            externs::decl_extern_elephc_tidy_node_attr_count(),
            externs::decl_extern_elephc_tidy_node_attr_name(),
            externs::decl_extern_elephc_tidy_node_attr_value(),
            helpers::decl_fn_elephc_tidy_raise(),
            helpers::decl_fn_elephc_tidy_numeric_prefix(),
            helpers::decl_fn_elephc_tidy_config_argument(),
            helpers::decl_fn_elephc_tidy_encoding_argument(),
            helpers::decl_fn_elephc_tidy_open_basedir_message(),
            helpers::decl_fn_elephc_tidy_load_config_file(),
            helpers::decl_fn_elephc_tidy_caller_label(),
            helpers::decl_fn_elephc_tidy_load_default_config(),
            helpers::decl_fn_elephc_tidy_new_doc(),
            helpers::decl_fn_elephc_tidy_apply_config(),
            helpers::decl_fn_elephc_tidy_set_encoding(),
            helpers::decl_fn_elephc_tidy_parse_buffer(),
            helpers::decl_fn_elephc_tidy_parse_into(),
            helpers::decl_fn_elephc_tidy_repair_parse_into(),
            helpers::decl_fn_elephc_tidy_read_file(),
            helpers::decl_fn_elephc_tidy_sync_properties(),
            helpers::decl_fn_elephc_tidy_get_option(),
            helpers::decl_fn_elephc_tidy_get_option_doc(),
            helpers::decl_fn_elephc_tidy_type_name(),
            helpers::decl_fn_elephc_tidy_document(),
            classes::decl_class_tidy(),
            classes::decl_class_tidynode(),
            functions::decl_fn_tidy_parse_string(),
            functions::decl_fn_tidy_get_error_buffer(),
            functions::decl_fn_tidy_get_output(),
            functions::decl_fn_tidy_parse_file(),
            functions::decl_fn_tidy_clean_repair(),
            helpers::decl_fn_elephc_tidy_repair_string(),
            functions::decl_fn_tidy_repair_string(),
            helpers::decl_fn_elephc_tidy_repair_file(),
            functions::decl_fn_tidy_repair_file(),
            functions::decl_fn_tidy_diagnose(),
            functions::decl_fn_tidy_get_release(),
            functions::decl_fn_tidy_get_opt_doc(),
            functions::decl_fn_tidy_get_config(),
            functions::decl_fn_tidy_get_status(),
            functions::decl_fn_tidy_get_html_ver(),
            functions::decl_fn_tidy_is_xhtml(),
            functions::decl_fn_tidy_is_xml(),
            functions::decl_fn_tidy_error_count(),
            functions::decl_fn_tidy_warning_count(),
            functions::decl_fn_tidy_access_count(),
            functions::decl_fn_tidy_config_count(),
            functions::decl_fn_tidy_getopt(),
            functions::decl_fn_tidy_get_root(),
            functions::decl_fn_tidy_get_html(),
            functions::decl_fn_tidy_get_head(),
            functions::decl_fn_tidy_get_body(),
            functions::decl_fn_ob_tidyhandler(),
        ]
    });
    // The builders create declarations with `Span::dummy()`. Type resolution reads
    // `span.line != 0` to distinguish a PHP-surface `array` (packed-or-hash) from an
    // internal raw packed-array ABI, so every tidy declaration must carry a synthetic
    // non-zero span before checking. The oracle strips spans and is unaffected.
    for stmt in &mut declarations {
        let span = crate::span::Span::synthetic();
        match &mut stmt.kind {
            crate::parser::ast::StmtKind::FunctionDecl { .. }
            | crate::parser::ast::StmtKind::ExternFunctionDecl { .. } => {
                stmt.span = span;
            }
            crate::parser::ast::StmtKind::ClassDecl {
                properties,
                methods,
                constants,
                ..
            } => {
                stmt.span = span;
                for property in properties {
                    property.span = span;
                }
                for method in methods {
                    method.span = span;
                }
                for constant in constants {
                    constant.span = span;
                }
            }
            _ => {}
        }
    }
    declarations
}
