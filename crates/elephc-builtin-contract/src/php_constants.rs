//! Purpose:
//! php's predefined integer constants, in the one crate the compiler and the eval interpreter
//! both depend on.
//!
//! Called from:
//! - `elephc::types::*_constants`, which re-export these under the paths their callers already use.
//! - `elephc_magician::interpreter::constant_eval`, which resolves names inside `eval()`.
//!
//! Key details:
//! - They used to live only in `elephc`, which `elephc-magician` does not depend on, so `eval()`
//!   resolved constants through a hand-written match of its own. MEASURED: 149 names the compiler
//!   declared were a runtime fatal inside `eval()` — `SEEK_SET`, `FILE_APPEND`, the whole `E_*`
//!   family and 95 `STREAM_*` names among them — while php answered a value.
//! - Every value here was measured against `php -n` 8.5.6. Two engines reading one table is what
//!   keeps them from drifting apart again.

/// `STREAM_SERVER_BIND`: give the socket its address.
pub const STREAM_SERVER_BIND: i64 = 4;

/// `STREAM_SERVER_LISTEN`: also make it a listening socket, which only a stream transport accepts.
/// `udp://` and `udg://` carry datagrams, so PHP fails a server asked for this on either of them.
pub const STREAM_SERVER_LISTEN: i64 = 8;

/// What `stream_socket_server()` uses when the caller omits `$flags`.
pub const STREAM_SERVER_DEFAULT_FLAGS: i64 = STREAM_SERVER_BIND | STREAM_SERVER_LISTEN;

