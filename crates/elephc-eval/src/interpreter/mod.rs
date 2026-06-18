//! Purpose:
//! Interprets EvalIR against a materialized caller scope.
//! The interpreter is generic over runtime value operations so it can execute
//! by manipulating opaque elephc runtime-cell handles.
//!
//! Called from:
//! - Future `crate::__elephc_eval_execute()` implementation.
//! - `cargo test -p elephc-eval` for scope/value-flow validation.
//!
//! Key details:
//! - This module does not own PHP values. Constants and operations are delegated
//!   to `RuntimeValueOps`, which will be backed by elephc runtime hooks.

mod builtins;

use crate::context::{ElephcEvalContext, NativeFunction};
use crate::errors::{EvalParseError, EvalStatus};
use crate::eval_ir::{
    EvalArrayElement, EvalBinOp, EvalCallArg, EvalCatch, EvalClass, EvalClassMethod, EvalConst,
    EvalExpr, EvalFunction, EvalMagicConst, EvalMatchArm, EvalProgram, EvalStmt, EvalSwitchCase,
    EvalUnaryOp,
};
use crate::json_validate::{self, JsonParseError, JsonParseErrorKind, JsonValue};
use crate::parser::parse_fragment;
use crate::scope::{ElephcEvalScope, ScopeCellOwnership, ScopeEntry};
use crate::value::RuntimeCellHandle;
use builtins::*;
use regex::bytes::{Captures, Regex, RegexBuilder};
use std::ffi::{CStr, CString};
use std::mem::MaybeUninit;
use std::net::ToSocketAddrs;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Internal statement-control result used to propagate eval returns and loops.
enum EvalControl {
    None,
    Return(RuntimeCellHandle),
    Throw(RuntimeCellHandle),
    Break,
    Continue,
}

/// Final result of executing a parsed eval program.
pub enum EvalOutcome {
    Value(RuntimeCellHandle),
    Throwable(RuntimeCellHandle),
}

/// One already evaluated function-like call argument.
#[derive(Clone)]
struct EvaluatedCallArg {
    name: Option<String>,
    value: RuntimeCellHandle,
}

/// One already evaluated PHP callback supported by the eval dispatcher.
enum EvaluatedCallable {
    Named(String),
    ObjectMethod {
        object: RuntimeCellHandle,
        method: String,
    },
}

/// Bound argument tuple for direct `array_splice()` calls.
type EvalArraySpliceDirectArgs = (
    String,
    RuntimeCellHandle,
    Option<RuntimeCellHandle>,
    Option<RuntimeCellHandle>,
);

/// Parsed flags for one eval `sprintf()` conversion specifier.
#[derive(Clone, Copy)]
struct EvalSprintfSpec {
    left_align: bool,
    force_sign: bool,
    space_sign: bool,
    zero_pad: bool,
    alternate: bool,
    width: Option<usize>,
    precision: Option<usize>,
    specifier: u8,
}

