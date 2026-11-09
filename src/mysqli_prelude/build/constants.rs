//! Purpose:
//! Builds the complete `MYSQLI_*` constant surface as AST, one `const`
//! declaration per helper, with php-src's exact values.
//!
//! Called from:
//! - `crate::mysqli_prelude::build::mysqli_declarations`, which also applies the
//!   version gates (PHP 8.1 / < 8.4 / 8.4 / 8.6 groups).
//!
//! Key details:
//! - TRANSCRIBED from `mysqli_prelude::constants::SRC` (`synthetic_class::transcribe`);
//!   the oracle `built_declarations_match_the_php_for_every_version` compares each
//!   built node against that PHP.

use crate::parser::ast::{Stmt};
use crate::synthetic_class::{
    e_bool,
    e_int,
    s_const,
};

/// `MYSQLI_READ_DEFAULT_GROUP` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_read_default_group() -> Stmt {
    s_const("MYSQLI_READ_DEFAULT_GROUP", e_int(5))
}

/// `MYSQLI_READ_DEFAULT_FILE` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_read_default_file() -> Stmt {
    s_const("MYSQLI_READ_DEFAULT_FILE", e_int(4))
}

/// `MYSQLI_OPT_CONNECT_TIMEOUT` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_opt_connect_timeout() -> Stmt {
    s_const("MYSQLI_OPT_CONNECT_TIMEOUT", e_int(0))
}

/// `MYSQLI_OPT_LOCAL_INFILE` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_opt_local_infile() -> Stmt {
    s_const("MYSQLI_OPT_LOCAL_INFILE", e_int(8))
}

/// `MYSQLI_INIT_COMMAND` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_init_command() -> Stmt {
    s_const("MYSQLI_INIT_COMMAND", e_int(3))
}

/// `MYSQLI_OPT_READ_TIMEOUT` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_opt_read_timeout() -> Stmt {
    s_const("MYSQLI_OPT_READ_TIMEOUT", e_int(11))
}

/// `MYSQLI_OPT_NET_CMD_BUFFER_SIZE` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_opt_net_cmd_buffer_size() -> Stmt {
    s_const("MYSQLI_OPT_NET_CMD_BUFFER_SIZE", e_int(202))
}

/// `MYSQLI_OPT_NET_READ_BUFFER_SIZE` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_opt_net_read_buffer_size() -> Stmt {
    s_const("MYSQLI_OPT_NET_READ_BUFFER_SIZE", e_int(203))
}

/// `MYSQLI_OPT_INT_AND_FLOAT_NATIVE` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_opt_int_and_float_native() -> Stmt {
    s_const("MYSQLI_OPT_INT_AND_FLOAT_NATIVE", e_int(201))
}

/// `MYSQLI_OPT_SSL_VERIFY_SERVER_CERT` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_opt_ssl_verify_server_cert() -> Stmt {
    s_const("MYSQLI_OPT_SSL_VERIFY_SERVER_CERT", e_int(21))
}

/// `MYSQLI_SERVER_PUBLIC_KEY` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_server_public_key() -> Stmt {
    s_const("MYSQLI_SERVER_PUBLIC_KEY", e_int(35))
}

/// `MYSQLI_SET_CHARSET_NAME` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_set_charset_name() -> Stmt {
    s_const("MYSQLI_SET_CHARSET_NAME", e_int(7))
}

/// `MYSQLI_OPT_CAN_HANDLE_EXPIRED_PASSWORDS` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_opt_can_handle_expired_passwords() -> Stmt {
    s_const("MYSQLI_OPT_CAN_HANDLE_EXPIRED_PASSWORDS", e_int(37))
}

/// `MYSQLI_CLIENT_SSL` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_client_ssl() -> Stmt {
    s_const("MYSQLI_CLIENT_SSL", e_int(2048))
}

/// `MYSQLI_CLIENT_COMPRESS` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_client_compress() -> Stmt {
    s_const("MYSQLI_CLIENT_COMPRESS", e_int(32))
}

/// `MYSQLI_CLIENT_INTERACTIVE` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_client_interactive() -> Stmt {
    s_const("MYSQLI_CLIENT_INTERACTIVE", e_int(1024))
}

/// `MYSQLI_CLIENT_IGNORE_SPACE` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_client_ignore_space() -> Stmt {
    s_const("MYSQLI_CLIENT_IGNORE_SPACE", e_int(256))
}