pub const STREAM_INT_CONSTANTS: &[(&str, i64)] = &[
    // Client / server connection flags. php-src orders the client bits
    // PERSISTENT, ASYNC_CONNECT, CONNECT (ext/standard/file.stub.php:126-140,
    // measured: `php -n -r 'var_dump(STREAM_CLIENT_CONNECT);'` = 4).
    ("STREAM_CLIENT_PERSISTENT", 1),
    ("STREAM_CLIENT_ASYNC_CONNECT", 2),
    ("STREAM_CLIENT_CONNECT", 4),
    ("STREAM_SERVER_BIND", STREAM_SERVER_BIND),
    ("STREAM_SERVER_LISTEN", STREAM_SERVER_LISTEN),
    // Shutdown directions for stream_socket_shutdown().
    ("STREAM_SHUT_RD", 0),
    ("STREAM_SHUT_WR", 1),
    ("STREAM_SHUT_RDWR", 2),
    // Out-of-band / peek flags for stream_socket_recvfrom().
    ("STREAM_OOB", 1),
    ("STREAM_PEEK", 2),
    // Stream filter chain direction.
    ("STREAM_FILTER_READ", 1),
    ("STREAM_FILTER_WRITE", 2),
    ("STREAM_FILTER_ALL", 3),
    // TLS crypto methods (client side).
    ("STREAM_CRYPTO_METHOD_SSLv2_CLIENT", 3),
    ("STREAM_CRYPTO_METHOD_SSLv3_CLIENT", 5),
    ("STREAM_CRYPTO_METHOD_SSLv23_CLIENT", 57),
    ("STREAM_CRYPTO_METHOD_TLS_CLIENT", 121),
    ("STREAM_CRYPTO_METHOD_TLSv1_0_CLIENT", 9),
    ("STREAM_CRYPTO_METHOD_TLSv1_1_CLIENT", 17),
    ("STREAM_CRYPTO_METHOD_TLSv1_2_CLIENT", 33),
    ("STREAM_CRYPTO_METHOD_TLSv1_3_CLIENT", 65),
    ("STREAM_CRYPTO_METHOD_ANY_CLIENT", 127),
    // TLS crypto methods (server side).
    ("STREAM_CRYPTO_METHOD_SSLv2_SERVER", 2),
    ("STREAM_CRYPTO_METHOD_SSLv3_SERVER", 4),
    ("STREAM_CRYPTO_METHOD_SSLv23_SERVER", 120),
    ("STREAM_CRYPTO_METHOD_TLS_SERVER", 120),
    ("STREAM_CRYPTO_METHOD_TLSv1_0_SERVER", 8),
    ("STREAM_CRYPTO_METHOD_TLSv1_1_SERVER", 16),
    ("STREAM_CRYPTO_METHOD_TLSv1_2_SERVER", 32),
    ("STREAM_CRYPTO_METHOD_TLSv1_3_SERVER", 64),
    ("STREAM_CRYPTO_METHOD_ANY_SERVER", 126),
    // TLS crypto protocol aliases.
    ("STREAM_CRYPTO_PROTO_SSLv3", 4),
    ("STREAM_CRYPTO_PROTO_TLSv1_0", 8),
    ("STREAM_CRYPTO_PROTO_TLSv1_1", 16),
    ("STREAM_CRYPTO_PROTO_TLSv1_2", 32),
    ("STREAM_CRYPTO_PROTO_TLSv1_3", 64),
    // Socket-pair domain / type / protocol (target-invariant values only;
    // STREAM_PF_INET6 is target-divergent and registered with the socket layer).
    ("STREAM_PF_INET", 2),
    ("STREAM_PF_UNIX", 1),
    ("STREAM_SOCK_STREAM", 1),
    ("STREAM_SOCK_DGRAM", 2),
    ("STREAM_SOCK_RAW", 3),
    ("STREAM_SOCK_RDM", 4),
    ("STREAM_SOCK_SEQPACKET", 5),
    ("STREAM_IPPROTO_IP", 0),
    ("STREAM_IPPROTO_TCP", 6),
    ("STREAM_IPPROTO_UDP", 17),
    ("STREAM_IPPROTO_ICMP", 1),
    ("STREAM_IPPROTO_RAW", 255),
    // Notification codes / severities for stream context notifiers.
    ("STREAM_NOTIFY_RESOLVE", 1),
    ("STREAM_NOTIFY_CONNECT", 2),
    ("STREAM_NOTIFY_AUTH_REQUIRED", 3),
    ("STREAM_NOTIFY_MIME_TYPE_IS", 4),
    ("STREAM_NOTIFY_FILE_SIZE_IS", 5),
    ("STREAM_NOTIFY_REDIRECTED", 6),
    ("STREAM_NOTIFY_PROGRESS", 7),
    ("STREAM_NOTIFY_COMPLETED", 8),
    ("STREAM_NOTIFY_FAILURE", 9),
    ("STREAM_NOTIFY_AUTH_RESULT", 10),
    ("STREAM_NOTIFY_SEVERITY_INFO", 0),
    ("STREAM_NOTIFY_SEVERITY_WARN", 1),
    ("STREAM_NOTIFY_SEVERITY_ERR", 2),
    // Wrapper / cast / meta / option / buffer / URL-stat flags.
    ("STREAM_IS_URL", 1),
    ("STREAM_USE_PATH", 1),
    ("STREAM_REPORT_ERRORS", 8),
    ("STREAM_CAST_FOR_SELECT", 3),
    ("STREAM_CAST_AS_STREAM", 0),
    ("STREAM_META_TOUCH", 1),
    ("STREAM_META_OWNER_NAME", 2),
    ("STREAM_META_OWNER", 3),
    ("STREAM_META_GROUP_NAME", 4),
    ("STREAM_META_GROUP", 5),
    ("STREAM_META_ACCESS", 6),
    ("STREAM_MKDIR_RECURSIVE", 1),
    ("STREAM_OPTION_BLOCKING", 1),
    ("STREAM_OPTION_READ_BUFFER", 2),
    ("STREAM_OPTION_WRITE_BUFFER", 3),
    ("STREAM_OPTION_READ_TIMEOUT", 4),
    ("STREAM_BUFFER_NONE", 0),
    ("STREAM_BUFFER_LINE", 1),
    ("STREAM_BUFFER_FULL", 2),
    ("STREAM_URL_STAT_LINK", 1),
    ("STREAM_URL_STAT_QUIET", 2),
    ("STREAM_MUST_SEEK", 16),
    ("STREAM_IGNORE_URL", 2),
    // User stream-filter return values / flags.
    ("PSFS_ERR_FATAL", 0),
    ("PSFS_FEED_ME", 1),
    ("PSFS_PASS_ON", 2),
    ("PSFS_FLAG_NORMAL", 0),
    ("PSFS_FLAG_FLUSH_INC", 1),
    ("PSFS_FLAG_FLUSH_CLOSE", 2),
    // file() / file_put_contents() flags.
    ("FILE_USE_INCLUDE_PATH", 1),
    ("FILE_IGNORE_NEW_LINES", 2),
    ("FILE_SKIP_EMPTY_LINES", 4),
    ("FILE_APPEND", 8),
    ("FILE_NO_DEFAULT_CONTEXT", 16),
    // scandir() sorting orders.
    ("SCANDIR_SORT_ASCENDING", 0),
    ("SCANDIR_SORT_DESCENDING", 1),
    ("SCANDIR_SORT_NONE", 2),
    // seek() whence constants (also used by fseek).
    ("SEEK_SET", 0),
    ("SEEK_CUR", 1),
    ("SEEK_END", 2),
    // STREAM_FROM_START / _CUR / _END are php-src *internal* whence names, not
    // PHP constants: `php -n -r 'var_dump(defined("STREAM_FROM_START"));'` is
    // false. Declaring them let a program name a constant php does not have and
    // still compile. Same for STREAM_META_MODIFIED and STREAM_OPTION_CHUNK_SIZE.
    // glob() flags. These are php's OWN numbers, not the host libc's: php 8.5 ships its own
    // glob, so the values are identical on every target. Measured across the three per-target
    // oracle manifests — including `GLOB_ONLYDIR = 1 << 30` on Linux, where glibc's own
    // `GLOB_ONLYDIR` is `1 << 13`. `__rt_glob` translates them to the platform's libc bits;
    // handing them to libc unchanged would select GLOB_LIMIT for GLOB_NOESCAPE on macOS.
    ("GLOB_ERR", crate::glob_flags::GLOB_ERR),
    ("GLOB_MARK", crate::glob_flags::GLOB_MARK),
    ("GLOB_NOCHECK", crate::glob_flags::GLOB_NOCHECK),
    ("GLOB_NOSORT", crate::glob_flags::GLOB_NOSORT),
    ("GLOB_BRACE", crate::glob_flags::GLOB_BRACE),
    ("GLOB_NOESCAPE", crate::glob_flags::GLOB_NOESCAPE),
    ("GLOB_ONLYDIR", crate::glob_flags::GLOB_ONLYDIR),
    // The OR of the seven above, which is what php validates `$flags` against.
    (
        "GLOB_AVAILABLE_FLAGS",
        crate::glob_flags::GLOB_AVAILABLE_FLAGS,
    ),
];

