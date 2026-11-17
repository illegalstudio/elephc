//! Purpose:
//! The complete `MYSQLI_*` constant surface, as an elephc-PHP source fragment
//! (the parse-parity oracle for `build::constants`). Every constant php-src's
//! mysqli registers (mysqlnd build) is declared with php-src's exact value, so
//! `defined('MYSQLI_…')` and every flag/option comparison match PHP.
//!
//! Called from:
//! - `crate::mysqli_prelude::fragments::source_for_version` (concatenated into
//!   the injected prelude).
//!
//! Key details:
//! - Values match php-src's mysqli + mysqlnd headers exactly (fetch modes, report
//!   flags, client flags, option ids, statement attributes, field flags, field
//!   types, refresh flags, transaction flags).
//! - Version-gated blocks (removed by `fragments::source_for_version`, mirrored by
//!   conditionals in `build::constants`): PHP 8.1 added `MYSQLI_IS_MARIADB`,
//!   `MYSQLI_OPT_LOAD_DATA_LOCAL_DIR`, and `MYSQLI_REFRESH_REPLICA`; PHP 8.4
//!   removed `MYSQLI_CURSOR_TYPE_FOR_UPDATE`, `MYSQLI_CURSOR_TYPE_SCROLLABLE`,
//!   `MYSQLI_SET_CHARSET_DIR`, `MYSQLI_STMT_ATTR_PREFETCH_ROWS`, and
//!   `MYSQLI_TYPE_INTERVAL` and added `MYSQLI_TYPE_VECTOR`; PHP 8.6 adds
//!   `MYSQLI_OPT_COMPRESS`.
//! - `MYSQLI_TYPE_VARCHAR` is an elephc extension (php-src never registered it).

/// The `MYSQLI_*` constant declarations (fragment without a `<?php` header).
pub(super) const SRC: &str = r#"
// -- mysqli constants (values match php-src's mysqli + mysqlnd) --

// mysqli_options() option ids.
const MYSQLI_READ_DEFAULT_GROUP = 5;
const MYSQLI_READ_DEFAULT_FILE = 4;
const MYSQLI_OPT_CONNECT_TIMEOUT = 0;
const MYSQLI_OPT_LOCAL_INFILE = 8;
const MYSQLI_INIT_COMMAND = 3;
const MYSQLI_OPT_READ_TIMEOUT = 11;
const MYSQLI_OPT_NET_CMD_BUFFER_SIZE = 202;
const MYSQLI_OPT_NET_READ_BUFFER_SIZE = 203;
const MYSQLI_OPT_INT_AND_FLOAT_NATIVE = 201;
const MYSQLI_OPT_SSL_VERIFY_SERVER_CERT = 21;
const MYSQLI_SERVER_PUBLIC_KEY = 35;
const MYSQLI_SET_CHARSET_NAME = 7;
const MYSQLI_OPT_CAN_HANDLE_EXPIRED_PASSWORDS = 37;

// real_connect() client flags.
const MYSQLI_CLIENT_SSL = 2048;
const MYSQLI_CLIENT_COMPRESS = 32;
const MYSQLI_CLIENT_INTERACTIVE = 1024;
const MYSQLI_CLIENT_IGNORE_SPACE = 256;
const MYSQLI_CLIENT_NO_SCHEMA = 16;
const MYSQLI_CLIENT_FOUND_ROWS = 2;
const MYSQLI_CLIENT_SSL_VERIFY_SERVER_CERT = 1073741824;
const MYSQLI_CLIENT_SSL_DONT_VERIFY_SERVER_CERT = 64;
const MYSQLI_CLIENT_CAN_HANDLE_EXPIRED_PASSWORDS = 4194304;

// Result modes for mysqli::query (MYSQLI_ASYNC is an optional bitmask flag).
const MYSQLI_STORE_RESULT = 0;
const MYSQLI_USE_RESULT = 1;
const MYSQLI_ASYNC = 8;
const MYSQLI_STORE_RESULT_COPY_DATA = 16;

// Fetch modes for mysqli_result::fetch_array / fetch_all.
const MYSQLI_ASSOC = 1;
const MYSQLI_NUM = 2;
const MYSQLI_BOTH = 3;

// mysqli_stmt::attr_get / attr_set attributes and cursor types.
const MYSQLI_STMT_ATTR_UPDATE_MAX_LENGTH = 0;
const MYSQLI_STMT_ATTR_CURSOR_TYPE = 1;
const MYSQLI_CURSOR_TYPE_NO_CURSOR = 0;
const MYSQLI_CURSOR_TYPE_READ_ONLY = 1;

// Column flags reported by mysqli_result::fetch_field()->flags.
const MYSQLI_NOT_NULL_FLAG = 1;
const MYSQLI_PRI_KEY_FLAG = 2;
const MYSQLI_UNIQUE_KEY_FLAG = 4;
const MYSQLI_MULTIPLE_KEY_FLAG = 8;
const MYSQLI_BLOB_FLAG = 16;
const MYSQLI_UNSIGNED_FLAG = 32;
const MYSQLI_ZEROFILL_FLAG = 64;
const MYSQLI_AUTO_INCREMENT_FLAG = 512;
const MYSQLI_TIMESTAMP_FLAG = 1024;
const MYSQLI_SET_FLAG = 2048;
const MYSQLI_NUM_FLAG = 32768;
const MYSQLI_PART_KEY_FLAG = 16384;
const MYSQLI_GROUP_FLAG = 32768;
const MYSQLI_ENUM_FLAG = 256;
const MYSQLI_BINARY_FLAG = 128;
const MYSQLI_NO_DEFAULT_VALUE_FLAG = 4096;
const MYSQLI_ON_UPDATE_NOW_FLAG = 8192;

