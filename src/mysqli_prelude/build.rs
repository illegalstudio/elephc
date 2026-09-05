//! Purpose:
//! Builds the mysqli standard-library surface as AST — the `MYSQLI_*` constants,
//! `mysqli_sql_exception`, `mysqli`, `mysqli_result` (with its iterator),
//! `mysqli_stmt`, and the `mysqli_*` procedural aliases — for one PHP
//! compatibility version. It replaces the PHP source the prelude used to
//! tokenize and parse on every mysqli compile.
//!
//! Called from:
//! - `crate::mysqli_prelude::inject_if_used`, after the PDO prelude injection
//!   and before name resolution.
//!
//! Key details:
//! - TRANSCRIBED, not rewritten: every declaration in the submodules was generated
//!   from the parse of the PHP it replaces (`synthetic_class::transcribe`, driven by
//!   `ELEPHC_TRANSCRIBE_WHICH=mysqli`), and the oracle
//!   `mysqli_prelude::oracle_tests::built_declarations_match_the_php_for_every_version`
//!   compares the built AST against that parse node by node for every
//!   `PhpVersion`. Edit the shape here only with that comparison in hand.
//! - EVERY VARIATION IS EXPRESSED HERE. `fragments::source_for_version` used to
//!   gate the PHP text with marker-delimited block removals and a literal rewrite;
//!   those are now three conditionals: `mysqli::$reportMode` defaults to `OFF`
//!   (0) under 8.0 and `ERROR|STRICT` (3) from 8.1, `fetch_column` (method and
//!   alias) exists from 8.1, `execute_query` (method and alias) from 8.2.
//! - Declaration ORDER follows the PHP fragment concatenation (constants,
//!   exception, connection, result, statement, procedural) so the oracle can zip
//!   the two programs; nothing depends on it at compile time because the prelude
//!   carries only hoisted declarations.
//! - One helper per declaration, never one expression for the whole surface: a
//!   prelude built as a single nested builder expression overflows the stack.

use crate::parser::ast::Program;
use crate::php_version::PhpVersion;
use crate::synthetic_class::internal_declarations;

mod connection;
mod constants;
mod exception;
mod procedural;
mod result;
mod statement;