/// Tuple of `(name, value)` pairs for PHP math integer constants.
///
/// `round()` uses these constants to select how exact `.5` ties are broken.
pub const MATH_INT_CONSTANTS: &[(&str, i64)] = &[
    ("PHP_ROUND_HALF_UP", 1),
    ("PHP_ROUND_HALF_DOWN", 2),
    ("PHP_ROUND_HALF_EVEN", 3),
    ("PHP_ROUND_HALF_ODD", 4),
];

/// Tuple of `(name, value)` pairs for every PHP `E_*` error-level constant.
///
/// Values mirror Zend's `zend_errors.h` severity bitmask so programs can build
/// and compare `error_reporting()` masks and pass levels to `trigger_error()`.
pub const ERROR_LEVEL_CONSTANTS: &[(&str, i64)] = &[
    ("E_ERROR", 1),
    ("E_WARNING", 2),
    ("E_PARSE", 4),
    ("E_NOTICE", 8),
    ("E_CORE_ERROR", 16),
    ("E_CORE_WARNING", 32),
    ("E_COMPILE_ERROR", 64),
    ("E_COMPILE_WARNING", 128),
    ("E_USER_ERROR", 256),
    ("E_USER_WARNING", 512),
    ("E_USER_NOTICE", 1024),
    ("E_STRICT", 2048),
    ("E_RECOVERABLE_ERROR", 4096),
    ("E_DEPRECATED", 8192),
    ("E_USER_DEPRECATED", 16384),
    // 30719, not 32767: php removed `E_STRICT`'s bit from the mask in 8.4. Measured on
    // `php -n` 8.5.6, where `E_ALL & E_STRICT` is 0 and naming `E_STRICT` is deprecated.
    ("E_ALL", 30719),
];

