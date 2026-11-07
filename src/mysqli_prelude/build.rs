//! Purpose:
//! Builds the mysqli standard-library surface as AST — the `MYSQLI_*` constants,
//! `mysqli_sql_exception`, `mysqli`, `mysqli_result` (with its iterator),
//! `mysqli_stmt`, `mysqli_driver`, `mysqli_warning`, and the `mysqli_*`
//! procedural aliases — for one PHP
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
//! - EVERY VARIATION IS EXPRESSED HERE. `fragments::source_for_version` gates the
//!   PHP text with marker-delimited block removals and two literal rewrites; their
//!   twins are conditionals: `mysqli::$reportMode` defaults to `OFF` (0) under 8.0
//!   and `ERROR|STRICT` (3) from 8.1; `fetch_column` (method and alias) and three
//!   constants exist from 8.1; `execute_query` (method and alias) from 8.2;
//!   PHP 8.4 swaps five legacy constants (and `mysqli_stmt`'s legacy attribute
//!   flag) for `MYSQLI_TYPE_VECTOR`; `quote_string` (method and alias) and
//!   `MYSQLI_OPT_COMPRESS` exist from 8.6.
//! - Declaration ORDER follows the PHP fragment concatenation (constants,
//!   exception, connection, result, statement, driver, procedural) so the oracle can zip
//!   the two programs; nothing depends on it at compile time because the prelude
//!   carries only hoisted declarations.
//! - One helper per declaration, never one expression for the whole surface: a
//!   prelude built as a single nested builder expression overflows the stack.

use crate::parser::ast::Program;
use crate::php_version::PhpVersion;
use crate::synthetic_class::internal_declarations;

mod connection;
mod constants;
mod driver;
mod exception;
mod procedural;
mod result;
mod statement;

