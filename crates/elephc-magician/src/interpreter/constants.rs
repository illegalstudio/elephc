//! Purpose:
//! Defines eval-local PHP compatibility constants and static lookup tables.
//! Builtin modules read these tables to mirror native elephc behavior for dynamic eval.
//!
//! Called from:
//! - `crate::interpreter::builtins` domain modules.
//! - `crate::interpreter` constant and JSON helpers.
//!
//! Key details:
//! - Values here are PHP-visible compatibility data; changing them changes eval semantics.

use std::sync::atomic::AtomicU64;

/// Requests raw binary output from the OpenSSL compatibility builtins.
pub(super) const EVAL_OPENSSL_RAW_DATA: i64 = 1;

/// Disables block-cipher padding in the OpenSSL compatibility builtins.
pub(super) const EVAL_OPENSSL_ZERO_PADDING: i64 = 2;

/// Prevents zero-padding short cipher keys in the OpenSSL compatibility builtins.
pub(super) const EVAL_OPENSSL_DONT_ZERO_PAD_KEY: i64 = 4;

/// Rejects encoded-words that RFC 2047 would not allow at that position.
pub(super) const EVAL_ICONV_MIME_DECODE_STRICT: i64 = 1;

/// Keeps undecodable MIME text verbatim instead of failing the whole call.
pub(super) const EVAL_ICONV_MIME_DECODE_CONTINUE_ON_ERROR: i64 = 2;

/// `parse_url()` component selector for the scheme.
pub(super) const EVAL_PHP_URL_SCHEME: i64 = 0;
/// `parse_url()` component selector for the host.
pub(super) const EVAL_PHP_URL_HOST: i64 = 1;
/// `parse_url()` component selector for the port.
pub(super) const EVAL_PHP_URL_PORT: i64 = 2;
/// `parse_url()` component selector for the user name.
pub(super) const EVAL_PHP_URL_USER: i64 = 3;
/// `parse_url()` component selector for the password.
pub(super) const EVAL_PHP_URL_PASS: i64 = 4;
/// `parse_url()` component selector for the path.
pub(super) const EVAL_PHP_URL_PATH: i64 = 5;
/// `parse_url()` component selector for the query.
pub(super) const EVAL_PHP_URL_QUERY: i64 = 6;
/// `parse_url()` component selector for the fragment.
pub(super) const EVAL_PHP_URL_FRAGMENT: i64 = 7;

/// Hash algorithm names supported by eval `hash_algos()`, matching native runtime order.
pub(super) const EVAL_HASH_ALGOS: &[&str] = &[
    "md2",
    "md4",
    "md5",
    "sha1",
    "sha224",
    "sha256",
    "sha384",
    "sha512",
    "sha512/224",
    "sha512/256",
    "sha3-224",
    "sha3-256",
    "sha3-384",
    "sha3-512",
    "ripemd128",
    "ripemd160",
    "ripemd256",
    "ripemd320",
    "whirlpool",
    "crc32",
    "crc32b",
    "crc32c",
    "adler32",
    "fnv132",
    "fnv1a32",
    "fnv164",
    "fnv1a64",
    "joaat",
];

/// Built-in stream wrappers reported by eval `stream_get_wrappers()`.
pub(super) const EVAL_STREAM_WRAPPERS: &[&str] = &[
    "https",
    "ftps",
    "compress.zlib",
    "compress.bzip2",
    "php",
    "file",
    "glob",
    "data",
    "http",
    "ftp",
    "phar",
];

/// Built-in stream transports reported by eval `stream_get_transports()`.
pub(super) const EVAL_STREAM_TRANSPORTS: &[&str] = &[
    "tcp", "udp", "unix", "udg", "tls", "ssl", "sslv2", "sslv3", "tlsv1.0", "tlsv1.1", "tlsv1.2",
    "tlsv1.3",
];

/// Monotonic salt mixed into eval `rand()`/`mt_rand()` and array key sampling.
pub(super) static EVAL_RANDOM_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Built-in stream filters reported by eval `stream_get_filters()`.
pub(super) const EVAL_STREAM_FILTERS: &[&str] = &[
    "zlib.*",
    "bzip2.*",
    "convert.iconv.*",
    "string.rot13",
    "string.toupper",
    "string.tolower",
    "convert.*",
    "consumed",
    "dechunk",
];