/// `MYSQLI_CLIENT_NO_SCHEMA` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_client_no_schema() -> Stmt {
    s_const("MYSQLI_CLIENT_NO_SCHEMA", e_int(16))
}

/// `MYSQLI_CLIENT_FOUND_ROWS` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_client_found_rows() -> Stmt {
    s_const("MYSQLI_CLIENT_FOUND_ROWS", e_int(2))
}

/// `MYSQLI_CLIENT_SSL_VERIFY_SERVER_CERT` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_client_ssl_verify_server_cert() -> Stmt {
    s_const("MYSQLI_CLIENT_SSL_VERIFY_SERVER_CERT", e_int(1073741824))
}

/// `MYSQLI_CLIENT_SSL_DONT_VERIFY_SERVER_CERT` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_client_ssl_dont_verify_server_cert() -> Stmt {
    s_const("MYSQLI_CLIENT_SSL_DONT_VERIFY_SERVER_CERT", e_int(64))
}

/// `MYSQLI_CLIENT_CAN_HANDLE_EXPIRED_PASSWORDS` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_client_can_handle_expired_passwords() -> Stmt {
    s_const("MYSQLI_CLIENT_CAN_HANDLE_EXPIRED_PASSWORDS", e_int(4194304))
}

/// `MYSQLI_STORE_RESULT` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_store_result() -> Stmt {
    s_const("MYSQLI_STORE_RESULT", e_int(0))
}

/// `MYSQLI_USE_RESULT` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_use_result() -> Stmt {
    s_const("MYSQLI_USE_RESULT", e_int(1))
}

/// `MYSQLI_ASYNC` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_async() -> Stmt {
    s_const("MYSQLI_ASYNC", e_int(8))
}

/// `MYSQLI_STORE_RESULT_COPY_DATA` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_store_result_copy_data() -> Stmt {
    s_const("MYSQLI_STORE_RESULT_COPY_DATA", e_int(16))
}

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

/// `MYSQLI_STMT_ATTR_UPDATE_MAX_LENGTH` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_stmt_attr_update_max_length() -> Stmt {
    s_const("MYSQLI_STMT_ATTR_UPDATE_MAX_LENGTH", e_int(0))
}

/// `MYSQLI_STMT_ATTR_CURSOR_TYPE` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_stmt_attr_cursor_type() -> Stmt {
    s_const("MYSQLI_STMT_ATTR_CURSOR_TYPE", e_int(1))
}

/// `MYSQLI_CURSOR_TYPE_NO_CURSOR` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_cursor_type_no_cursor() -> Stmt {
    s_const("MYSQLI_CURSOR_TYPE_NO_CURSOR", e_int(0))
}

/// `MYSQLI_CURSOR_TYPE_READ_ONLY` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_cursor_type_read_only() -> Stmt {
    s_const("MYSQLI_CURSOR_TYPE_READ_ONLY", e_int(1))
}

/// `MYSQLI_NOT_NULL_FLAG` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_not_null_flag() -> Stmt {
    s_const("MYSQLI_NOT_NULL_FLAG", e_int(1))
}

/// `MYSQLI_PRI_KEY_FLAG` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_pri_key_flag() -> Stmt {
    s_const("MYSQLI_PRI_KEY_FLAG", e_int(2))
}

/// `MYSQLI_UNIQUE_KEY_FLAG` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_unique_key_flag() -> Stmt {
    s_const("MYSQLI_UNIQUE_KEY_FLAG", e_int(4))
}

/// `MYSQLI_MULTIPLE_KEY_FLAG` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_multiple_key_flag() -> Stmt {
    s_const("MYSQLI_MULTIPLE_KEY_FLAG", e_int(8))
}

/// `MYSQLI_BLOB_FLAG` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_blob_flag() -> Stmt {
    s_const("MYSQLI_BLOB_FLAG", e_int(16))
}

/// `MYSQLI_UNSIGNED_FLAG` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_unsigned_flag() -> Stmt {
    s_const("MYSQLI_UNSIGNED_FLAG", e_int(32))
}

/// `MYSQLI_ZEROFILL_FLAG` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_zerofill_flag() -> Stmt {
    s_const("MYSQLI_ZEROFILL_FLAG", e_int(64))
}

