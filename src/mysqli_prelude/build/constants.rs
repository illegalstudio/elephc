//! Purpose:
//! Builds the locked v1 `MYSQLI_*` constant surface as AST: fetch modes, report
//! flags, client flags, option ids, transaction flags, and the column-type ids
//! `mysqli_result::fetch_field()->type` reports. Only the constants the v1 subset
//! consumes are declared, so `defined('MYSQLI_…')` stays honest.
//!
//! Called from:
//! - `crate::mysqli_prelude::build::mysqli_declarations`.
//!
//! Key details:
//! - TRANSCRIBED from `mysqli_prelude::constants::SRC` (`synthetic_class::transcribe`);
//!   the oracle `built_declarations_match_the_php_for_every_version` compares each
//!   built node against that PHP.
//! - Values match php-src's mysqli exactly.
//! - `MYSQLI_CLIENT_SSL` is declared but rejected at connect time with a clear
//!   error: elephc's mysqli does not support the SSL client flag (documented
//!   divergence; PDO MySQL TLS attributes are the supported TLS path).

use crate::parser::ast::{Stmt};
use crate::synthetic_class::{
    e_int,
    s_const,
};

// Fetch modes for mysqli_result::fetch_array / fetch_all.
/// `MYSQLI_ASSOC` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_assoc() -> Stmt {
    s_const("MYSQLI_ASSOC", e_int(1))
}

/// `MYSQLI_NUM` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_num() -> Stmt {
    s_const("MYSQLI_NUM", e_int(2))
}

/// `MYSQLI_BOTH` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_both() -> Stmt {
    s_const("MYSQLI_BOTH", e_int(3))
}

// Result modes for mysqli::query. MYSQLI_USE_RESULT is accepted but still buffered (documented
// divergence: true unbuffered use_result is out of scope).
/// `MYSQLI_STORE_RESULT` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_store_result() -> Stmt {
    s_const("MYSQLI_STORE_RESULT", e_int(0))
}

/// `MYSQLI_USE_RESULT` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_use_result() -> Stmt {
    s_const("MYSQLI_USE_RESULT", e_int(1))
}

// mysqli_report() flags. PHP 8.1+ defaults to ERROR|STRICT; PHP 8.0 to OFF.
/// `MYSQLI_REPORT_OFF` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_report_off() -> Stmt {
    s_const("MYSQLI_REPORT_OFF", e_int(0))
}

/// `MYSQLI_REPORT_ERROR` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_report_error() -> Stmt {
    s_const("MYSQLI_REPORT_ERROR", e_int(1))
}

/// `MYSQLI_REPORT_STRICT` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_report_strict() -> Stmt {
    s_const("MYSQLI_REPORT_STRICT", e_int(2))
}

/// `MYSQLI_REPORT_INDEX` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_report_index() -> Stmt {
    s_const("MYSQLI_REPORT_INDEX", e_int(4))
}

/// `MYSQLI_REPORT_ALL` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_report_all() -> Stmt {
    s_const("MYSQLI_REPORT_ALL", e_int(255))
}

// real_connect() client flags. MYSQLI_CLIENT_SSL is declared so programs that pass it compile, but
// connect rejects it with a clear error (no TLS support in elephc's mysqli surface).
/// `MYSQLI_CLIENT_COMPRESS` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_client_compress() -> Stmt {
    s_const("MYSQLI_CLIENT_COMPRESS", e_int(32))
}

/// `MYSQLI_CLIENT_FOUND_ROWS` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_client_found_rows() -> Stmt {
    s_const("MYSQLI_CLIENT_FOUND_ROWS", e_int(2))
}

/// `MYSQLI_CLIENT_IGNORE_SPACE` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_client_ignore_space() -> Stmt {
    s_const("MYSQLI_CLIENT_IGNORE_SPACE", e_int(256))
}

/// `MYSQLI_CLIENT_INTERACTIVE` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_client_interactive() -> Stmt {
    s_const("MYSQLI_CLIENT_INTERACTIVE", e_int(1024))
}

/// `MYSQLI_CLIENT_SSL` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_client_ssl() -> Stmt {
    s_const("MYSQLI_CLIENT_SSL", e_int(2048))
}

// mysqli_options() option ids (the three the v1 subset honors).
/// `MYSQLI_OPT_CONNECT_TIMEOUT` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_opt_connect_timeout() -> Stmt {
    s_const("MYSQLI_OPT_CONNECT_TIMEOUT", e_int(0))
}

/// `MYSQLI_INIT_COMMAND` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_init_command() -> Stmt {
    s_const("MYSQLI_INIT_COMMAND", e_int(3))
}

/// `MYSQLI_SET_CHARSET_NAME` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_set_charset_name() -> Stmt {
    s_const("MYSQLI_SET_CHARSET_NAME", e_int(7))
}

// begin_transaction() flags.
/// `MYSQLI_TRANS_START_WITH_CONSISTENT_SNAPSHOT` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_trans_start_with_consistent_snapshot() -> Stmt {
    s_const("MYSQLI_TRANS_START_WITH_CONSISTENT_SNAPSHOT", e_int(1))
}

/// `MYSQLI_TRANS_START_READ_WRITE` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_trans_start_read_write() -> Stmt {
    s_const("MYSQLI_TRANS_START_READ_WRITE", e_int(2))
}

/// `MYSQLI_TRANS_START_READ_ONLY` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_trans_start_read_only() -> Stmt {
    s_const("MYSQLI_TRANS_START_READ_ONLY", e_int(4))
}

// commit() / rollback() completion flags (composed into the SQL: AND [NO] CHAIN and [NO] RELEASE).
/// `MYSQLI_TRANS_COR_AND_CHAIN` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_trans_cor_and_chain() -> Stmt {
    s_const("MYSQLI_TRANS_COR_AND_CHAIN", e_int(1))
}