/// Builds the whole mysqli surface for `php_version`, one declaration per helper.
pub(crate) fn mysqli_declarations(php_version: PhpVersion) -> Program {
    internal_declarations(move || {
        let mut declarations = Vec::new();
        declarations.extend([
            constants::decl_const_mysqli_read_default_group(),
            constants::decl_const_mysqli_read_default_file(),
            constants::decl_const_mysqli_opt_connect_timeout(),
            constants::decl_const_mysqli_opt_local_infile(),
            constants::decl_const_mysqli_init_command(),
            constants::decl_const_mysqli_opt_read_timeout(),
            constants::decl_const_mysqli_opt_net_cmd_buffer_size(),
            constants::decl_const_mysqli_opt_net_read_buffer_size(),
            constants::decl_const_mysqli_opt_int_and_float_native(),
            constants::decl_const_mysqli_opt_ssl_verify_server_cert(),
            constants::decl_const_mysqli_server_public_key(),
            constants::decl_const_mysqli_set_charset_name(),
            constants::decl_const_mysqli_opt_can_handle_expired_passwords(),
            constants::decl_const_mysqli_client_ssl(),
            constants::decl_const_mysqli_client_compress(),
            constants::decl_const_mysqli_client_interactive(),
            constants::decl_const_mysqli_client_ignore_space(),
            constants::decl_const_mysqli_client_no_schema(),
            constants::decl_const_mysqli_client_found_rows(),
            constants::decl_const_mysqli_client_ssl_verify_server_cert(),
            constants::decl_const_mysqli_client_ssl_dont_verify_server_cert(),
            constants::decl_const_mysqli_client_can_handle_expired_passwords(),
            constants::decl_const_mysqli_store_result(),
            constants::decl_const_mysqli_use_result(),
            constants::decl_const_mysqli_async(),
            constants::decl_const_mysqli_store_result_copy_data(),
            constants::decl_const_mysqli_assoc(),
            constants::decl_const_mysqli_num(),
            constants::decl_const_mysqli_both(),
            constants::decl_const_mysqli_stmt_attr_update_max_length(),
            constants::decl_const_mysqli_stmt_attr_cursor_type(),
            constants::decl_const_mysqli_cursor_type_no_cursor(),
            constants::decl_const_mysqli_cursor_type_read_only(),
            constants::decl_const_mysqli_not_null_flag(),
            constants::decl_const_mysqli_pri_key_flag(),
            constants::decl_const_mysqli_unique_key_flag(),
            constants::decl_const_mysqli_multiple_key_flag(),
            constants::decl_const_mysqli_blob_flag(),
            constants::decl_const_mysqli_unsigned_flag(),
            constants::decl_const_mysqli_zerofill_flag(),
            constants::decl_const_mysqli_auto_increment_flag(),
            constants::decl_const_mysqli_timestamp_flag(),
            constants::decl_const_mysqli_set_flag(),
            constants::decl_const_mysqli_num_flag(),
            constants::decl_const_mysqli_part_key_flag(),
            constants::decl_const_mysqli_group_flag(),
            constants::decl_const_mysqli_enum_flag(),
            constants::decl_const_mysqli_binary_flag(),
            constants::decl_const_mysqli_no_default_value_flag(),
            constants::decl_const_mysqli_on_update_now_flag(),
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
            constants::decl_const_mysqli_type_char(),
            constants::decl_const_mysqli_type_geometry(),
            constants::decl_const_mysqli_no_data(),
            constants::decl_const_mysqli_data_truncated(),
            constants::decl_const_mysqli_report_index(),
            constants::decl_const_mysqli_report_error(),
            constants::decl_const_mysqli_report_strict(),
            constants::decl_const_mysqli_report_all(),
            constants::decl_const_mysqli_report_off(),
            constants::decl_const_mysqli_debug_trace_enabled(),
            constants::decl_const_mysqli_server_query_no_good_index_used(),
            constants::decl_const_mysqli_server_query_no_index_used(),
            constants::decl_const_mysqli_server_query_was_slow(),
            constants::decl_const_mysqli_server_ps_out_params(),
            constants::decl_const_mysqli_refresh_grant(),
            constants::decl_const_mysqli_refresh_log(),
            constants::decl_const_mysqli_refresh_tables(),
            constants::decl_const_mysqli_refresh_hosts(),
            constants::decl_const_mysqli_refresh_status(),
            constants::decl_const_mysqli_refresh_threads(),
            constants::decl_const_mysqli_refresh_slave(),
            constants::decl_const_mysqli_refresh_master(),
            constants::decl_const_mysqli_refresh_backup_log(),
            constants::decl_const_mysqli_trans_start_with_consistent_snapshot(),
            constants::decl_const_mysqli_trans_start_read_write(),
            constants::decl_const_mysqli_trans_start_read_only(),
            constants::decl_const_mysqli_trans_cor_and_chain(),
            constants::decl_const_mysqli_trans_cor_and_no_chain(),
            constants::decl_const_mysqli_trans_cor_release(),
            constants::decl_const_mysqli_trans_cor_no_release(),
        ]);
        // PHP 8.1 added MYSQLI_OPT_LOAD_DATA_LOCAL_DIR, MYSQLI_REFRESH_REPLICA, and MYSQLI_IS_MARIADB.
        if php_version >= PhpVersion::Php81 {
            declarations.push(constants::decl_const_mysqli_opt_load_data_local_dir());
        }
        if php_version >= PhpVersion::Php81 {
            declarations.push(constants::decl_const_mysqli_refresh_replica());
        }
        if php_version >= PhpVersion::Php81 {
            declarations.push(constants::decl_const_mysqli_is_mariadb());
        }
        // PHP 8.4 removed the prefetch / FOR_UPDATE / SCROLLABLE / charset-dir / INTERVAL constants.
        if php_version < PhpVersion::Php84 {
            declarations.push(constants::decl_const_mysqli_stmt_attr_prefetch_rows());
        }
        if php_version < PhpVersion::Php84 {
            declarations.push(constants::decl_const_mysqli_cursor_type_for_update());
        }
        if php_version < PhpVersion::Php84 {
            declarations.push(constants::decl_const_mysqli_cursor_type_scrollable());
        }
        if php_version < PhpVersion::Php84 {
            declarations.push(constants::decl_const_mysqli_set_charset_dir());
        }
        if php_version < PhpVersion::Php84 {
            declarations.push(constants::decl_const_mysqli_type_interval());
        }
        // MYSQLI_TYPE_VECTOR arrived in PHP 8.4.
        if php_version >= PhpVersion::Php84 {
            declarations.push(constants::decl_const_mysqli_type_vector());
        }
        // MYSQLI_OPT_COMPRESS is PHP 8.6+.
        if php_version >= PhpVersion::Php86 {
            declarations.push(constants::decl_const_mysqli_opt_compress());
        }
        declarations.extend([
            exception::decl_class_mysqli_sql_exception(),
            exception::decl_fn_mysqli_report(),
            connection::decl_class_mysqli(php_version),
            result::decl_class_mysqli_result(php_version),
            result::decl_class_elephcmysqliresultiterator(),
            statement::decl_class_mysqli_stmt(php_version),
            driver::decl_class_mysqli_driver(),
            driver::decl_class_mysqli_warning(),
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
        ]);
        // `mysqli_quote_string` is PHP 8.6+, like the method it forwards to.
        if php_version >= PhpVersion::Php86 {
            declarations.push(procedural::decl_fn_mysqli_quote_string());
        }
        declarations.extend([
            procedural::decl_fn_mysqli_begin_transaction(),
            procedural::decl_fn_mysqli_commit(),
            procedural::decl_fn_mysqli_rollback(),
            procedural::decl_fn_mysqli_savepoint(),
            procedural::decl_fn_mysqli_release_savepoint(),
            procedural::decl_fn_mysqli_autocommit(),
            procedural::decl_fn_mysqli_options(),
            procedural::decl_fn_mysqli_set_opt(),
            procedural::decl_fn_mysqli_ssl_set(),
            procedural::decl_fn_mysqli_change_user(),
            procedural::decl_fn_mysqli_kill(),
            procedural::decl_fn_mysqli_refresh(),
            procedural::decl_fn_mysqli_dump_debug_info(),
            procedural::decl_fn_mysqli_debug(),
            procedural::decl_fn_mysqli_get_server_info(),
            procedural::decl_fn_mysqli_get_client_info(),
            procedural::decl_fn_mysqli_get_client_version(),
            procedural::decl_fn_mysqli_get_host_info(),
            procedural::decl_fn_mysqli_get_proto_info(),
            procedural::decl_fn_mysqli_get_server_version(),
            procedural::decl_fn_mysqli_get_connection_stats(),
            procedural::decl_fn_mysqli_get_client_stats(),
            procedural::decl_fn_mysqli_get_links_stats(),
            procedural::decl_fn_mysqli_get_warnings(),
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
            procedural::decl_fn_mysqli_reap_async_query(),
            procedural::decl_fn_mysqli_poll(),
            procedural::decl_fn_mysqli_fetch_assoc(),
            procedural::decl_fn_mysqli_fetch_row(),
            procedural::decl_fn_mysqli_fetch_array(),
            procedural::decl_fn_mysqli_fetch_object(),
            procedural::decl_fn_mysqli_fetch_all(),
        ]);
        // `mysqli_fetch_column` arrived with `mysqli_result::fetch_column` in PHP 8.1.
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
            procedural::decl_fn_mysqli_fetch_lengths(),
            procedural::decl_fn_mysqli_field_seek(),
            procedural::decl_fn_mysqli_field_tell(),
            procedural::decl_fn_mysqli_free_result(),
            procedural::decl_fn_mysqli_prepare(),
        ]);
        // `mysqli_execute_query` is PHP 8.2+, like the method it forwards to.
        if php_version >= PhpVersion::Php82 {
            declarations.push(procedural::decl_fn_mysqli_execute_query());
        }
        declarations.extend([
            procedural::decl_fn_mysqli_stmt_init(),
            procedural::decl_fn_mysqli_stmt_bind_param(),
            procedural::decl_fn_mysqli_stmt_bind_result(),
            procedural::decl_fn_mysqli_stmt_execute(),
            procedural::decl_fn_mysqli_execute(),
            procedural::decl_fn_mysqli_stmt_prepare(),
            procedural::decl_fn_mysqli_stmt_fetch(),
            procedural::decl_fn_mysqli_stmt_get_result(),
            procedural::decl_fn_mysqli_stmt_store_result(),
            procedural::decl_fn_mysqli_stmt_result_metadata(),
            procedural::decl_fn_mysqli_stmt_data_seek(),
            procedural::decl_fn_mysqli_stmt_more_results(),
            procedural::decl_fn_mysqli_stmt_next_result(),
            procedural::decl_fn_mysqli_stmt_send_long_data(),
            procedural::decl_fn_mysqli_stmt_attr_get(),
            procedural::decl_fn_mysqli_stmt_attr_set(),
            procedural::decl_fn_mysqli_stmt_get_warnings(),
            procedural::decl_fn_mysqli_stmt_reset(),
            procedural::decl_fn_mysqli_stmt_close(),
            procedural::decl_fn_mysqli_stmt_free_result(),
            procedural::decl_fn_mysqli_stmt_affected_rows(),
            procedural::decl_fn_mysqli_stmt_errno(),
            procedural::decl_fn_mysqli_stmt_error(),
            procedural::decl_fn_mysqli_stmt_error_list(),
            procedural::decl_fn_mysqli_stmt_num_rows(),
            procedural::decl_fn_mysqli_stmt_param_count(),
            procedural::decl_fn_mysqli_stmt_field_count(),
            procedural::decl_fn_mysqli_stmt_insert_id(),
            procedural::decl_fn_mysqli_stmt_sqlstate(),
            procedural::decl_fn_mysqli_get_charset(),
            procedural::decl_fn_mysqli_thread_safe(),
        ]);
        declarations
    })
}
