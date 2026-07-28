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
    "file",
    "php",
    "data",
    "ftp",
    "http",
    "https",
    "ftps",
    "compress.zlib",
    "compress.bzip2",
    "phar",
    "glob",
];

/// Built-in stream transports reported by eval `stream_get_transports()`.
#[cfg(not(windows))]
pub(super) const EVAL_STREAM_TRANSPORTS: &[&str] = &[
    "tcp", "udp", "unix", "udg", "tls", "ssl", "tlsv1.0", "tlsv1.1", "tlsv1.2", "tlsv1.3",
];

/// Built-in stream transports reported by eval `stream_get_transports()` on Windows.
#[cfg(windows)]
pub(super) const EVAL_STREAM_TRANSPORTS: &[&str] = &[
    "tcp", "udp", "tls", "ssl", "tlsv1.0", "tlsv1.1", "tlsv1.2", "tlsv1.3",
];

/// Monotonic salt mixed into eval `rand()`/`mt_rand()` and array key sampling.
pub(super) static EVAL_RANDOM_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Built-in stream filters reported by eval `stream_get_filters()`.
pub(super) const EVAL_STREAM_FILTERS: &[&str] = &[
    "string.toupper",
    "string.tolower",
    "string.rot13",
    "string.strip_tags",
    "convert.base64-encode",
    "convert.base64-decode",
    "convert.quoted-printable-encode",
    "convert.quoted-printable-decode",
    "convert.iconv.*",
    "dechunk",
    "zlib.deflate",
    "zlib.inflate",
    "bzip2.compress",
    "bzip2.decompress",
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

/// Root package manifest used to mirror native `phpversion()` in the eval crate.
pub(super) const EVAL_ROOT_CARGO_TOML: &str = include_str!("../../../../Cargo.toml");

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
pub(super) const EVAL_COUNT_NORMAL: i64 = 0;
pub(super) const EVAL_COUNT_RECURSIVE: i64 = 1;
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