/// PHP `ENT_*` integer constants consumed by `htmlspecialchars()`/`htmlentities()` flags.
pub const ENT_INT_CONSTANTS: &[(&str, i64)] = &[
    ("ENT_QUOTES", 3),
    ("ENT_COMPAT", 2),
    ("ENT_NOQUOTES", 0),
    ("ENT_HTML401", 0),
    ("ENT_HTML5", 48),
    ("ENT_XHTML", 32),
    ("ENT_XML1", 16),
    ("ENT_SUBSTITUTE", 8),
    ("ENT_IGNORE", 4),
];

/// Tuple of `(name, value)` pairs for ext-zlib's encoding constants.
pub const ZLIB_INT_CONSTANTS: &[(&str, i64)] = &[
    ("ZLIB_ENCODING_RAW", -15),
    ("ZLIB_ENCODING_DEFLATE", 15),
    ("ZLIB_ENCODING_GZIP", 31),
];

/// The capability flags `ob_start()` gives a buffer when the caller names none.
///
/// `PHP_OUTPUT_HANDLER_CLEANABLE | FLUSHABLE | REMOVABLE`. The runtime writes this number into
/// every buffer it creates, so it lives beside the table that publishes it under php's name rather
/// than as a literal in one emitter and a name in another.
pub const OUTPUT_HANDLER_STDFLAGS: i64 = 16 | 32 | 64;

/// Tuple of `(name, value)` pairs for the output-buffering handler constants.
///
/// MEASURED on `php -n` 8.5.6 through `get_defined_constants()`. The first seven are PHASE bits a
/// handler receives in its second argument; the next four are the CAPABILITY flags `ob_start()`
/// takes in its third, with `STDFLAGS` the union of the three php defaults; the last three are
/// STATUS bits `ob_get_status()` reports. `CONT` and `END` are php's own aliases for `WRITE` and
/// `FINAL`, so fourteen names carry eleven distinct values.
pub const OUTPUT_HANDLER_INT_CONSTANTS: &[(&str, i64)] = &[
    ("PHP_OUTPUT_HANDLER_START", 1),
    ("PHP_OUTPUT_HANDLER_WRITE", 0),
    ("PHP_OUTPUT_HANDLER_FLUSH", 4),
    ("PHP_OUTPUT_HANDLER_CLEAN", 2),
    ("PHP_OUTPUT_HANDLER_FINAL", 8),
    ("PHP_OUTPUT_HANDLER_CONT", 0),
    ("PHP_OUTPUT_HANDLER_END", 8),
    ("PHP_OUTPUT_HANDLER_CLEANABLE", 16),
    ("PHP_OUTPUT_HANDLER_FLUSHABLE", 32),
    ("PHP_OUTPUT_HANDLER_REMOVABLE", 64),
    ("PHP_OUTPUT_HANDLER_STDFLAGS", OUTPUT_HANDLER_STDFLAGS),
    ("PHP_OUTPUT_HANDLER_STARTED", 4096),
    ("PHP_OUTPUT_HANDLER_DISABLED", 8192),
    ("PHP_OUTPUT_HANDLER_PROCESSED", 16384),
];

/// Tuple of `(name, value)` pairs for every `ext/json` integer constant.
///
/// Example entries: `("JSON_HEX_TAG", 1)`, `("JSON_ERROR_NONE", 0)`.
pub const JSON_INT_CONSTANTS: &[(&str, i64)] = &[
    // Encoding flags (bitmask passed to json_encode).
    ("JSON_HEX_TAG", 1),
    ("JSON_HEX_AMP", 2),
    ("JSON_HEX_APOS", 4),
    ("JSON_HEX_QUOT", 8),
    ("JSON_FORCE_OBJECT", 16),
    ("JSON_NUMERIC_CHECK", 32),
    ("JSON_UNESCAPED_SLASHES", 64),
    ("JSON_PRETTY_PRINT", 128),
    ("JSON_UNESCAPED_UNICODE", 256),
    ("JSON_PARTIAL_OUTPUT_ON_ERROR", 512),
    ("JSON_PRESERVE_ZERO_FRACTION", 1024),
    ("JSON_INVALID_UTF8_IGNORE", 1_048_576),
    ("JSON_INVALID_UTF8_SUBSTITUTE", 2_097_152),
    ("JSON_THROW_ON_ERROR", 4_194_304),
    // Decoding flags (bitmask passed to json_decode / json_validate).
    ("JSON_OBJECT_AS_ARRAY", 1),
    ("JSON_BIGINT_AS_STRING", 2),
    // Error codes returned by json_last_error().
    ("JSON_ERROR_NONE", 0),
    ("JSON_ERROR_DEPTH", 1),
    ("JSON_ERROR_STATE_MISMATCH", 2),
    ("JSON_ERROR_CTRL_CHAR", 3),
    ("JSON_ERROR_SYNTAX", 4),
    ("JSON_ERROR_UTF8", 5),
    ("JSON_ERROR_RECURSION", 6),
    ("JSON_ERROR_INF_OR_NAN", 7),
    ("JSON_ERROR_UNSUPPORTED_TYPE", 8),
    ("JSON_ERROR_INVALID_PROPERTY_NAME", 9),
    ("JSON_ERROR_UTF16", 10),
];