/// `MYSQLI_AUTO_INCREMENT_FLAG` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_auto_increment_flag() -> Stmt {
    s_const("MYSQLI_AUTO_INCREMENT_FLAG", e_int(512))
}

/// `MYSQLI_TIMESTAMP_FLAG` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_timestamp_flag() -> Stmt {
    s_const("MYSQLI_TIMESTAMP_FLAG", e_int(1024))
}

/// `MYSQLI_SET_FLAG` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_set_flag() -> Stmt {
    s_const("MYSQLI_SET_FLAG", e_int(2048))
}

/// `MYSQLI_NUM_FLAG` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_num_flag() -> Stmt {
    s_const("MYSQLI_NUM_FLAG", e_int(32768))
}

/// `MYSQLI_PART_KEY_FLAG` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_part_key_flag() -> Stmt {
    s_const("MYSQLI_PART_KEY_FLAG", e_int(16384))
}

/// `MYSQLI_GROUP_FLAG` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_group_flag() -> Stmt {
    s_const("MYSQLI_GROUP_FLAG", e_int(32768))
}

/// `MYSQLI_ENUM_FLAG` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_enum_flag() -> Stmt {
    s_const("MYSQLI_ENUM_FLAG", e_int(256))
}

/// `MYSQLI_BINARY_FLAG` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_binary_flag() -> Stmt {
    s_const("MYSQLI_BINARY_FLAG", e_int(128))
}

/// `MYSQLI_NO_DEFAULT_VALUE_FLAG` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_no_default_value_flag() -> Stmt {
    s_const("MYSQLI_NO_DEFAULT_VALUE_FLAG", e_int(4096))
}

/// `MYSQLI_ON_UPDATE_NOW_FLAG` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_on_update_now_flag() -> Stmt {
    s_const("MYSQLI_ON_UPDATE_NOW_FLAG", e_int(8192))
}

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

/// `MYSQLI_TYPE_CHAR` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_type_char() -> Stmt {
    s_const("MYSQLI_TYPE_CHAR", e_int(1))
}

/// `MYSQLI_TYPE_GEOMETRY` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_type_geometry() -> Stmt {
    s_const("MYSQLI_TYPE_GEOMETRY", e_int(255))
}

/// `MYSQLI_NO_DATA` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_no_data() -> Stmt {
    s_const("MYSQLI_NO_DATA", e_int(100))
}

/// `MYSQLI_DATA_TRUNCATED` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_data_truncated() -> Stmt {
    s_const("MYSQLI_DATA_TRUNCATED", e_int(101))
}

/// `MYSQLI_REPORT_INDEX` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_report_index() -> Stmt {
    s_const("MYSQLI_REPORT_INDEX", e_int(4))
}

/// `MYSQLI_REPORT_ERROR` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_report_error() -> Stmt {
    s_const("MYSQLI_REPORT_ERROR", e_int(1))
}

/// `MYSQLI_REPORT_STRICT` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_report_strict() -> Stmt {
    s_const("MYSQLI_REPORT_STRICT", e_int(2))
}

/// `MYSQLI_REPORT_ALL` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_report_all() -> Stmt {
    s_const("MYSQLI_REPORT_ALL", e_int(255))
}

/// `MYSQLI_REPORT_OFF` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_report_off() -> Stmt {
    s_const("MYSQLI_REPORT_OFF", e_int(0))
}

/// `MYSQLI_DEBUG_TRACE_ENABLED` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_debug_trace_enabled() -> Stmt {
    s_const("MYSQLI_DEBUG_TRACE_ENABLED", e_int(0))
}

/// `MYSQLI_SERVER_QUERY_NO_GOOD_INDEX_USED` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_server_query_no_good_index_used() -> Stmt {
    s_const("MYSQLI_SERVER_QUERY_NO_GOOD_INDEX_USED", e_int(16))
}

/// `MYSQLI_SERVER_QUERY_NO_INDEX_USED` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_server_query_no_index_used() -> Stmt {
    s_const("MYSQLI_SERVER_QUERY_NO_INDEX_USED", e_int(32))
}

/// `MYSQLI_SERVER_QUERY_WAS_SLOW` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_server_query_was_slow() -> Stmt {
    s_const("MYSQLI_SERVER_QUERY_WAS_SLOW", e_int(2048))
}