/// SPL/core type names reported by eval `spl_classes()`.
///
/// Mirrors `src/codegen/builtins/spl/mod.rs::SPL_CLASS_NAMES` so dynamic eval
/// exposes the same static registry snapshot as native code.
pub(super) const EVAL_SPL_CLASS_NAMES: &[&str] = &[
    "AppendIterator",
    "ArrayAccess",
    "ArrayIterator",
    "ArrayObject",
    "BadFunctionCallException",
    "BadMethodCallException",
    "CachingIterator",
    "CallbackFilterIterator",
    "Countable",
    "DomainException",
    "DirectoryIterator",
    "EmptyIterator",
    "Error",
    "Exception",
    "FilterIterator",
    "FilesystemIterator",
    "GlobIterator",
    "InfiniteIterator",
    "InvalidArgumentException",
    "Iterator",
    "IteratorAggregate",
    "IteratorIterator",
    "JsonSerializable",
    "LengthException",
    "LimitIterator",
    "LogicException",
    "MultipleIterator",
    "NoRewindIterator",
    "OuterIterator",
    "OutOfBoundsException",
    "OutOfRangeException",
    "OverflowException",
    "ParentIterator",
    "RangeException",
    "RecursiveArrayIterator",
    "RecursiveCachingIterator",
    "RecursiveCallbackFilterIterator",
    "RecursiveDirectoryIterator",
    "RecursiveFilterIterator",
    "RecursiveIterator",
    "RecursiveIteratorIterator",
    "RecursiveRegexIterator",
    "RegexIterator",
    "RuntimeException",
    "SeekableIterator",
    "SplDoublyLinkedList",
    "SplFixedArray",
    "SplFileInfo",
    "SplFileObject",
    "SplObserver",
    "SplQueue",
    "SplStack",
    "SplSubject",
    "SplTempFileObject",
    "Stringable",
    "Throwable",
    "Traversable",
    "TypeError",
    "UnderflowException",
    "UnexpectedValueException",
    "ValueError",
];