/// Tuple of `(name, value)` pairs for every `ext/session` integer constant.
///
/// Entries: `("PHP_SESSION_DISABLED", 0)`, `("PHP_SESSION_NONE", 1)`,
/// `("PHP_SESSION_ACTIVE", 2)`.
pub const SESSION_INT_CONSTANTS: &[(&str, i64)] = &[
    ("PHP_SESSION_DISABLED", 0),
    ("PHP_SESSION_NONE", 1),
    ("PHP_SESSION_ACTIVE", 2),
];

/// Tuple of `(name, value)` pairs for every `ext/date` integer constant.
///
/// The `SUNFUNCS_RET_*` constants are the `$returnFormat` selector passed to
/// `date_sunrise()` / `date_sunset()`: `TIMESTAMP` (0) yields a Unix timestamp,
/// `STRING` (1, the default) an `"HH:MM"` string, and `DOUBLE` (2) the hour of the
/// day as a float. They are deprecated in PHP 8.1 alongside the functions themselves.
pub const DATE_INT_CONSTANTS: &[(&str, i64)] = &[
    ("SUNFUNCS_RET_TIMESTAMP", 0),
    ("SUNFUNCS_RET_STRING", 1),
    ("SUNFUNCS_RET_DOUBLE", 2),
    // ext/calendar: calendar selectors for cal_to_jd()/cal_from_jd()/cal_days_in_month()/cal_info().
    ("CAL_GREGORIAN", 0),
    ("CAL_JULIAN", 1),
    ("CAL_JEWISH", 2),
    ("CAL_FRENCH", 3),
    ("CAL_NUM_CALS", 4),
    // jddayofweek() $mode selectors.
    ("CAL_DOW_DAYNO", 0),
    ("CAL_DOW_LONG", 1),
    ("CAL_DOW_SHORT", 2),
    // jdmonthname() $mode selectors.
    ("CAL_MONTH_GREGORIAN_SHORT", 0),
    ("CAL_MONTH_GREGORIAN_LONG", 1),
    ("CAL_MONTH_JULIAN_SHORT", 2),
    ("CAL_MONTH_JULIAN_LONG", 3),
    ("CAL_MONTH_JEWISH", 4),
    ("CAL_MONTH_FRENCH", 5),
    // easter_date()/easter_days() $mode selectors.
    ("CAL_EASTER_DEFAULT", 0),
    ("CAL_EASTER_ROMAN", 1),
    ("CAL_EASTER_ALWAYS_GREGORIAN", 2),
    ("CAL_EASTER_ALWAYS_JULIAN", 3),
    // jdtojewish() Hebrew-formatting flags (accepted; Hebrew output is not produced).
    ("CAL_JEWISH_ADD_ALAFIM_GERESH", 2),
    ("CAL_JEWISH_ADD_ALAFIM", 4),
    ("CAL_JEWISH_ADD_GERESHAYIM", 8),
];