/// `MYSQLI_SERVER_PS_OUT_PARAMS` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_server_ps_out_params() -> Stmt {
    s_const("MYSQLI_SERVER_PS_OUT_PARAMS", e_int(4096))
}

/// `MYSQLI_REFRESH_GRANT` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_refresh_grant() -> Stmt {
    s_const("MYSQLI_REFRESH_GRANT", e_int(1))
}

/// `MYSQLI_REFRESH_LOG` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_refresh_log() -> Stmt {
    s_const("MYSQLI_REFRESH_LOG", e_int(2))
}

/// `MYSQLI_REFRESH_TABLES` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_refresh_tables() -> Stmt {
    s_const("MYSQLI_REFRESH_TABLES", e_int(4))
}

/// `MYSQLI_REFRESH_HOSTS` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_refresh_hosts() -> Stmt {
    s_const("MYSQLI_REFRESH_HOSTS", e_int(8))
}

/// `MYSQLI_REFRESH_STATUS` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_refresh_status() -> Stmt {
    s_const("MYSQLI_REFRESH_STATUS", e_int(16))
}

/// `MYSQLI_REFRESH_THREADS` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_refresh_threads() -> Stmt {
    s_const("MYSQLI_REFRESH_THREADS", e_int(32))
}

/// `MYSQLI_REFRESH_SLAVE` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_refresh_slave() -> Stmt {
    s_const("MYSQLI_REFRESH_SLAVE", e_int(64))
}

/// `MYSQLI_REFRESH_MASTER` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_refresh_master() -> Stmt {
    s_const("MYSQLI_REFRESH_MASTER", e_int(128))
}

/// `MYSQLI_REFRESH_BACKUP_LOG` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_refresh_backup_log() -> Stmt {
    s_const("MYSQLI_REFRESH_BACKUP_LOG", e_int(2097152))
}

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

/// `MYSQLI_OPT_LOAD_DATA_LOCAL_DIR` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_opt_load_data_local_dir() -> Stmt {
    s_const("MYSQLI_OPT_LOAD_DATA_LOCAL_DIR", e_int(43))
}

/// `MYSQLI_REFRESH_REPLICA` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_refresh_replica() -> Stmt {
    s_const("MYSQLI_REFRESH_REPLICA", e_int(64))
}

/// `MYSQLI_IS_MARIADB` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_is_mariadb() -> Stmt {
    s_const("MYSQLI_IS_MARIADB", e_bool(false))
}

/// `MYSQLI_STMT_ATTR_PREFETCH_ROWS` — transcribed from the PHP form (PHP < 8.4 only).
pub(super) fn decl_const_mysqli_stmt_attr_prefetch_rows() -> Stmt {
    s_const("MYSQLI_STMT_ATTR_PREFETCH_ROWS", e_int(2))
}

/// `MYSQLI_CURSOR_TYPE_FOR_UPDATE` — transcribed from the PHP form (PHP < 8.4 only).
pub(super) fn decl_const_mysqli_cursor_type_for_update() -> Stmt {
    s_const("MYSQLI_CURSOR_TYPE_FOR_UPDATE", e_int(2))
}

/// `MYSQLI_CURSOR_TYPE_SCROLLABLE` — transcribed from the PHP form (PHP < 8.4 only).
pub(super) fn decl_const_mysqli_cursor_type_scrollable() -> Stmt {
    s_const("MYSQLI_CURSOR_TYPE_SCROLLABLE", e_int(4))
}

/// `MYSQLI_SET_CHARSET_DIR` — transcribed from the PHP form (PHP < 8.4 only).
pub(super) fn decl_const_mysqli_set_charset_dir() -> Stmt {
    s_const("MYSQLI_SET_CHARSET_DIR", e_int(6))
}

/// `MYSQLI_TYPE_INTERVAL` — transcribed from the PHP form (PHP < 8.4 only).
pub(super) fn decl_const_mysqli_type_interval() -> Stmt {
    s_const("MYSQLI_TYPE_INTERVAL", e_int(247))
}

/// `MYSQLI_TYPE_VECTOR` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_type_vector() -> Stmt {
    s_const("MYSQLI_TYPE_VECTOR", e_int(242))
}

/// `MYSQLI_OPT_COMPRESS` — transcribed from the PHP form.
pub(super) fn decl_const_mysqli_opt_compress() -> Stmt {
    s_const("MYSQLI_OPT_COMPRESS", e_int(1))
}