/// Builds the whole mysqli surface for `php_version`, one declaration per helper.
pub(crate) fn mysqli_declarations(php_version: PhpVersion) -> Program {
    internal_declarations(move || {
        let mut declarations = Vec::new();
        declarations.extend([
            constants::decl_const_mysqli_assoc(),
            constants::decl_const_mysqli_num(),
            constants::decl_const_mysqli_both(),
            constants::decl_const_mysqli_store_result(),
            constants::decl_const_mysqli_use_result(),
            constants::decl_const_mysqli_report_off(),
            constants::decl_const_mysqli_report_error(),
            constants::decl_const_mysqli_report_strict(),
            constants::decl_const_mysqli_report_index(),
            constants::decl_const_mysqli_report_all(),
            constants::decl_const_mysqli_client_compress(),
            constants::decl_const_mysqli_client_found_rows(),
            constants::decl_const_mysqli_client_ignore_space(),
            constants::decl_const_mysqli_client_interactive(),
            constants::decl_const_mysqli_client_ssl(),
            constants::decl_const_mysqli_opt_connect_timeout(),
            constants::decl_const_mysqli_init_command(),
            constants::decl_const_mysqli_set_charset_name(),
            constants::decl_const_mysqli_trans_start_with_consistent_snapshot(),
            constants::decl_const_mysqli_trans_start_read_write(),
            constants::decl_const_mysqli_trans_start_read_only(),
            constants::decl_const_mysqli_trans_cor_and_chain(),
            constants::decl_const_mysqli_trans_cor_and_no_chain(),
            constants::decl_const_mysqli_trans_cor_release(),
            constants::decl_const_mysqli_trans_cor_no_release(),
            constants::decl_const_mysqli_type_decimal(),
            constants::decl_const_mysqli_type_tiny(),
            constants::decl_const_mysqli_type_short(),
            constants::decl_const_mysqli_type_long(),
            constants::decl_const_mysqli_type_float(),
            constants::decl_const_mysqli_type_double(),
            constants::decl_const_mysqli_type_null(),
            constants::decl_const_mysqli_type_timestamp(),
            constants::decl_const_mysqli_type_longlong(),
            constants::decl_const_mysqli_type_int24(),
            constants::decl_const_mysqli_type_date(),
            constants::decl_const_mysqli_type_time(),
            constants::decl_const_mysqli_type_datetime(),
            constants::decl_const_mysqli_type_year(),
            constants::decl_const_mysqli_type_newdate(),
            constants::decl_const_mysqli_type_varchar(),
            constants::decl_const_mysqli_type_bit(),
            constants::decl_const_mysqli_type_json(),
            constants::decl_const_mysqli_type_newdecimal(),
            constants::decl_const_mysqli_type_enum(),
            constants::decl_const_mysqli_type_set(),
            constants::decl_const_mysqli_type_tiny_blob(),
            constants::decl_const_mysqli_type_medium_blob(),
            constants::decl_const_mysqli_type_long_blob(),
            constants::decl_const_mysqli_type_blob(),
            constants::decl_const_mysqli_type_var_string(),
            constants::decl_const_mysqli_type_string(),
            constants::decl_const_mysqli_type_geometry(),
            exception::decl_class_mysqli_sql_exception(),
            exception::decl_fn_mysqli_report(),
            connection::decl_class_mysqli(php_version),
            result::decl_class_mysqli_result(php_version),
            result::decl_class_elephcmysqliresultiterator(),
            statement::decl_class_mysqli_stmt(),
            procedural::decl_fn_mysqli_connect(),
            procedural::decl_fn_mysqli_init(),
            procedural::decl_fn_mysqli_real_connect(),
            procedural::decl_fn_mysqli_close(),
            procedural::decl_fn_mysqli_ping(),
            procedural::decl_fn_mysqli_select_db(),
            procedural::decl_fn_mysqli_set_charset(),
            procedural::decl_fn_mysqli_character_set_name(),
            procedural::decl_fn_mysqli_real_escape_string(),
            procedural::decl_fn_mysqli_escape_string(),
            procedural::decl_fn_mysqli_begin_transaction(),
            procedural::decl_fn_mysqli_commit(),
            procedural::decl_fn_mysqli_rollback(),
            procedural::decl_fn_mysqli_savepoint(),
            procedural::decl_fn_mysqli_release_savepoint(),
            procedural::decl_fn_mysqli_autocommit(),
            procedural::decl_fn_mysqli_options(),
            procedural::decl_fn_mysqli_set_opt(),
            procedural::decl_fn_mysqli_get_server_info(),
            procedural::decl_fn_mysqli_get_client_info(),
            procedural::decl_fn_mysqli_get_host_info(),
            procedural::decl_fn_mysqli_get_proto_info(),
            procedural::decl_fn_mysqli_get_server_version(),
            procedural::decl_fn_mysqli_get_client_version(),
            procedural::decl_fn_mysqli_stat(),
            procedural::decl_fn_mysqli_thread_id(),
            procedural::decl_fn_mysqli_connect_errno(),
            procedural::decl_fn_mysqli_connect_error(),
            procedural::decl_fn_mysqli_errno(),
            procedural::decl_fn_mysqli_error(),
            procedural::decl_fn_mysqli_error_list(),
            procedural::decl_fn_mysqli_sqlstate(),
            procedural::decl_fn_mysqli_affected_rows(),
            procedural::decl_fn_mysqli_insert_id(),
            procedural::decl_fn_mysqli_field_count(),
            procedural::decl_fn_mysqli_warning_count(),
            procedural::decl_fn_mysqli_info(),
            procedural::decl_fn_mysqli_query(),
            procedural::decl_fn_mysqli_real_query(),
            procedural::decl_fn_mysqli_multi_query(),
            procedural::decl_fn_mysqli_more_results(),
            procedural::decl_fn_mysqli_next_result(),
            procedural::decl_fn_mysqli_store_result(),
            procedural::decl_fn_mysqli_use_result(),
            procedural::decl_fn_mysqli_fetch_assoc(),
            procedural::decl_fn_mysqli_fetch_row(),
            procedural::decl_fn_mysqli_fetch_array(),
            procedural::decl_fn_mysqli_fetch_object(),
            procedural::decl_fn_mysqli_fetch_all(),
        ]);
        // `mysqli_fetch_column` arrived with `mysqli_result::fetch_column` in PHP 8.1;
        // under 8.0 the alias is ABSENT (a `function_exists` probe must say so).
        if php_version >= PhpVersion::Php81 {
            declarations.push(procedural::decl_fn_mysqli_fetch_column());
        }
        declarations.extend([
            procedural::decl_fn_mysqli_num_rows(),
            procedural::decl_fn_mysqli_num_fields(),
            procedural::decl_fn_mysqli_data_seek(),
            procedural::decl_fn_mysqli_fetch_field(),
            procedural::decl_fn_mysqli_fetch_fields(),
            procedural::decl_fn_mysqli_fetch_field_direct(),
            procedural::decl_fn_mysqli_free_result(),
            procedural::decl_fn_mysqli_prepare(),
        ]);
        // `mysqli_execute_query` is PHP 8.2+, like the method it forwards to.
        if php_version >= PhpVersion::Php82 {
            declarations.push(procedural::decl_fn_mysqli_execute_query());
        }
        declarations.extend([
            procedural::decl_fn_mysqli_stmt_bind_param(),
            procedural::decl_fn_mysqli_stmt_execute(),
            procedural::decl_fn_mysqli_stmt_get_result(),
            procedural::decl_fn_mysqli_stmt_store_result(),
            procedural::decl_fn_mysqli_stmt_reset(),
            procedural::decl_fn_mysqli_stmt_close(),
            procedural::decl_fn_mysqli_stmt_affected_rows(),
            procedural::decl_fn_mysqli_stmt_errno(),
            procedural::decl_fn_mysqli_stmt_error(),
            procedural::decl_fn_mysqli_stmt_num_rows(),
            procedural::decl_fn_mysqli_stmt_param_count(),
            procedural::decl_fn_mysqli_stmt_sqlstate(),
            procedural::decl_fn_mysqli_stmt_field_count(),
            procedural::decl_fn_mysqli_stmt_insert_id(),
            procedural::decl_fn_mysqli_stmt_error_list(),
            procedural::decl_fn_mysqli_stmt_free_result(),
            procedural::decl_fn_mysqli_stmt_prepare(),
            procedural::decl_fn_mysqli_execute(),
            procedural::decl_fn_mysqli_stmt_init(),
            procedural::decl_fn_mysqli_fetch_lengths(),
            procedural::decl_fn_mysqli_field_seek(),
            procedural::decl_fn_mysqli_field_tell(),
            procedural::decl_fn_mysqli_get_charset(),
            procedural::decl_fn_mysqli_thread_safe(),
        ]);
        declarations
    })
}