// Column types reported by mysqli_result::fetch_field()->type (MySQL protocol).
const MYSQLI_TYPE_DECIMAL = 0;
const MYSQLI_TYPE_TINY = 1;
const MYSQLI_TYPE_SHORT = 2;
const MYSQLI_TYPE_LONG = 3;
const MYSQLI_TYPE_FLOAT = 4;
const MYSQLI_TYPE_DOUBLE = 5;
const MYSQLI_TYPE_NULL = 6;
const MYSQLI_TYPE_TIMESTAMP = 7;
const MYSQLI_TYPE_LONGLONG = 8;
const MYSQLI_TYPE_INT24 = 9;
const MYSQLI_TYPE_DATE = 10;
const MYSQLI_TYPE_TIME = 11;
const MYSQLI_TYPE_DATETIME = 12;
const MYSQLI_TYPE_YEAR = 13;
const MYSQLI_TYPE_NEWDATE = 14;
// elephc extension: php-src never registered MYSQLI_TYPE_VARCHAR.
const MYSQLI_TYPE_VARCHAR = 15;
const MYSQLI_TYPE_BIT = 16;
const MYSQLI_TYPE_JSON = 245;
const MYSQLI_TYPE_NEWDECIMAL = 246;
const MYSQLI_TYPE_ENUM = 247;
const MYSQLI_TYPE_SET = 248;
const MYSQLI_TYPE_TINY_BLOB = 249;
const MYSQLI_TYPE_MEDIUM_BLOB = 250;
const MYSQLI_TYPE_LONG_BLOB = 251;
const MYSQLI_TYPE_BLOB = 252;
const MYSQLI_TYPE_VAR_STRING = 253;
const MYSQLI_TYPE_STRING = 254;
const MYSQLI_TYPE_CHAR = 1;
const MYSQLI_TYPE_GEOMETRY = 255;

// Deprecated / informational codes kept for compatibility.
const MYSQLI_NO_DATA = 100;
const MYSQLI_DATA_TRUNCATED = 101;

// mysqli_report() flags. PHP 8.1+ defaults to ERROR|STRICT; PHP 8.0 to OFF.
const MYSQLI_REPORT_INDEX = 4;
const MYSQLI_REPORT_ERROR = 1;
const MYSQLI_REPORT_STRICT = 2;
const MYSQLI_REPORT_ALL = 255;
const MYSQLI_REPORT_OFF = 0;

// mysqlnd is a release build here: debug tracing is not compiled in.
const MYSQLI_DEBUG_TRACE_ENABLED = 0;

// Server status bits (deprecated since 8.1 as unused).
const MYSQLI_SERVER_QUERY_NO_GOOD_INDEX_USED = 16;
const MYSQLI_SERVER_QUERY_NO_INDEX_USED = 32;
const MYSQLI_SERVER_QUERY_WAS_SLOW = 2048;
const MYSQLI_SERVER_PS_OUT_PARAMS = 4096;

// mysqli::refresh() flags.
const MYSQLI_REFRESH_GRANT = 1;
const MYSQLI_REFRESH_LOG = 2;
const MYSQLI_REFRESH_TABLES = 4;
const MYSQLI_REFRESH_HOSTS = 8;
const MYSQLI_REFRESH_STATUS = 16;
const MYSQLI_REFRESH_THREADS = 32;
const MYSQLI_REFRESH_SLAVE = 64;
const MYSQLI_REFRESH_MASTER = 128;
const MYSQLI_REFRESH_BACKUP_LOG = 2097152;

// begin_transaction() flags.
const MYSQLI_TRANS_START_WITH_CONSISTENT_SNAPSHOT = 1;
const MYSQLI_TRANS_START_READ_WRITE = 2;
const MYSQLI_TRANS_START_READ_ONLY = 4;

// commit() / rollback() completion flags (composed into the SQL: AND [NO] CHAIN
// and [NO] RELEASE).
const MYSQLI_TRANS_COR_AND_CHAIN = 1;
const MYSQLI_TRANS_COR_AND_NO_CHAIN = 2;
const MYSQLI_TRANS_COR_RELEASE = 4;
const MYSQLI_TRANS_COR_NO_RELEASE = 8;

// -- elephc PHP >= 8.1 mysqli constants begin --
const MYSQLI_OPT_LOAD_DATA_LOCAL_DIR = 43;
const MYSQLI_REFRESH_REPLICA = 64;
const MYSQLI_IS_MARIADB = false;
// -- elephc PHP >= 8.1 mysqli constants end --

// -- elephc PHP < 8.4 mysqli constants begin --
const MYSQLI_STMT_ATTR_PREFETCH_ROWS = 2;
const MYSQLI_CURSOR_TYPE_FOR_UPDATE = 2;
const MYSQLI_CURSOR_TYPE_SCROLLABLE = 4;
const MYSQLI_SET_CHARSET_DIR = 6;
const MYSQLI_TYPE_INTERVAL = 247;
// -- elephc PHP < 8.4 mysqli constants end --

// -- elephc PHP >= 8.4 mysqli constants begin --
const MYSQLI_TYPE_VECTOR = 242;
// -- elephc PHP >= 8.4 mysqli constants end --

// -- elephc PHP >= 8.6 mysqli constants begin --
const MYSQLI_OPT_COMPRESS = 1;
// -- elephc PHP >= 8.6 mysqli constants end --
"#;