/// Tuple of `(name, value)` pairs for PHP array integer constants.
///
/// `array_filter()` uses the `ARRAY_FILTER_*` constants to select which callback arguments
/// are passed; `count()` uses the `COUNT_*` constants to select flat or recursive counting.
pub const ARRAY_INT_CONSTANTS: &[(&str, i64)] = &[
    // `ARRAY_FILTER_USE_VALUE` is deliberately absent: php does not define it. Measured on
    // `php -n` 8.5.6, `defined("ARRAY_FILTER_USE_VALUE")` is false while the other two are true.
    // Mode 0 is still the value mode — php's own ValueError message names all three — but a
    // program that spells the constant fatals under php, so it must not compile here.
    ("ARRAY_FILTER_USE_BOTH", 1),
    ("ARRAY_FILTER_USE_KEY", 2),
    ("COUNT_NORMAL", 0),
    ("COUNT_RECURSIVE", 1),
];

/// Tuple of `(name, value)` pairs for PHP string integer constants.
///
/// `str_pad()` uses these constants to select which side of the input is padded.
pub const STRING_INT_CONSTANTS: &[(&str, i64)] = &[
    ("STR_PAD_LEFT", 0),
    ("STR_PAD_RIGHT", 1),
    ("STR_PAD_BOTH", 2),
];

/// Tuple of `(name, value)` pairs for the supported OpenSSL cipher flags.
pub const OPENSSL_INT_CONSTANTS: &[(&str, i64)] = &[
    ("OPENSSL_RAW_DATA", 1),
    ("OPENSSL_ZERO_PADDING", 2),
    ("OPENSSL_DONT_ZERO_PAD_KEY", 4),
];

/// PHP preg integer constants used by regex builtins and SPL regex iterators.
pub const PREG_INT_CONSTANTS: &[(&str, i64)] = &[
    ("PREG_PATTERN_ORDER", 1),
    ("PREG_SET_ORDER", 2),
    ("PREG_OFFSET_CAPTURE", 256),
    ("PREG_UNMATCHED_AS_NULL", 512),
    ("PREG_SPLIT_NO_EMPTY", 1),
    ("PREG_SPLIT_DELIM_CAPTURE", 2),
    ("PREG_SPLIT_OFFSET_CAPTURE", 4),
];

/// Every table above, for a caller that needs the whole predefined-constant surface.
///
/// `eval()` resolves through this, so a table added here reaches both engines at once.
pub const ALL_INT_CONSTANT_TABLES: &[&[(&str, i64)]] = &[
    STREAM_INT_CONSTANTS,
    MATH_INT_CONSTANTS,
    ERROR_LEVEL_CONSTANTS,
    ENT_INT_CONSTANTS,
    ZLIB_INT_CONSTANTS,
    OUTPUT_HANDLER_INT_CONSTANTS,
    JSON_INT_CONSTANTS,
    SESSION_INT_CONSTANTS,
    DATE_INT_CONSTANTS,
    ARRAY_INT_CONSTANTS,
    STRING_INT_CONSTANTS,
    OPENSSL_INT_CONSTANTS,
    PREG_INT_CONSTANTS,
];

/// Looks one predefined integer constant up by the name php publishes it under.
pub fn int_constant(name: &str) -> Option<i64> {
    ALL_INT_CONSTANT_TABLES
        .iter()
        .flat_map(|table| table.iter())
        .find(|(declared, _)| *declared == name)
        .map(|(_, value)| *value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// No constant is declared twice, and never with two different values.
    ///
    /// The tables were separate files before they were one module, so a name could appear in two
    /// of them without anything noticing — and `int_constant` would then answer whichever came
    /// first, which is not a property anyone should have to know.
    fn every_name_is_declared_once() {
        let mut seen: Vec<(&str, i64)> = Vec::new();
        for table in ALL_INT_CONSTANT_TABLES {
            for (name, value) in table.iter() {
                if let Some((_, previous)) = seen.iter().find(|(declared, _)| declared == name) {
                    panic!("{name} is declared twice, as {previous} and as {value}");
                }
                seen.push((name, *value));
            }
        }
        assert!(!seen.is_empty());
    }

    #[test]
    /// The lookup answers what the tables hold.
    fn int_constant_reads_the_tables() {
        assert_eq!(int_constant("SEEK_SET"), Some(0));
        assert_eq!(int_constant("STREAM_FILTER_READ"), Some(1));
        assert_eq!(int_constant("GLOB_MARK"), Some(8));
        assert_eq!(int_constant("NOT_A_PHP_CONSTANT"), None);
    }
}