/// Eval-visible predefined constant payloads that are not stored in the dynamic context.
enum EvalPredefinedConstant {
    Int(i64),
    Float(f64),
    String(&'static str),
}

/// Hash algorithm names supported by eval `hash_algos()`, matching native runtime order.
const EVAL_HASH_ALGOS: &[&str] = &[
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
const EVAL_STREAM_WRAPPERS: &[&str] = &[
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
const EVAL_STREAM_TRANSPORTS: &[&str] = &[
    "tcp", "udp", "unix", "udg", "tls", "ssl", "sslv2", "sslv3", "tlsv1.0", "tlsv1.1", "tlsv1.2",
    "tlsv1.3",
];

/// Monotonic salt mixed into eval `rand()`/`mt_rand()` and array key sampling.
static EVAL_RANDOM_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Built-in stream filters reported by eval `stream_get_filters()`.
const EVAL_STREAM_FILTERS: &[&str] = &[
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
const EVAL_SPL_CLASS_NAMES: &[&str] = &[
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
const EVAL_MONTH_NAMES: &[&str; 12] = &[
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
const EVAL_MONTH_SHORT_NAMES: &[&str; 12] = &[
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Full English weekday names used by eval `date()`.
const EVAL_WEEKDAY_NAMES: &[&str; 7] = &[
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];

/// Short English weekday names used by eval `date()`.
const EVAL_WEEKDAY_SHORT_NAMES: &[&str; 7] = &["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/// Root package manifest used to mirror native `phpversion()` in the eval crate.
const EVAL_ROOT_CARGO_TOML: &str = include_str!("../../../../Cargo.toml");

unsafe extern "C" {
    /// Reverse-resolves one socket address through libc's `gethostbyaddr`.
    #[link_name = "gethostbyaddr"]
    fn libc_gethostbyaddr(
        addr: *const libc::c_void,
        len: libc::socklen_t,
        type_: libc::c_int,
    ) -> *mut libc::hostent;

    /// Looks up one IP protocol entry by protocol name or alias.
    #[link_name = "getprotobyname"]
    fn libc_getprotobyname(name: *const libc::c_char) -> *mut libc::protoent;

    /// Looks up one IP protocol entry by protocol number.
    #[link_name = "getprotobynumber"]
    fn libc_getprotobynumber(proto: libc::c_int) -> *mut libc::protoent;

    /// Looks up one internet service entry by service name and protocol.
    #[link_name = "getservbyname"]
    fn libc_getservbyname(
        name: *const libc::c_char,
        proto: *const libc::c_char,
    ) -> *mut libc::servent;

    /// Looks up one internet service entry by port and protocol.
    #[link_name = "getservbyport"]
    fn libc_getservbyport(port: libc::c_int, proto: *const libc::c_char) -> *mut libc::servent;
}

/// Runtime value hooks required by the EvalIR interpreter.
pub trait RuntimeValueOps {
    /// Creates a runtime indexed-array cell with room for at least `capacity` elements.
    fn array_new(&mut self, capacity: usize) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Creates a runtime associative-array cell with room for at least `capacity` elements.
    fn assoc_new(&mut self, capacity: usize) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Reads one element from a runtime Mixed cell using PHP array-read semantics.
    ///
    /// Missing keys and non-array receivers return PHP null, matching the generated
    /// `__rt_mixed_array_get` runtime helper.
    fn array_get(
        &mut self,
        array: RuntimeCellHandle,
        index: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Checks whether a normalized PHP array key exists without conflating null values with misses.
    fn array_key_exists(
        &mut self,
        key: RuntimeCellHandle,
        array: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Returns the foreach-visible key at a zero-based iteration position.
    fn array_iter_key(
        &mut self,
        array: RuntimeCellHandle,
        position: usize,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Writes one element to a runtime array-like Mixed cell and returns the target cell.
    fn array_set(
        &mut self,
        array: RuntimeCellHandle,
        index: RuntimeCellHandle,
        value: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Reads a named property from a runtime object held in a boxed Mixed cell.
    fn property_get(
        &mut self,
        object: RuntimeCellHandle,
        property: &str,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Writes a named property on a runtime object held in a boxed Mixed cell.
    fn property_set(
        &mut self,
        object: RuntimeCellHandle,
        property: &str,
        value: RuntimeCellHandle,
    ) -> Result<(), EvalStatus>;

    /// Returns the number of public JSON-visible properties on a runtime object.
    fn object_property_len(&mut self, object: RuntimeCellHandle) -> Result<usize, EvalStatus>;

    /// Returns the public property key at a zero-based JSON object iteration position.
    fn object_property_iter_key(
        &mut self,
        object: RuntimeCellHandle,
        position: usize,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Calls a named method on a runtime object held in a boxed Mixed cell.
    fn method_call(
        &mut self,
        object: RuntimeCellHandle,
        method: &str,
        args: Vec<RuntimeCellHandle>,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Creates a named runtime object without constructor arguments.
    fn new_object(&mut self, class_name: &str) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Calls the runtime constructor for an object when the class declares one.
    fn construct_object(
        &mut self,
        object: RuntimeCellHandle,
        args: Vec<RuntimeCellHandle>,
    ) -> Result<(), EvalStatus>;

    /// Returns whether a runtime class table contains the requested class name.
    fn class_exists(&mut self, name: &str) -> Result<bool, EvalStatus>;

    /// Returns whether a runtime interface table contains the requested interface name.
    fn interface_exists(&mut self, name: &str) -> Result<bool, EvalStatus>;

    /// Returns whether a runtime trait table contains the requested trait name.
    fn trait_exists(&mut self, name: &str) -> Result<bool, EvalStatus>;

    /// Returns whether a runtime enum table contains the requested enum name.
    fn enum_exists(&mut self, name: &str) -> Result<bool, EvalStatus>;

    /// Tests whether a boxed object cell satisfies a class/interface relation.
    fn object_is_a(
        &mut self,
        object_or_class: RuntimeCellHandle,
        target_class: &str,
        exclude_self: bool,
    ) -> Result<bool, EvalStatus>;

    /// Returns the PHP-visible runtime class name for an object cell.
    fn object_class_name(
        &mut self,
        object: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Returns the PHP-visible parent class name for an object or class-name cell.
    fn parent_class_name(
        &mut self,
        object_or_class: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Returns the visible element count for an array-like runtime cell.
    fn array_len(&mut self, array: RuntimeCellHandle) -> Result<usize, EvalStatus>;

    /// Returns whether a runtime cell can be indexed like an array by eval writes.
    fn is_array_like(&mut self, value: RuntimeCellHandle) -> Result<bool, EvalStatus>;

    /// Returns whether a runtime cell holds PHP null.
    fn is_null(&mut self, value: RuntimeCellHandle) -> Result<bool, EvalStatus>;

    /// Returns the concrete boxed Mixed runtime tag after unwrapping nested Mixed cells.
    fn type_tag(&mut self, value: RuntimeCellHandle) -> Result<u64, EvalStatus>;

    /// Returns the unboxed object payload pointer used for PHP object identity.
    fn object_identity(&mut self, object: RuntimeCellHandle) -> Result<u64, EvalStatus>;

    /// Releases one owned runtime cell that is no longer held by the eval scope.
    fn release(&mut self, value: RuntimeCellHandle) -> Result<(), EvalStatus>;

    /// Retains one runtime cell so the eval caller receives an independent owner.
    fn retain(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Emits or suppresses one PHP runtime warning through the target runtime.
    fn warning(&mut self, message: &str) -> Result<(), EvalStatus>;

    /// Creates a runtime null cell.
    fn null(&mut self) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Creates a runtime bool cell.
    fn bool_value(&mut self, value: bool) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Creates a runtime int cell.
    fn int(&mut self, value: i64) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Creates a runtime float cell.
    fn float(&mut self, value: f64) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Creates a runtime string cell.
    fn string(&mut self, value: &str) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Creates a runtime byte-string cell from raw PHP string bytes.
    fn string_bytes_value(&mut self, value: &[u8]) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Casts one runtime cell to a boxed PHP integer cell.
    fn cast_int(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Casts one runtime cell to a boxed PHP float cell.
    fn cast_float(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Casts one runtime cell to a boxed PHP string cell.
    fn cast_string(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Casts one runtime cell to a boxed PHP boolean cell.
    fn cast_bool(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Computes PHP `abs()` for one runtime cell while preserving integer/float result typing.
    fn abs(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Computes PHP `ceil()` for one runtime cell after PHP numeric conversion.
    fn ceil(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Computes PHP `floor()` for one runtime cell after PHP numeric conversion.
    fn floor(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Computes PHP `sqrt()` for one runtime cell after PHP numeric conversion.
    fn sqrt(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Reverses a string value using PHP `strrev()` byte-string semantics.
    fn strrev(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Divides two runtime cells using PHP `fdiv()` semantics.
    fn fdiv(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Computes the floating-point remainder using PHP `fmod()` semantics.
    fn fmod(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Adds two runtime cells using PHP addition semantics.
    fn add(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Subtracts two runtime cells using PHP numeric semantics.
    fn sub(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Multiplies two runtime cells using PHP numeric semantics.
    fn mul(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Divides two runtime cells using PHP numeric semantics.
    fn div(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Computes modulo for two runtime cells using PHP integer modulo semantics.
    fn modulo(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Raises one runtime cell to another using PHP exponentiation semantics.
    fn pow(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Rounds one runtime cell using PHP `round()` semantics and optional precision.
    fn round(
        &mut self,
        value: RuntimeCellHandle,
        precision: Option<RuntimeCellHandle>,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Applies an integer bitwise or shift operation to two runtime cells.
    fn bitwise(
        &mut self,
        op: EvalBinOp,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Applies integer bitwise NOT to one runtime cell.
    fn bit_not(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Concatenates two runtime cells using PHP string conversion semantics.
    fn concat(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Compares two runtime cells and returns a boxed PHP boolean cell.
    fn compare(
        &mut self,
        op: EvalBinOp,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Compares two runtime cells and returns a boxed PHP spaceship integer.
    fn spaceship(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Emits one runtime cell to stdout using PHP echo semantics.
    fn echo(&mut self, value: RuntimeCellHandle) -> Result<(), EvalStatus>;

    /// Casts one runtime cell to a PHP string and copies its bytes for parsing.
    fn string_bytes(&mut self, value: RuntimeCellHandle) -> Result<Vec<u8>, EvalStatus>;

    /// Converts one runtime cell to PHP boolean truthiness.
    fn truthy(&mut self, value: RuntimeCellHandle) -> Result<bool, EvalStatus>;
}

const EVAL_TAG_INT: u64 = 0;
const EVAL_TAG_STRING: u64 = 1;
const EVAL_TAG_FLOAT: u64 = 2;
const EVAL_TAG_BOOL: u64 = 3;
const EVAL_TAG_ARRAY: u64 = 4;
const EVAL_TAG_ASSOC: u64 = 5;
const EVAL_TAG_OBJECT: u64 = 6;
const EVAL_TAG_NULL: u64 = 8;
const EVAL_TAG_RESOURCE: u64 = 9;
const DEFINE_ALREADY_DEFINED_WARNING: &str = "Warning: define(): Constant already defined\n";
const HEX2BIN_ODD_LENGTH_WARNING: &str =
    "Warning: hex2bin(): Hexadecimal input string must have an even length\n";
const HEX2BIN_INVALID_WARNING: &str =
    "Warning: hex2bin(): Input string must be hexadecimal string\n";
const EVAL_PATHINFO_DIRNAME: i64 = 1;
const EVAL_PATHINFO_BASENAME: i64 = 2;
const EVAL_PATHINFO_EXTENSION: i64 = 4;
const EVAL_PATHINFO_FILENAME: i64 = 8;
const EVAL_PATHINFO_ALL: i64 = 15;
const EVAL_FNM_NOESCAPE: i64 = 1;
const EVAL_FNM_PATHNAME: i64 = 2;
const EVAL_FNM_PERIOD: i64 = 4;
const EVAL_FNM_CASEFOLD: i64 = 16;
const EVAL_ARRAY_FILTER_USE_VALUE: i64 = 0;
const EVAL_ARRAY_FILTER_USE_BOTH: i64 = 1;
const EVAL_ARRAY_FILTER_USE_KEY: i64 = 2;
const EVAL_COUNT_NORMAL: i64 = 0;
const EVAL_COUNT_RECURSIVE: i64 = 1;
const EVAL_PREG_SPLIT_NO_EMPTY: i64 = 1;
const EVAL_PREG_SPLIT_DELIM_CAPTURE: i64 = 2;
const EVAL_PREG_SPLIT_OFFSET_CAPTURE: i64 = 4;
const EVAL_PREG_PATTERN_ORDER: i64 = 1;
const EVAL_PREG_SET_ORDER: i64 = 2;
const EVAL_PREG_OFFSET_CAPTURE: i64 = 256;
const EVAL_PREG_UNMATCHED_AS_NULL: i64 = 512;
const EVAL_JSON_ERROR_NONE: i64 = 0;
const EVAL_JSON_ERROR_DEPTH: i64 = 1;
const EVAL_JSON_ERROR_STATE_MISMATCH: i64 = 2;
const EVAL_JSON_ERROR_CTRL_CHAR: i64 = 3;
const EVAL_JSON_ERROR_SYNTAX: i64 = 4;
const EVAL_JSON_ERROR_UTF8: i64 = 5;
const EVAL_JSON_ERROR_RECURSION: i64 = 6;
const EVAL_JSON_ERROR_INF_OR_NAN: i64 = 7;
const EVAL_JSON_ERROR_UNSUPPORTED_TYPE: i64 = 8;
const EVAL_JSON_ERROR_INVALID_PROPERTY_NAME: i64 = 9;
const EVAL_JSON_ERROR_UTF16: i64 = 10;
const EVAL_JSON_HEX_TAG: i64 = 1;
const EVAL_JSON_HEX_AMP: i64 = 2;
const EVAL_JSON_HEX_APOS: i64 = 4;
const EVAL_JSON_HEX_QUOT: i64 = 8;
const EVAL_JSON_BIGINT_AS_STRING: i64 = 2;
const EVAL_JSON_FORCE_OBJECT: i64 = 16;
const EVAL_JSON_NUMERIC_CHECK: i64 = 32;
const EVAL_JSON_UNESCAPED_SLASHES: i64 = 64;
const EVAL_JSON_PRETTY_PRINT: i64 = 128;
const EVAL_JSON_UNESCAPED_UNICODE: i64 = 256;
const EVAL_JSON_PARTIAL_OUTPUT_ON_ERROR: i64 = 512;
const EVAL_JSON_PRESERVE_ZERO_FRACTION: i64 = 1024;
const EVAL_JSON_INVALID_UTF8_IGNORE: i64 = 1_048_576;
const EVAL_JSON_INVALID_UTF8_SUBSTITUTE: i64 = 2_097_152;
const EVAL_JSON_THROW_ON_ERROR: i64 = 4_194_304;
const EVAL_JSON_INF_OR_NAN_MESSAGE: &str = "Inf and NaN cannot be JSON encoded";
const EVAL_JSON_UTF8_MESSAGE: &str = "Malformed UTF-8 characters, possibly incorrectly encoded";

unsafe extern "C" {
    /// Sets the process file-creation mask and returns the previous mask.
    fn umask(mask: u32) -> u32;
}

/// Executes an EvalIR program and returns the eval result cell.
pub fn execute_program(
    program: &EvalProgram,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut context = ElephcEvalContext::new();
    execute_program_with_context(&mut context, program, scope, values)
}

/// Executes an EvalIR program with a persistent eval context for dynamic declarations.
pub fn execute_program_with_context(
    context: &mut ElephcEvalContext,
    program: &EvalProgram,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match execute_program_outcome_with_context(context, program, scope, values)? {
        EvalOutcome::Value(result) => Ok(result),
        EvalOutcome::Throwable(error) => {
            context.set_pending_throw(error);
            Err(EvalStatus::UncaughtThrowable)
        }
    }
}

/// Executes an EvalIR program and preserves escaping Throwable cells.
pub fn execute_program_outcome_with_context(
    context: &mut ElephcEvalContext,
    program: &EvalProgram,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalOutcome, EvalStatus> {
    match execute_statements(program.statements(), context, scope, values) {
        Ok(EvalControl::None) => values.null().map(EvalOutcome::Value),
        Ok(EvalControl::Return(result)) => Ok(EvalOutcome::Value(result)),
        Ok(EvalControl::Throw(result)) => Ok(EvalOutcome::Throwable(result)),
        Ok(EvalControl::Break | EvalControl::Continue) => Err(EvalStatus::UnsupportedConstruct),
        Err(EvalStatus::UncaughtThrowable) => context
            .take_pending_throw()
            .map(EvalOutcome::Throwable)
            .ok_or(EvalStatus::UncaughtThrowable),
        Err(status) => Err(status),
    }
}

/// Executes a zero-argument function declared in the shared eval context.
pub fn execute_context_function_zero_args(
    context: &mut ElephcEvalContext,
    name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    execute_context_function(context, name, Vec::new(), values)
}

/// Executes a function declared in the shared eval context with prepared argument cells.
pub fn execute_context_function(
    context: &mut ElephcEvalContext,
    name: &str,
    args: Vec<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match execute_context_function_outcome(context, name, args, values)? {
        EvalOutcome::Value(result) => Ok(result),
        EvalOutcome::Throwable(error) => {
            context.set_pending_throw(error);
            Err(EvalStatus::UncaughtThrowable)
        }
    }
}

/// Executes a function declared in the shared eval context and preserves thrown cells.
pub fn execute_context_function_outcome(
    context: &mut ElephcEvalContext,
    name: &str,
    args: Vec<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalOutcome, EvalStatus> {
    context
        .function(name)
        .cloned()
        .map_or(Err(EvalStatus::UnsupportedConstruct), |function| {
            match eval_dynamic_function_with_values(&function, args, context, values) {
                Ok(result) => Ok(EvalOutcome::Value(result)),
                Err(EvalStatus::UncaughtThrowable) => context
                    .take_pending_throw()
                    .map(EvalOutcome::Throwable)
                    .ok_or(EvalStatus::UncaughtThrowable),
                Err(status) => Err(status),
            }
        })
}

/// Executes a named eval-context callable with arguments from a PHP array container.
pub fn execute_context_function_call_array(
    context: &mut ElephcEvalContext,
    name: &str,
    arg_array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match execute_context_function_call_array_outcome(context, name, arg_array, values)? {
        EvalOutcome::Value(result) => Ok(result),
        EvalOutcome::Throwable(error) => {
            context.set_pending_throw(error);
            Err(EvalStatus::UncaughtThrowable)
        }
    }
}

/// Executes a named eval-context callable from an argument array and preserves thrown cells.
pub fn execute_context_function_call_array_outcome(
    context: &mut ElephcEvalContext,
    name: &str,
    arg_array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalOutcome, EvalStatus> {
    if !values.is_array_like(arg_array)? {
        return Err(EvalStatus::RuntimeFatal);
    }
    let evaluated_args = eval_array_call_arg_values(arg_array, values)?;
    match eval_callable_with_call_array_args(name, evaluated_args, context, values) {
        Ok(result) => Ok(EvalOutcome::Value(result)),
        Err(EvalStatus::UncaughtThrowable) => context
            .take_pending_throw()
            .map(EvalOutcome::Throwable)
            .ok_or(EvalStatus::UncaughtThrowable),
        Err(status) => Err(status),
    }
}

/// Executes statements in source order and propagates the first eval `return`.
fn execute_statements(
    statements: &[EvalStmt],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    for stmt in statements {
        match execute_stmt(stmt, context, scope, values)? {
            EvalControl::None => {}
            control => return Ok(control),
        }
    }
    Ok(EvalControl::None)
}

/// Returns the eval-visible entry for a variable, following `global` aliases.
fn scope_entry(
    context: &ElephcEvalContext,
    scope: &ElephcEvalScope,
    name: &str,
) -> Option<ScopeEntry> {
    let Some(global_name) = scope.global_alias_target(name) else {
        return scope.entry(name);
    };
    let Some(global_scope) = context.global_scope_ptr() else {
        return scope.entry(name);
    };
    let current_scope = scope as *const ElephcEvalScope as *mut ElephcEvalScope;
    if global_scope == current_scope {
        return scope.entry(global_name);
    }
    unsafe {
        global_scope
            .as_ref()
            .and_then(|scope| scope.entry(global_name))
    }
}

/// Returns the eval-visible cell for a variable, following `global` aliases.
fn visible_scope_cell(
    context: &ElephcEvalContext,
    scope: &ElephcEvalScope,
    name: &str,
) -> Option<RuntimeCellHandle> {
    scope_entry(context, scope, name)
        .filter(|entry| entry.flags().is_visible())
        .map(ScopeEntry::cell)
}

/// Stores a variable cell, redirecting `global` aliases to the global scope.
fn set_scope_cell(
    context: &ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    name: impl Into<String>,
    cell: RuntimeCellHandle,
    ownership: ScopeCellOwnership,
) -> Result<Vec<RuntimeCellHandle>, EvalStatus> {
    let name = name.into();
    if let Some(global_name) = scope.global_alias_target(&name).map(str::to_string) {
        let Some(global_scope) = context.global_scope_ptr() else {
            return Err(EvalStatus::RuntimeFatal);
        };
        let current_scope = scope as *mut ElephcEvalScope;
        if global_scope == current_scope {
            return Ok(scope.set_respecting_references(global_name, cell, ownership));
        }
        let Some(global_scope) = (unsafe { global_scope.as_mut() }) else {
            return Err(EvalStatus::RuntimeFatal);
        };
        return Ok(global_scope.set_respecting_references(global_name, cell, ownership));
    }
    Ok(scope.set_respecting_references(name, cell, ownership))
}

/// Creates a PHP reference alias between two eval-visible variable names.
fn set_reference_alias(
    context: &ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    target: &str,
    source: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<RuntimeCellHandle>, EvalStatus> {
    if let Some(global_name) = scope.global_alias_target(source).map(str::to_string) {
        scope.mark_global_alias_to(target.to_string(), global_name);
        return Ok(Vec::new());
    }
    let (cell, ownership) = scope_entry(context, scope, source)
        .filter(|entry| entry.flags().is_visible())
        .map_or_else(
            || values.null().map(|cell| (cell, ScopeCellOwnership::Owned)),
            |entry| Ok((entry.cell(), entry.flags().ownership)),
        )?;
    Ok(scope.set_reference(target.to_string(), source.to_string(), cell, ownership))
}

/// Unsets a variable, removing only the local alias when the name is global.
fn unset_scope_cell(
    scope: &mut ElephcEvalScope,
    name: impl Into<String>,
) -> Option<RuntimeCellHandle> {
    let name = name.into();
    if scope.is_global_alias(&name) {
        scope.clear_global_alias(&name);
    }
    scope.unset_respecting_references(name)
}

/// Marks variables as aliases to the context global scope for later reads/writes.
fn execute_global_stmt(
    vars: &[String],
    context: &ElephcEvalContext,
    scope: &mut ElephcEvalScope,
) -> Result<(), EvalStatus> {
    if context.global_scope_ptr().is_none() {
        return Err(EvalStatus::RuntimeFatal);
    }
    for name in vars {
        scope.mark_global_alias(name.clone());
    }
    Ok(())
}

/// Executes one statement and returns `Some` only for eval `return`.
fn execute_stmt(
    stmt: &EvalStmt,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    match stmt {
        EvalStmt::ArrayAppendVar { name, value } => {
            let mut ownership = ScopeCellOwnership::Owned;
            let array = if let Some(existing) =
                scope_entry(context, scope, name).filter(|entry| entry.flags().is_visible())
            {
                if values.is_array_like(existing.cell())? {
                    let tag = values.type_tag(existing.cell())?;
                    if !matches!(tag, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
                        return Err(EvalStatus::UnsupportedConstruct);
                    }
                    ownership = existing.flags().ownership;
                    existing.cell()
                } else {
                    values.array_new(1)?
                }
            } else {
                values.array_new(1)?
            };
            let index = eval_array_append_key(array, values)?;
            let value = eval_expr(value, context, scope, values)?;
            let array = values.array_set(array, index, value)?;
            for replaced in set_scope_cell(context, scope, name.clone(), array, ownership)? {
                values.release(replaced)?;
            }
            Ok(EvalControl::None)
        }
        EvalStmt::ArraySetVar { name, index, value } => {
            let mut ownership = ScopeCellOwnership::Owned;
            let array = if let Some(existing) =
                scope_entry(context, scope, name).filter(|entry| entry.flags().is_visible())
            {
                if values.is_array_like(existing.cell())? {
                    ownership = existing.flags().ownership;
                    existing.cell()
                } else {
                    values.array_new(1)?
                }
            } else {
                values.array_new(1)?
            };
            let index = eval_expr(index, context, scope, values)?;
            let value = eval_expr(value, context, scope, values)?;
            let array = values.array_set(array, index, value)?;
            for replaced in set_scope_cell(context, scope, name.clone(), array, ownership)? {
                values.release(replaced)?;
            }
            Ok(EvalControl::None)
        }
        EvalStmt::Break => Ok(EvalControl::Break),
        EvalStmt::Continue => Ok(EvalControl::Continue),
        EvalStmt::DoWhile { body, condition } => {
            execute_do_while_stmt(body, condition, context, scope, values)
        }
        EvalStmt::Echo(expr) => {
            let value = eval_expr(expr, context, scope, values)?;
            values.echo(value)?;
            Ok(EvalControl::None)
        }
        EvalStmt::For {
            init,
            condition,
            update,
            body,
        } => execute_for_stmt(
            init,
            condition.as_ref(),
            update,
            body,
            context,
            scope,
            values,
        ),
        EvalStmt::ClassDecl(class) => {
            execute_class_decl_stmt(class, context, values)?;
            Ok(EvalControl::None)
        }
        EvalStmt::Foreach {
            array,
            key_name,
            value_name,
            body,
        } => execute_foreach_stmt(
            array,
            key_name.as_deref(),
            value_name,
            body,
            context,
            scope,
            values,
        ),
        EvalStmt::FunctionDecl { name, params, body } => {
            let key = name.to_ascii_lowercase();
            context
                .define_function(
                    key,
                    EvalFunction::new(name.clone(), params.clone(), body.clone()),
                )
                .map_err(|_| EvalStatus::RuntimeFatal)?;
            Ok(EvalControl::None)
        }
        EvalStmt::Global { vars } => {
            execute_global_stmt(vars, context, scope)?;
            Ok(EvalControl::None)
        }
        EvalStmt::If {
            condition,
            then_branch,
            else_branch,
        } => {
            let condition = eval_expr(condition, context, scope, values)?;
            if values.truthy(condition)? {
                execute_statements(then_branch, context, scope, values)
            } else {
                execute_statements(else_branch, context, scope, values)
            }
        }
        EvalStmt::Return(Some(expr)) => Ok(EvalControl::Return(eval_expr(
            expr, context, scope, values,
        )?)),
        EvalStmt::Return(None) => Ok(EvalControl::Return(values.null()?)),
        EvalStmt::ReferenceAssign { target, source } => {
            for replaced in set_reference_alias(context, scope, target, source, values)? {
                values.release(replaced)?;
            }
            Ok(EvalControl::None)
        }
        EvalStmt::StaticVar { name, init } => {
            execute_static_var_stmt(name, init, context, scope, values)?;
            Ok(EvalControl::None)
        }
        EvalStmt::PropertySet {
            object,
            property,
            value,
        } => {
            let object = eval_expr(object, context, scope, values)?;
            let value = eval_expr(value, context, scope, values)?;
            values.property_set(object, property, value)?;
            Ok(EvalControl::None)
        }
        EvalStmt::StoreVar { name, value } => {
            let value = eval_expr(value, context, scope, values)?;
            for replaced in set_scope_cell(
                context,
                scope,
                name.clone(),
                value,
                ScopeCellOwnership::Owned,
            )? {
                values.release(replaced)?;
            }
            Ok(EvalControl::None)
        }
        EvalStmt::Switch { expr, cases } => {
            execute_switch_stmt(expr, cases, context, scope, values)
        }
        EvalStmt::Throw(expr) => {
            let thrown = eval_expr(expr, context, scope, values)?;
            if values.type_tag(thrown)? != EVAL_TAG_OBJECT {
                return Err(EvalStatus::RuntimeFatal);
            }
            Ok(EvalControl::Throw(thrown))
        }
        EvalStmt::Try {
            body,
            catches,
            finally_body,
        } => execute_try_stmt(body, catches, finally_body, context, scope, values),
        EvalStmt::UnsetVar { name } => {
            if let Some(replaced) = unset_scope_cell(scope, name.clone()) {
                values.release(replaced)?;
            }
            Ok(EvalControl::None)
        }
        EvalStmt::While { condition, body } => {
            while {
                let condition = eval_expr(condition, context, scope, values)?;
                values.truthy(condition)?
            } {
                match execute_statements(body, context, scope, values)? {
                    EvalControl::None | EvalControl::Continue => {}
                    EvalControl::Break => break,
                    EvalControl::Throw(result) => return Ok(EvalControl::Throw(result)),
                    EvalControl::Return(result) => return Ok(EvalControl::Return(result)),
                }
            }
            Ok(EvalControl::None)
        }
        EvalStmt::Expr(expr) => {
            let _ = eval_expr(expr, context, scope, values)?;
            Ok(EvalControl::None)
        }
    }
}

/// Executes an eval `try` body and handles supported `catch` clauses.
fn execute_try_stmt(
    body: &[EvalStmt],
    catches: &[EvalCatch],
    finally_body: &[EvalStmt],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    let control = match execute_statements(body, context, scope, values) {
        Ok(EvalControl::Throw(thrown)) => {
            execute_matching_catch(thrown, catches, context, scope, values)?
        }
        Err(EvalStatus::UncaughtThrowable) => {
            let Some(thrown) = context.take_pending_throw() else {
                return Err(EvalStatus::UncaughtThrowable);
            };
            execute_matching_catch(thrown, catches, context, scope, values)?
        }
        Ok(control) => control,
        Err(status) => return Err(status),
    };
    if finally_body.is_empty() {
        return Ok(control);
    }
    match execute_statements(finally_body, context, scope, values) {
        Ok(EvalControl::None) => Ok(control),
        Ok(finally_control) => {
            release_overridden_control(control, values)?;
            Ok(finally_control)
        }
        Err(status) => {
            release_overridden_control(control, values)?;
            Err(status)
        }
    }
}

/// Releases a pending control-flow value when `finally` replaces that action.
fn release_overridden_control(
    control: EvalControl,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    match control {
        EvalControl::Return(value) | EvalControl::Throw(value) => values.release(value),
        EvalControl::None | EvalControl::Break | EvalControl::Continue => Ok(()),
    }
}

/// Executes the first supported catch clause for a thrown eval object.
fn execute_matching_catch(
    thrown: RuntimeCellHandle,
    catches: &[EvalCatch],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    let mut matched = None;
    for catch in catches {
        if catch_types_match_thrown(thrown, &catch.class_names, values)? {
            matched = Some(catch);
            break;
        }
    }
    let Some(catch) = matched else {
        return Ok(EvalControl::Throw(thrown));
    };
    if let Some(var_name) = &catch.var_name {
        for replaced in set_scope_cell(
            context,
            scope,
            var_name.clone(),
            thrown,
            ScopeCellOwnership::Owned,
        )? {
            values.release(replaced)?;
        }
    } else {
        values.release(thrown)?;
    }
    execute_statements(&catch.body, context, scope, values)
}

/// Returns true when any type in one catch clause accepts the thrown object.
fn catch_types_match_thrown(
    thrown: RuntimeCellHandle,
    class_names: &[String],
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    for class_name in class_names {
        let class_name = class_name.trim_start_matches('\\');
        if class_name.eq_ignore_ascii_case("Throwable") {
            return Ok(true);
        }
        if values.object_is_a(thrown, class_name, false)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Registers an eval-declared class in the dynamic class table.
fn execute_class_decl_stmt(
    class: &EvalClass,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let name = class.name().trim_start_matches('\\');
    if context.has_class(name) || values.class_exists(name)? {
        return Err(EvalStatus::RuntimeFatal);
    }
    if context.define_class(class.clone()) {
        Ok(())
    } else {
        Err(EvalStatus::RuntimeFatal)
    }
}

/// Creates a backing object for an eval-declared class and runs its constructor.
fn eval_dynamic_class_new_object(
    class: &EvalClass,
    evaluated_args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    caller_scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let object = values.new_object("stdClass")?;
    let identity = values.object_identity(object)?;
    context.register_dynamic_object(identity, class.name());
    for property in class.properties() {
        let value = if let Some(default) = property.default() {
            eval_expr(default, context, caller_scope, values)?
        } else {
            values.null()?
        };
        values.property_set(object, property.name(), value)?;
    }
    if let Some(constructor) = class.method("__construct") {
        eval_dynamic_method_with_values(
            class.name(),
            constructor,
            object,
            evaluated_args,
            context,
            values,
        )?;
    } else if !evaluated_args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(object)
}

/// Dispatches a method call to an eval-declared class method or to the runtime hook.
fn eval_method_call_result(
    object: RuntimeCellHandle,
    method_name: &str,
    evaluated_args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Ok(identity) = values.object_identity(object) else {
        return values.method_call(object, method_name, evaluated_args);
    };
    let Some(class) = context.dynamic_object_class(identity) else {
        return values.method_call(object, method_name, evaluated_args);
    };
    let class_name = class.name().to_string();
    let method = class
        .method(method_name)
        .cloned()
        .ok_or(EvalStatus::RuntimeFatal)?;
    eval_dynamic_method_with_values(
        &class_name,
        &method,
        object,
        evaluated_args,
        context,
        values,
    )
}

/// Executes one eval-declared class method with `$this` bound in method scope.
fn eval_dynamic_method_with_values(
    class_name: &str,
    method: &EvalClassMethod,
    object: RuntimeCellHandle,
    evaluated_args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let evaluated_args =
        bind_evaluated_function_args(method.params(), positional_args(evaluated_args))?;
    let mut method_scope = ElephcEvalScope::new();
    method_scope.set("this", object, ScopeCellOwnership::Borrowed);
    for (name, value) in method.params().iter().zip(evaluated_args) {
        method_scope.set(name.clone(), value, ScopeCellOwnership::Borrowed);
    }
    let qualified_method_name =
        format!("{}::{}", class_name.trim_start_matches('\\'), method.name());
    let static_names = static_var_names(method.body());
    context.push_function(qualified_method_name.clone());
    let result = execute_statements(method.body(), context, &mut method_scope, values);
    let persist_result = persist_static_locals(
        context,
        &qualified_method_name,
        &static_names,
        &method_scope,
        values,
    );
    context.pop_function();
    persist_result?;
    match result? {
        EvalControl::None => values.null(),
        EvalControl::Return(result) => Ok(result),
        EvalControl::Throw(result) => {
            context.set_pending_throw(result);
            Err(EvalStatus::UncaughtThrowable)
        }
        EvalControl::Break | EvalControl::Continue => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Wraps positional method arguments into the shared dynamic-call binding shape.
fn positional_args(args: Vec<RuntimeCellHandle>) -> Vec<EvaluatedCallArg> {
    args.into_iter()
        .map(|value| EvaluatedCallArg { name: None, value })
        .collect()
}

/// Executes a PHP `static $name = expr;` declaration in the current eval scope.
fn execute_static_var_stmt(
    name: &str,
    init: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Some(function_name) = context.current_function().map(str::to_string) else {
        let value = eval_expr(init, context, scope, values)?;
        if let Some(replaced) = scope.set(name.to_string(), value, ScopeCellOwnership::Owned) {
            values.release(replaced)?;
        }
        return Ok(());
    };
    if scope.contains_visible(name) {
        return Ok(());
    }
    let value = if let Some(value) = context.static_local(&function_name, name) {
        value
    } else {
        let value = eval_expr(init, context, scope, values)?;
        let _ = context.set_static_local(function_name.clone(), name.to_string(), value);
        value
    };
    if let Some(replaced) = scope.set(name.to_string(), value, ScopeCellOwnership::Borrowed) {
        values.release(replaced)?;
    }
    Ok(())
}

/// Executes a PHP switch with loose case matching, default fallback, and fallthrough.
fn execute_switch_stmt(
    expr: &EvalExpr,
    cases: &[EvalSwitchCase],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    let subject = eval_expr(expr, context, scope, values)?;
    let mut default_index = None;
    let mut matched_index = None;
    for (index, case) in cases.iter().enumerate() {
        let Some(condition) = &case.condition else {
            if default_index.is_none() {
                default_index = Some(index);
            }
            continue;
        };
        let condition = eval_expr(condition, context, scope, values)?;
        let matches = values.compare(EvalBinOp::LooseEq, subject, condition)?;
        if values.truthy(matches)? {
            matched_index = Some(index);
            break;
        }
    }
    let Some(start_index) = matched_index.or(default_index) else {
        return Ok(EvalControl::None);
    };
    for case in &cases[start_index..] {
        match execute_statements(&case.body, context, scope, values)? {
            EvalControl::None => {}
            EvalControl::Break | EvalControl::Continue => break,
            EvalControl::Throw(result) => return Ok(EvalControl::Throw(result)),
            EvalControl::Return(result) => return Ok(EvalControl::Return(result)),
        }
    }
    Ok(EvalControl::None)
}

/// Executes a PHP `do/while` loop, evaluating the condition after every body run.
fn execute_do_while_stmt(
    body: &[EvalStmt],
    condition: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    loop {
        match execute_statements(body, context, scope, values)? {
            EvalControl::None | EvalControl::Continue => {}
            EvalControl::Break => break,
            EvalControl::Throw(result) => return Ok(EvalControl::Throw(result)),
            EvalControl::Return(result) => return Ok(EvalControl::Return(result)),
        }
        let condition = eval_expr(condition, context, scope, values)?;
        if !values.truthy(condition)? {
            break;
        }
    }
    Ok(EvalControl::None)
}

/// Executes a PHP `for` loop while preserving update-on-continue semantics.
fn execute_for_stmt(
    init: &[EvalStmt],
    condition: Option<&EvalExpr>,
    update: &[EvalStmt],
    body: &[EvalStmt],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    match execute_statements(init, context, scope, values)? {
        EvalControl::None | EvalControl::Continue => {}
        EvalControl::Break => return Ok(EvalControl::None),
        EvalControl::Throw(result) => return Ok(EvalControl::Throw(result)),
        EvalControl::Return(result) => return Ok(EvalControl::Return(result)),
    }
    loop {
        if let Some(condition) = condition {
            let condition = eval_expr(condition, context, scope, values)?;
            if !values.truthy(condition)? {
                break;
            }
        }
        match execute_statements(body, context, scope, values)? {
            EvalControl::None | EvalControl::Continue => {}
            EvalControl::Break => break,
            EvalControl::Throw(result) => return Ok(EvalControl::Throw(result)),
            EvalControl::Return(result) => return Ok(EvalControl::Return(result)),
        }
        match execute_statements(update, context, scope, values)? {
            EvalControl::None | EvalControl::Continue => {}
            EvalControl::Break => break,
            EvalControl::Throw(result) => return Ok(EvalControl::Throw(result)),
            EvalControl::Return(result) => return Ok(EvalControl::Return(result)),
        }
    }
    Ok(EvalControl::None)
}

/// Executes a PHP `foreach` loop over eval array values.
fn execute_foreach_stmt(
    array: &EvalExpr,
    key_name: Option<&str>,
    value_name: &str,
    body: &[EvalStmt],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    let array = eval_expr(array, context, scope, values)?;
    let len = values.array_len(array)?;
    for index in 0..len {
        let key = values.array_iter_key(array, index)?;
        let value = values.array_get(array, key)?;
        if let Some(key_name) = key_name {
            for replaced in set_scope_cell(
                context,
                scope,
                key_name.to_string(),
                key,
                ScopeCellOwnership::Owned,
            )? {
                values.release(replaced)?;
            }
        } else {
            values.release(key)?;
        }
        for replaced in set_scope_cell(
            context,
            scope,
            value_name.to_string(),
            value,
            ScopeCellOwnership::Owned,
        )? {
            values.release(replaced)?;
        }
        match execute_statements(body, context, scope, values)? {
            EvalControl::None | EvalControl::Continue => {}
            EvalControl::Break => break,
            EvalControl::Throw(result) => return Ok(EvalControl::Throw(result)),
            EvalControl::Return(result) => return Ok(EvalControl::Return(result)),
        }
    }
    Ok(EvalControl::None)
}

/// Returns PHP's next automatic integer key for `$array[]` append writes.
fn eval_array_append_key(
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let mut next_key = None;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        if values.type_tag(key)? != EVAL_TAG_INT {
            continue;
        }
        let one = values.int(1)?;
        let candidate = values.add(key, one)?;
        let replace = if let Some(current) = next_key {
            let is_greater = values.compare(EvalBinOp::Gt, candidate, current)?;
            values.truthy(is_greater)?
        } else {
            true
        };
        if replace {
            next_key = Some(candidate);
        }
    }
    next_key.map_or_else(|| values.int(0), Ok)
}

/// Evaluates one expression to an opaque runtime-cell handle.
fn eval_expr(
    expr: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match expr {
        EvalExpr::Array(elements) => {
            if elements
                .iter()
                .any(|element| matches!(element, EvalArrayElement::KeyValue { .. }))
            {
                eval_assoc_array(elements, context, scope, values)
            } else {
                eval_indexed_array(elements, context, scope, values)
            }
        }
        EvalExpr::ArrayGet { array, index } => {
            let array = eval_expr(array, context, scope, values)?;
            let index = eval_expr(index, context, scope, values)?;
            values.array_get(array, index)
        }
        EvalExpr::Call { name, args } => eval_call(name, args, context, scope, values),
        EvalExpr::Const(value) => eval_const(value, values),
        EvalExpr::ConstFetch(name) => eval_const_fetch(name, context, values),
        EvalExpr::DynamicCall { callee, args } => {
            eval_dynamic_call(callee, args, context, scope, values)
        }
        EvalExpr::Include {
            path,
            required,
            once,
        } => eval_include_expr(path, *required, *once, context, scope, values),
        EvalExpr::LoadVar(name) => {
            visible_scope_cell(context, scope, name).map_or_else(|| values.null(), Ok)
        }
        EvalExpr::Magic(magic) => eval_magic_const(magic, context, values),
        EvalExpr::Match {
            subject,
            arms,
            default,
        } => eval_match_expr(subject, arms, default.as_deref(), context, scope, values),
        EvalExpr::NamespacedCall {
            name,
            fallback_name,
            args,
        } => eval_namespaced_call(name, fallback_name, args, context, scope, values),
        EvalExpr::NamespacedConstFetch {
            name,
            fallback_name,
        } => eval_namespaced_const_fetch(name, fallback_name, context, values),
        EvalExpr::NewObject { class_name, args } => {
            let args = eval_method_call_arg_values(args, context, scope, values)?;
            if let Some(class) = context.class(class_name).cloned() {
                eval_dynamic_class_new_object(&class, args, context, scope, values)
            } else {
                values
                    .new_object(class_name)
                    .and_then(|object| values.construct_object(object, args).map(|()| object))
            }
        }
        EvalExpr::MethodCall {
            object,
            method,
            args,
        } => {
            let object = eval_expr(object, context, scope, values)?;
            let evaluated_args = eval_method_call_arg_values(args, context, scope, values)?;
            eval_method_call_result(object, method, evaluated_args, context, values)
        }
        EvalExpr::NullCoalesce { value, default } => {
            let value = eval_expr(value, context, scope, values)?;
            if values.is_null(value)? {
                eval_expr(default, context, scope, values)
            } else {
                Ok(value)
            }
        }
        EvalExpr::PropertyGet { object, property } => {
            let object = eval_expr(object, context, scope, values)?;
            values.property_get(object, property)
        }
        EvalExpr::Print(inner) => {
            let value = eval_expr(inner, context, scope, values)?;
            values.echo(value)?;
            values.int(1)
        }
        EvalExpr::Ternary {
            condition,
            then_branch,
            else_branch,
        } => {
            let condition = eval_expr(condition, context, scope, values)?;
            if values.truthy(condition)? {
                if let Some(then_branch) = then_branch {
                    eval_expr(then_branch, context, scope, values)
                } else {
                    Ok(condition)
                }
            } else {
                eval_expr(else_branch, context, scope, values)
            }
        }
        EvalExpr::Unary { op, expr } => {
            let value = eval_expr(expr, context, scope, values)?;
            match op {
                EvalUnaryOp::Plus => {
                    let zero = values.int(0)?;
                    values.add(zero, value)
                }
                EvalUnaryOp::Negate => {
                    let zero = values.int(0)?;
                    values.sub(zero, value)
                }
                EvalUnaryOp::LogicalNot => {
                    let truthy = values.truthy(value)?;
                    values.bool_value(!truthy)
                }
                EvalUnaryOp::BitNot => values.bit_not(value),
            }
        }
        EvalExpr::Binary { op, left, right } => {
            if *op == EvalBinOp::LogicalAnd {
                let left = eval_expr(left, context, scope, values)?;
                if !values.truthy(left)? {
                    return values.bool_value(false);
                }
                let right = eval_expr(right, context, scope, values)?;
                let truthy = values.truthy(right)?;
                return values.bool_value(truthy);
            }
            if *op == EvalBinOp::LogicalOr {
                let left = eval_expr(left, context, scope, values)?;
                if values.truthy(left)? {
                    return values.bool_value(true);
                }
                let right = eval_expr(right, context, scope, values)?;
                let truthy = values.truthy(right)?;
                return values.bool_value(truthy);
            }
            let left = eval_expr(left, context, scope, values)?;
            let right = eval_expr(right, context, scope, values)?;
            match op {
                EvalBinOp::Add => values.add(left, right),
                EvalBinOp::Sub => values.sub(left, right),
                EvalBinOp::Mul => values.mul(left, right),
                EvalBinOp::Div => values.div(left, right),
                EvalBinOp::Mod => values.modulo(left, right),
                EvalBinOp::Pow => values.pow(left, right),
                EvalBinOp::BitAnd
                | EvalBinOp::BitOr
                | EvalBinOp::BitXor
                | EvalBinOp::ShiftLeft
                | EvalBinOp::ShiftRight => values.bitwise(*op, left, right),
                EvalBinOp::Concat => values.concat(left, right),
                EvalBinOp::LogicalXor => {
                    let left_truthy = values.truthy(left)?;
                    let right_truthy = values.truthy(right)?;
                    values.bool_value(left_truthy ^ right_truthy)
                }
                EvalBinOp::LooseEq
                | EvalBinOp::LooseNotEq
                | EvalBinOp::StrictEq
                | EvalBinOp::StrictNotEq
                | EvalBinOp::Lt
                | EvalBinOp::LtEq
                | EvalBinOp::Gt
                | EvalBinOp::GtEq => values.compare(*op, left, right),
                EvalBinOp::Spaceship => values.spaceship(left, right),
                EvalBinOp::LogicalAnd | EvalBinOp::LogicalOr => {
                    Err(EvalStatus::UnsupportedConstruct)
                }
            }
        }
    }
}

/// Evaluates a PHP `match` expression with strict comparison and lazy arm values.
fn eval_match_expr(
    subject: &EvalExpr,
    arms: &[EvalMatchArm],
    default: Option<&EvalExpr>,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let subject = eval_expr(subject, context, scope, values)?;
    for arm in arms {
        for pattern in &arm.patterns {
            let pattern = eval_expr(pattern, context, scope, values)?;
            let matched = values.compare(EvalBinOp::StrictEq, subject, pattern)?;
            if values.truthy(matched)? {
                return eval_expr(&arm.value, context, scope, values);
            }
        }
    }
    default
        .map(|expr| eval_expr(expr, context, scope, values))
        .unwrap_or(Err(EvalStatus::RuntimeFatal))
}

/// Returns cloned positional argument expressions, rejecting named arguments.
fn positional_call_arg_exprs(args: &[EvalCallArg]) -> Result<Vec<EvalExpr>, EvalStatus> {
    if args
        .iter()
        .any(|arg| arg.name().is_some() || arg.is_spread())
    {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(args.iter().map(|arg| arg.value().clone()).collect())
}

/// Evaluates a positional-only call argument list in source order.
fn eval_positional_call_arg_values(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<RuntimeCellHandle>, EvalStatus> {
    if args
        .iter()
        .any(|arg| arg.name().is_some() || arg.is_spread())
    {
        return Err(EvalStatus::RuntimeFatal);
    }
    let mut evaluated_args = Vec::with_capacity(args.len());
    for arg in args {
        evaluated_args.push(eval_expr(arg.value(), context, scope, values)?);
    }
    Ok(evaluated_args)
}

/// Evaluates method-call arguments, allowing numeric spread but not named args.
fn eval_method_call_arg_values(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<RuntimeCellHandle>, EvalStatus> {
    let evaluated_args = eval_call_arg_values(args, context, scope, values)?;
    if evaluated_args.iter().any(|arg| arg.name.is_some()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(evaluated_args.into_iter().map(|arg| arg.value).collect())
}

/// Evaluates supported function-like calls from a runtime eval fragment.
fn eval_call(
    name: &str,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if eval_expr_language_construct_name(name) {
        let args = positional_call_arg_exprs(args)?;
        return eval_positional_expr_call(name, &args, context, scope, values);
    }
    if matches!(
        name,
        "array_pop"
            | "array_push"
            | "array_shift"
            | "array_splice"
            | "array_unshift"
            | "arsort"
            | "asort"
            | "krsort"
            | "ksort"
            | "natcasesort"
            | "natsort"
            | "rsort"
            | "shuffle"
            | "sort"
            | "settype"
            | "uasort"
            | "uksort"
            | "usort"
    ) {
        return eval_builtin_array_pop_shift_call(name, args, context, scope, values);
    }
    if eval_php_visible_builtin_exists(name) {
        if eval_call_args_are_plain_positional(args) {
            let args = positional_call_arg_exprs(args)?;
            return eval_positional_expr_call(name, &args, context, scope, values);
        }
        return eval_builtin_call(name, args, context, scope, values);
    }

    if let Some(function) = context.function(name).cloned() {
        return eval_dynamic_function(&function, args, context, scope, values);
    }
    if let Some(function) = context.native_function(name) {
        return eval_native_function(function, args, context, scope, values);
    }
    Err(EvalStatus::UnsupportedConstruct)
}

/// Evaluates an unqualified namespaced function call with PHP's global fallback.
fn eval_namespaced_call(
    name: &str,
    fallback_name: &str,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Some(function) = context.function(name).cloned() {
        return eval_dynamic_function(&function, args, context, scope, values);
    }
    if let Some(function) = context.native_function(name) {
        return eval_native_function(function, args, context, scope, values);
    }
    eval_call(fallback_name, args, context, scope, values)
}

/// Evaluates a variable or expression callable and dispatches it with source-order arguments.
fn eval_dynamic_call(
    callee: &EvalExpr,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let callback = eval_expr(callee, context, scope, values)?;
    let callback = eval_callable(callback, values)?;
    let evaluated_args = eval_call_arg_values(args, context, scope, values)?;
    eval_evaluated_callable_with_call_array_args(&callback, evaluated_args, context, values)
}

/// Returns true for language constructs that need unevaluated argument expressions.
fn eval_expr_language_construct_name(name: &str) -> bool {
    matches!(name, "empty" | "eval" | "isset")
}

/// Returns true when every source argument is plain positional.
fn eval_call_args_are_plain_positional(args: &[EvalCallArg]) -> bool {
    args.iter()
        .all(|arg| arg.name().is_none() && !arg.is_spread())
}

/// Evaluates builtins and language constructs after positional-only argument validation.
fn eval_positional_expr_call(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "abs" => eval_builtin_abs(args, context, scope, values),
        "addslashes" | "stripslashes" => eval_builtin_slashes(name, args, context, scope, values),
        "array_combine" => eval_builtin_array_combine(args, context, scope, values),
        "array_chunk" => eval_builtin_array_chunk(args, context, scope, values),
        "array_column" => eval_builtin_array_column(args, context, scope, values),
        "array_fill" => eval_builtin_array_fill(args, context, scope, values),
        "array_fill_keys" => eval_builtin_array_fill_keys(args, context, scope, values),
        "array_filter" => eval_builtin_array_filter(args, context, scope, values),
        "array_flip" => eval_builtin_array_flip(args, context, scope, values),
        "array_map" => eval_builtin_array_map(args, context, scope, values),
        "array_reduce" => eval_builtin_array_reduce(args, context, scope, values),
        "array_walk" => eval_builtin_array_walk(args, context, scope, values),
        "array_keys" | "array_values" => {
            eval_builtin_array_projection(name, args, context, scope, values)
        }
        "array_key_exists" => eval_builtin_array_key_exists(args, context, scope, values),
        "array_diff" | "array_intersect" => {
            eval_builtin_array_value_set(name, args, context, scope, values)
        }
        "array_diff_key" | "array_intersect_key" => {
            eval_builtin_array_key_set(name, args, context, scope, values)
        }
        "array_merge" => eval_builtin_array_merge(args, context, scope, values),
        "array_product" | "array_sum" => {
            eval_builtin_array_aggregate(name, args, context, scope, values)
        }
        "array_pad" => eval_builtin_array_pad(args, context, scope, values),
        "array_rand" => eval_builtin_array_rand(args, context, scope, values),
        "array_reverse" => eval_builtin_array_reverse(args, context, scope, values),
        "array_search" | "in_array" => {
            eval_builtin_array_search(name, args, context, scope, values)
        }
        "array_slice" => eval_builtin_array_slice(args, context, scope, values),
        "array_unique" => eval_builtin_array_unique(args, context, scope, values),
        "acos" | "asin" | "atan" | "cos" | "cosh" | "deg2rad" | "exp" | "log2" | "log10"
        | "rad2deg" | "sin" | "sinh" | "tan" | "tanh" => {
            eval_builtin_float_unary(name, args, context, scope, values)
        }
        "atan2" | "hypot" => eval_builtin_float_pair(name, args, context, scope, values),
        "base64_encode" => eval_builtin_base64_encode(args, context, scope, values),
        "base64_decode" => eval_builtin_base64_decode(args, context, scope, values),
        "basename" => eval_builtin_basename(args, context, scope, values),
        "bin2hex" => eval_builtin_bin2hex(args, context, scope, values),
        "ceil" => eval_builtin_ceil(args, context, scope, values),
        "chdir" | "mkdir" | "rmdir" => {
            eval_builtin_unary_path_bool(name, args, context, scope, values)
        }
        "chmod" => eval_builtin_chmod(args, context, scope, values),
        "chr" => eval_builtin_chr(args, context, scope, values),
        "clamp" => eval_builtin_clamp(args, context, scope, values),
        "clearstatcache" => eval_builtin_clearstatcache(args, context, scope, values),
        "call_user_func" => eval_builtin_call_user_func(args, context, scope, values),
        "call_user_func_array" => eval_builtin_call_user_func_array(args, context, scope, values),
        "class_exists" => eval_builtin_class_exists(args, context, scope, values),
        "interface_exists" => eval_builtin_interface_exists(args, context, scope, values),
        "trait_exists" | "enum_exists" => {
            eval_builtin_class_like_exists(name, args, context, scope, values)
        }
        "is_a" | "is_subclass_of" => eval_builtin_is_a_relation(name, args, context, scope, values),
        "chop" => eval_builtin_trim_like(name, args, context, scope, values),
        "boolval" | "floatval" | "intval" | "strval" => {
            eval_builtin_cast(name, args, context, scope, values)
        }
        "count" => eval_builtin_count(args, context, scope, values),
        "copy" | "link" | "rename" | "symlink" => {
            eval_builtin_binary_path_bool(name, args, context, scope, values)
        }
        "crc32" => eval_builtin_crc32(args, context, scope, values),
        "ctype_alnum" | "ctype_alpha" | "ctype_digit" | "ctype_space" => {
            eval_builtin_ctype(name, args, context, scope, values)
        }
        "date" => eval_builtin_date(args, context, scope, values),
        "define" => eval_builtin_define(args, context, scope, values),
        "defined" => eval_builtin_defined(args, context, scope, values),
        "dirname" => eval_builtin_dirname(args, context, scope, values),
        "disk_free_space" | "disk_total_space" => {
            eval_builtin_disk_space(name, args, context, scope, values)
        }
        "empty" => eval_builtin_empty(args, context, scope, values),
        "eval" => eval_nested_eval(args, context, scope, values),
        "explode" => eval_builtin_explode(args, context, scope, values),
        "fdiv" | "fmod" => eval_builtin_float_binary(name, args, context, scope, values),
        "file" => eval_builtin_file(args, context, scope, values),
        "file_exists" => eval_builtin_file_probe(name, args, context, scope, values),
        "fileatime" | "filectime" | "filegroup" | "fileinode" | "filemtime" | "fileowner"
        | "fileperms" => eval_builtin_file_stat_scalar(name, args, context, scope, values),
        "file_get_contents" => eval_builtin_file_get_contents(args, context, scope, values),
        "file_put_contents" => eval_builtin_file_put_contents(args, context, scope, values),
        "filesize" => eval_builtin_filesize(args, context, scope, values),
        "filetype" => eval_builtin_filetype(args, context, scope, values),
        "fnmatch" => eval_builtin_fnmatch(args, context, scope, values),
        "stat" | "lstat" => eval_builtin_stat_array(name, args, context, scope, values),
        "floor" => eval_builtin_floor(args, context, scope, values),
        "function_exists" | "is_callable" => {
            eval_builtin_function_probe(args, context, scope, values)
        }
        "gethostbyaddr" => eval_builtin_gethostbyaddr(args, context, scope, values),
        "gethostbyname" => eval_builtin_gethostbyname(args, context, scope, values),
        "gethostname" => eval_builtin_gethostname(args, values),
        "getprotobyname" => eval_builtin_getprotobyname(args, context, scope, values),
        "getprotobynumber" => eval_builtin_getprotobynumber(args, context, scope, values),
        "getservbyname" => eval_builtin_getservbyname(args, context, scope, values),
        "getservbyport" => eval_builtin_getservbyport(args, context, scope, values),
        "get_class" => eval_builtin_get_class(args, context, scope, values),
        "get_parent_class" => eval_builtin_get_parent_class(args, context, scope, values),
        "get_resource_id" | "get_resource_type" => {
            eval_builtin_resource_introspection(name, args, context, scope, values)
        }
        "getcwd" => eval_builtin_getcwd(args, values),
        "getenv" => eval_builtin_getenv(args, context, scope, values),
        "gettype" => eval_builtin_gettype(args, context, scope, values),
        "glob" => eval_builtin_glob(args, context, scope, values),
        "hash" | "hash_file" | "hash_hmac" | "md5" | "sha1" => {
            eval_builtin_hash_one_shot(name, args, context, scope, values)
        }
        "hash_algos" => eval_builtin_hash_algos(args, values),
        "hash_equals" => eval_builtin_hash_equals(args, context, scope, values),
        "hex2bin" => eval_builtin_hex2bin(args, context, scope, values),
        "html_entity_decode" | "htmlentities" | "htmlspecialchars" => {
            eval_builtin_html_entity(name, args, context, scope, values)
        }
        "implode" => eval_builtin_implode(args, context, scope, values),
        "inet_ntop" => eval_builtin_inet_ntop(args, context, scope, values),
        "inet_pton" => eval_builtin_inet_pton(args, context, scope, values),
        "intdiv" => eval_builtin_intdiv(args, context, scope, values),
        "iterator_apply" => eval_builtin_iterator_apply(args, context, scope, values),
        "iterator_count" => eval_builtin_iterator_count(args, context, scope, values),
        "iterator_to_array" => eval_builtin_iterator_to_array(args, context, scope, values),
        "is_dir" | "is_executable" | "is_file" | "is_link" | "is_readable" | "is_writable"
        | "is_writeable" => eval_builtin_file_probe(name, args, context, scope, values),
        "is_array" | "is_bool" | "is_double" | "is_finite" | "is_float" | "is_infinite"
        | "is_int" | "is_integer" | "is_iterable" | "is_long" | "is_nan" | "is_null"
        | "is_numeric" | "is_object" | "is_real" | "is_resource" | "is_string" => {
            eval_builtin_type_predicate(name, args, context, scope, values)
        }
        "ip2long" => eval_builtin_ip2long(args, context, scope, values),
        "json_decode" => eval_builtin_json_decode(args, context, scope, values),
        "json_encode" => eval_builtin_json_encode(args, context, scope, values),
        "json_last_error" => eval_builtin_json_last_error(args, context, values),
        "json_last_error_msg" => eval_builtin_json_last_error_msg(args, context, values),
        "json_validate" => eval_builtin_json_validate(args, context, scope, values),
        "linkinfo" => eval_builtin_linkinfo(args, context, scope, values),
        "ltrim" | "rtrim" => eval_builtin_trim_like(name, args, context, scope, values),
        "log" => eval_builtin_log(args, context, scope, values),
        "max" | "min" => eval_builtin_min_max(name, args, context, scope, values),
        "microtime" => eval_builtin_microtime(args, context, scope, values),
        "mktime" => eval_builtin_mktime(args, context, scope, values),
        "nl2br" => eval_builtin_nl2br(args, context, scope, values),
        "number_format" => eval_builtin_number_format(args, context, scope, values),
        "ord" => eval_builtin_ord(args, context, scope, values),
        "pathinfo" => eval_builtin_pathinfo(args, context, scope, values),
        "pi" => eval_builtin_pi(args, values),
        "php_uname" => eval_builtin_php_uname(args, context, scope, values),
        "phpversion" => eval_builtin_phpversion(args, values),
        "pow" => eval_builtin_pow(args, context, scope, values),
        "preg_match" => eval_builtin_preg_match(args, context, scope, values),
        "preg_match_all" => eval_builtin_preg_match_all(args, context, scope, values),
        "preg_replace" => eval_builtin_preg_replace(args, context, scope, values),
        "preg_replace_callback" => eval_builtin_preg_replace_callback(args, context, scope, values),
        "preg_split" => eval_builtin_preg_split(args, context, scope, values),
        "print_r" => eval_builtin_print_r(args, context, scope, values),
        "putenv" => eval_builtin_putenv(args, context, scope, values),
        "rand" | "mt_rand" => eval_builtin_rand(args, context, scope, values),
        "random_int" => eval_builtin_random_int(args, context, scope, values),
        "range" => eval_builtin_range(args, context, scope, values),
        "rawurldecode" | "urldecode" => eval_builtin_url_decode(name, args, context, scope, values),
        "rawurlencode" | "urlencode" => eval_builtin_url_encode(name, args, context, scope, values),
        "readfile" => eval_builtin_readfile(args, context, scope, values),
        "readlink" => eval_builtin_readlink(args, context, scope, values),
        "realpath" => eval_builtin_realpath(args, context, scope, values),
        "realpath_cache_get" => eval_builtin_realpath_cache_get(args, values),
        "realpath_cache_size" => eval_builtin_realpath_cache_size(args, values),
        "round" => eval_builtin_round(args, context, scope, values),
        "scandir" => eval_builtin_scandir(args, context, scope, values),
        "isset" => eval_builtin_isset(args, context, scope, values),
        "sleep" => eval_builtin_sleep(args, context, scope, values),
        "sqrt" => eval_builtin_sqrt(args, context, scope, values),
        "spl_classes" => eval_builtin_spl_classes(args, values),
        "spl_object_id" | "spl_object_hash" => {
            eval_builtin_spl_object_identity(name, args, context, scope, values)
        }
        "sscanf" => eval_builtin_sscanf(args, context, scope, values),
        "sprintf" | "printf" => eval_builtin_sprintf_like(name, args, context, scope, values),
        "sys_get_temp_dir" => eval_builtin_sys_get_temp_dir(args, values),
        "tempnam" => eval_builtin_tempnam(args, context, scope, values),
        "time" => eval_builtin_time(args, values),
        "touch" => eval_builtin_touch(args, context, scope, values),
        "stream_get_filters" | "stream_get_transports" | "stream_get_wrappers" => {
            eval_builtin_stream_introspection(name, args, values)
        }
        "strtotime" => eval_builtin_strtotime(args, context, scope, values),
        "unlink" => eval_builtin_unlink(args, context, scope, values),
        "strrev" => eval_builtin_strrev(args, context, scope, values),
        "str_repeat" => eval_builtin_str_repeat(args, context, scope, values),
        "str_replace" | "str_ireplace" => {
            eval_builtin_str_replace(name, args, context, scope, values)
        }
        "str_pad" => eval_builtin_str_pad(args, context, scope, values),
        "str_split" => eval_builtin_str_split(args, context, scope, values),
        "strstr" => eval_builtin_strstr(args, context, scope, values),
        "substr" => eval_builtin_substr(args, context, scope, values),
        "substr_replace" => eval_builtin_substr_replace(args, context, scope, values),
        "str_contains" | "str_starts_with" | "str_ends_with" => {
            eval_builtin_string_search(name, args, context, scope, values)
        }
        "strcmp" | "strcasecmp" => eval_builtin_string_compare(name, args, context, scope, values),
        "strlen" => eval_builtin_strlen(args, context, scope, values),
        "strpos" | "strrpos" => eval_builtin_string_position(name, args, context, scope, values),
        "lcfirst" | "strtolower" | "strtoupper" | "ucfirst" => {
            eval_builtin_string_case(name, args, context, scope, values)
        }
        "long2ip" => eval_builtin_long2ip(args, context, scope, values),
        "trim" => eval_builtin_trim_like(name, args, context, scope, values),
        "ucwords" => eval_builtin_ucwords(args, context, scope, values),
        "umask" => eval_builtin_umask(args, context, scope, values),
        "usleep" => eval_builtin_usleep(args, context, scope, values),
        "var_dump" => eval_builtin_var_dump(args, context, scope, values),
        "vsprintf" | "vprintf" => eval_builtin_vsprintf_like(name, args, context, scope, values),
        "wordwrap" => eval_builtin_wordwrap(args, context, scope, values),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Evaluates nested `eval(...)` calls against the current materialized scope.
fn eval_nested_eval(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [code] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let code = eval_expr(code, context, scope, values)?;
    let code = values.string_bytes(code)?;
    let program = parse_fragment(&code).map_err(EvalParseError::status)?;
    execute_program_with_context(context, &program, scope, values)
}

/// Evaluates an eval-fragment include or require expression.
fn eval_include_expr(
    path: &EvalExpr,
    required: bool,
    once: bool,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_expr(path, context, scope, values)?;
    let path = eval_path_string(path, values)?;
    let resolved_path = eval_resolve_include_path(&path, context);
    let include_key = eval_include_key(&resolved_path);
    if once && context.has_included_file(&include_key) {
        return values.bool_value(true);
    }
    let bytes = match std::fs::read(&resolved_path) {
        Ok(bytes) => bytes,
        Err(_) => return eval_include_missing_file(&path, required, values),
    };
    context.mark_included_file(include_key);
    eval_execute_include_bytes(&bytes, &resolved_path, context, scope, values)
}

/// Returns the include/require result for a file that cannot be opened.
fn eval_include_missing_file(
    path: &str,
    required: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let construct = if required { "require" } else { "include" };
    values.warning(&format!(
        "Warning: {construct}({path}): Failed to open stream: No such file or directory\n"
    ))?;
    values.warning(&format!(
        "Warning: {construct}(): Failed opening '{path}' for inclusion\n"
    ))?;
    if required {
        Err(EvalStatus::RuntimeFatal)
    } else {
        values.bool_value(false)
    }
}

/// Resolves eval include paths using PHP's cwd-first and caller-directory fallback.
fn eval_resolve_include_path(path: &str, context: &ElephcEvalContext) -> std::path::PathBuf {
    let raw_path = std::path::Path::new(path);
    if raw_path.is_absolute() || raw_path.exists() {
        return raw_path.to_path_buf();
    }
    if context.call_dir().is_empty() {
        return raw_path.to_path_buf();
    }
    let caller_path = std::path::Path::new(context.call_dir()).join(raw_path);
    if caller_path.exists() {
        caller_path
    } else {
        raw_path.to_path_buf()
    }
}

/// Builds the stable include_once key for a resolved path.
fn eval_include_key(path: &std::path::Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

/// Executes a local include file, alternating raw output and PHP code blocks.
fn eval_execute_include_bytes(
    bytes: &[u8],
    path: &std::path::Path,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut cursor = 0;
    while let Some((tag_start, code_start)) = eval_find_php_open_tag(bytes, cursor) {
        eval_echo_include_bytes(&bytes[cursor..tag_start], values)?;
        let close = eval_find_php_close_tag(bytes, code_start);
        let code_end = close.unwrap_or(bytes.len());
        match eval_execute_include_code(&bytes[code_start..code_end], path, context, scope, values)?
        {
            EvalControl::None => {}
            EvalControl::Return(value) => return Ok(value),
            EvalControl::Throw(value) => {
                context.set_pending_throw(value);
                return Err(EvalStatus::UncaughtThrowable);
            }
            EvalControl::Break | EvalControl::Continue => {
                return Err(EvalStatus::UnsupportedConstruct);
            }
        }
        let Some(close) = close else {
            return values.int(1);
        };
        cursor = close + 2;
    }
    eval_echo_include_bytes(&bytes[cursor..], values)?;
    values.int(1)
}

/// Parses and executes one PHP code block from an included file.
fn eval_execute_include_code(
    code: &[u8],
    path: &std::path::Path,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    let program = parse_fragment(code).map_err(EvalParseError::status)?;
    let previous = context.call_site();
    let file = path.to_string_lossy().into_owned();
    let dir = path
        .parent()
        .map(|parent| parent.to_string_lossy().into_owned())
        .unwrap_or_default();
    context.set_call_site(file.clone(), dir, 1);
    context.set_file_magic_override(Some(file));
    let result = execute_statements(program.statements(), context, scope, values);
    context.set_call_site(previous.0, previous.1, previous.2);
    context.set_file_magic_override(previous.3);
    result
}

/// Echoes raw non-PHP include bytes through the eval value hooks.
fn eval_echo_include_bytes(
    bytes: &[u8],
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if bytes.is_empty() {
        return Ok(());
    }
    let output = values.string_bytes_value(bytes)?;
    values.echo(output)
}

/// Finds the next `<?php` opening tag and returns tag and code byte offsets.
fn eval_find_php_open_tag(bytes: &[u8], start: usize) -> Option<(usize, usize)> {
    bytes
        .get(start..)?
        .windows(5)
        .position(eval_is_php_open_tag)
        .map(|offset| {
            let tag_start = start + offset;
            (tag_start, tag_start + 5)
        })
}

/// Returns true when a five-byte window is a case-insensitive `<?php` tag.
fn eval_is_php_open_tag(window: &[u8]) -> bool {
    window.len() == 5
        && window[0] == b'<'
        && window[1] == b'?'
        && window[2].eq_ignore_ascii_case(&b'p')
        && window[3].eq_ignore_ascii_case(&b'h')
        && window[4].eq_ignore_ascii_case(&b'p')
}

/// Finds the next PHP closing tag after a code block start.
fn eval_find_php_close_tag(bytes: &[u8], start: usize) -> Option<usize> {
    bytes
        .get(start..)?
        .windows(2)
        .position(|window| window == b"?>")
        .map(|offset| start + offset)
}

/// Evaluates the builtin `strlen(...)` for one PHP-coerced string argument.
fn eval_builtin_strlen(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    let bytes = values.string_bytes(value)?;
    let len = i64::try_from(bytes.len()).map_err(|_| EvalStatus::RuntimeFatal)?;
    values.int(len)
}

/// Evaluates the builtin `ord(...)` for the first byte of one coerced string.
fn eval_builtin_ord(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_ord_result(value, values)
}

/// Returns the first byte of one converted string, or zero for an empty string.
fn eval_ord_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    values.int(i64::from(bytes.first().copied().unwrap_or(0)))
}

/// Evaluates the builtin `count(...)` for one runtime array-like argument.
fn eval_builtin_count(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value] => {
            let value = eval_expr(value, context, scope, values)?;
            eval_count_result(value, None, values)
        }
        [value, mode] => {
            let value = eval_expr(value, context, scope, values)?;
            let mode = eval_expr(mode, context, scope, values)?;
            eval_count_result(value, Some(mode), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Counts an eval array with PHP normal or recursive mode semantics.
fn eval_count_result(
    value: RuntimeCellHandle,
    mode: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mode = match mode {
        Some(mode) => eval_int_value(mode, values)?,
        None => EVAL_COUNT_NORMAL,
    };
    let len = match mode {
        EVAL_COUNT_NORMAL => values.array_len(value)?,
        EVAL_COUNT_RECURSIVE => eval_count_recursive_len(value, values, &mut Vec::new())?,
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let len = i64::try_from(len).map_err(|_| EvalStatus::RuntimeFatal)?;
    values.int(len)
}

/// Recursively counts nested eval arrays for `count($value, COUNT_RECURSIVE)`.
fn eval_count_recursive_len(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
    arrays_seen: &mut Vec<usize>,
) -> Result<usize, EvalStatus> {
    let address = value.as_ptr() as usize;
    if arrays_seen.contains(&address) {
        return Ok(0);
    }
    arrays_seen.push(address);

    let len = values.array_len(value)?;
    let mut total = len;
    for position in 0..len {
        let key = values.array_iter_key(value, position)?;
        let element = values.array_get(value, key)?;
        if values.is_array_like(element)? {
            total = total
                .checked_add(eval_count_recursive_len(element, values, arrays_seen)?)
                .ok_or(EvalStatus::RuntimeFatal)?;
        }
    }

    arrays_seen.pop();
    Ok(total)
}

/// Evaluates PHP `json_encode()` for zero-flag scalar and array values.
fn eval_builtin_json_encode(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value] => {
            let value = eval_expr(value, context, scope, values)?;
            eval_json_encode_result(value, None, None, context, values)
        }
        [value, flags] => {
            let value = eval_expr(value, context, scope, values)?;
            let flags = eval_expr(flags, context, scope, values)?;
            eval_json_encode_result(value, Some(flags), None, context, values)
        }
        [value, flags, depth] => {
            let value = eval_expr(value, context, scope, values)?;
            let flags = eval_expr(flags, context, scope, values)?;
            let depth = eval_expr(depth, context, scope, values)?;
            eval_json_encode_result(value, Some(flags), Some(depth), context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Encodes one runtime cell as a JSON string for eval's supported flag subset.
fn eval_json_encode_result(
    value: RuntimeCellHandle,
    flags: Option<RuntimeCellHandle>,
    depth: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let flags = flags
        .map(|flags| eval_int_value(flags, values))
        .transpose()?
        .unwrap_or(0);
    let supported_flags = EVAL_JSON_HEX_TAG
        | EVAL_JSON_HEX_AMP
        | EVAL_JSON_HEX_APOS
        | EVAL_JSON_HEX_QUOT
        | EVAL_JSON_UNESCAPED_SLASHES
        | EVAL_JSON_UNESCAPED_UNICODE
        | EVAL_JSON_FORCE_OBJECT
        | EVAL_JSON_PRETTY_PRINT
        | EVAL_JSON_PARTIAL_OUTPUT_ON_ERROR
        | EVAL_JSON_PRESERVE_ZERO_FRACTION
        | EVAL_JSON_INVALID_UTF8_IGNORE
        | EVAL_JSON_INVALID_UTF8_SUBSTITUTE
        | EVAL_JSON_THROW_ON_ERROR;
    let supported_flags = supported_flags | EVAL_JSON_NUMERIC_CHECK;
    if flags & !supported_flags != 0 {
        return Err(EvalStatus::UnsupportedConstruct);
    }
    let depth = depth
        .map(|depth| eval_int_value(depth, values))
        .transpose()?
        .unwrap_or(512);
    if depth <= 0 {
        return Err(EvalStatus::RuntimeFatal);
    }

    let mut output = Vec::new();
    let mut error = None;
    eval_json_encode_append(
        value,
        values,
        flags,
        depth as usize,
        0,
        &mut Vec::new(),
        &mut error,
        &mut output,
    )?;
    if let Some(error) = error {
        context.set_json_error(error.code, error.message);
        if flags & EVAL_JSON_PARTIAL_OUTPUT_ON_ERROR == 0 {
            if flags & EVAL_JSON_THROW_ON_ERROR != 0 {
                return eval_throw_json_exception(error.code, error.message, context, values);
            }
            return values.bool_value(false);
        }
    } else {
        context.clear_json_error();
    }
    values.string_bytes_value(&output)
}

/// Evaluates PHP `json_decode()` for eval-supported JSON text and flags.
fn eval_builtin_json_decode(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [json] => {
            let json = eval_expr(json, context, scope, values)?;
            eval_json_decode_result(json, None, None, None, context, values)
        }
        [json, associative] => {
            let json = eval_expr(json, context, scope, values)?;
            let associative = eval_expr(associative, context, scope, values)?;
            eval_json_decode_result(json, Some(associative), None, None, context, values)
        }
        [json, associative, depth] => {
            let json = eval_expr(json, context, scope, values)?;
            let associative = eval_expr(associative, context, scope, values)?;
            let depth = eval_expr(depth, context, scope, values)?;
            eval_json_decode_result(json, Some(associative), Some(depth), None, context, values)
        }
        [json, associative, depth, flags] => {
            let json = eval_expr(json, context, scope, values)?;
            let associative = eval_expr(associative, context, scope, values)?;
            let depth = eval_expr(depth, context, scope, values)?;
            let flags = eval_expr(flags, context, scope, values)?;
            eval_json_decode_result(
                json,
                Some(associative),
                Some(depth),
                Some(flags),
                context,
                values,
            )
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Decodes one JSON string into eval runtime cells and records PHP JSON parse state.
fn eval_json_decode_result(
    json: RuntimeCellHandle,
    associative: Option<RuntimeCellHandle>,
    depth: Option<RuntimeCellHandle>,
    flags: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let flags = flags
        .map(|flags| eval_int_value(flags, values))
        .transpose()?
        .unwrap_or(0);
    let supported_flags = EVAL_JSON_BIGINT_AS_STRING
        | EVAL_JSON_INVALID_UTF8_IGNORE
        | EVAL_JSON_INVALID_UTF8_SUBSTITUTE
        | EVAL_JSON_THROW_ON_ERROR;
    if flags & !supported_flags != 0 {
        return Err(EvalStatus::UnsupportedConstruct);
    }
    let objects_as_assoc = associative
        .map(|associative| values.truthy(associative))
        .transpose()?
        .unwrap_or(false);
    let depth = depth
        .map(|depth| eval_int_value(depth, values))
        .transpose()?
        .unwrap_or(512);
    if depth <= 0 {
        return Err(EvalStatus::RuntimeFatal);
    }

    let bytes = values.string_bytes(json)?;
    let decoded_result = if flags & EVAL_JSON_INVALID_UTF8_SUBSTITUTE != 0 {
        json_validate::decode_result_substituting_invalid_utf8(&bytes, depth as usize)
    } else if flags & EVAL_JSON_INVALID_UTF8_IGNORE != 0 {
        json_validate::decode_result_ignoring_invalid_utf8(&bytes, depth as usize)
    } else {
        json_validate::decode_result(&bytes, depth as usize)
    };
    let decoded = match decoded_result {
        Ok(decoded) => decoded,
        Err(error) => {
            let (code, message) = eval_json_parse_error_details(error, &bytes);
            if flags & EVAL_JSON_THROW_ON_ERROR != 0 {
                return eval_throw_json_exception(code, &message, context, values);
            }
            context.set_json_error(code, message);
            return values.null();
        }
    };
    context.clear_json_error();
    eval_json_decode_to_cell(decoded, flags, objects_as_assoc, values)
}

/// Materializes one parsed JSON value as an eval runtime cell.
fn eval_json_decode_to_cell(
    value: JsonValue,
    flags: i64,
    objects_as_assoc: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match value {
        JsonValue::Null => values.null(),
        JsonValue::Bool(value) => values.bool_value(value),
        JsonValue::Number(value) => eval_json_decode_number_to_cell(&value, flags, values),
        JsonValue::String(value) => values.string_bytes_value(&value),
        JsonValue::Array(elements) => {
            let mut result = values.array_new(elements.len())?;
            for (index, element) in elements.into_iter().enumerate() {
                let index = i64::try_from(index).map_err(|_| EvalStatus::RuntimeFatal)?;
                let key = values.int(index)?;
                let element = eval_json_decode_to_cell(element, flags, objects_as_assoc, values)?;
                result = values.array_set(result, key, element)?;
            }
            Ok(result)
        }
        JsonValue::Object(entries) => {
            if !objects_as_assoc {
                return eval_json_decode_object_to_cell(entries, flags, values);
            }
            let mut result = values.assoc_new(entries.len())?;
            for (key, value) in entries {
                let key = values.string_bytes_value(&key)?;
                let value = eval_json_decode_to_cell(value, flags, objects_as_assoc, values)?;
                result = values.array_set(result, key, value)?;
            }
            Ok(result)
        }
    }
}

/// Materializes a parsed JSON object as a `stdClass` runtime object.
fn eval_json_decode_object_to_cell(
    entries: Vec<(Vec<u8>, JsonValue)>,
    flags: i64,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let object = values.new_object("stdClass")?;
    for (key, value) in entries {
        let key = std::str::from_utf8(&key).map_err(|_| EvalStatus::RuntimeFatal)?;
        let value = eval_json_decode_to_cell(value, flags, false, values)?;
        values.property_set(object, key, value)?;
    }
    Ok(object)
}

/// Materializes one JSON number as an int when possible and as a float otherwise.
fn eval_json_decode_number_to_cell(
    value: &[u8],
    flags: i64,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if flags & EVAL_JSON_BIGINT_AS_STRING != 0 && eval_json_number_overflows_i64(value) {
        return values.string_bytes_value(value);
    }
    let value = std::str::from_utf8(value).map_err(|_| EvalStatus::RuntimeFatal)?;
    if !value.bytes().any(|byte| matches!(byte, b'.' | b'e' | b'E')) {
        if let Ok(integer) = value.parse::<i64>() {
            return values.int(integer);
        }
    }
    let float = value.parse::<f64>().map_err(|_| EvalStatus::RuntimeFatal)?;
    values.float(float)
}

/// Returns true when one integer-grammar JSON number exceeds PHP's int range.
fn eval_json_number_overflows_i64(value: &[u8]) -> bool {
    if value.iter().any(|byte| matches!(*byte, b'.' | b'e' | b'E')) {
        return false;
    }
    let (negative, digits) = if let Some(digits) = value.strip_prefix(b"-") {
        (true, digits)
    } else {
        (false, value)
    };
    let threshold = if negative {
        b"9223372036854775808".as_slice()
    } else {
        b"9223372036854775807".as_slice()
    };
    digits.len() > threshold.len() || digits.len() == threshold.len() && digits > threshold
}

/// Evaluates PHP `json_last_error()` from the eval interpreter's current JSON state.
fn eval_builtin_json_last_error(
    args: &[EvalExpr],
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    values.int(context.json_last_error())
}

/// Evaluates PHP `json_last_error_msg()` from the eval interpreter's current JSON state.
fn eval_builtin_json_last_error_msg(
    args: &[EvalExpr],
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    values.string(context.json_last_error_msg())
}

/// Evaluates PHP `json_validate()` for zero-flag JSON text validation.
fn eval_builtin_json_validate(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [json] => {
            let json = eval_expr(json, context, scope, values)?;
            eval_json_validate_result(json, None, None, context, values)
        }
        [json, depth] => {
            let json = eval_expr(json, context, scope, values)?;
            let depth = eval_expr(depth, context, scope, values)?;
            eval_json_validate_result(json, Some(depth), None, context, values)
        }
        [json, depth, flags] => {
            let json = eval_expr(json, context, scope, values)?;
            let depth = eval_expr(depth, context, scope, values)?;
            let flags = eval_expr(flags, context, scope, values)?;
            eval_json_validate_result(json, Some(depth), Some(flags), context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Validates JSON text with eval's current zero-flag JSON subset and records JSON state.
fn eval_json_validate_result(
    json: RuntimeCellHandle,
    depth: Option<RuntimeCellHandle>,
    flags: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let flags = flags
        .map(|flags| eval_int_value(flags, values))
        .transpose()?
        .unwrap_or(0);
    if flags & !EVAL_JSON_INVALID_UTF8_IGNORE != 0 {
        return Err(EvalStatus::UnsupportedConstruct);
    }
    let depth = depth
        .map(|depth| eval_int_value(depth, values))
        .transpose()?
        .unwrap_or(512);
    if depth <= 0 {
        return Err(EvalStatus::RuntimeFatal);
    }

    let bytes = values.string_bytes(json)?;
    let result = if flags & EVAL_JSON_INVALID_UTF8_IGNORE != 0 {
        json_validate::decode_result_ignoring_invalid_utf8(&bytes, depth as usize)
    } else {
        json_validate::decode_result(&bytes, depth as usize)
    };
    match result {
        Ok(_) => {
            context.clear_json_error();
            values.bool_value(true)
        }
        Err(error) => {
            eval_record_json_parse_error(context, error, &bytes);
            values.bool_value(false)
        }
    }
}

/// Records one parser error into the eval-local PHP JSON error slots.
fn eval_record_json_parse_error(
    context: &mut ElephcEvalContext,
    error: JsonParseError,
    bytes: &[u8],
) {
    let (code, message) = eval_json_parse_error_details(error, bytes);
    context.set_json_error(code, message);
}

/// Builds the PHP JSON error code and message for one parser failure.
fn eval_json_parse_error_details(error: JsonParseError, bytes: &[u8]) -> (i64, String) {
    let (code, message) = eval_json_parse_error_status(error.kind());
    let message = eval_json_error_message_with_location(message, bytes, error.offset());
    (code, message)
}

/// Creates and schedules a `JsonException` through eval's normal Throwable channel.
fn eval_throw_json_exception(
    code: i64,
    message: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    context.set_json_error(code, message.to_string());
    let exception = values.new_object("JsonException")?;
    let message = values.string(message)?;
    let code = values.int(code)?;
    values.construct_object(exception, vec![message, code])?;
    context.set_pending_throw(exception);
    Err(EvalStatus::UncaughtThrowable)
}

/// Maps eval JSON parser failures to PHP `JSON_ERROR_*` codes and messages.
fn eval_json_parse_error_status(error: JsonParseErrorKind) -> (i64, &'static str) {
    match error {
        JsonParseErrorKind::Depth => (EVAL_JSON_ERROR_DEPTH, "Maximum stack depth exceeded"),
        JsonParseErrorKind::Syntax => (EVAL_JSON_ERROR_SYNTAX, "Syntax error"),
        JsonParseErrorKind::ControlChar => (
            EVAL_JSON_ERROR_CTRL_CHAR,
            "Control character error, possibly incorrectly encoded",
        ),
        JsonParseErrorKind::Utf8 => (EVAL_JSON_ERROR_UTF8, EVAL_JSON_UTF8_MESSAGE),
        JsonParseErrorKind::Utf16 => (
            EVAL_JSON_ERROR_UTF16,
            "Single unpaired UTF-16 surrogate in unicode escape",
        ),
    }
}

/// Adds PHP's JSON line/column suffix to one base error message.
fn eval_json_error_message_with_location(message: &str, bytes: &[u8], offset: usize) -> String {
    let (line, column) = eval_json_error_location(bytes, offset);
    format!("{message} near location {line}:{column}")
}

/// Converts a zero-based JSON byte offset into PHP-style one-based line and column.
fn eval_json_error_location(bytes: &[u8], offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut column = 1;
    let offset = offset.min(bytes.len());
    for byte in &bytes[..offset] {
        if *byte == b'\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (line, column)
}

/// Appends one JSON value to the output buffer.
fn eval_json_encode_append(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
    flags: i64,
    depth_limit: usize,
    depth: usize,
    arrays_seen: &mut Vec<usize>,
    error: &mut Option<EvalJsonEncodeError>,
    output: &mut Vec<u8>,
) -> Result<(), EvalStatus> {
    match values.type_tag(value)? {
        EVAL_TAG_INT => output.extend_from_slice(&values.string_bytes(value)?),
        EVAL_TAG_FLOAT => {
            eval_json_encode_append_float(value, values, flags, error, output)?;
        }
        EVAL_TAG_STRING => eval_json_encode_append_string(
            &values.string_bytes(value)?,
            flags,
            EvalJsonStringPosition::Value,
            error,
            output,
        )?,
        EVAL_TAG_BOOL => {
            if values.truthy(value)? {
                output.extend_from_slice(b"true");
            } else {
                output.extend_from_slice(b"false");
            }
        }
        EVAL_TAG_ARRAY => {
            eval_json_encode_append_indexed_array(
                value,
                values,
                flags,
                depth_limit,
                depth,
                arrays_seen,
                error,
                output,
            )?;
        }
        EVAL_TAG_ASSOC => {
            eval_json_encode_append_assoc(
                value,
                values,
                flags,
                depth_limit,
                depth,
                arrays_seen,
                error,
                output,
            )?;
        }
        EVAL_TAG_OBJECT => {
            eval_json_encode_append_object(
                value,
                values,
                flags,
                depth_limit,
                depth,
                arrays_seen,
                error,
                output,
            )?;
        }
        EVAL_TAG_NULL | EVAL_TAG_RESOURCE => output.extend_from_slice(b"null"),
        _ => return Err(EvalStatus::UnsupportedConstruct),
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct EvalJsonEncodeError {
    code: i64,
    message: &'static str,
}

/// Marks whether a JSON string is being encoded as a value or as an object key.
#[derive(Clone, Copy)]
enum EvalJsonStringPosition {
    Value,
    Key,
}

/// Appends one JSON float while preserving a `.0` suffix when requested.
fn eval_json_encode_append_float(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
    flags: i64,
    error: &mut Option<EvalJsonEncodeError>,
    output: &mut Vec<u8>,
) -> Result<(), EvalStatus> {
    let float = eval_float_value(value, values)?;
    if !float.is_finite() {
        *error = Some(EvalJsonEncodeError {
            code: EVAL_JSON_ERROR_INF_OR_NAN,
            message: EVAL_JSON_INF_OR_NAN_MESSAGE,
        });
        output.push(b'0');
        return Ok(());
    }
    let bytes = values.string_bytes(value)?;
    output.extend_from_slice(&bytes);
    if flags & EVAL_JSON_PRESERVE_ZERO_FRACTION != 0
        && !bytes.iter().any(|byte| matches!(*byte, b'.' | b'e' | b'E'))
    {
        output.extend_from_slice(b".0");
    }
    Ok(())
}

/// Appends one indexed eval array as a JSON array or forced JSON object.
fn eval_json_encode_append_indexed_array(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
    flags: i64,
    depth_limit: usize,
    depth: usize,
    arrays_seen: &mut Vec<usize>,
    error: &mut Option<EvalJsonEncodeError>,
    output: &mut Vec<u8>,
) -> Result<(), EvalStatus> {
    eval_json_encode_enter_array(value, depth_limit, depth, arrays_seen)?;
    let force_object = flags & EVAL_JSON_FORCE_OBJECT != 0;
    let pretty = flags & EVAL_JSON_PRETTY_PRINT != 0;
    output.push(if force_object { b'{' } else { b'[' });
    let len = values.array_len(value)?;
    if pretty && len > 0 {
        output.push(b'\n');
    }
    for position in 0..len {
        if position > 0 {
            output.push(b',');
            if pretty {
                output.push(b'\n');
            }
        }
        if pretty {
            eval_json_encode_pretty_indent(output, depth + 1);
        }
        let key = values.array_iter_key(value, position)?;
        if force_object {
            eval_json_encode_append_string(
                &values.string_bytes(key)?,
                flags & !EVAL_JSON_NUMERIC_CHECK,
                EvalJsonStringPosition::Key,
                error,
                output,
            )?;
            eval_json_encode_append_colon(flags, output);
        }
        let element = values.array_get(value, key)?;
        eval_json_encode_append(
            element,
            values,
            flags,
            depth_limit,
            depth + 1,
            arrays_seen,
            error,
            output,
        )?;
    }
    if pretty && len > 0 {
        output.push(b'\n');
        eval_json_encode_pretty_indent(output, depth);
    }
    output.push(if force_object { b'}' } else { b']' });
    arrays_seen.pop();
    Ok(())
}

/// Appends one associative eval array as a JSON object.
fn eval_json_encode_append_assoc(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
    flags: i64,
    depth_limit: usize,
    depth: usize,
    arrays_seen: &mut Vec<usize>,
    error: &mut Option<EvalJsonEncodeError>,
    output: &mut Vec<u8>,
) -> Result<(), EvalStatus> {
    eval_json_encode_enter_array(value, depth_limit, depth, arrays_seen)?;
    let pretty = flags & EVAL_JSON_PRETTY_PRINT != 0;
    output.push(b'{');
    let len = values.array_len(value)?;
    if pretty && len > 0 {
        output.push(b'\n');
    }
    for position in 0..len {
        if position > 0 {
            output.push(b',');
            if pretty {
                output.push(b'\n');
            }
        }
        if pretty {
            eval_json_encode_pretty_indent(output, depth + 1);
        }
        let key = values.array_iter_key(value, position)?;
        eval_json_encode_append_string(
            &values.string_bytes(key)?,
            flags & !EVAL_JSON_NUMERIC_CHECK,
            EvalJsonStringPosition::Key,
            error,
            output,
        )?;
        eval_json_encode_append_colon(flags, output);
        let element = values.array_get(value, key)?;
        eval_json_encode_append(
            element,
            values,
            flags,
            depth_limit,
            depth + 1,
            arrays_seen,
            error,
            output,
        )?;
    }
    if pretty && len > 0 {
        output.push(b'\n');
        eval_json_encode_pretty_indent(output, depth);
    }
    output.push(b'}');
    arrays_seen.pop();
    Ok(())
}

/// Appends one eval runtime object as a JSON object.
fn eval_json_encode_append_object(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
    flags: i64,
    depth_limit: usize,
    depth: usize,
    arrays_seen: &mut Vec<usize>,
    error: &mut Option<EvalJsonEncodeError>,
    output: &mut Vec<u8>,
) -> Result<(), EvalStatus> {
    eval_json_encode_enter_array(value, depth_limit, depth, arrays_seen)?;
    let pretty = flags & EVAL_JSON_PRETTY_PRINT != 0;
    output.push(b'{');
    let len = values.object_property_len(value)?;
    if pretty && len > 0 {
        output.push(b'\n');
    }
    for position in 0..len {
        if position > 0 {
            output.push(b',');
            if pretty {
                output.push(b'\n');
            }
        }
        if pretty {
            eval_json_encode_pretty_indent(output, depth + 1);
        }
        let key = values.object_property_iter_key(value, position)?;
        let key_bytes = values.string_bytes(key)?;
        eval_json_encode_append_string(
            &key_bytes,
            flags & !EVAL_JSON_NUMERIC_CHECK,
            EvalJsonStringPosition::Key,
            error,
            output,
        )?;
        eval_json_encode_append_colon(flags, output);
        let property = std::str::from_utf8(&key_bytes).map_err(|_| EvalStatus::RuntimeFatal)?;
        let element = values.property_get(value, property)?;
        eval_json_encode_append(
            element,
            values,
            flags,
            depth_limit,
            depth + 1,
            arrays_seen,
            error,
            output,
        )?;
    }
    if pretty && len > 0 {
        output.push(b'\n');
        eval_json_encode_pretty_indent(output, depth);
    }
    output.push(b'}');
    arrays_seen.pop();
    Ok(())
}

/// Appends a JSON object colon, including pretty-print spacing when active.
fn eval_json_encode_append_colon(flags: i64, output: &mut Vec<u8>) {
    if flags & EVAL_JSON_PRETTY_PRINT != 0 {
        output.extend_from_slice(b": ");
    } else {
        output.push(b':');
    }
}

/// Appends PHP's four-space JSON pretty-print indentation for one nesting level.
fn eval_json_encode_pretty_indent(output: &mut Vec<u8>, depth: usize) {
    for _ in 0..depth {
        output.extend_from_slice(b"    ");
    }
}

/// Records entry into one JSON array/object, rejecting depth overrun and recursion.
fn eval_json_encode_enter_array(
    value: RuntimeCellHandle,
    depth_limit: usize,
    depth: usize,
    arrays_seen: &mut Vec<usize>,
) -> Result<(), EvalStatus> {
    if depth >= depth_limit {
        return Err(EvalStatus::RuntimeFatal);
    }
    let address = value.as_ptr() as usize;
    if arrays_seen.contains(&address) {
        return Err(EvalStatus::RuntimeFatal);
    }
    arrays_seen.push(address);
    Ok(())
}

/// Appends one JSON string with eval-supported PHP flag handling.
fn eval_json_encode_append_string(
    bytes: &[u8],
    flags: i64,
    position: EvalJsonStringPosition,
    error: &mut Option<EvalJsonEncodeError>,
    output: &mut Vec<u8>,
) -> Result<(), EvalStatus> {
    if flags & EVAL_JSON_NUMERIC_CHECK != 0 {
        if let Some(number) = eval_json_numeric_check_bytes(bytes) {
            output.extend_from_slice(&number);
            return Ok(());
        }
    }
    let start_len = output.len();
    output.push(b'"');
    if let Ok(value) = std::str::from_utf8(bytes) {
        for character in value.chars() {
            eval_json_encode_append_char(character, flags, output);
        }
    } else if flags & (EVAL_JSON_INVALID_UTF8_IGNORE | EVAL_JSON_INVALID_UTF8_SUBSTITUTE) == 0 {
        output.truncate(start_len);
        *error = Some(EvalJsonEncodeError {
            code: EVAL_JSON_ERROR_UTF8,
            message: EVAL_JSON_UTF8_MESSAGE,
        });
        match position {
            EvalJsonStringPosition::Value => output.extend_from_slice(b"null"),
            EvalJsonStringPosition::Key => output.extend_from_slice(b"\"\""),
        }
        return Ok(());
    } else {
        eval_json_encode_append_invalid_utf8_bytes(bytes, flags, output)?;
    }
    output.push(b'"');
    Ok(())
}

/// Appends one valid UTF-8 character using PHP JSON string escaping rules.
fn eval_json_encode_append_char(character: char, flags: i64, output: &mut Vec<u8>) {
    if character.is_ascii() {
        eval_json_encode_append_ascii_byte(character as u8, flags, output);
    } else if flags & EVAL_JSON_UNESCAPED_UNICODE != 0 {
        let mut buffer = [0_u8; 4];
        output.extend_from_slice(character.encode_utf8(&mut buffer).as_bytes());
    } else {
        eval_json_encode_append_unicode_escape(character as u32, output);
    }
}

/// Appends one ASCII byte using JSON escaping rules shared by UTF-8 and fallback paths.
fn eval_json_encode_append_ascii_byte(byte: u8, flags: i64, output: &mut Vec<u8>) {
    match byte {
        b'"' if flags & EVAL_JSON_HEX_QUOT != 0 => output.extend_from_slice(b"\\u0022"),
        b'"' => output.extend_from_slice(b"\\\""),
        b'\\' => output.extend_from_slice(b"\\\\"),
        b'/' if flags & EVAL_JSON_UNESCAPED_SLASHES == 0 => {
            output.extend_from_slice(b"\\/");
        }
        b'/' => output.push(b'/'),
        b'<' if flags & EVAL_JSON_HEX_TAG != 0 => output.extend_from_slice(b"\\u003C"),
        b'>' if flags & EVAL_JSON_HEX_TAG != 0 => output.extend_from_slice(b"\\u003E"),
        b'&' if flags & EVAL_JSON_HEX_AMP != 0 => output.extend_from_slice(b"\\u0026"),
        b'\'' if flags & EVAL_JSON_HEX_APOS != 0 => output.extend_from_slice(b"\\u0027"),
        b'\x08' => output.extend_from_slice(b"\\b"),
        b'\x0c' => output.extend_from_slice(b"\\f"),
        b'\n' => output.extend_from_slice(b"\\n"),
        b'\r' => output.extend_from_slice(b"\\r"),
        b'\t' => output.extend_from_slice(b"\\t"),
        control @ 0x00..=0x1f => {
            output.extend_from_slice(format!("\\u{control:04x}").as_bytes());
        }
        _ => output.push(byte),
    }
}

/// Appends valid scalar values as PHP JSON `\uXXXX` escapes, using surrogate pairs when needed.
fn eval_json_encode_append_unicode_escape(codepoint: u32, output: &mut Vec<u8>) {
    if codepoint <= 0xffff {
        output.extend_from_slice(format!("\\u{codepoint:04x}").as_bytes());
        return;
    }

    let codepoint = codepoint - 0x1_0000;
    let high = 0xd800 + ((codepoint >> 10) & 0x3ff);
    let low = 0xdc00 + (codepoint & 0x3ff);
    output.extend_from_slice(format!("\\u{high:04x}\\u{low:04x}").as_bytes());
}

/// Appends malformed UTF-8 bytes according to PHP's JSON invalid-UTF-8 flags.
fn eval_json_encode_append_invalid_utf8_bytes(
    mut bytes: &[u8],
    flags: i64,
    output: &mut Vec<u8>,
) -> Result<(), EvalStatus> {
    while !bytes.is_empty() {
        match std::str::from_utf8(bytes) {
            Ok(value) => {
                for character in value.chars() {
                    eval_json_encode_append_char(character, flags, output);
                }
                return Ok(());
            }
            Err(error) => {
                let valid = &bytes[..error.valid_up_to()];
                for character in std::str::from_utf8(valid)
                    .map_err(|_| EvalStatus::RuntimeFatal)?
                    .chars()
                {
                    eval_json_encode_append_char(character, flags, output);
                }
                let invalid_len = error
                    .error_len()
                    .unwrap_or(bytes.len() - valid.len())
                    .max(1);
                if flags & EVAL_JSON_INVALID_UTF8_IGNORE == 0 {
                    eval_json_encode_append_char('\u{fffd}', flags, output);
                }
                bytes = &bytes[valid.len() + invalid_len.min(bytes.len() - valid.len())..];
            }
        }
    }
    Ok(())
}

/// Returns the JSON number bytes for a PHP numeric string when `JSON_NUMERIC_CHECK` applies.
fn eval_json_numeric_check_bytes(bytes: &[u8]) -> Option<Vec<u8>> {
    let value = std::str::from_utf8(bytes).ok()?.trim();
    if value.is_empty() {
        return None;
    }
    let integer_grammar = value
        .bytes()
        .all(|byte| byte.is_ascii_digit() || matches!(byte, b'+' | b'-'));
    if integer_grammar {
        if let Ok(integer) = value.parse::<i64>() {
            return Some(integer.to_string().into_bytes());
        }
    }
    let number = value.parse::<f64>().ok()?;
    if number.is_finite() {
        Some(number.to_string().into_bytes())
    } else {
        None
    }
}

/// Evaluates PHP `print_r()` over one eval expression.
fn eval_builtin_print_r(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_print_r_result(value, values)
}

/// Emits one eval value using elephc's supported `print_r()` output shape.
fn eval_print_r_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if matches!(values.type_tag(value)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        let output = values.string_bytes_value(b"Array\n")?;
        values.echo(output)?;
    } else {
        values.echo(value)?;
    }
    values.bool_value(true)
}

/// Evaluates PHP `var_dump()` over one eval expression and returns null.
fn eval_builtin_var_dump(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_var_dump_result(value, values)
}

/// Emits one eval value using PHP-style `var_dump()` debug formatting.
fn eval_var_dump_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut output = Vec::new();
    let mut arrays_seen = Vec::new();
    eval_var_dump_append_value(value, values, 0, &mut arrays_seen, &mut output)?;
    let output = values.string_bytes_value(&output)?;
    values.echo(output)?;
    values.null()
}

/// Appends one value and its nested array entries to a `var_dump()` byte buffer.
fn eval_var_dump_append_value(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
    depth: usize,
    arrays_seen: &mut Vec<usize>,
    output: &mut Vec<u8>,
) -> Result<(), EvalStatus> {
    match values.type_tag(value)? {
        EVAL_TAG_INT => eval_var_dump_append_scalar(b"int", value, values, depth, output),
        EVAL_TAG_STRING => eval_var_dump_append_string(value, values, depth, output),
        EVAL_TAG_FLOAT => eval_var_dump_append_scalar(b"float", value, values, depth, output),
        EVAL_TAG_BOOL => eval_var_dump_append_bool(value, values, depth, output),
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => {
            eval_var_dump_append_array(value, values, depth, arrays_seen, output)
        }
        EVAL_TAG_OBJECT => {
            eval_var_dump_append_indent(depth, output);
            output.extend_from_slice(b"object(Object)\n");
            Ok(())
        }
        EVAL_TAG_NULL => {
            eval_var_dump_append_indent(depth, output);
            output.extend_from_slice(b"NULL\n");
            Ok(())
        }
        EVAL_TAG_RESOURCE => {
            eval_var_dump_append_indent(depth, output);
            output.extend_from_slice(b"resource(0) of type (stream)\n");
            Ok(())
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Appends one integer-like or float-like `var_dump()` scalar line.
fn eval_var_dump_append_scalar(
    label: &[u8],
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
    depth: usize,
    output: &mut Vec<u8>,
) -> Result<(), EvalStatus> {
    eval_var_dump_append_indent(depth, output);
    output.extend_from_slice(label);
    output.extend_from_slice(b"(");
    output.extend_from_slice(&values.string_bytes(value)?);
    output.extend_from_slice(b")\n");
    Ok(())
}

/// Appends one string `var_dump()` line while preserving raw PHP string bytes.
fn eval_var_dump_append_string(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
    depth: usize,
    output: &mut Vec<u8>,
) -> Result<(), EvalStatus> {
    let bytes = values.string_bytes(value)?;
    eval_var_dump_append_indent(depth, output);
    output.extend_from_slice(b"string(");
    output.extend_from_slice(bytes.len().to_string().as_bytes());
    output.extend_from_slice(b") \"");
    output.extend_from_slice(&bytes);
    output.extend_from_slice(b"\"\n");
    Ok(())
}

/// Appends one boolean `var_dump()` line from PHP truthiness.
fn eval_var_dump_append_bool(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
    depth: usize,
    output: &mut Vec<u8>,
) -> Result<(), EvalStatus> {
    eval_var_dump_append_indent(depth, output);
    if values.truthy(value)? {
        output.extend_from_slice(b"bool(true)\n");
    } else {
        output.extend_from_slice(b"bool(false)\n");
    }
    Ok(())
}

/// Appends one array shell and recursively emits foreach-visible entries.
fn eval_var_dump_append_array(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
    depth: usize,
    arrays_seen: &mut Vec<usize>,
    output: &mut Vec<u8>,
) -> Result<(), EvalStatus> {
    let address = value.as_ptr() as usize;
    if arrays_seen.contains(&address) {
        eval_var_dump_append_indent(depth, output);
        output.extend_from_slice(b"*RECURSION*\n");
        return Ok(());
    }

    arrays_seen.push(address);
    let len = values.array_len(value)?;
    eval_var_dump_append_indent(depth, output);
    output.extend_from_slice(b"array(");
    output.extend_from_slice(len.to_string().as_bytes());
    output.extend_from_slice(b") {\n");
    for position in 0..len {
        let key = values.array_iter_key(value, position)?;
        let element = values.array_get(value, key)?;
        eval_var_dump_append_key(key, values, depth + 1, output)?;
        eval_var_dump_append_value(element, values, depth + 1, arrays_seen, output)?;
    }
    eval_var_dump_append_indent(depth, output);
    output.extend_from_slice(b"}\n");
    arrays_seen.pop();
    Ok(())
}

/// Appends one array key line for an indexed or associative `var_dump()` entry.
fn eval_var_dump_append_key(
    key: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
    depth: usize,
    output: &mut Vec<u8>,
) -> Result<(), EvalStatus> {
    eval_var_dump_append_indent(depth, output);
    output.extend_from_slice(b"[");
    match values.type_tag(key)? {
        EVAL_TAG_STRING => {
            output.extend_from_slice(b"\"");
            output.extend_from_slice(&values.string_bytes(key)?);
            output.extend_from_slice(b"\"");
        }
        _ => output.extend_from_slice(&values.string_bytes(key)?),
    }
    output.extend_from_slice(b"]=>\n");
    Ok(())
}

/// Appends the two-space indentation used by PHP `var_dump()` arrays.
fn eval_var_dump_append_indent(depth: usize, output: &mut Vec<u8>) {
    for _ in 0..depth {
        output.extend_from_slice(b"  ");
    }
}

/// Evaluates an eval-declared user function with PHP-style argument binding.
fn eval_dynamic_function(
    function: &EvalFunction,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    caller_scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let evaluated_args =
        eval_function_call_args(function.params(), args, context, caller_scope, values)?;
    eval_dynamic_function_with_values(function, evaluated_args, context, values)
}

/// Evaluates and binds function-like arguments to parameter order.
fn eval_function_call_args(
    params: &[String],
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    caller_scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<RuntimeCellHandle>, EvalStatus> {
    let evaluated_args = eval_call_arg_values(args, context, caller_scope, values)?;
    bind_evaluated_function_args(params, evaluated_args)
}

/// Evaluates source-order call arguments while preserving named-argument metadata.
fn eval_call_arg_values(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    caller_scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<EvaluatedCallArg>, EvalStatus> {
    let mut evaluated_args = Vec::with_capacity(args.len());
    let mut saw_named = false;

    for arg in args {
        if arg.is_spread() {
            if saw_named {
                return Err(EvalStatus::RuntimeFatal);
            }
            let spread = eval_expr(arg.value(), context, caller_scope, values)?;
            if !values.is_array_like(spread)? {
                return Err(EvalStatus::RuntimeFatal);
            }
            append_unpacked_call_arg_values(spread, &mut evaluated_args, &mut saw_named, values)?;
            continue;
        }

        if let Some(name) = arg.name() {
            saw_named = true;
            let value = eval_expr(arg.value(), context, caller_scope, values)?;
            evaluated_args.push(EvaluatedCallArg {
                name: Some(name.to_string()),
                value,
            });
            continue;
        }

        if saw_named {
            return Err(EvalStatus::RuntimeFatal);
        }
        let value = eval_expr(arg.value(), context, caller_scope, values)?;
        evaluated_args.push(EvaluatedCallArg { name: None, value });
    }

    Ok(evaluated_args)
}

/// Converts a `call_user_func_array` argument array into ordered call arguments.
fn eval_array_call_arg_values(
    arg_array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<EvaluatedCallArg>, EvalStatus> {
    let len = values.array_len(arg_array)?;
    let mut evaluated_args = Vec::with_capacity(len);
    let mut saw_named = false;
    append_unpacked_call_arg_values(arg_array, &mut evaluated_args, &mut saw_named, values)?;
    Ok(evaluated_args)
}

/// Appends one unpacked array's values using PHP named-argument key semantics.
fn append_unpacked_call_arg_values(
    array: RuntimeCellHandle,
    evaluated_args: &mut Vec<EvaluatedCallArg>,
    saw_named: &mut bool,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let len = values.array_len(array)?;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, key)?;
        match values.type_tag(key)? {
            EVAL_TAG_INT => {
                if *saw_named {
                    return Err(EvalStatus::RuntimeFatal);
                }
                evaluated_args.push(EvaluatedCallArg { name: None, value });
            }
            EVAL_TAG_STRING => {
                *saw_named = true;
                let name = values.string_bytes(key)?;
                let name = String::from_utf8(name).map_err(|_| EvalStatus::RuntimeFatal)?;
                evaluated_args.push(EvaluatedCallArg {
                    name: Some(name),
                    value,
                });
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        }
    }
    Ok(())
}

/// Binds evaluated positional and named values to declared parameter order.
fn bind_evaluated_function_args(
    params: &[String],
    evaluated_args: Vec<EvaluatedCallArg>,
) -> Result<Vec<RuntimeCellHandle>, EvalStatus> {
    let mut bound_args = vec![None; params.len()];
    let mut next_positional = 0;

    for arg in evaluated_args {
        if let Some(name) = arg.name {
            bind_dynamic_named_arg(params, &mut bound_args, &name, arg.value)?;
        } else {
            bind_dynamic_positional_arg(&mut bound_args, &mut next_positional, arg.value)?;
        }
    }

    bound_args
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or(EvalStatus::RuntimeFatal)
}

/// Binds one positional dynamic-call value to the next declared parameter slot.
fn bind_dynamic_positional_arg(
    bound_args: &mut [Option<RuntimeCellHandle>],
    next_positional: &mut usize,
    value: RuntimeCellHandle,
) -> Result<(), EvalStatus> {
    if *next_positional >= bound_args.len() || bound_args[*next_positional].is_some() {
        return Err(EvalStatus::RuntimeFatal);
    }
    bound_args[*next_positional] = Some(value);
    *next_positional += 1;
    Ok(())
}

/// Binds one named dynamic-call value to the matching declared parameter slot.
fn bind_dynamic_named_arg(
    params: &[String],
    bound_args: &mut [Option<RuntimeCellHandle>],
    name: &str,
    value: RuntimeCellHandle,
) -> Result<(), EvalStatus> {
    let Some(param_index) = params.iter().position(|param| param == name) else {
        return Err(EvalStatus::RuntimeFatal);
    };
    if bound_args[param_index].is_some() {
        return Err(EvalStatus::RuntimeFatal);
    }
    bound_args[param_index] = Some(value);
    Ok(())
}

/// Evaluates an eval-declared function after its positional arguments are prepared.
fn eval_dynamic_function_with_values(
    function: &EvalFunction,
    evaluated_args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut function_scope = ElephcEvalScope::new();
    for (name, value) in function.params().iter().zip(evaluated_args) {
        function_scope.set(name.clone(), value, ScopeCellOwnership::Borrowed);
    }
    let static_names = static_var_names(function.body());
    context.push_function(function.name());
    let result = execute_statements(function.body(), context, &mut function_scope, values);
    let persist_result = persist_static_locals(
        context,
        function.name(),
        &static_names,
        &function_scope,
        values,
    );
    context.pop_function();
    persist_result?;
    match result? {
        EvalControl::None => values.null(),
        EvalControl::Return(result) => Ok(result),
        EvalControl::Throw(result) => {
            context.set_pending_throw(result);
            Err(EvalStatus::UncaughtThrowable)
        }
        EvalControl::Break | EvalControl::Continue => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Persists static local variables from one eval-declared function activation.
fn persist_static_locals(
    context: &mut ElephcEvalContext,
    function_name: &str,
    names: &[String],
    scope: &ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for name in names {
        if let Some(cell) = scope.visible_cell(name) {
            if let Some(replaced) =
                context.set_static_local(function_name.to_string(), name.clone(), cell)
            {
                values.release(replaced)?;
            }
        }
    }
    Ok(())
}

/// Returns the distinct static local names declared anywhere in an eval function body.
fn static_var_names(body: &[EvalStmt]) -> Vec<String> {
    let mut names = std::collections::HashSet::new();
    collect_static_var_names(body, &mut names);
    names.into_iter().collect()
}

/// Recursively collects static local declaration names from eval statements.
fn collect_static_var_names(body: &[EvalStmt], names: &mut std::collections::HashSet<String>) {
    for stmt in body {
        match stmt {
            EvalStmt::StaticVar { name, .. } => {
                names.insert(name.clone());
            }
            EvalStmt::DoWhile { body, .. }
            | EvalStmt::Foreach { body, .. }
            | EvalStmt::For { body, .. }
            | EvalStmt::While { body, .. } => collect_static_var_names(body, names),
            EvalStmt::FunctionDecl { .. } => {}
            EvalStmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                collect_static_var_names(then_branch, names);
                collect_static_var_names(else_branch, names);
            }
            EvalStmt::Switch { cases, .. } => {
                for case in cases {
                    collect_static_var_names(&case.body, names);
                }
            }
            EvalStmt::Try {
                body,
                catches,
                finally_body,
            } => {
                collect_static_var_names(body, names);
                for catch in catches {
                    collect_static_var_names(&catch.body, names);
                }
                collect_static_var_names(finally_body, names);
            }
            EvalStmt::ArrayAppendVar { .. }
            | EvalStmt::ArraySetVar { .. }
            | EvalStmt::Break
            | EvalStmt::ClassDecl(_)
            | EvalStmt::Continue
            | EvalStmt::Echo(_)
            | EvalStmt::Expr(_)
            | EvalStmt::Global { .. }
            | EvalStmt::PropertySet { .. }
            | EvalStmt::ReferenceAssign { .. }
            | EvalStmt::Return(_)
            | EvalStmt::StoreVar { .. }
            | EvalStmt::Throw(_)
            | EvalStmt::UnsetVar { .. } => {}
        }
    }
}

/// Evaluates a registered AOT function through its descriptor-compatible invoker.
fn eval_native_function(
    function: NativeFunction,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    caller_scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let evaluated_args = if function.param_names().len() == function.param_count() {
        eval_function_call_args(function.param_names(), args, context, caller_scope, values)?
    } else {
        eval_positional_call_arg_values(args, context, caller_scope, values)?
    };
    eval_native_function_with_values(function, evaluated_args, values)
}

/// Invokes a registered AOT function after its positional arguments are prepared.
fn eval_native_function_with_values(
    function: NativeFunction,
    evaluated_args: Vec<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if evaluated_args.len() != function.param_count() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let arg_array = values.array_new(evaluated_args.len())?;
    for (index, value) in evaluated_args.into_iter().enumerate() {
        let index = values.int(index as i64)?;
        let _ = values.array_set(arg_array, index, value)?;
    }
    let result = unsafe { function.call(arg_array) };
    values.release(arg_array)?;
    if result.is_null() {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(result)
}

/// Evaluates an indexed array literal into a boxed runtime Mixed array.
fn eval_indexed_array(
    elements: &[EvalArrayElement],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let array = values.array_new(elements.len())?;
    for (index, element) in elements.iter().enumerate() {
        let EvalArrayElement::Value(element) = element else {
            return Err(EvalStatus::UnsupportedConstruct);
        };
        let index = values.int(index as i64)?;
        let value = eval_expr(element, context, scope, values)?;
        let _ = values.array_set(array, index, value)?;
    }
    Ok(array)
}

/// Evaluates an associative array literal into a boxed runtime Mixed hash.
fn eval_assoc_array(
    elements: &[EvalArrayElement],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let array = values.assoc_new(elements.len())?;
    let mut next_key = None;
    for element in elements {
        let (key, value) = match element {
            EvalArrayElement::Value(value) => {
                let key = match next_key {
                    Some(next_key) => next_key,
                    None => values.int(0)?,
                };
                let one = values.int(1)?;
                next_key = Some(values.add(key, one)?);
                (key, value)
            }
            EvalArrayElement::KeyValue { key, value } => {
                let key = eval_expr(key, context, scope, values)?;
                next_key = eval_array_next_key_after_explicit_key(key, next_key, values)?;
                (key, value)
            }
        };
        let value = eval_expr(value, context, scope, values)?;
        let _ = values.array_set(array, key, value)?;
    }
    Ok(array)
}

/// Advances an array literal's automatic key after an integer-normalized explicit key.
fn eval_array_next_key_after_explicit_key(
    key: RuntimeCellHandle,
    current_next_key: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let key = match values.type_tag(key)? {
        EVAL_TAG_INT => key,
        EVAL_TAG_STRING => {
            let bytes = values.string_bytes(key)?;
            let Some(key) = eval_numeric_string_array_key(&bytes) else {
                return Ok(current_next_key);
            };
            values.int(key)?
        }
        EVAL_TAG_NULL => return Ok(current_next_key),
        _ => values.cast_int(key)?,
    };
    let one = values.int(1)?;
    let candidate = values.add(key, one)?;
    let replace = if let Some(current_next_key) = current_next_key {
        let is_greater = values.compare(EvalBinOp::Gt, candidate, current_next_key)?;
        values.truthy(is_greater)?
    } else {
        true
    };
    Ok(if replace {
        Some(candidate)
    } else {
        current_next_key
    })
}

/// Parses PHP integer-string array keys that normalize to integer keys.
fn eval_numeric_string_array_key(bytes: &[u8]) -> Option<i64> {
    if bytes.is_empty() {
        return None;
    }

    let (negative, digits) = if bytes[0] == b'-' {
        if bytes.len() == 1 {
            return None;
        }
        (true, &bytes[1..])
    } else {
        (false, bytes)
    };

    if digits[0] == b'0' {
        return if !negative && digits.len() == 1 {
            Some(0)
        } else {
            None
        };
    }
    if digits.iter().any(|byte| !byte.is_ascii_digit()) {
        return None;
    }

    let limit = if negative {
        i64::MAX as u128 + 1
    } else {
        i64::MAX as u128
    };
    let mut value = 0u128;
    for digit in digits {
        value = (value * 10) + u128::from(digit - b'0');
        if value > limit {
            return None;
        }
    }

    if negative {
        if value == i64::MAX as u128 + 1 {
            Some(i64::MIN)
        } else {
            Some(-(value as i64))
        }
    } else {
        Some(value as i64)
    }
}

/// Converts one EvalIR constant into a runtime-cell handle.
fn eval_const(
    value: &EvalConst,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match value {
        EvalConst::Null => values.null(),
        EvalConst::Bool(value) => values.bool_value(*value),
        EvalConst::Int(value) => values.int(*value),
        EvalConst::Float(value) => values.float(*value),
        EvalConst::String(value) => values.string(value),
    }
}

/// Loads a retained value for one eval-defined dynamic constant.
fn eval_const_fetch(
    name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Some(value) = eval_predefined_constant(name, values)? {
        return Ok(value);
    }
    let Some(value) = context.constant(name) else {
        return Err(EvalStatus::RuntimeFatal);
    };
    values.retain(value)
}

/// Fetches a namespaced constant and falls back to the global constant namespace.
fn eval_namespaced_const_fetch(
    name: &str,
    fallback_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Some(value) = eval_predefined_constant(name, values)? {
        return Ok(value);
    }
    if let Some(value) = context.constant(name) {
        return values.retain(value);
    }
    eval_const_fetch(fallback_name, context, values)
}

/// Materializes one eval-visible predefined constant into a runtime cell.
fn eval_predefined_constant(
    name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some(value) = eval_predefined_constant_value(name) else {
        return Ok(None);
    };
    match value {
        EvalPredefinedConstant::Int(value) => values.int(value).map(Some),
        EvalPredefinedConstant::Float(value) => values.float(value).map(Some),
        EvalPredefinedConstant::String(value) => values.string(value).map(Some),
    }
}

/// Returns eval-visible predefined constants that do not live in dynamic context.
fn eval_predefined_constant_value(name: &str) -> Option<EvalPredefinedConstant> {
    match name.trim_start_matches('\\') {
        "PATHINFO_DIRNAME" => Some(EvalPredefinedConstant::Int(EVAL_PATHINFO_DIRNAME)),
        "PATHINFO_BASENAME" => Some(EvalPredefinedConstant::Int(EVAL_PATHINFO_BASENAME)),
        "PATHINFO_EXTENSION" => Some(EvalPredefinedConstant::Int(EVAL_PATHINFO_EXTENSION)),
        "PATHINFO_FILENAME" => Some(EvalPredefinedConstant::Int(EVAL_PATHINFO_FILENAME)),
        "PATHINFO_ALL" => Some(EvalPredefinedConstant::Int(EVAL_PATHINFO_ALL)),
        "FNM_NOESCAPE" => Some(EvalPredefinedConstant::Int(EVAL_FNM_NOESCAPE)),
        "FNM_PATHNAME" => Some(EvalPredefinedConstant::Int(EVAL_FNM_PATHNAME)),
        "FNM_PERIOD" => Some(EvalPredefinedConstant::Int(EVAL_FNM_PERIOD)),
        "FNM_CASEFOLD" => Some(EvalPredefinedConstant::Int(EVAL_FNM_CASEFOLD)),
        "ARRAY_FILTER_USE_VALUE" => Some(EvalPredefinedConstant::Int(EVAL_ARRAY_FILTER_USE_VALUE)),
        "ARRAY_FILTER_USE_BOTH" => Some(EvalPredefinedConstant::Int(EVAL_ARRAY_FILTER_USE_BOTH)),
        "ARRAY_FILTER_USE_KEY" => Some(EvalPredefinedConstant::Int(EVAL_ARRAY_FILTER_USE_KEY)),
        "COUNT_NORMAL" => Some(EvalPredefinedConstant::Int(EVAL_COUNT_NORMAL)),
        "COUNT_RECURSIVE" => Some(EvalPredefinedConstant::Int(EVAL_COUNT_RECURSIVE)),
        "PREG_SPLIT_NO_EMPTY" => Some(EvalPredefinedConstant::Int(EVAL_PREG_SPLIT_NO_EMPTY)),
        "PREG_SPLIT_DELIM_CAPTURE" => {
            Some(EvalPredefinedConstant::Int(EVAL_PREG_SPLIT_DELIM_CAPTURE))
        }
        "PREG_SPLIT_OFFSET_CAPTURE" => {
            Some(EvalPredefinedConstant::Int(EVAL_PREG_SPLIT_OFFSET_CAPTURE))
        }
        "PREG_PATTERN_ORDER" => Some(EvalPredefinedConstant::Int(EVAL_PREG_PATTERN_ORDER)),
        "PREG_SET_ORDER" => Some(EvalPredefinedConstant::Int(EVAL_PREG_SET_ORDER)),
        "PREG_OFFSET_CAPTURE" => Some(EvalPredefinedConstant::Int(EVAL_PREG_OFFSET_CAPTURE)),
        "PREG_UNMATCHED_AS_NULL" => Some(EvalPredefinedConstant::Int(EVAL_PREG_UNMATCHED_AS_NULL)),
        "JSON_ERROR_NONE" => Some(EvalPredefinedConstant::Int(EVAL_JSON_ERROR_NONE)),
        "JSON_ERROR_DEPTH" => Some(EvalPredefinedConstant::Int(EVAL_JSON_ERROR_DEPTH)),
        "JSON_ERROR_STATE_MISMATCH" => {
            Some(EvalPredefinedConstant::Int(EVAL_JSON_ERROR_STATE_MISMATCH))
        }
        "JSON_ERROR_CTRL_CHAR" => Some(EvalPredefinedConstant::Int(EVAL_JSON_ERROR_CTRL_CHAR)),
        "JSON_ERROR_SYNTAX" => Some(EvalPredefinedConstant::Int(EVAL_JSON_ERROR_SYNTAX)),
        "JSON_ERROR_UTF8" => Some(EvalPredefinedConstant::Int(EVAL_JSON_ERROR_UTF8)),
        "JSON_ERROR_RECURSION" => Some(EvalPredefinedConstant::Int(EVAL_JSON_ERROR_RECURSION)),
        "JSON_ERROR_INF_OR_NAN" => Some(EvalPredefinedConstant::Int(EVAL_JSON_ERROR_INF_OR_NAN)),
        "JSON_ERROR_UNSUPPORTED_TYPE" => Some(EvalPredefinedConstant::Int(
            EVAL_JSON_ERROR_UNSUPPORTED_TYPE,
        )),
        "JSON_ERROR_INVALID_PROPERTY_NAME" => Some(EvalPredefinedConstant::Int(
            EVAL_JSON_ERROR_INVALID_PROPERTY_NAME,
        )),
        "JSON_ERROR_UTF16" => Some(EvalPredefinedConstant::Int(EVAL_JSON_ERROR_UTF16)),
        "JSON_HEX_TAG" => Some(EvalPredefinedConstant::Int(EVAL_JSON_HEX_TAG)),
        "JSON_HEX_AMP" => Some(EvalPredefinedConstant::Int(EVAL_JSON_HEX_AMP)),
        "JSON_HEX_APOS" => Some(EvalPredefinedConstant::Int(EVAL_JSON_HEX_APOS)),
        "JSON_HEX_QUOT" => Some(EvalPredefinedConstant::Int(EVAL_JSON_HEX_QUOT)),
        "JSON_BIGINT_AS_STRING" => Some(EvalPredefinedConstant::Int(EVAL_JSON_BIGINT_AS_STRING)),
        "JSON_FORCE_OBJECT" => Some(EvalPredefinedConstant::Int(EVAL_JSON_FORCE_OBJECT)),
        "JSON_NUMERIC_CHECK" => Some(EvalPredefinedConstant::Int(EVAL_JSON_NUMERIC_CHECK)),
        "JSON_UNESCAPED_SLASHES" => Some(EvalPredefinedConstant::Int(EVAL_JSON_UNESCAPED_SLASHES)),
        "JSON_UNESCAPED_UNICODE" => Some(EvalPredefinedConstant::Int(EVAL_JSON_UNESCAPED_UNICODE)),
        "JSON_PARTIAL_OUTPUT_ON_ERROR" => Some(EvalPredefinedConstant::Int(
            EVAL_JSON_PARTIAL_OUTPUT_ON_ERROR,
        )),
        "JSON_PRETTY_PRINT" => Some(EvalPredefinedConstant::Int(EVAL_JSON_PRETTY_PRINT)),
        "JSON_PRESERVE_ZERO_FRACTION" => Some(EvalPredefinedConstant::Int(
            EVAL_JSON_PRESERVE_ZERO_FRACTION,
        )),
        "JSON_INVALID_UTF8_IGNORE" => {
            Some(EvalPredefinedConstant::Int(EVAL_JSON_INVALID_UTF8_IGNORE))
        }
        "JSON_INVALID_UTF8_SUBSTITUTE" => Some(EvalPredefinedConstant::Int(
            EVAL_JSON_INVALID_UTF8_SUBSTITUTE,
        )),
        "JSON_THROW_ON_ERROR" => Some(EvalPredefinedConstant::Int(EVAL_JSON_THROW_ON_ERROR)),
        "INF" => Some(EvalPredefinedConstant::Float(f64::INFINITY)),
        "NAN" => Some(EvalPredefinedConstant::Float(f64::NAN)),
        "PHP_INT_MAX" => Some(EvalPredefinedConstant::Int(i64::MAX)),
        "PHP_EOL" => Some(EvalPredefinedConstant::String("\n")),
        "PHP_OS" => Some(EvalPredefinedConstant::String(eval_php_os_name())),
        "DIRECTORY_SEPARATOR" => Some(EvalPredefinedConstant::String("/")),
        _ => None,
    }
}

/// Returns the PHP OS constant for the host platform running the eval bridge.
fn eval_php_os_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "Darwin"
    } else {
        "Linux"
    }
}

/// Resolves one eval magic constant against fragment and dynamic-call metadata.
fn eval_magic_const(
    magic: &EvalMagicConst,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match magic {
        EvalMagicConst::File => values.string(&context.eval_file_magic()),
        EvalMagicConst::Dir => values.string(context.call_dir()),
        EvalMagicConst::Line(line) => values.int(*line),
        EvalMagicConst::Function => values.string(context.current_function().unwrap_or("")),
        EvalMagicConst::Method => values.string(context.current_function().unwrap_or("")),
        EvalMagicConst::Class | EvalMagicConst::Namespace | EvalMagicConst::Trait => {
            values.string("")
        }
    }
}

/// Returns the current interpreter availability status for the ABI stub.
pub fn current_stub_status() -> EvalStatus {
    EvalStatus::UnsupportedConstruct
}

#[cfg(test)]
mod tests;