/// `MYSQLI_TRANS_COR_AND_NO_CHAIN` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_trans_cor_and_no_chain() -> Stmt {
    s_const("MYSQLI_TRANS_COR_AND_NO_CHAIN", e_int(2))
}

/// `MYSQLI_TRANS_COR_RELEASE` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_trans_cor_release() -> Stmt {
    s_const("MYSQLI_TRANS_COR_RELEASE", e_int(4))
}

/// `MYSQLI_TRANS_COR_NO_RELEASE` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_trans_cor_no_release() -> Stmt {
    s_const("MYSQLI_TRANS_COR_NO_RELEASE", e_int(8))
}

// Column types reported by mysqli_result::fetch_field()->type, mapped from the bridge's native
// wire-type names (values match php-src / the MySQL protocol).
/// `MYSQLI_TYPE_DECIMAL` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_type_decimal() -> Stmt {
    s_const("MYSQLI_TYPE_DECIMAL", e_int(0))
}

/// `MYSQLI_TYPE_TINY` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_type_tiny() -> Stmt {
    s_const("MYSQLI_TYPE_TINY", e_int(1))
}

/// `MYSQLI_TYPE_SHORT` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_type_short() -> Stmt {
    s_const("MYSQLI_TYPE_SHORT", e_int(2))
}

/// `MYSQLI_TYPE_LONG` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_type_long() -> Stmt {
    s_const("MYSQLI_TYPE_LONG", e_int(3))
}

/// `MYSQLI_TYPE_FLOAT` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_type_float() -> Stmt {
    s_const("MYSQLI_TYPE_FLOAT", e_int(4))
}

/// `MYSQLI_TYPE_DOUBLE` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_type_double() -> Stmt {
    s_const("MYSQLI_TYPE_DOUBLE", e_int(5))
}

/// `MYSQLI_TYPE_NULL` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_type_null() -> Stmt {
    s_const("MYSQLI_TYPE_NULL", e_int(6))
}

/// `MYSQLI_TYPE_TIMESTAMP` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_type_timestamp() -> Stmt {
    s_const("MYSQLI_TYPE_TIMESTAMP", e_int(7))
}

/// `MYSQLI_TYPE_LONGLONG` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_type_longlong() -> Stmt {
    s_const("MYSQLI_TYPE_LONGLONG", e_int(8))
}

/// `MYSQLI_TYPE_INT24` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_type_int24() -> Stmt {
    s_const("MYSQLI_TYPE_INT24", e_int(9))
}

/// `MYSQLI_TYPE_DATE` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_type_date() -> Stmt {
    s_const("MYSQLI_TYPE_DATE", e_int(10))
}

/// `MYSQLI_TYPE_TIME` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_type_time() -> Stmt {
    s_const("MYSQLI_TYPE_TIME", e_int(11))
}

/// `MYSQLI_TYPE_DATETIME` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_type_datetime() -> Stmt {
    s_const("MYSQLI_TYPE_DATETIME", e_int(12))
}

/// `MYSQLI_TYPE_YEAR` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_type_year() -> Stmt {
    s_const("MYSQLI_TYPE_YEAR", e_int(13))
}

/// `MYSQLI_TYPE_NEWDATE` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_type_newdate() -> Stmt {
    s_const("MYSQLI_TYPE_NEWDATE", e_int(14))
}

/// `MYSQLI_TYPE_VARCHAR` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_type_varchar() -> Stmt {
    s_const("MYSQLI_TYPE_VARCHAR", e_int(15))
}

/// `MYSQLI_TYPE_BIT` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_type_bit() -> Stmt {
    s_const("MYSQLI_TYPE_BIT", e_int(16))
}

/// `MYSQLI_TYPE_JSON` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_type_json() -> Stmt {
    s_const("MYSQLI_TYPE_JSON", e_int(245))
}

/// `MYSQLI_TYPE_NEWDECIMAL` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_type_newdecimal() -> Stmt {
    s_const("MYSQLI_TYPE_NEWDECIMAL", e_int(246))
}

/// `MYSQLI_TYPE_ENUM` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_type_enum() -> Stmt {
    s_const("MYSQLI_TYPE_ENUM", e_int(247))
}

/// `MYSQLI_TYPE_SET` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_type_set() -> Stmt {
    s_const("MYSQLI_TYPE_SET", e_int(248))
}

/// `MYSQLI_TYPE_TINY_BLOB` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_type_tiny_blob() -> Stmt {
    s_const("MYSQLI_TYPE_TINY_BLOB", e_int(249))
}

/// `MYSQLI_TYPE_MEDIUM_BLOB` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_type_medium_blob() -> Stmt {
    s_const("MYSQLI_TYPE_MEDIUM_BLOB", e_int(250))
}

/// `MYSQLI_TYPE_LONG_BLOB` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_type_long_blob() -> Stmt {
    s_const("MYSQLI_TYPE_LONG_BLOB", e_int(251))
}

/// `MYSQLI_TYPE_BLOB` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_type_blob() -> Stmt {
    s_const("MYSQLI_TYPE_BLOB", e_int(252))
}

/// `MYSQLI_TYPE_VAR_STRING` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_type_var_string() -> Stmt {
    s_const("MYSQLI_TYPE_VAR_STRING", e_int(253))
}

/// `MYSQLI_TYPE_STRING` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_type_string() -> Stmt {
    s_const("MYSQLI_TYPE_STRING", e_int(254))
}

/// `MYSQLI_TYPE_GEOMETRY` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_type_geometry() -> Stmt {
    s_const("MYSQLI_TYPE_GEOMETRY", e_int(255))
}