/// Full English month names used by eval `date()`.
pub(super) const EVAL_MONTH_NAMES: &[&str; 12] = &[
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// Short English month names used by eval `date()`.
pub(super) const EVAL_MONTH_SHORT_NAMES: &[&str; 12] = &[
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Full English weekday names used by eval `date()`.
pub(super) const EVAL_WEEKDAY_NAMES: &[&str; 7] = &[
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];

/// Short English weekday names used by eval `date()`.
pub(super) const EVAL_WEEKDAY_SHORT_NAMES: &[&str; 7] =
    &["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/// `PHP_MAJOR_VERSION` — invariant across every profile elephc supports, so unlike
/// `PHP_VERSION` / `PHP_VERSION_ID` / `PHP_MINOR_VERSION` it needs no lookup through
/// [`crate::eval_php_profile`]: 8.2 through 8.5 all report `8`.
pub(super) const EVAL_PHP_MAJOR_VERSION: i64 = 8;

/// `PHP_RELEASE_VERSION` — always `0`, and therefore invariant across profiles: elephc
/// targets a language profile, not an upstream patch release, so there is no engine build
/// whose patch component could differ. Reference PHP 8.5.6 reports `6`.
pub(super) const EVAL_PHP_RELEASE_VERSION: i64 = 0;

/// `PHP_EXTRA_VERSION` — the empty string, exactly as reference PHP reports for a release
/// build (verified on 8.5.6), and invariant across profiles for the same reason
/// [`EVAL_PHP_RELEASE_VERSION`] is.
pub(super) const EVAL_PHP_EXTRA_VERSION: &str = "";

/// `PHP_SAPI` reported from inside `eval()`.
///
/// KEEP IN SYNC with `crate::web_prelude::sapi_name()` in the compiler. Unlike the version
/// surface — which the compiler forwards through
/// [`crate::eval_php_profile::set_eval_php_version_id`] — nothing forwards `--web`, so this
/// stays the CLI default, the same choice `opcache_reset` makes with `is_web_sapi = false`.
/// DOCUMENTED DIVERGENCE: inside a `--web` binary, native `PHP_SAPI` is `cli-server` while
/// `eval('echo PHP_SAPI;')` reports `cli`. Closing it would take the same one-call bridge the
/// version surface uses.
pub(super) const EVAL_PHP_SAPI: &str = "cli";

pub(super) const DEFINE_ALREADY_DEFINED_WARNING: &str =
    "Warning: define(): Constant already defined\n";
pub(super) const HEX2BIN_ODD_LENGTH_WARNING: &str =
    "Warning: hex2bin(): Hexadecimal input string must have an even length\n";
pub(super) const HEX2BIN_INVALID_WARNING: &str =
    "Warning: hex2bin(): Input string must be hexadecimal string\n";
pub(super) const EVAL_PATHINFO_DIRNAME: i64 = 1;
pub(super) const EVAL_PATHINFO_BASENAME: i64 = 2;
pub(super) const EVAL_PATHINFO_EXTENSION: i64 = 4;
pub(super) const EVAL_PATHINFO_FILENAME: i64 = 8;
pub(super) const EVAL_PATHINFO_ALL: i64 = 15;
pub(super) const EVAL_FNM_NOESCAPE: i64 = 1;
pub(super) const EVAL_FNM_PATHNAME: i64 = 2;
pub(super) const EVAL_FNM_PERIOD: i64 = 4;
pub(super) const EVAL_FNM_CASEFOLD: i64 = 16;
pub(super) const EVAL_LOCK_SH: i64 = 1;
pub(super) const EVAL_LOCK_EX: i64 = 2;
pub(super) const EVAL_LOCK_UN: i64 = 3;
pub(super) const EVAL_LOCK_NB: i64 = 4;
pub(super) const EVAL_ARRAY_FILTER_USE_VALUE: i64 = 0;
pub(super) const EVAL_ARRAY_FILTER_USE_BOTH: i64 = 1;
pub(super) const EVAL_ARRAY_FILTER_USE_KEY: i64 = 2;
/// `str_pad()` pads on the left of the input.
pub(super) const EVAL_STR_PAD_LEFT: i64 = 0;
/// `str_pad()` pads on the right of the input, which is PHP's default.
pub(super) const EVAL_STR_PAD_RIGHT: i64 = 1;
/// `str_pad()` splits the padding across both sides of the input.
pub(super) const EVAL_STR_PAD_BOTH: i64 = 2;
pub(super) const EVAL_COUNT_NORMAL: i64 = 0;
pub(super) const EVAL_COUNT_RECURSIVE: i64 = 1;
/// `round()` breaks exact `.5` ties away from zero, which is PHP's default.
pub(super) const EVAL_PHP_ROUND_HALF_UP: i64 = 1;
/// `round()` breaks exact `.5` ties toward zero.
pub(super) const EVAL_PHP_ROUND_HALF_DOWN: i64 = 2;
/// `round()` breaks exact `.5` ties toward the nearest even digit.
pub(super) const EVAL_PHP_ROUND_HALF_EVEN: i64 = 3;
/// `round()` breaks exact `.5` ties toward the nearest odd digit.
pub(super) const EVAL_PHP_ROUND_HALF_ODD: i64 = 4;
pub(super) const EVAL_PREG_SPLIT_NO_EMPTY: i64 = 1;
pub(super) const EVAL_PREG_SPLIT_DELIM_CAPTURE: i64 = 2;
pub(super) const EVAL_PREG_SPLIT_OFFSET_CAPTURE: i64 = 4;
pub(super) const EVAL_PREG_PATTERN_ORDER: i64 = 1;
pub(super) const EVAL_PREG_SET_ORDER: i64 = 2;
pub(super) const EVAL_PREG_OFFSET_CAPTURE: i64 = 256;
pub(super) const EVAL_PREG_UNMATCHED_AS_NULL: i64 = 512;
pub(super) const EVAL_JSON_ERROR_NONE: i64 = 0;
pub(super) const EVAL_JSON_ERROR_DEPTH: i64 = 1;
pub(super) const EVAL_JSON_ERROR_STATE_MISMATCH: i64 = 2;
pub(super) const EVAL_JSON_ERROR_CTRL_CHAR: i64 = 3;
pub(super) const EVAL_JSON_ERROR_SYNTAX: i64 = 4;
pub(super) const EVAL_JSON_ERROR_UTF8: i64 = 5;
pub(super) const EVAL_JSON_ERROR_RECURSION: i64 = 6;
pub(super) const EVAL_JSON_ERROR_INF_OR_NAN: i64 = 7;
pub(super) const EVAL_JSON_ERROR_UNSUPPORTED_TYPE: i64 = 8;
pub(super) const EVAL_JSON_ERROR_INVALID_PROPERTY_NAME: i64 = 9;
pub(super) const EVAL_JSON_ERROR_UTF16: i64 = 10;
pub(super) const EVAL_JSON_HEX_TAG: i64 = 1;
pub(super) const EVAL_JSON_HEX_AMP: i64 = 2;
pub(super) const EVAL_JSON_HEX_APOS: i64 = 4;
pub(super) const EVAL_JSON_HEX_QUOT: i64 = 8;
pub(super) const EVAL_JSON_BIGINT_AS_STRING: i64 = 2;
pub(super) const EVAL_JSON_FORCE_OBJECT: i64 = 16;
pub(super) const EVAL_JSON_NUMERIC_CHECK: i64 = 32;
pub(super) const EVAL_JSON_UNESCAPED_SLASHES: i64 = 64;
pub(super) const EVAL_JSON_PRETTY_PRINT: i64 = 128;
pub(super) const EVAL_JSON_UNESCAPED_UNICODE: i64 = 256;
pub(super) const EVAL_JSON_PARTIAL_OUTPUT_ON_ERROR: i64 = 512;
pub(super) const EVAL_JSON_PRESERVE_ZERO_FRACTION: i64 = 1024;
pub(super) const EVAL_JSON_INVALID_UTF8_IGNORE: i64 = 1_048_576;
pub(super) const EVAL_JSON_INVALID_UTF8_SUBSTITUTE: i64 = 2_097_152;
pub(super) const EVAL_JSON_THROW_ON_ERROR: i64 = 4_194_304;
pub(super) const EVAL_JSON_INF_OR_NAN_MESSAGE: &str = "Inf and NaN cannot be JSON encoded";
pub(super) const EVAL_JSON_UTF8_MESSAGE: &str =
    "Malformed UTF-8 characters, possibly incorrectly encoded";
