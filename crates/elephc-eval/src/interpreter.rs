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

use crate::context::{ElephcEvalContext, NativeFunction};
use crate::errors::{EvalParseError, EvalStatus};
use crate::eval_ir::{
    EvalArrayElement, EvalBinOp, EvalCallArg, EvalConst, EvalExpr, EvalFunction, EvalMagicConst,
    EvalProgram, EvalStmt, EvalSwitchCase, EvalUnaryOp,
};
use crate::parser::parse_fragment;
use crate::scope::{ElephcEvalScope, ScopeCellOwnership, ScopeEntry};
use crate::value::RuntimeCellHandle;
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
    Break,
    Continue,
}

/// One already evaluated function-like call argument.
struct EvaluatedCallArg {
    name: Option<String>,
    value: RuntimeCellHandle,
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
    "tcp", "udp", "unix", "udg", "tls", "ssl", "sslv2", "sslv3", "tlsv1.0", "tlsv1.1",
    "tlsv1.2", "tlsv1.3",
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
const EVAL_ROOT_CARGO_TOML: &str = include_str!("../../../Cargo.toml");

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
    fn libc_getservbyport(
        port: libc::c_int,
        proto: *const libc::c_char,
    ) -> *mut libc::servent;
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

    /// Returns the visible element count for an array-like runtime cell.
    fn array_len(&mut self, array: RuntimeCellHandle) -> Result<usize, EvalStatus>;

    /// Returns whether a runtime cell can be indexed like an array by eval writes.
    fn is_array_like(&mut self, value: RuntimeCellHandle) -> Result<bool, EvalStatus>;

    /// Returns whether a runtime cell holds PHP null.
    fn is_null(&mut self, value: RuntimeCellHandle) -> Result<bool, EvalStatus>;

    /// Returns the concrete boxed Mixed runtime tag after unwrapping nested Mixed cells.
    fn type_tag(&mut self, value: RuntimeCellHandle) -> Result<u64, EvalStatus>;

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
    match execute_statements(program.statements(), context, scope, values)? {
        EvalControl::None => values.null(),
        EvalControl::Return(result) => Ok(result),
        EvalControl::Break | EvalControl::Continue => Err(EvalStatus::UnsupportedConstruct),
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
    context
        .function(name)
        .cloned()
        .map_or(Err(EvalStatus::UnsupportedConstruct), |function| {
            eval_dynamic_function_with_values(&function, args, context, values)
        })
}

/// Executes a named eval-context callable with arguments from a PHP array container.
pub fn execute_context_function_call_array(
    context: &mut ElephcEvalContext,
    name: &str,
    arg_array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !values.is_array_like(arg_array)? {
        return Err(EvalStatus::RuntimeFatal);
    }
    let evaluated_args = eval_array_call_arg_values(arg_array, values)?;
    eval_callable_with_call_array_args(name, evaluated_args, context, values)
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
        EvalStmt::ClassDecl { name } => {
            execute_class_decl_stmt(name, context, values)?;
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

/// Registers an empty eval-declared class name in the dynamic class table.
fn execute_class_decl_stmt(
    name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let name = name.trim_start_matches('\\');
    if context.has_class(name) || values.class_exists(name)? {
        return Err(EvalStatus::RuntimeFatal);
    }
    if context.define_class(name) {
        Ok(())
    } else {
        Err(EvalStatus::RuntimeFatal)
    }
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
            EvalControl::Return(result) => return Ok(EvalControl::Return(result)),
        }
        match execute_statements(update, context, scope, values)? {
            EvalControl::None | EvalControl::Continue => {}
            EvalControl::Break => break,
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
        EvalExpr::LoadVar(name) => {
            visible_scope_cell(context, scope, name).map_or_else(|| values.null(), Ok)
        }
        EvalExpr::Magic(magic) => eval_magic_const(magic, context, values),
        EvalExpr::NewObject { class_name, args } => {
            let args = eval_method_call_arg_values(args, context, scope, values)?;
            values
                .new_object(class_name)
                .and_then(|object| values.construct_object(object, args).map(|()| object))
        }
        EvalExpr::MethodCall {
            object,
            method,
            args,
        } => {
            let object = eval_expr(object, context, scope, values)?;
            let evaluated_args = eval_method_call_arg_values(args, context, scope, values)?;
            values.method_call(object, method, evaluated_args)
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
        "addslashes" | "stripslashes" => {
            eval_builtin_slashes(name, args, context, scope, values)
        }
        "array_combine" => eval_builtin_array_combine(args, context, scope, values),
        "array_chunk" => eval_builtin_array_chunk(args, context, scope, values),
        "array_column" => eval_builtin_array_column(args, context, scope, values),
        "array_fill" => eval_builtin_array_fill(args, context, scope, values),
        "array_fill_keys" => eval_builtin_array_fill_keys(args, context, scope, values),
        "array_flip" => eval_builtin_array_flip(args, context, scope, values),
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
        "getcwd" => eval_builtin_getcwd(args, values),
        "getenv" => eval_builtin_getenv(args, context, scope, values),
        "gettype" => eval_builtin_gettype(args, context, scope, values),
        "glob" => eval_builtin_glob(args, context, scope, values),
        "hash" | "hash_hmac" | "md5" | "sha1" => {
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
        "is_dir" | "is_executable" | "is_file" | "is_link" | "is_readable" | "is_writable"
        | "is_writeable" => eval_builtin_file_probe(name, args, context, scope, values),
        "is_array" | "is_bool" | "is_double" | "is_finite" | "is_float" | "is_infinite"
        | "is_int" | "is_integer" | "is_iterable" | "is_long" | "is_nan" | "is_null"
        | "is_numeric" | "is_real" | "is_resource" | "is_string" => {
            eval_builtin_type_predicate(name, args, context, scope, values)
        }
        "ip2long" => eval_builtin_ip2long(args, context, scope, values),
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
        "putenv" => eval_builtin_putenv(args, context, scope, values),
        "rand" | "mt_rand" => eval_builtin_rand(args, context, scope, values),
        "range" => eval_builtin_range(args, context, scope, values),
        "rawurldecode" | "urldecode" => {
            eval_builtin_url_decode(name, args, context, scope, values)
        }
        "rawurlencode" | "urlencode" => {
            eval_builtin_url_encode(name, args, context, scope, values)
        }
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
        "wordwrap" => eval_builtin_wordwrap(args, context, scope, values),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Evaluates string-name function probes against eval and supported builtin tables.
fn eval_builtin_function_probe(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [name] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let name = eval_expr(name, context, scope, values)?;
    let name = values.string_bytes(name)?;
    let name = String::from_utf8(name).map_err(|_| EvalStatus::RuntimeFatal)?;
    let name = name.trim_start_matches('\\').to_ascii_lowercase();
    values.bool_value(eval_function_probe_exists(context, &name))
}

/// Evaluates `define(name, value)` for eval dynamic constant-name registration.
fn eval_builtin_define(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [name, value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let name = eval_expr(name, context, scope, values)?;
    let value = eval_expr(value, context, scope, values)?;
    let defined = eval_define_name(name, value, context, values)?;
    values.bool_value(defined)
}

/// Evaluates `defined(name)` against eval dynamic constant names.
fn eval_builtin_defined(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [name] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let name = eval_expr(name, context, scope, values)?;
    let exists = eval_defined_name(name, context, values)?;
    values.bool_value(exists)
}

/// Evaluates `define(...)` from already materialized call arguments.
fn eval_define_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [name, value] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let defined = eval_define_name(*name, *value, context, values)?;
    values.bool_value(defined)
}

/// Evaluates `defined(...)` from already materialized call arguments.
fn eval_defined_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [name] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let exists = eval_defined_name(*name, context, values)?;
    values.bool_value(exists)
}

/// Normalizes and registers one eval dynamic constant name.
fn eval_define_name(
    name: RuntimeCellHandle,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let name = eval_constant_name(name, values)?;
    if name.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    if eval_predefined_int_constant(&name).is_some() || context.has_constant(&name) {
        values.warning(DEFINE_ALREADY_DEFINED_WARNING)?;
        return Ok(false);
    }
    let value = values.retain(value)?;
    if context.define_constant(&name, value) {
        Ok(true)
    } else {
        values.release(value)?;
        Ok(false)
    }
}

/// Normalizes and probes one eval dynamic constant name.
fn eval_defined_name(
    name: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let name = eval_constant_name(name, values)?;
    Ok(eval_predefined_int_constant(&name).is_some() || context.has_constant(&name))
}

/// Reads a PHP constant name from a runtime cell without changing case.
fn eval_constant_name(
    name: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let name = values.string_bytes(name)?;
    String::from_utf8(name).map_err(|_| EvalStatus::RuntimeFatal)
}

/// Evaluates `class_exists(...)` against dynamic and generated class-name tables.
fn eval_builtin_class_exists(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let name = match args {
        [name] => eval_expr(name, context, scope, values)?,
        [name, autoload] => {
            let name = eval_expr(name, context, scope, values)?;
            let _ = eval_expr(autoload, context, scope, values)?;
            name
        }
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let exists = eval_class_exists_name(name, context, values)?;
    values.bool_value(exists)
}

/// Evaluates `class_exists(...)` from already materialized call arguments.
fn eval_class_exists_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let exists = match evaluated_args {
        [name] => eval_class_exists_name(*name, context, values)?,
        [name, _autoload] => eval_class_exists_name(*name, context, values)?,
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    values.bool_value(exists)
}

/// Normalizes a PHP class-name cell and probes dynamic names before generated classes.
fn eval_class_exists_name(
    name: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let name = values.string_bytes(name)?;
    let name = String::from_utf8(name).map_err(|_| EvalStatus::RuntimeFatal)?;
    let name = name.trim_start_matches('\\');
    if context.has_class(name) {
        return Ok(true);
    }
    values.class_exists(name)
}

/// Evaluates PHP's `isset(...)` language construct over eval-visible values.
fn eval_builtin_isset(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.is_empty() {
        return values.bool_value(false);
    }
    for arg in args {
        if !eval_isset_arg(arg, context, scope, values)? {
            return values.bool_value(false);
        }
    }
    values.bool_value(true)
}

/// Evaluates PHP's `empty(...)` language construct over eval-visible values.
fn eval_builtin_empty(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [arg] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let empty = eval_empty_arg(arg, context, scope, values)?;
    values.bool_value(empty)
}

/// Evaluates one `empty` operand without warning or failing on missing variables.
fn eval_empty_arg(
    arg: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if let EvalExpr::LoadVar(name) = arg {
        let Some(value) = visible_scope_cell(context, scope, name) else {
            return Ok(true);
        };
        return Ok(!values.truthy(value)?);
    }
    let value = eval_expr(arg, context, scope, values)?;
    Ok(!values.truthy(value)?)
}

/// Evaluates one `isset` operand without allocating a null cell for missing variables.
fn eval_isset_arg(
    arg: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if let EvalExpr::LoadVar(name) = arg {
        let Some(value) = visible_scope_cell(context, scope, name) else {
            return Ok(false);
        };
        return Ok(!values.is_null(value)?);
    }
    let value = eval_expr(arg, context, scope, values)?;
    Ok(!values.is_null(value)?)
}

/// Returns true when a PHP function name is visible to eval builtin probes.
fn eval_function_probe_exists(context: &ElephcEvalContext, name: &str) -> bool {
    !name.contains("::") && (context.has_function(name) || eval_php_visible_builtin_exists(name))
}

/// Returns true for PHP-visible builtin names implemented by the eval interpreter.
fn eval_php_visible_builtin_exists(name: &str) -> bool {
    matches!(
        name,
            "abs"
            | "addslashes"
            | "array_chunk"
            | "array_column"
            | "array_combine"
            | "array_fill"
            | "array_fill_keys"
            | "array_flip"
            | "array_key_exists"
            | "array_keys"
            | "array_diff"
            | "array_intersect"
            | "array_diff_key"
            | "array_intersect_key"
            | "array_merge"
            | "array_pad"
            | "array_product"
            | "array_rand"
            | "array_reverse"
            | "array_search"
            | "array_slice"
            | "array_sum"
            | "array_unique"
            | "array_values"
            | "acos"
            | "asin"
            | "atan"
            | "atan2"
            | "basename"
            | "base64_decode"
            | "base64_encode"
            | "bin2hex"
            | "ceil"
            | "chdir"
            | "chmod"
            | "call_user_func"
            | "call_user_func_array"
            | "class_exists"
            | "boolval"
            | "chop"
            | "chr"
            | "clamp"
            | "clearstatcache"
            | "count"
            | "copy"
            | "cos"
            | "cosh"
            | "crc32"
            | "ctype_alnum"
            | "ctype_alpha"
            | "ctype_digit"
            | "ctype_space"
            | "date"
            | "define"
            | "defined"
            | "deg2rad"
            | "dirname"
            | "disk_free_space"
            | "disk_total_space"
            | "exp"
            | "explode"
            | "fdiv"
            | "file"
            | "file_exists"
            | "fileatime"
            | "filectime"
            | "filegroup"
            | "file_get_contents"
            | "fileinode"
            | "filemtime"
            | "fileowner"
            | "fileperms"
            | "file_put_contents"
            | "filesize"
            | "filetype"
            | "fnmatch"
            | "floor"
            | "floatval"
            | "fmod"
            | "function_exists"
            | "gethostbyaddr"
            | "gethostbyname"
            | "gethostname"
            | "getprotobyname"
            | "getprotobynumber"
            | "getservbyname"
            | "getservbyport"
            | "getcwd"
            | "getenv"
            | "gettype"
            | "glob"
            | "hash"
            | "hash_algos"
            | "hash_equals"
            | "hash_hmac"
            | "hex2bin"
            | "html_entity_decode"
            | "htmlentities"
            | "htmlspecialchars"
            | "hypot"
            | "implode"
            | "in_array"
            | "inet_ntop"
            | "inet_pton"
            | "intdiv"
            | "ip2long"
            | "is_dir"
            | "is_executable"
            | "is_file"
            | "is_link"
            | "is_readable"
            | "is_writable"
            | "is_writeable"
            | "intval"
            | "link"
            | "linkinfo"
            | "ltrim"
            | "is_callable"
            | "is_array"
            | "is_bool"
            | "is_double"
            | "is_finite"
            | "is_float"
            | "is_infinite"
            | "is_int"
            | "is_integer"
            | "is_iterable"
            | "is_long"
            | "is_nan"
            | "is_null"
            | "is_numeric"
            | "is_real"
            | "is_resource"
            | "is_string"
            | "lcfirst"
            | "log"
            | "log2"
            | "log10"
            | "long2ip"
            | "max"
            | "md5"
            | "microtime"
            | "min"
            | "mkdir"
            | "mktime"
            | "mt_rand"
            | "nl2br"
            | "number_format"
            | "ord"
            | "pathinfo"
            | "pi"
            | "pow"
            | "php_uname"
            | "phpversion"
            | "putenv"
            | "rand"
            | "range"
            | "rad2deg"
            | "rawurldecode"
            | "rawurlencode"
            | "readfile"
            | "readlink"
            | "realpath"
            | "realpath_cache_get"
            | "realpath_cache_size"
            | "rename"
            | "rtrim"
            | "round"
            | "rmdir"
            | "scandir"
            | "sleep"
            | "sha1"
            | "sin"
            | "sinh"
            | "sqrt"
            | "spl_classes"
            | "strcasecmp"
            | "stream_get_filters"
            | "stream_get_transports"
            | "stream_get_wrappers"
            | "str_contains"
            | "str_ends_with"
            | "str_ireplace"
            | "str_repeat"
            | "str_replace"
            | "str_starts_with"
            | "strcmp"
            | "stat"
            | "strlen"
            | "strpos"
            | "strrpos"
            | "strrev"
            | "str_pad"
            | "str_split"
            | "strstr"
            | "strtotime"
            | "substr"
            | "stripslashes"
            | "strtolower"
            | "strtoupper"
            | "strval"
            | "symlink"
            | "sys_get_temp_dir"
            | "tempnam"
            | "tan"
            | "tanh"
            | "time"
            | "touch"
            | "trim"
            | "substr_replace"
            | "ucfirst"
            | "ucwords"
            | "unlink"
            | "umask"
            | "urldecode"
            | "urlencode"
            | "usleep"
            | "wordwrap"
            | "lstat"
    )
}

/// Evaluates a direct PHP-visible builtin call with named or spread arguments.
fn eval_builtin_call(
    name: &str,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let evaluated_args = eval_call_arg_values(args, context, scope, values)?;
    let evaluated_args = bind_evaluated_builtin_args(name, evaluated_args)?;
    let Some(result) = eval_builtin_with_values(name, &evaluated_args, context, values)? else {
        return Err(EvalStatus::UnsupportedConstruct);
    };
    Ok(result)
}

/// Binds evaluated builtin arguments to PHP parameter order when names are used.
fn bind_evaluated_builtin_args(
    name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
) -> Result<Vec<RuntimeCellHandle>, EvalStatus> {
    if evaluated_args.iter().all(|arg| arg.name.is_none()) {
        return Ok(evaluated_args.into_iter().map(|arg| arg.value).collect());
    }

    let params = eval_builtin_param_names(name).ok_or(EvalStatus::RuntimeFatal)?;
    let mut bound_args = vec![None; params.len()];
    let mut next_positional = 0;

    for arg in evaluated_args {
        if let Some(name) = arg.name {
            bind_builtin_named_arg(params, &mut bound_args, &name, arg.value)?;
        } else {
            bind_dynamic_positional_arg(&mut bound_args, &mut next_positional, arg.value)?;
        }
    }

    collect_contiguous_bound_args(bound_args)
}

/// Binds one named builtin-call value to the matching PHP parameter slot.
fn bind_builtin_named_arg(
    params: &[&str],
    bound_args: &mut [Option<RuntimeCellHandle>],
    name: &str,
    value: RuntimeCellHandle,
) -> Result<(), EvalStatus> {
    let Some(param_index) = params.iter().position(|param| *param == name) else {
        return Err(EvalStatus::RuntimeFatal);
    };
    if bound_args[param_index].is_some() {
        return Err(EvalStatus::RuntimeFatal);
    }
    bound_args[param_index] = Some(value);
    Ok(())
}

/// Collects ordered bound arguments, rejecting gaps where defaults would be needed.
fn collect_contiguous_bound_args(
    bound_args: Vec<Option<RuntimeCellHandle>>,
) -> Result<Vec<RuntimeCellHandle>, EvalStatus> {
    let Some(last_index) = bound_args.iter().rposition(Option::is_some) else {
        return Ok(Vec::new());
    };
    bound_args
        .into_iter()
        .take(last_index + 1)
        .collect::<Option<Vec<_>>>()
        .ok_or(EvalStatus::RuntimeFatal)
}

/// Returns PHP parameter names for builtin calls implemented by eval.
fn eval_builtin_param_names(name: &str) -> Option<&'static [&'static str]> {
    match name {
        "abs" | "ceil" | "floor" | "sqrt" => Some(&["num"]),
        "array_chunk" => Some(&["array", "length"]),
        "array_column" => Some(&["array", "column_key"]),
        "array_combine" => Some(&["keys", "values"]),
        "array_fill" => Some(&["start_index", "count", "value"]),
        "array_fill_keys" => Some(&["keys", "value"]),
        "array_flip" | "array_keys" | "array_product" | "array_sum" | "array_unique"
        | "array_rand" | "array_values" => Some(&["array"]),
        "array_key_exists" => Some(&["key", "array"]),
        "array_pad" => Some(&["array", "length", "value"]),
        "array_reverse" => Some(&["array", "preserve_keys"]),
        "array_search" | "in_array" => Some(&["needle", "haystack", "strict"]),
        "array_slice" => Some(&["array", "offset", "length"]),
        "acos" | "asin" | "atan" | "cos" | "cosh" | "deg2rad" | "exp" | "log2" | "log10"
        | "rad2deg" | "sin" | "sinh" | "tan" | "tanh" => Some(&["num"]),
        "atan2" => Some(&["y", "x"]),
        "basename" => Some(&["path", "suffix"]),
        "addslashes" | "base64_decode" | "base64_encode" | "bin2hex" | "hex2bin"
        | "rawurldecode" | "rawurlencode" | "stripslashes" | "urldecode" | "urlencode" => {
            Some(&["string"])
        }
        "boolval" | "floatval" | "gettype" | "intval" | "is_array" | "is_bool" | "is_double"
        | "is_finite" | "is_float" | "is_infinite" | "is_int" | "is_integer"
        | "is_iterable" | "is_long" | "is_nan" | "is_null" | "is_numeric" | "is_real"
        | "is_resource" | "is_string" | "is_callable" | "strval" => Some(&["value"]),
        "call_user_func" => Some(&["callback"]),
        "call_user_func_array" => Some(&["callback", "args"]),
        "class_exists" => Some(&["class", "autoload"]),
        "chdir" | "mkdir" | "rmdir" | "scandir" => Some(&["directory"]),
        "chmod" => Some(&["filename", "permissions"]),
        "chr" => Some(&["codepoint"]),
        "clamp" => Some(&["value", "min", "max"]),
        "clearstatcache" => Some(&["clear_realpath_cache", "filename"]),
        "chop" | "ltrim" | "rtrim" | "trim" => Some(&["string", "characters"]),
        "count" => Some(&["value", "mode"]),
        "copy" | "rename" => Some(&["from", "to"]),
        "crc32" => Some(&["string"]),
        "ctype_alnum" | "ctype_alpha" | "ctype_digit" | "ctype_space" => Some(&["text"]),
        "date" => Some(&["format", "timestamp"]),
        "define" => Some(&["constant_name", "value"]),
        "defined" => Some(&["constant_name"]),
        "dirname" => Some(&["path", "levels"]),
        "disk_free_space" | "disk_total_space" => Some(&["directory"]),
        "explode" => Some(&["separator", "string"]),
        "fdiv" | "fmod" => Some(&["num1", "num2"]),
        "fnmatch" => Some(&["pattern", "filename", "flags"]),
        "file" | "file_get_contents" | "file_exists" | "fileatime" | "filectime" | "filegroup"
        | "fileinode" | "filemtime" | "fileowner" | "fileperms" | "filesize" | "filetype"
        | "is_dir" | "is_executable" | "is_file" | "is_link" | "is_readable" | "is_writable"
        | "is_writeable" | "lstat" | "readfile" | "stat" | "unlink" => Some(&["filename"]),
        "file_put_contents" => Some(&["filename", "data"]),
        "function_exists" => Some(&["function"]),
        "gethostbyaddr" => Some(&["ip"]),
        "gethostbyname" => Some(&["hostname"]),
        "gethostname" => Some(&[]),
        "getprotobyname" => Some(&["protocol"]),
        "getprotobynumber" => Some(&["protocol"]),
        "getservbyname" => Some(&["service", "protocol"]),
        "getservbyport" => Some(&["port", "protocol"]),
        "getcwd" => Some(&[]),
        "getenv" => Some(&["name"]),
        "glob" => Some(&["pattern"]),
        "hash" => Some(&["algo", "data", "binary"]),
        "hash_algos" => Some(&[]),
        "hash_equals" => Some(&["known_string", "user_string"]),
        "hash_hmac" => Some(&["algo", "data", "key", "binary"]),
        "hypot" => Some(&["x", "y"]),
        "html_entity_decode" | "htmlentities" | "htmlspecialchars" => Some(&["string"]),
        "implode" => Some(&["separator", "array"]),
        "inet_ntop" => Some(&["ip"]),
        "inet_pton" => Some(&["ip"]),
        "intdiv" => Some(&["num1", "num2"]),
        "ip2long" => Some(&["ip"]),
        "link" | "symlink" => Some(&["target", "link"]),
        "linkinfo" | "readlink" => Some(&["path"]),
        "log" => Some(&["num", "base"]),
        "max" | "min" => Some(&["value"]),
        "md5" | "sha1" => Some(&["string", "binary"]),
        "microtime" => Some(&["as_float"]),
        "mktime" => Some(&["hour", "minute", "second", "month", "day", "year"]),
        "nl2br" => Some(&["string", "use_xhtml"]),
        "number_format" => Some(&["num", "decimals", "decimal_separator", "thousands_separator"]),
        "ord" => Some(&["character"]),
        "pathinfo" => Some(&["path", "flags"]),
        "pi" => Some(&[]),
        "php_uname" => Some(&["mode"]),
        "phpversion" => Some(&[]),
        "pow" => Some(&["num", "exponent"]),
        "putenv" => Some(&["assignment"]),
        "rand" | "mt_rand" => Some(&["min", "max"]),
        "range" => Some(&["start", "end"]),
        "realpath" => Some(&["path"]),
        "realpath_cache_get" | "realpath_cache_size" => Some(&[]),
        "round" => Some(&["num", "precision"]),
        "sleep" => Some(&["seconds"]),
        "spl_classes" => Some(&[]),
        "stream_get_filters" | "stream_get_transports" | "stream_get_wrappers" => Some(&[]),
        "strcasecmp" | "strcmp" => Some(&["string1", "string2"]),
        "str_contains" | "str_ends_with" | "str_starts_with" => Some(&["haystack", "needle"]),
        "strtotime" => Some(&["datetime"]),
        "strstr" => Some(&["haystack", "needle", "before_needle"]),
        "str_pad" => Some(&["string", "length", "pad_string", "pad_type"]),
        "str_replace" | "str_ireplace" => Some(&["search", "replace", "subject"]),
        "strpos" | "strrpos" => Some(&["haystack", "needle", "offset"]),
        "str_repeat" => Some(&["string", "times"]),
        "str_split" => Some(&["string", "length"]),
        "substr" => Some(&["string", "offset", "length"]),
        "substr_replace" => Some(&["string", "replace", "offset", "length"]),
        "sys_get_temp_dir" | "time" => Some(&[]),
        "tempnam" => Some(&["directory", "prefix"]),
        "touch" => Some(&["filename", "mtime", "atime"]),
        "lcfirst" | "strlen" | "strrev" | "strtolower" | "strtoupper" | "ucfirst" => {
            Some(&["string"])
        }
        "long2ip" => Some(&["ip"]),
        "ucwords" => Some(&["string", "separators"]),
        "umask" => Some(&["mask"]),
        "usleep" => Some(&["microseconds"]),
        "wordwrap" => Some(&["string", "width", "break", "cut_long_words"]),
        _ => None,
    }
}

/// Evaluates `call_user_func($name, ...$args)` inside a runtime eval fragment.
fn eval_builtin_call_user_func(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let mut evaluated_args = Vec::with_capacity(args.len());
    for arg in args {
        evaluated_args.push(eval_expr(arg, context, scope, values)?);
    }
    eval_call_user_func_with_values(evaluated_args, context, values)
}

/// Evaluates `call_user_func_array($name, $args)` inside a runtime eval fragment.
fn eval_builtin_call_user_func_array(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [callback, arg_array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let callback = eval_expr(callback, context, scope, values)?;
    let arg_array = eval_expr(arg_array, context, scope, values)?;
    eval_call_user_func_array_with_values(callback, arg_array, context, values)
}

/// Dispatches `call_user_func_array` after callback and array arguments are evaluated.
fn eval_call_user_func_array_with_values(
    callback: RuntimeCellHandle,
    arg_array: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let callback = values.string_bytes(callback)?;
    let callback = String::from_utf8(callback).map_err(|_| EvalStatus::RuntimeFatal)?;
    let callback = callback.trim_start_matches('\\').to_ascii_lowercase();
    if callback.contains("::") {
        return Err(EvalStatus::UnsupportedConstruct);
    }
    if !values.is_array_like(arg_array)? {
        return Err(EvalStatus::RuntimeFatal);
    }
    let evaluated_args = eval_array_call_arg_values(arg_array, values)?;
    eval_callable_with_call_array_args(&callback, evaluated_args, context, values)
}

/// Dispatches `call_user_func` after its callback and arguments are already evaluated.
fn eval_call_user_func_with_values(
    evaluated_args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some((callback, callback_args)) = evaluated_args.split_first() else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let callback = values.string_bytes(*callback)?;
    let callback = String::from_utf8(callback).map_err(|_| EvalStatus::RuntimeFatal)?;
    let callback = callback.trim_start_matches('\\').to_ascii_lowercase();
    if callback.contains("::") {
        return Err(EvalStatus::UnsupportedConstruct);
    }
    eval_callable_with_values(&callback, callback_args.to_vec(), context, values)
}

/// Invokes a PHP-visible callable name with source-order positional values.
fn eval_callable_with_values(
    name: &str,
    evaluated_args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Some(result) = eval_builtin_with_values(name, &evaluated_args, context, values)? {
        return Ok(result);
    }
    if let Some(function) = context.function(name).cloned() {
        return eval_dynamic_function_with_values(&function, evaluated_args, context, values);
    }
    if let Some(function) = context.native_function(name) {
        return eval_native_function_with_values(function, evaluated_args, values);
    }
    Err(EvalStatus::UnsupportedConstruct)
}

/// Invokes a callable with arguments that may carry `call_user_func_array` names.
fn eval_callable_with_call_array_args(
    name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if evaluated_args.iter().all(|arg| arg.name.is_none()) {
        let evaluated_args = evaluated_args.into_iter().map(|arg| arg.value).collect();
        return eval_callable_with_values(name, evaluated_args, context, values);
    }
    if eval_php_visible_builtin_exists(name) {
        let evaluated_args = bind_evaluated_builtin_args(name, evaluated_args)?;
        let Some(result) = eval_builtin_with_values(name, &evaluated_args, context, values)? else {
            return Err(EvalStatus::UnsupportedConstruct);
        };
        return Ok(result);
    }
    if let Some(function) = context.function(name).cloned() {
        let evaluated_args = bind_evaluated_function_args(function.params(), evaluated_args)?;
        return eval_dynamic_function_with_values(&function, evaluated_args, context, values);
    }
    if let Some(function) = context.native_function(name) {
        if function.param_names().len() != function.param_count() {
            return Err(EvalStatus::RuntimeFatal);
        }
        let evaluated_args = bind_evaluated_function_args(function.param_names(), evaluated_args)?;
        return eval_native_function_with_values(function, evaluated_args, values);
    }
    Err(EvalStatus::UnsupportedConstruct)
}

/// Evaluates PHP-visible builtins when they are invoked through a dynamic callable name.
fn eval_builtin_with_values(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let result = match name {
        "abs" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.abs(*value)?
        }
        "addslashes" | "stripslashes" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_slashes_result(name, *value, values)?
        }
        "array_combine" => {
            let [keys, values_array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_combine_result(*keys, *values_array, values)?
        }
        "array_column" => {
            let [array, column_key] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_column_result(*array, *column_key, values)?
        }
        "array_chunk" => {
            let [array, length] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_chunk_result(*array, *length, values)?
        }
        "array_fill" => {
            let [start, count, value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_fill_result(*start, *count, *value, values)?
        }
        "array_fill_keys" => {
            let [keys, value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_fill_keys_result(*keys, *value, values)?
        }
        "array_flip" => {
            let [array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_flip_result(*array, values)?
        }
        "array_pad" => {
            let [array, length, value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_pad_result(*array, *length, *value, values)?
        }
        "array_product" | "array_sum" => {
            let [array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_aggregate_result(name, *array, values)?
        }
        "array_keys" | "array_values" => {
            let [array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_projection_result(name, *array, values)?
        }
        "array_key_exists" => {
            let [key, array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.array_key_exists(*key, *array)?
        }
        "array_diff" | "array_intersect" => {
            let [left, right] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_value_set_result(name, *left, *right, values)?
        }
        "array_diff_key" | "array_intersect_key" => {
            let [left, right] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_key_set_result(name, *left, *right, values)?
        }
        "array_merge" => {
            let [left, right] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_merge_result(*left, *right, values)?
        }
        "array_rand" => {
            let [array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_rand_result(*array, values)?
        }
        "array_reverse" => match evaluated_args {
            [array] => eval_array_reverse_result(*array, false, values)?,
            [array, preserve_keys] => {
                let preserve_keys = values.truthy(*preserve_keys)?;
                eval_array_reverse_result(*array, preserve_keys, values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "array_search" | "in_array" => {
            let [needle, array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_search_result(name, *needle, *array, values)?
        }
        "array_slice" => match evaluated_args {
            [array, offset] => eval_array_slice_result(*array, *offset, None, values)?,
            [array, offset, length] => {
                eval_array_slice_result(*array, *offset, Some(*length), values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "array_unique" => {
            let [array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_unique_result(*array, values)?
        }
        "range" => {
            let [start, end] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_range_result(*start, *end, values)?
        }
        "base64_encode" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_base64_encode_result(*value, values)?
        }
        "base64_decode" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_base64_decode_result(*value, values)?
        }
        "acos" | "asin" | "atan" | "cos" | "cosh" | "deg2rad" | "exp" | "log2" | "log10"
        | "rad2deg" | "sin" | "sinh" | "tan" | "tanh" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_float_unary_result(name, *value, values)?
        }
        "atan2" | "hypot" => {
            let [left, right] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_float_pair_result(name, *left, *right, values)?
        }
        "bin2hex" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_bin2hex_result(*value, values)?
        }
        "ceil" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.ceil(*value)?
        }
        "chr" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_chr_result(*value, values)?
        }
        "chdir" | "mkdir" | "rmdir" => {
            let [path] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_unary_path_bool_result(name, *path, values)?
        }
        "chmod" => {
            let [filename, permissions] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_chmod_result(*filename, *permissions, values)?
        }
        "clearstatcache" => {
            if evaluated_args.len() > 2 {
                return Err(EvalStatus::RuntimeFatal);
            }
            values.null()?
        }
        "clamp" => {
            let [value, min, max] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_clamp_result(*value, *min, *max, values)?
        }
        "copy" | "link" | "rename" | "symlink" => {
            let [from, to] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_binary_path_bool_result(name, *from, *to, values)?
        }
        "floor" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.floor(*value)?
        }
        "fdiv" | "fmod" => {
            let [left, right] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_float_binary_result(name, *left, *right, values)?
        }
        "file" => {
            let [filename] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_file_result(*filename, values)?
        }
        "file_exists" | "is_dir" | "is_executable" | "is_file" | "is_link" | "is_readable"
        | "is_writable" | "is_writeable" => {
            let [filename] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_file_probe_result(name, *filename, values)?
        }
        "fileatime" | "filectime" | "filegroup" | "fileinode" | "filemtime" | "fileowner"
        | "fileperms" => {
            let [filename] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_file_stat_scalar_result(name, *filename, values)?
        }
        "file_get_contents" => {
            let [filename] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_file_get_contents_result(*filename, values)?
        }
        "file_put_contents" => {
            let [filename, data] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_file_put_contents_result(*filename, *data, values)?
        }
        "filesize" => {
            let [filename] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_filesize_result(*filename, values)?
        }
        "filetype" => {
            let [filename] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_filetype_result(*filename, values)?
        }
        "fnmatch" => match evaluated_args {
            [pattern, filename] => eval_fnmatch_result(*pattern, *filename, None, values)?,
            [pattern, filename, flags] => {
                eval_fnmatch_result(*pattern, *filename, Some(*flags), values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "stat" | "lstat" => {
            let [filename] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_stat_array_result(name, *filename, values)?
        }
        "linkinfo" => {
            let [path] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_linkinfo_result(*path, values)?
        }
        "log" => match evaluated_args {
            [num] => eval_log_result(*num, None, values)?,
            [num, base] => eval_log_result(*num, Some(*base), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "readfile" => {
            let [filename] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_readfile_result(*filename, values)?
        }
        "pi" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            values.float(std::f64::consts::PI)?
        }
        "php_uname" => match evaluated_args {
            [] => eval_php_uname_result(None, values)?,
            [mode] => eval_php_uname_result(Some(*mode), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "pow" => {
            let [left, right] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.pow(*left, *right)?
        }
        "rand" | "mt_rand" => match evaluated_args {
            [] => eval_rand_result(None, None, values)?,
            [min, max] => eval_rand_result(Some(*min), Some(*max), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "rawurldecode" | "urldecode" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_url_decode_result(name, *value, values)?
        }
        "rawurlencode" | "urlencode" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_url_encode_result(name, *value, values)?
        }
        "round" => match evaluated_args {
            [value] => values.round(*value, None)?,
            [value, precision] => values.round(*value, Some(*precision))?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "scandir" => {
            let [directory] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_scandir_result(*directory, values)?
        }
        "sqrt" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.sqrt(*value)?
        }
        "spl_classes" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_spl_classes_result(values)?
        }
        "strrev" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.strrev(*value)?
        }
        "str_repeat" => {
            let [value, times] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_str_repeat_result(*value, *times, values)?
        }
        "str_replace" | "str_ireplace" => {
            let [search, replace, subject] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_str_replace_result(name, *search, *replace, *subject, values)?
        }
        "str_pad" => match evaluated_args {
            [value, length] => eval_str_pad_result(*value, *length, None, None, values)?,
            [value, length, pad_string] => {
                eval_str_pad_result(*value, *length, Some(*pad_string), None, values)?
            }
            [value, length, pad_string, pad_type] => eval_str_pad_result(
                *value,
                *length,
                Some(*pad_string),
                Some(*pad_type),
                values,
            )?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "str_split" => match evaluated_args {
            [value] => eval_str_split_result(*value, None, values)?,
            [value, length] => eval_str_split_result(*value, Some(*length), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "substr" => match evaluated_args {
            [value, offset] => eval_substr_result(*value, *offset, None, values)?,
            [value, offset, length] => eval_substr_result(*value, *offset, Some(*length), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "substr_replace" => match evaluated_args {
            [value, replace, offset] => {
                eval_substr_replace_result(*value, *replace, *offset, None, values)?
            }
            [value, replace, offset, length] => {
                eval_substr_replace_result(*value, *replace, *offset, Some(*length), values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "call_user_func" => {
            return eval_call_user_func_with_values(evaluated_args.to_vec(), context, values)
                .map(Some);
        }
        "call_user_func_array" => {
            let [callback, arg_array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            return eval_call_user_func_array_with_values(*callback, *arg_array, context, values)
                .map(Some);
        }
        "boolval" | "floatval" | "intval" | "strval" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_cast_result(name, *value, values)?
        }
        "count" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            let len = values.array_len(*value)?;
            let len = i64::try_from(len).map_err(|_| EvalStatus::RuntimeFatal)?;
            values.int(len)?
        }
        "crc32" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_crc32_result(*value, values)?
        }
        "ctype_alnum" | "ctype_alpha" | "ctype_digit" | "ctype_space" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_ctype_result(name, *value, values)?
        }
        "date" => match evaluated_args {
            [format] => eval_date_result(*format, None, values)?,
            [format, timestamp] => eval_date_result(*format, Some(*timestamp), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "define" => eval_define_result(evaluated_args, context, values)?,
        "defined" => eval_defined_result(evaluated_args, context, values)?,
        "explode" => {
            let [separator, string] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_explode_result(*separator, *string, values)?
        }
        "ord" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_ord_result(*value, values)?
        }
        "implode" => {
            let [separator, array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_implode_result(*separator, *array, values)?
        }
        "max" | "min" => eval_min_max_result(name, evaluated_args, values)?,
        "microtime" => match evaluated_args {
            [] | [_] => eval_microtime_result(values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "mktime" => {
            let [hour, minute, second, month, day, year] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_mktime_result(*hour, *minute, *second, *month, *day, *year, values)?
        },
        "nl2br" => match evaluated_args {
            [value] => eval_nl2br_result(*value, true, values)?,
            [value, use_xhtml] => {
                let use_xhtml = values.truthy(*use_xhtml)?;
                eval_nl2br_result(*value, use_xhtml, values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "number_format" => match evaluated_args {
            [value] => eval_number_format_result(*value, None, None, None, values)?,
            [value, decimals] => {
                eval_number_format_result(*value, Some(*decimals), None, None, values)?
            }
            [value, decimals, decimal_separator] => eval_number_format_result(
                *value,
                Some(*decimals),
                Some(*decimal_separator),
                None,
                values,
            )?,
            [value, decimals, decimal_separator, thousands_separator] => eval_number_format_result(
                *value,
                Some(*decimals),
                Some(*decimal_separator),
                Some(*thousands_separator),
                values,
            )?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "basename" => match evaluated_args {
            [path] => eval_basename_result(*path, None, values)?,
            [path, suffix] => eval_basename_result(*path, Some(*suffix), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "dirname" => match evaluated_args {
            [path] => eval_dirname_result(*path, None, values)?,
            [path, levels] => eval_dirname_result(*path, Some(*levels), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "disk_free_space" | "disk_total_space" => {
            let [directory] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_disk_space_result(name, *directory, values)?
        }
        "trim" | "ltrim" | "rtrim" | "chop" => match evaluated_args {
            [value] => eval_trim_like_result(name, *value, None, values)?,
            [value, mask] => eval_trim_like_result(name, *value, Some(*mask), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "function_exists" | "is_callable" => {
            let [name] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            let name = values.string_bytes(*name)?;
            let name = String::from_utf8(name).map_err(|_| EvalStatus::RuntimeFatal)?;
            let name = name.trim_start_matches('\\').to_ascii_lowercase();
            values.bool_value(eval_function_probe_exists(context, &name))?
        }
        "class_exists" => eval_class_exists_result(evaluated_args, context, values)?,
        "gethostbyaddr" => {
            let [ip] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_gethostbyaddr_result(*ip, values)?
        }
        "gethostbyname" => {
            let [hostname] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_gethostbyname_result(*hostname, values)?
        }
        "gethostname" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_gethostname_result(values)?
        }
        "getprotobyname" => {
            let [protocol] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_getprotobyname_result(*protocol, values)?
        }
        "getprotobynumber" => {
            let [protocol] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_getprotobynumber_result(*protocol, values)?
        }
        "getservbyname" => {
            let [service, protocol] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_getservbyname_result(*service, *protocol, values)?
        }
        "getservbyport" => {
            let [port, protocol] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_getservbyport_result(*port, *protocol, values)?
        }
        "getcwd" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_getcwd_result(values)?
        }
        "getenv" => {
            let [name] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_getenv_result(*name, values)?
        }
        "gettype" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_gettype_result(*value, values)?
        }
        "glob" => {
            let [pattern] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_glob_result(*pattern, values)?
        }
        "hash" | "hash_hmac" | "md5" | "sha1" => {
            eval_hash_one_shot_result(name, evaluated_args, values)?
        }
        "hash_algos" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_hash_algos_result(values)?
        }
        "hash_equals" => {
            let [known, user] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_hash_equals_result(*known, *user, values)?
        }
        "hex2bin" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_hex2bin_result(*value, values)?
        }
        "html_entity_decode" | "htmlentities" | "htmlspecialchars" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_html_entity_result(name, *value, values)?
        }
        "inet_ntop" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_inet_ntop_result(*value, values)?
        }
        "inet_pton" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_inet_pton_result(*value, values)?
        }
        "intdiv" => {
            let [left, right] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_intdiv_result(*left, *right, values)?
        }
        "ip2long" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_ip2long_result(*value, values)?
        }
        "phpversion" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_phpversion_result(values)?
        }
        "pathinfo" => match evaluated_args {
            [path] => eval_pathinfo_result(*path, None, values)?,
            [path, flags] => eval_pathinfo_result(*path, Some(*flags), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "putenv" => {
            let [assignment] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_putenv_result(*assignment, values)?
        }
        "realpath" => {
            let [path] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_realpath_result(*path, values)?
        }
        "realpath_cache_get" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_realpath_cache_get_result(values)?
        }
        "realpath_cache_size" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_realpath_cache_size_result(values)?
        }
        "is_array" | "is_bool" | "is_double" | "is_finite" | "is_float" | "is_infinite"
        | "is_int" | "is_integer" | "is_iterable" | "is_long" | "is_nan" | "is_null"
        | "is_numeric" | "is_real" | "is_resource" | "is_string" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_type_predicate_result(name, *value, values)?
        }
        "sys_get_temp_dir" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_sys_get_temp_dir_result(values)?
        }
        "tempnam" => {
            let [directory, prefix] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_tempnam_result(*directory, *prefix, values)?
        }
        "sleep" => {
            let [seconds] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_sleep_result(*seconds, values)?
        }
        "time" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_time_result(values)?
        }
        "touch" => match evaluated_args {
            [filename] => eval_touch_result(*filename, None, None, values)?,
            [filename, mtime] => eval_touch_result(*filename, Some(*mtime), None, values)?,
            [filename, mtime, atime] => {
                eval_touch_result(*filename, Some(*mtime), Some(*atime), values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "stream_get_filters" | "stream_get_transports" | "stream_get_wrappers" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_stream_introspection_result(name, values)?
        }
        "strtotime" => {
            let [datetime] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_strtotime_result(*datetime, values)?
        }
        "umask" => match evaluated_args {
            [] => eval_umask_result(None, values)?,
            [mask] => eval_umask_result(Some(*mask), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "usleep" => {
            let [microseconds] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_usleep_result(*microseconds, values)?
        }
        "readlink" => {
            let [path] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_readlink_result(*path, values)?
        }
        "unlink" => {
            let [filename] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_unlink_result(*filename, values)?
        }
        "strlen" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            let bytes = values.string_bytes(*value)?;
            let len = i64::try_from(bytes.len()).map_err(|_| EvalStatus::RuntimeFatal)?;
            values.int(len)?
        }
        "strpos" | "strrpos" => {
            let [haystack, needle] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_string_position_result(name, *haystack, *needle, values)?
        }
        "str_contains" | "str_starts_with" | "str_ends_with" => {
            let [haystack, needle] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_string_search_result(name, *haystack, *needle, values)?
        }
        "strstr" => match evaluated_args {
            [haystack, needle] => eval_strstr_result(*haystack, *needle, false, values)?,
            [haystack, needle, before_needle] => {
                let before_needle = values.truthy(*before_needle)?;
                eval_strstr_result(*haystack, *needle, before_needle, values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "strcmp" | "strcasecmp" => {
            let [left, right] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_string_compare_result(name, *left, *right, values)?
        }
        "lcfirst" | "strtolower" | "strtoupper" | "ucfirst" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_string_case_result(name, *value, values)?
        }
        "long2ip" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_long2ip_result(*value, values)?
        }
        "ucwords" => match evaluated_args {
            [value] => eval_ucwords_result(*value, None, values)?,
            [value, separators] => eval_ucwords_result(*value, Some(*separators), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "wordwrap" => match evaluated_args {
            [value] => eval_wordwrap_result(*value, None, None, None, values)?,
            [value, width] => eval_wordwrap_result(*value, Some(*width), None, None, values)?,
            [value, width, break_string] => {
                eval_wordwrap_result(*value, Some(*width), Some(*break_string), None, values)?
            }
            [value, width, break_string, cut] => eval_wordwrap_result(
                *value,
                Some(*width),
                Some(*break_string),
                Some(*cut),
                values,
            )?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        _ => return Ok(None),
    };
    Ok(Some(result))
}

/// Evaluates PHP's `abs(...)` over one eval expression.
fn eval_builtin_abs(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    values.abs(value)
}

/// Evaluates PHP array aggregate builtins over one eval array expression.
fn eval_builtin_array_aggregate(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let array = eval_expr(array, context, scope, values)?;
    eval_array_aggregate_result(name, array, values)
}

/// Computes `array_sum()` or `array_product()` through eval's numeric value hooks.
fn eval_array_aggregate_result(
    name: &str,
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let mut result = match name {
        "array_sum" => values.int(0)?,
        "array_product" => values.int(1)?,
        _ => return Err(EvalStatus::UnsupportedConstruct),
    };
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, key)?;
        result = match name {
            "array_sum" => values.add(result, value)?,
            "array_product" => values.mul(result, value)?,
            _ => return Err(EvalStatus::UnsupportedConstruct),
        };
    }
    Ok(result)
}

/// Evaluates PHP `array_combine()` over key and value array expressions.
fn eval_builtin_array_combine(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [keys, values_array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let keys = eval_expr(keys, context, scope, values)?;
    let values_array = eval_expr(values_array, context, scope, values)?;
    eval_array_combine_result(keys, values_array, values)
}

/// Builds the associative result for `array_combine()` from two eval arrays.
fn eval_array_combine_result(
    keys: RuntimeCellHandle,
    values_array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(keys)?;
    if len != values.array_len(values_array)? {
        return Err(EvalStatus::RuntimeFatal);
    }

    let mut result = values.assoc_new(len)?;
    for position in 0..len {
        let source_key = values.array_iter_key(keys, position)?;
        let target_key = values.array_get(keys, source_key)?;
        let target_key = values.cast_string(target_key)?;
        let value_key = values.array_iter_key(values_array, position)?;
        let value = values.array_get(values_array, value_key)?;
        result = values.array_set(result, target_key, value)?;
    }
    Ok(result)
}

/// Evaluates PHP `array_column()` over row-array and column-key expressions.
fn eval_builtin_array_column(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array, column_key] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let array = eval_expr(array, context, scope, values)?;
    let column_key = eval_expr(column_key, context, scope, values)?;
    eval_array_column_result(array, column_key, values)
}

/// Builds `array_column()` by extracting present row columns into a reindexed array.
fn eval_array_column_result(
    array: RuntimeCellHandle,
    column_key: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let mut result = values.array_new(len)?;
    let mut output_index = 0_i64;
    for position in 0..len {
        let row_key = values.array_iter_key(array, position)?;
        let row = values.array_get(array, row_key)?;
        if !matches!(values.type_tag(row)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
            continue;
        }
        let exists = values.array_key_exists(column_key, row)?;
        if !values.truthy(exists)? {
            continue;
        }
        let column = values.array_get(row, column_key)?;
        let target_key = values.int(output_index)?;
        output_index = output_index
            .checked_add(1)
            .ok_or(EvalStatus::RuntimeFatal)?;
        result = values.array_set(result, target_key, column)?;
    }
    Ok(result)
}

/// Evaluates PHP `array_fill()` over start, count, and value expressions.
fn eval_builtin_array_fill(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [start, count, value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let start = eval_expr(start, context, scope, values)?;
    let count = eval_expr(count, context, scope, values)?;
    let value = eval_expr(value, context, scope, values)?;
    eval_array_fill_result(start, count, value, values)
}

/// Builds an `array_fill()` result with PHP's explicit integer key range.
fn eval_array_fill_result(
    start: RuntimeCellHandle,
    count: RuntimeCellHandle,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let start = eval_int_value(start, values)?;
    let count = eval_int_value(count, values)?;
    if count < 0 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let count = usize::try_from(count).map_err(|_| EvalStatus::RuntimeFatal)?;
    let mut result = if start == 0 {
        values.array_new(count)?
    } else {
        values.assoc_new(count)?
    };
    for offset in 0..count {
        let offset = i64::try_from(offset).map_err(|_| EvalStatus::RuntimeFatal)?;
        let key = start.checked_add(offset).ok_or(EvalStatus::RuntimeFatal)?;
        let key = values.int(key)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Evaluates PHP `array_fill_keys()` over key-array and value expressions.
fn eval_builtin_array_fill_keys(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [keys, value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let keys = eval_expr(keys, context, scope, values)?;
    let value = eval_expr(value, context, scope, values)?;
    eval_array_fill_keys_result(keys, value, values)
}

/// Builds an `array_fill_keys()` result preserving the source key iteration order.
fn eval_array_fill_keys_result(
    keys: RuntimeCellHandle,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(keys)?;
    let mut result = values.assoc_new(len)?;
    for position in 0..len {
        let source_key = values.array_iter_key(keys, position)?;
        let target_key = values.array_get(keys, source_key)?;
        result = values.array_set(result, target_key, value)?;
    }
    Ok(result)
}

/// Evaluates PHP `array_chunk()` over one array and chunk-size expression.
fn eval_builtin_array_chunk(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array, length] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let array = eval_expr(array, context, scope, values)?;
    let length = eval_expr(length, context, scope, values)?;
    eval_array_chunk_result(array, length, values)
}

/// Builds an `array_chunk()` result as nested reindexed arrays.
fn eval_array_chunk_result(
    array: RuntimeCellHandle,
    length: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let chunk_size = eval_int_value(length, values)?;
    if chunk_size <= 0 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let chunk_size = usize::try_from(chunk_size).map_err(|_| EvalStatus::RuntimeFatal)?;
    let len = values.array_len(array)?;
    let chunk_count = len.div_ceil(chunk_size);
    let mut result = values.array_new(chunk_count)?;

    for chunk_index in 0..chunk_count {
        let start = chunk_index * chunk_size;
        let end = usize::min(start + chunk_size, len);
        let mut chunk = values.array_new(end - start)?;
        for source_position in start..end {
            let source_key = values.array_iter_key(array, source_position)?;
            let value = values.array_get(array, source_key)?;
            let target_index =
                i64::try_from(source_position - start).map_err(|_| EvalStatus::RuntimeFatal)?;
            let target_index = values.int(target_index)?;
            chunk = values.array_set(chunk, target_index, value)?;
        }
        let result_key = i64::try_from(chunk_index).map_err(|_| EvalStatus::RuntimeFatal)?;
        let result_key = values.int(result_key)?;
        result = values.array_set(result, result_key, chunk)?;
    }

    Ok(result)
}

/// Evaluates PHP `array_slice()` over array, offset, and optional length expressions.
fn eval_builtin_array_slice(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [array, offset] => {
            let array = eval_expr(array, context, scope, values)?;
            let offset = eval_expr(offset, context, scope, values)?;
            eval_array_slice_result(array, offset, None, values)
        }
        [array, offset, length] => {
            let array = eval_expr(array, context, scope, values)?;
            let offset = eval_expr(offset, context, scope, values)?;
            let length = eval_expr(length, context, scope, values)?;
            eval_array_slice_result(array, offset, Some(length), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Builds an `array_slice()` result with PHP offset and length bounds.
fn eval_array_slice_result(
    array: RuntimeCellHandle,
    offset: RuntimeCellHandle,
    length: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let offset = eval_int_value(offset, values)?;
    let start = eval_slice_start(len, offset)?;
    let end = match length {
        Some(length) if values.type_tag(length)? != EVAL_TAG_NULL => {
            eval_slice_end(len, start, eval_int_value(length, values)?)?
        }
        _ => len,
    };

    let mut result = values.array_new(end.saturating_sub(start))?;
    for source_position in start..end {
        let source_key = values.array_iter_key(array, source_position)?;
        let source_value = values.array_get(array, source_key)?;
        let target_key = i64::try_from(source_position - start)
            .map_err(|_| EvalStatus::RuntimeFatal)?;
        let target_key = values.int(target_key)?;
        result = values.array_set(result, target_key, source_value)?;
    }
    Ok(result)
}

/// Converts a PHP array-slice offset into a bounded source position.
fn eval_slice_start(len: usize, offset: i64) -> Result<usize, EvalStatus> {
    if offset >= 0 {
        let offset = usize::try_from(offset).map_err(|_| EvalStatus::RuntimeFatal)?;
        return Ok(usize::min(offset, len));
    }

    let tail = offset
        .checked_abs()
        .ok_or(EvalStatus::RuntimeFatal)
        .and_then(|value| usize::try_from(value).map_err(|_| EvalStatus::RuntimeFatal))?;
    Ok(len.saturating_sub(tail))
}

/// Converts a PHP array-slice length into a bounded exclusive end position.
fn eval_slice_end(len: usize, start: usize, length: i64) -> Result<usize, EvalStatus> {
    if length >= 0 {
        let length = usize::try_from(length).map_err(|_| EvalStatus::RuntimeFatal)?;
        return Ok(usize::min(start.saturating_add(length), len));
    }

    let tail = length
        .checked_abs()
        .ok_or(EvalStatus::RuntimeFatal)
        .and_then(|value| usize::try_from(value).map_err(|_| EvalStatus::RuntimeFatal))?;
    Ok(usize::max(start, len.saturating_sub(tail)))
}

/// Evaluates PHP `array_pad()` over array, target length, and pad value expressions.
fn eval_builtin_array_pad(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array, length, value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let array = eval_expr(array, context, scope, values)?;
    let length = eval_expr(length, context, scope, values)?;
    let value = eval_expr(value, context, scope, values)?;
    eval_array_pad_result(array, length, value, values)
}

/// Builds an `array_pad()` result by copying values and padding left or right.
fn eval_array_pad_result(
    array: RuntimeCellHandle,
    length: RuntimeCellHandle,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let target = eval_int_value(length, values)?;
    let target_len = target
        .checked_abs()
        .ok_or(EvalStatus::RuntimeFatal)
        .and_then(|value| usize::try_from(value).map_err(|_| EvalStatus::RuntimeFatal))?;
    let result_len = usize::max(len, target_len);
    let pad_count = result_len.saturating_sub(len);
    let mut result = values.array_new(result_len)?;
    let mut output_index = 0usize;

    if target < 0 {
        let (padded, next_index) =
            eval_array_pad_append_repeated(result, output_index, pad_count, value, values)?;
        result = padded;
        output_index = next_index;
    }

    for position in 0..len {
        let source_key = values.array_iter_key(array, position)?;
        let source_value = values.array_get(array, source_key)?;
        let target_key = i64::try_from(output_index).map_err(|_| EvalStatus::RuntimeFatal)?;
        let target_key = values.int(target_key)?;
        result = values.array_set(result, target_key, source_value)?;
        output_index += 1;
    }

    if target > 0 {
        result =
            eval_array_pad_append_repeated(result, output_index, pad_count, value, values)?.0;
    }

    Ok(result)
}

/// Appends the same pad value at consecutive indexed positions in an array result.
fn eval_array_pad_append_repeated(
    mut array: RuntimeCellHandle,
    start_index: usize,
    count: usize,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, usize), EvalStatus> {
    let mut next_index = start_index;
    for _ in 0..count {
        let key = i64::try_from(next_index).map_err(|_| EvalStatus::RuntimeFatal)?;
        let key = values.int(key)?;
        array = values.array_set(array, key, value)?;
        next_index += 1;
    }
    Ok((array, next_index))
}

/// Evaluates PHP `array_flip()` over one eval array expression.
fn eval_builtin_array_flip(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let array = eval_expr(array, context, scope, values)?;
    eval_array_flip_result(array, values)
}

/// Builds the associative result for `array_flip()` using PHP's valid value-key subset.
fn eval_array_flip_result(
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let mut result = values.assoc_new(len)?;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, key)?;
        if !matches!(values.type_tag(value)?, EVAL_TAG_INT | EVAL_TAG_STRING) {
            continue;
        }
        result = values.array_set(result, value, key)?;
    }
    Ok(result)
}

/// Evaluates PHP `array_unique()` over one eval array expression.
fn eval_builtin_array_unique(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let array = eval_expr(array, context, scope, values)?;
    eval_array_unique_result(array, values)
}

/// Builds `array_unique()` by comparing values with PHP's default string comparison mode.
fn eval_array_unique_result(
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let mut seen = Vec::<Vec<u8>>::with_capacity(len);
    let mut result = values.assoc_new(len)?;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, key)?;
        let unique_key = values.string_bytes(value)?;
        if seen.iter().any(|seen_key| seen_key == &unique_key) {
            continue;
        }
        seen.push(unique_key);
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Evaluates PHP array projection builtins over one eval array expression.
fn eval_builtin_array_projection(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let array = eval_expr(array, context, scope, values)?;
    eval_array_projection_result(name, array, values)
}

/// Builds the indexed result array for `array_keys()` or `array_values()`.
fn eval_array_projection_result(
    name: &str,
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let mut result = values.array_new(len)?;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let value = match name {
            "array_keys" => key,
            "array_values" => values.array_get(array, key)?,
            _ => return Err(EvalStatus::UnsupportedConstruct),
        };
        let index = values.int(position as i64)?;
        result = values.array_set(result, index, value)?;
    }
    Ok(result)
}

/// Evaluates PHP `array_reverse()` over an eval array expression.
fn eval_builtin_array_reverse(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [array] => {
            let array = eval_expr(array, context, scope, values)?;
            eval_array_reverse_result(array, false, values)
        }
        [array, preserve_keys] => {
            let array = eval_expr(array, context, scope, values)?;
            let preserve_keys = eval_expr(preserve_keys, context, scope, values)?;
            let preserve_keys = values.truthy(preserve_keys)?;
            eval_array_reverse_result(array, preserve_keys, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Builds an `array_reverse()` result while preserving PHP key rules.
fn eval_array_reverse_result(
    array: RuntimeCellHandle,
    preserve_keys: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let mut keys = Vec::with_capacity(len);
    let mut has_string_key = false;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        has_string_key |= values.type_tag(key)? == EVAL_TAG_STRING;
        keys.push(key);
    }

    let mut result = if preserve_keys || has_string_key {
        values.assoc_new(len)?
    } else {
        values.array_new(len)?
    };
    let mut next_numeric_key = 0_i64;

    for key in keys.into_iter().rev() {
        let value = values.array_get(array, key)?;
        let target_key = if preserve_keys || values.type_tag(key)? == EVAL_TAG_STRING {
            key
        } else {
            let key = values.int(next_numeric_key)?;
            next_numeric_key += 1;
            key
        };
        result = values.array_set(result, target_key, value)?;
    }
    Ok(result)
}

/// Evaluates PHP `array_key_exists()` over a key and array expression.
fn eval_builtin_array_key_exists(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [key, array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let key = eval_expr(key, context, scope, values)?;
    let array = eval_expr(array, context, scope, values)?;
    values.array_key_exists(key, array)
}

/// Evaluates PHP array search builtins over a needle and haystack expression.
fn eval_builtin_array_search(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [needle, array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let needle = eval_expr(needle, context, scope, values)?;
    let array = eval_expr(array, context, scope, values)?;
    eval_array_search_result(name, needle, array, values)
}

/// Searches an eval array with PHP's default loose comparison semantics.
fn eval_array_search_result(
    name: &str,
    needle: RuntimeCellHandle,
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, key)?;
        let equal = values.compare(EvalBinOp::LooseEq, needle, value)?;
        if values.truthy(equal)? {
            return match name {
                "in_array" => values.bool_value(true),
                "array_search" => Ok(key),
                _ => Err(EvalStatus::UnsupportedConstruct),
            };
        }
    }
    match name {
        "in_array" => values.bool_value(false),
        "array_search" => values.bool_value(false),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Evaluates PHP value-set array builtins over two eval array expressions.
fn eval_builtin_array_value_set(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [left, right] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let left = eval_expr(left, context, scope, values)?;
    let right = eval_expr(right, context, scope, values)?;
    eval_array_value_set_result(name, left, right, values)
}

/// Builds `array_diff()` or `array_intersect()` using PHP's default string comparison mode.
fn eval_array_value_set_result(
    name: &str,
    left: RuntimeCellHandle,
    right: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let left_len = values.array_len(left)?;
    let right_len = values.array_len(right)?;
    let mut right_values = Vec::with_capacity(right_len);
    for position in 0..right_len {
        let key = values.array_iter_key(right, position)?;
        let value = values.array_get(right, key)?;
        right_values.push(values.string_bytes(value)?);
    }

    let mut result = values.assoc_new(left_len)?;
    for position in 0..left_len {
        let key = values.array_iter_key(left, position)?;
        let value = values.array_get(left, key)?;
        let comparable = values.string_bytes(value)?;
        let found = right_values.iter().any(|right_value| right_value == &comparable);
        let keep = match name {
            "array_diff" => !found,
            "array_intersect" => found,
            _ => return Err(EvalStatus::UnsupportedConstruct),
        };
        if keep {
            result = values.array_set(result, key, value)?;
        }
    }
    Ok(result)
}

/// Evaluates PHP key-set array builtins over two eval array expressions.
fn eval_builtin_array_key_set(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [left, right] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let left = eval_expr(left, context, scope, values)?;
    let right = eval_expr(right, context, scope, values)?;
    eval_array_key_set_result(name, left, right, values)
}

/// Builds `array_diff_key()` or `array_intersect_key()` by testing first-array keys.
fn eval_array_key_set_result(
    name: &str,
    left: RuntimeCellHandle,
    right: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let left_len = values.array_len(left)?;
    let mut result = values.assoc_new(left_len)?;
    for position in 0..left_len {
        let key = values.array_iter_key(left, position)?;
        let value = values.array_get(left, key)?;
        let exists = values.array_key_exists(key, right)?;
        let found = values.truthy(exists)?;
        let keep = match name {
            "array_diff_key" => !found,
            "array_intersect_key" => found,
            _ => return Err(EvalStatus::UnsupportedConstruct),
        };
        if keep {
            result = values.array_set(result, key, value)?;
        }
    }
    Ok(result)
}

/// Evaluates PHP `array_rand()` over one eval array expression.
fn eval_builtin_array_rand(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let array = eval_expr(array, context, scope, values)?;
    eval_array_rand_result(array, values)
}

/// Returns a valid random key from a non-empty eval array.
fn eval_array_rand_result(
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    if len == 0 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let position = eval_random_position(len);
    values.array_iter_key(array, position)
}

/// Chooses a pseudo-random array position within `[0, len)`.
fn eval_random_position(len: usize) -> usize {
    (eval_random_u128() % (len as u128)) as usize
}

/// Produces a process-local pseudo-random word for non-cryptographic eval builtins.
fn eval_random_u128() -> u128 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let counter = u128::from(EVAL_RANDOM_COUNTER.fetch_add(1, Ordering::Relaxed));
    let pid = u128::from(std::process::id());
    let mut value = nanos ^ (counter.wrapping_mul(0x9e37_79b9_7f4a_7c15)) ^ (pid << 64);
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

/// Evaluates PHP `rand()` and `mt_rand()` over zero args or an inclusive range.
fn eval_builtin_rand(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [] => eval_rand_result(None, None, values),
        [min, max] => {
            let min = eval_expr(min, context, scope, values)?;
            let max = eval_expr(max, context, scope, values)?;
            eval_rand_result(Some(min), Some(max), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Returns one non-cryptographic random integer using PHP's inclusive range rules.
fn eval_rand_result(
    min: Option<RuntimeCellHandle>,
    max: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (min, max) = match (min, max) {
        (None, None) => (0, i64::from(i32::MAX)),
        (Some(min), Some(max)) => (eval_int_value(min, values)?, eval_int_value(max, values)?),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let low = min.min(max);
    let high = min.max(max);
    let width = (i128::from(high) - i128::from(low) + 1) as u128;
    let offset = (eval_random_u128() % width) as i128;
    let sampled = i128::from(low) + offset;
    let sampled = i64::try_from(sampled).map_err(|_| EvalStatus::RuntimeFatal)?;
    values.int(sampled)
}

/// Evaluates PHP `range()` over integer-compatible start and end expressions.
fn eval_builtin_range(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [start, end] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let start = eval_expr(start, context, scope, values)?;
    let end = eval_expr(end, context, scope, values)?;
    eval_range_result(start, end, values)
}

/// Builds an inclusive ascending or descending integer `range()` result.
fn eval_range_result(
    start: RuntimeCellHandle,
    end: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let start = eval_int_value(start, values)?;
    let end = eval_int_value(end, values)?;
    let distance = if start <= end {
        end.checked_sub(start).ok_or(EvalStatus::RuntimeFatal)?
    } else {
        start.checked_sub(end).ok_or(EvalStatus::RuntimeFatal)?
    };
    let count = distance.checked_add(1).ok_or(EvalStatus::RuntimeFatal)?;
    let count = usize::try_from(count).map_err(|_| EvalStatus::RuntimeFatal)?;
    let step = if start <= end { 1_i64 } else { -1_i64 };
    let mut current = start;
    let mut result = values.array_new(count)?;

    for index in 0..count {
        let key = i64::try_from(index).map_err(|_| EvalStatus::RuntimeFatal)?;
        let key = values.int(key)?;
        let value = values.int(current)?;
        result = values.array_set(result, key, value)?;
        if index + 1 < count {
            current = current.checked_add(step).ok_or(EvalStatus::RuntimeFatal)?;
        }
    }
    Ok(result)
}

/// Evaluates PHP `array_merge()` over two array expressions.
fn eval_builtin_array_merge(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [left, right] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let left = eval_expr(left, context, scope, values)?;
    let right = eval_expr(right, context, scope, values)?;
    eval_array_merge_result(left, right, values)
}

/// Builds an `array_merge()` result with PHP numeric reindexing and string-key overwrites.
fn eval_array_merge_result(
    left: RuntimeCellHandle,
    right: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let left_len = values.array_len(left)?;
    let right_len = values.array_len(right)?;
    let capacity = left_len.checked_add(right_len).ok_or(EvalStatus::RuntimeFatal)?;
    let mut result = values.assoc_new(capacity)?;
    let mut next_numeric_key = 0_i64;
    result = eval_array_merge_append_operand(result, left, &mut next_numeric_key, values)?;
    eval_array_merge_append_operand(result, right, &mut next_numeric_key, values)
}

/// Appends one source array to an `array_merge()` result using PHP key handling.
fn eval_array_merge_append_operand(
    mut result: RuntimeCellHandle,
    source: RuntimeCellHandle,
    next_numeric_key: &mut i64,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(source)?;
    for position in 0..len {
        let source_key = values.array_iter_key(source, position)?;
        let source_value = values.array_get(source, source_key)?;
        let target_key = if values.type_tag(source_key)? == EVAL_TAG_STRING {
            source_key
        } else {
            let target_key = values.int(*next_numeric_key)?;
            *next_numeric_key = (*next_numeric_key)
                .checked_add(1)
                .ok_or(EvalStatus::RuntimeFatal)?;
            target_key
        };
        result = values.array_set(result, target_key, source_value)?;
    }
    Ok(result)
}

/// Evaluates PHP `explode()` over separator and string expressions.
fn eval_builtin_explode(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [separator, string] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let separator = eval_expr(separator, context, scope, values)?;
    let string = eval_expr(string, context, scope, values)?;
    eval_explode_result(separator, string, values)
}

/// Splits one PHP byte string into an indexed array using a non-empty separator.
fn eval_explode_result(
    separator: RuntimeCellHandle,
    string: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let separator = values.string_bytes(separator)?;
    if separator.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let string = values.string_bytes(string)?;
    let mut result = values.array_new(0)?;
    let mut start = 0;
    let mut index = 0_i64;
    while let Some(found) = eval_find_subslice(&string, &separator, start) {
        result = eval_push_explode_segment(result, index, &string[start..found], values)?;
        start = found + separator.len();
        index += 1;
    }
    eval_push_explode_segment(result, index, &string[start..], values)
}

/// Appends one split segment to an indexed `explode()` result array.
fn eval_push_explode_segment(
    array: RuntimeCellHandle,
    index: i64,
    segment: &[u8],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let key = values.int(index)?;
    let value = values.string_bytes_value(segment)?;
    values.array_set(array, key, value)
}

/// Finds `needle` inside `haystack` starting from one byte offset.
fn eval_find_subslice(haystack: &[u8], needle: &[u8], start: usize) -> Option<usize> {
    haystack
        .get(start..)?
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|position| position + start)
}

/// Evaluates PHP `implode()` over separator and array expressions.
fn eval_builtin_implode(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [separator, array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let separator = eval_expr(separator, context, scope, values)?;
    let array = eval_expr(array, context, scope, values)?;
    eval_implode_result(separator, array, values)
}

/// Joins array values in eval iteration order using PHP string conversion.
fn eval_implode_result(
    separator: RuntimeCellHandle,
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !values.is_array_like(array)? {
        return Err(EvalStatus::RuntimeFatal);
    }
    let separator = values.string_bytes(separator)?;
    let len = values.array_len(array)?;
    let mut output = Vec::new();
    for position in 0..len {
        if position > 0 {
            output.extend_from_slice(&separator);
        }
        let key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, key)?;
        let value = values.string_bytes(value)?;
        output.extend_from_slice(&value);
    }
    values.string_bytes_value(&output)
}

/// Evaluates PHP's `ceil(...)` over one eval expression.
fn eval_builtin_ceil(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    values.ceil(value)
}

/// Evaluates PHP's `floor(...)` over one eval expression.
fn eval_builtin_floor(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    values.floor(value)
}

/// Evaluates PHP's zero-argument `pi()` builtin.
fn eval_builtin_pi(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    values.float(std::f64::consts::PI)
}

/// Evaluates PHP's `pow(...)` over two eval expressions.
fn eval_builtin_pow(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [left, right] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let left = eval_expr(left, context, scope, values)?;
    let right = eval_expr(right, context, scope, values)?;
    values.pow(left, right)
}

/// Evaluates PHP's `round(...)` over one value and an optional precision expression.
fn eval_builtin_round(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value] => {
            let value = eval_expr(value, context, scope, values)?;
            values.round(value, None)
        }
        [value, precision] => {
            let value = eval_expr(value, context, scope, values)?;
            let precision = eval_expr(precision, context, scope, values)?;
            values.round(value, Some(precision))
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `number_format(...)` over one number and optional separators.
fn eval_builtin_number_format(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value] => {
            let value = eval_expr(value, context, scope, values)?;
            eval_number_format_result(value, None, None, None, values)
        }
        [value, decimals] => {
            let value = eval_expr(value, context, scope, values)?;
            let decimals = eval_expr(decimals, context, scope, values)?;
            eval_number_format_result(value, Some(decimals), None, None, values)
        }
        [value, decimals, decimal_separator] => {
            let value = eval_expr(value, context, scope, values)?;
            let decimals = eval_expr(decimals, context, scope, values)?;
            let decimal_separator = eval_expr(decimal_separator, context, scope, values)?;
            eval_number_format_result(value, Some(decimals), Some(decimal_separator), None, values)
        }
        [value, decimals, decimal_separator, thousands_separator] => {
            let value = eval_expr(value, context, scope, values)?;
            let decimals = eval_expr(decimals, context, scope, values)?;
            let decimal_separator = eval_expr(decimal_separator, context, scope, values)?;
            let thousands_separator = eval_expr(thousands_separator, context, scope, values)?;
            eval_number_format_result(
                value,
                Some(decimals),
                Some(decimal_separator),
                Some(thousands_separator),
                values,
            )
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Formats one PHP numeric value with grouped thousands and fixed decimals.
fn eval_number_format_result(
    value: RuntimeCellHandle,
    decimals: Option<RuntimeCellHandle>,
    decimal_separator: Option<RuntimeCellHandle>,
    thousands_separator: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let value = eval_float_value(value, values)?;
    let decimals = match decimals {
        Some(decimals) => eval_int_value(decimals, values)?,
        None => 0,
    };
    let decimal_separator = match decimal_separator {
        Some(separator) => values.string_bytes(separator)?,
        None => b".".to_vec(),
    };
    let thousands_separator = match thousands_separator {
        Some(separator) => values.string_bytes(separator)?,
        None => b",".to_vec(),
    };
    let output =
        eval_number_format_bytes(value, decimals, &decimal_separator, &thousands_separator)?;
    values.string_bytes_value(&output)
}

/// Converts one eval value to PHP float and returns the scalar payload.
fn eval_float_value(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<f64, EvalStatus> {
    let value = values.cast_float(value)?;
    let bytes = values.string_bytes(value)?;
    std::str::from_utf8(&bytes)
        .map_err(|_| EvalStatus::RuntimeFatal)?
        .parse::<f64>()
        .map_err(|_| EvalStatus::RuntimeFatal)
}

/// Produces PHP `number_format()` bytes for finite scalar values.
fn eval_number_format_bytes(
    value: f64,
    decimals: i64,
    decimal_separator: &[u8],
    thousands_separator: &[u8],
) -> Result<Vec<u8>, EvalStatus> {
    if !value.is_finite() {
        return Ok(value.to_string().into_bytes());
    }
    let decimals = decimals.clamp(-308, 308);
    let display_decimals = decimals.max(0) as usize;
    let abs_value = value.abs();
    let scaled = if decimals >= 0 {
        let scale = 10_f64.powi(decimals as i32);
        (abs_value * scale).round()
    } else {
        let scale = 10_f64.powi((-decimals) as i32);
        (abs_value / scale).round() * scale
    };
    if scaled > (u128::MAX as f64) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let scaled = scaled as u128;
    let scale = 10_u128
        .checked_pow(display_decimals as u32)
        .ok_or(EvalStatus::RuntimeFatal)?;
    let integer = if display_decimals == 0 {
        scaled
    } else {
        scaled / scale
    };
    let fraction = if display_decimals == 0 {
        0
    } else {
        scaled % scale
    };

    let mut output = Vec::new();
    if value.is_sign_negative() && scaled != 0 {
        output.push(b'-');
    }
    eval_append_grouped_decimal(&mut output, integer, thousands_separator);
    if display_decimals > 0 {
        output.extend_from_slice(decimal_separator);
        let fraction = format!("{fraction:0display_decimals$}");
        output.extend_from_slice(fraction.as_bytes());
    }
    Ok(output)
}

/// Appends one unsigned decimal integer with optional three-digit grouping.
fn eval_append_grouped_decimal(output: &mut Vec<u8>, value: u128, separator: &[u8]) {
    let digits = value.to_string();
    if separator.is_empty() {
        output.extend_from_slice(digits.as_bytes());
        return;
    }
    let first_group = match digits.len() % 3 {
        0 => 3,
        len => len,
    };
    output.extend_from_slice(&digits.as_bytes()[..first_group]);
    let mut index = first_group;
    while index < digits.len() {
        output.extend_from_slice(separator);
        output.extend_from_slice(&digits.as_bytes()[index..index + 3]);
        index += 3;
    }
}

/// Evaluates PHP's `sqrt(...)` over one eval expression.
fn eval_builtin_sqrt(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    values.sqrt(value)
}

/// Evaluates PHP's `strrev(...)` over one eval expression.
fn eval_builtin_strrev(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    values.strrev(value)
}

/// Evaluates PHP's `chr(...)` over one eval expression.
fn eval_builtin_chr(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_chr_result(value, values)
}

/// Converts one eval value to a PHP integer and returns the low byte as a string.
fn eval_chr_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let value = eval_int_value(value, values)?;
    values.string_bytes_value(&[value as u8])
}

/// Evaluates PHP's `str_repeat(...)` over one eval expression pair.
fn eval_builtin_str_repeat(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value, times] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    let times = eval_expr(times, context, scope, values)?;
    eval_str_repeat_result(value, times, values)
}

/// Repeats one PHP string byte sequence according to a PHP-cast integer count.
fn eval_str_repeat_result(
    value: RuntimeCellHandle,
    times: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let times = eval_int_value(times, values)?;
    if times < 0 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let times = usize::try_from(times).map_err(|_| EvalStatus::RuntimeFatal)?;
    let capacity = bytes
        .len()
        .checked_mul(times)
        .ok_or(EvalStatus::RuntimeFatal)?;
    let mut output = Vec::with_capacity(capacity);
    for _ in 0..times {
        output.extend_from_slice(&bytes);
    }
    values.string_bytes_value(&output)
}

/// Evaluates PHP's `str_replace(...)` or `str_ireplace(...)` over eval expressions.
fn eval_builtin_str_replace(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [search, replace, subject] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let search = eval_expr(search, context, scope, values)?;
    let replace = eval_expr(replace, context, scope, values)?;
    let subject = eval_expr(subject, context, scope, values)?;
    eval_str_replace_result(name, search, replace, subject, values)
}

/// Replaces every non-overlapping occurrence of a byte-string needle in a subject.
fn eval_str_replace_result(
    name: &str,
    search: RuntimeCellHandle,
    replace: RuntimeCellHandle,
    subject: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let search = values.string_bytes(search)?;
    let replace = values.string_bytes(replace)?;
    let subject = values.string_bytes(subject)?;
    if search.is_empty() {
        return values.string_bytes_value(&subject);
    }

    let mut output = Vec::with_capacity(subject.len());
    let mut start = 0;
    while let Some(found) = eval_find_replace_match(name, &subject, &search, start)? {
        output.extend_from_slice(&subject[start..found]);
        output.extend_from_slice(&replace);
        start = found + search.len();
    }
    output.extend_from_slice(&subject[start..]);
    values.string_bytes_value(&output)
}

/// Finds the next replacement match using case-sensitive or ASCII-insensitive comparison.
fn eval_find_replace_match(
    name: &str,
    subject: &[u8],
    search: &[u8],
    start: usize,
) -> Result<Option<usize>, EvalStatus> {
    match name {
        "str_replace" => Ok(eval_find_subslice(subject, search, start)),
        "str_ireplace" => Ok(subject
            .get(start..)
            .and_then(|tail| {
                tail.windows(search.len())
                    .position(|window| window.eq_ignore_ascii_case(search))
            })
            .map(|position| position + start)),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Evaluates PHP `str_pad(...)` over a string, target length, pad string, and pad mode.
fn eval_builtin_str_pad(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value, length] => {
            let value = eval_expr(value, context, scope, values)?;
            let length = eval_expr(length, context, scope, values)?;
            eval_str_pad_result(value, length, None, None, values)
        }
        [value, length, pad_string] => {
            let value = eval_expr(value, context, scope, values)?;
            let length = eval_expr(length, context, scope, values)?;
            let pad_string = eval_expr(pad_string, context, scope, values)?;
            eval_str_pad_result(value, length, Some(pad_string), None, values)
        }
        [value, length, pad_string, pad_type] => {
            let value = eval_expr(value, context, scope, values)?;
            let length = eval_expr(length, context, scope, values)?;
            let pad_string = eval_expr(pad_string, context, scope, values)?;
            let pad_type = eval_expr(pad_type, context, scope, values)?;
            eval_str_pad_result(value, length, Some(pad_string), Some(pad_type), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Pads one byte string to a PHP target length using cyclic pad bytes.
fn eval_str_pad_result(
    value: RuntimeCellHandle,
    length: RuntimeCellHandle,
    pad_string: Option<RuntimeCellHandle>,
    pad_type: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let target_length = eval_int_value(length, values)?;
    let Ok(target_length) = usize::try_from(target_length) else {
        return values.string_bytes_value(&bytes);
    };
    if target_length <= bytes.len() {
        return values.string_bytes_value(&bytes);
    }

    let pad_string = match pad_string {
        Some(pad_string) => values.string_bytes(pad_string)?,
        None => b" ".to_vec(),
    };
    if pad_string.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let pad_type = match pad_type {
        Some(pad_type) => eval_int_value(pad_type, values)?,
        None => 1,
    };
    let (left_pad, right_pad) =
        eval_str_pad_sides(target_length - bytes.len(), pad_type)?;
    let capacity = bytes
        .len()
        .checked_add(left_pad)
        .and_then(|size| size.checked_add(right_pad))
        .ok_or(EvalStatus::RuntimeFatal)?;
    let mut output = Vec::with_capacity(capacity);
    eval_append_repeated_pad(&mut output, &pad_string, left_pad);
    output.extend_from_slice(&bytes);
    eval_append_repeated_pad(&mut output, &pad_string, right_pad);
    values.string_bytes_value(&output)
}

/// Splits a `str_pad()` pad budget into left and right byte counts.
fn eval_str_pad_sides(pad_budget: usize, pad_type: i64) -> Result<(usize, usize), EvalStatus> {
    match pad_type {
        0 => Ok((pad_budget, 0)),
        1 => Ok((0, pad_budget)),
        2 => Ok((pad_budget / 2, pad_budget - (pad_budget / 2))),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Appends `count` bytes by cycling through the provided non-empty pad string.
fn eval_append_repeated_pad(output: &mut Vec<u8>, pad_string: &[u8], count: usize) {
    for index in 0..count {
        output.push(pad_string[index % pad_string.len()]);
    }
}

/// Evaluates PHP `str_split(...)` over one string and optional chunk length.
fn eval_builtin_str_split(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value] => {
            let value = eval_expr(value, context, scope, values)?;
            eval_str_split_result(value, None, values)
        }
        [value, length] => {
            let value = eval_expr(value, context, scope, values)?;
            let length = eval_expr(length, context, scope, values)?;
            eval_str_split_result(value, Some(length), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Splits one byte string into indexed string chunks using PHP `str_split()` rules.
fn eval_str_split_result(
    value: RuntimeCellHandle,
    length: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let length = match length {
        Some(length) => eval_int_value(length, values)?,
        None => 1,
    };
    if length <= 0 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let length = usize::try_from(length).map_err(|_| EvalStatus::RuntimeFatal)?;
    let mut result = values.array_new(0)?;
    for (index, chunk) in bytes.chunks(length).enumerate() {
        let index = i64::try_from(index).map_err(|_| EvalStatus::RuntimeFatal)?;
        let key = values.int(index)?;
        let value = values.string_bytes_value(chunk)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Evaluates PHP's `nl2br(...)` over one eval expression and optional XHTML flag.
fn eval_builtin_nl2br(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value] => {
            let value = eval_expr(value, context, scope, values)?;
            eval_nl2br_result(value, true, values)
        }
        [value, use_xhtml] => {
            let value = eval_expr(value, context, scope, values)?;
            let use_xhtml = eval_expr(use_xhtml, context, scope, values)?;
            let use_xhtml = values.truthy(use_xhtml)?;
            eval_nl2br_result(value, use_xhtml, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Inserts an HTML line break before each PHP newline sequence while preserving bytes.
fn eval_nl2br_result(
    value: RuntimeCellHandle,
    use_xhtml: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let br = if use_xhtml {
        b"<br />".as_slice()
    } else {
        b"<br>".as_slice()
    };
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'\r' || byte == b'\n' {
            output.extend_from_slice(br);
            output.push(byte);
            if index + 1 < bytes.len()
                && ((byte == b'\r' && bytes[index + 1] == b'\n')
                    || (byte == b'\n' && bytes[index + 1] == b'\r'))
            {
                output.push(bytes[index + 1]);
                index += 2;
                continue;
            }
        } else {
            output.push(byte);
        }
        index += 1;
    }
    values.string_bytes_value(&output)
}

/// Evaluates PHP's `substr(...)` over one eval string, offset, and optional length.
fn eval_builtin_substr(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value, offset] => {
            let value = eval_expr(value, context, scope, values)?;
            let offset = eval_expr(offset, context, scope, values)?;
            eval_substr_result(value, offset, None, values)
        }
        [value, offset, length] => {
            let value = eval_expr(value, context, scope, values)?;
            let offset = eval_expr(offset, context, scope, values)?;
            let length = eval_expr(length, context, scope, values)?;
            eval_substr_result(value, offset, Some(length), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Slices a PHP byte string using PHP `substr()` offset and length rules.
fn eval_substr_result(
    value: RuntimeCellHandle,
    offset: RuntimeCellHandle,
    length: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let total = i64::try_from(bytes.len()).map_err(|_| EvalStatus::RuntimeFatal)?;
    let offset = eval_int_value(offset, values)?;
    let start = if offset < 0 {
        (total + offset).max(0)
    } else {
        offset.min(total)
    };
    let end = match length {
        None => total,
        Some(length) if values.is_null(length)? => total,
        Some(length) => {
            let length = eval_int_value(length, values)?;
            if length < 0 {
                (total + length).max(0)
            } else {
                start.saturating_add(length).min(total)
            }
        }
    };
    let end = end.max(start);
    let start = usize::try_from(start).map_err(|_| EvalStatus::RuntimeFatal)?;
    let end = usize::try_from(end).map_err(|_| EvalStatus::RuntimeFatal)?;
    values.string_bytes_value(&bytes[start..end])
}

/// Evaluates PHP's `substr_replace(...)` over eval scalar byte strings.
fn eval_builtin_substr_replace(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value, replace, offset] => {
            let value = eval_expr(value, context, scope, values)?;
            let replace = eval_expr(replace, context, scope, values)?;
            let offset = eval_expr(offset, context, scope, values)?;
            eval_substr_replace_result(value, replace, offset, None, values)
        }
        [value, replace, offset, length] => {
            let value = eval_expr(value, context, scope, values)?;
            let replace = eval_expr(replace, context, scope, values)?;
            let offset = eval_expr(offset, context, scope, values)?;
            let length = eval_expr(length, context, scope, values)?;
            eval_substr_replace_result(value, replace, offset, Some(length), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Replaces the byte range selected by PHP `substr_replace()` scalar rules.
fn eval_substr_replace_result(
    value: RuntimeCellHandle,
    replace: RuntimeCellHandle,
    offset: RuntimeCellHandle,
    length: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let replacement = values.string_bytes(replace)?;
    let total = i64::try_from(bytes.len()).map_err(|_| EvalStatus::RuntimeFatal)?;
    let offset = eval_int_value(offset, values)?;
    let start = if offset < 0 {
        (total + offset).max(0)
    } else {
        offset.min(total)
    };
    let end = match length {
        None => total,
        Some(length) if values.is_null(length)? => total,
        Some(length) => {
            let length = eval_int_value(length, values)?;
            if length < 0 {
                (total + length).max(start)
            } else {
                start.saturating_add(length).min(total)
            }
        }
    };
    let start = usize::try_from(start).map_err(|_| EvalStatus::RuntimeFatal)?;
    let end = usize::try_from(end).map_err(|_| EvalStatus::RuntimeFatal)?;
    let mut output = Vec::with_capacity(bytes.len() + replacement.len());
    output.extend_from_slice(&bytes[..start]);
    output.extend_from_slice(&replacement);
    output.extend_from_slice(&bytes[end..]);
    values.string_bytes_value(&output)
}

/// Evaluates eval HTML entity encode/decode builtins over one string expression.
fn eval_builtin_html_entity(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_html_entity_result(name, value, values)
}

/// Applies the eval-supported HTML entity transform for one PHP string value.
fn eval_html_entity_result(
    name: &str,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "htmlspecialchars" | "htmlentities" => eval_htmlspecialchars_result(value, values),
        "html_entity_decode" => eval_html_entity_decode_result(value, values),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Encodes the HTML-special byte characters covered by elephc's static helper.
fn eval_htmlspecialchars_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let mut output = Vec::with_capacity(bytes.len());
    for byte in bytes {
        match byte {
            b'&' => output.extend_from_slice(b"&amp;"),
            b'<' => output.extend_from_slice(b"&lt;"),
            b'>' => output.extend_from_slice(b"&gt;"),
            b'"' => output.extend_from_slice(b"&quot;"),
            b'\'' => output.extend_from_slice(b"&#039;"),
            _ => output.push(byte),
        }
    }
    values.string_bytes_value(&output)
}

/// Decodes one pass of the HTML entities emitted by the eval/static encoders.
fn eval_html_entity_decode_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'&' {
            if let Some((decoded, width)) = eval_html_entity_at(&bytes[index..]) {
                output.push(decoded);
                index += width;
                continue;
            }
        }
        output.push(bytes[index]);
        index += 1;
    }
    values.string_bytes_value(&output)
}

/// Returns the decoded byte and consumed width for one supported HTML entity.
fn eval_html_entity_at(bytes: &[u8]) -> Option<(u8, usize)> {
    for (entity, decoded) in [
        (b"&lt;".as_slice(), b'<'),
        (b"&gt;".as_slice(), b'>'),
        (b"&quot;".as_slice(), b'"'),
        (b"&#039;".as_slice(), b'\''),
        (b"&#39;".as_slice(), b'\''),
        (b"&amp;".as_slice(), b'&'),
    ] {
        if bytes.starts_with(entity) {
            return Some((decoded, entity.len()));
        }
    }
    None
}

/// Evaluates PHP URL encode builtins over one eval string expression.
fn eval_builtin_url_encode(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_url_encode_result(name, value, values)
}

/// Percent-encodes one PHP string using query-style or RFC 3986 URL rules.
fn eval_url_encode_result(
    name: &str,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let mut output = Vec::with_capacity(bytes.len());
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for byte in bytes {
        if eval_url_encode_keeps_byte(name, byte)? {
            output.push(byte);
        } else if name == "urlencode" && byte == b' ' {
            output.push(b'+');
        } else {
            output.push(b'%');
            output.push(HEX[(byte >> 4) as usize]);
            output.push(HEX[(byte & 0x0f) as usize]);
        }
    }
    values.string_bytes_value(&output)
}

/// Returns whether a byte remains unescaped for the selected PHP URL encoder.
fn eval_url_encode_keeps_byte(name: &str, byte: u8) -> Result<bool, EvalStatus> {
    let common = byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.');
    match name {
        "urlencode" => Ok(common),
        "rawurlencode" => Ok(common || byte == b'~'),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Evaluates PHP URL decode builtins over one eval string expression.
fn eval_builtin_url_decode(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_url_decode_result(name, value, values)
}

/// Decodes `%XX` sequences and optionally maps `+` to space for `urldecode()`.
fn eval_url_decode_result(
    name: &str,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let plus_to_space = match name {
        "urldecode" => true,
        "rawurldecode" => false,
        _ => return Err(EvalStatus::UnsupportedConstruct),
    };
    let bytes = values.string_bytes(value)?;
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'+' && plus_to_space {
            output.push(b' ');
            index += 1;
        } else if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) = (
                eval_hex_nibble(bytes[index + 1]),
                eval_hex_nibble(bytes[index + 2]),
            ) {
                output.push((high << 4) | low);
                index += 3;
                continue;
            }
            output.push(bytes[index]);
            index += 1;
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    values.string_bytes_value(&output)
}

/// Evaluates PHP `ctype_*` predicates over one eval string expression.
fn eval_builtin_ctype(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_ctype_result(name, value, values)
}

/// Returns the PHP boolean result for one ASCII `ctype_*` byte-string check.
fn eval_ctype_result(
    name: &str,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let mut matches = !bytes.is_empty();
    for byte in bytes {
        if !eval_ctype_byte_matches(name, byte)? {
            matches = false;
            break;
        }
    }
    values.bool_value(matches)
}

/// Checks one byte against the selected PHP ASCII character class.
fn eval_ctype_byte_matches(name: &str, byte: u8) -> Result<bool, EvalStatus> {
    match name {
        "ctype_alpha" => Ok(byte.is_ascii_alphabetic()),
        "ctype_digit" => Ok(byte.is_ascii_digit()),
        "ctype_alnum" => Ok(byte.is_ascii_alphanumeric()),
        "ctype_space" => Ok(matches!(byte, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Evaluates PHP `crc32(...)` over one eval string expression.
fn eval_builtin_crc32(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_crc32_result(value, values)
}

/// Computes PHP's non-negative CRC-32 integer over one converted byte string.
fn eval_crc32_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    values.int(i64::from(eval_crc32_bytes(&bytes)))
}

/// Evaluates one-shot PHP hash digest builtins over eval expressions.
fn eval_builtin_hash_one_shot(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut evaluated_args = Vec::with_capacity(args.len());
    for arg in args {
        evaluated_args.push(eval_expr(arg, context, scope, values)?);
    }
    eval_hash_one_shot_result(name, &evaluated_args, values)
}

/// Computes the result for one-shot PHP hash digest builtins from evaluated args.
fn eval_hash_one_shot_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "md5" | "sha1" => {
            let (data, binary) = match evaluated_args {
                [data] => (*data, false),
                [data, binary] => (*data, values.truthy(*binary)?),
                _ => return Err(EvalStatus::RuntimeFatal),
            };
            let data = values.string_bytes(data)?;
            eval_hash_digest_result(name.as_bytes(), &data, binary, values)
        }
        "hash" => {
            let (algo, data, binary) = match evaluated_args {
                [algo, data] => (*algo, *data, false),
                [algo, data, binary] => (*algo, *data, values.truthy(*binary)?),
                _ => return Err(EvalStatus::RuntimeFatal),
            };
            let algo = values.string_bytes(algo)?;
            let data = values.string_bytes(data)?;
            eval_hash_digest_result(&algo, &data, binary, values)
        }
        "hash_hmac" => {
            let (algo, data, key, binary) = match evaluated_args {
                [algo, data, key] => (*algo, *data, *key, false),
                [algo, data, key, binary] => (*algo, *data, *key, values.truthy(*binary)?),
                _ => return Err(EvalStatus::RuntimeFatal),
            };
            let algo = values.string_bytes(algo)?;
            let data = values.string_bytes(data)?;
            let key = values.string_bytes(key)?;
            eval_hash_hmac_result(&algo, &data, &key, binary, values)
        }
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Computes a one-shot raw digest and formats it as PHP hex or raw bytes.
fn eval_hash_digest_result(
    algo: &[u8],
    data: &[u8],
    binary: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let raw = eval_crypto_hash(algo, data)?;
    eval_format_digest_result(&raw, binary, values)
}

/// Computes a one-shot raw HMAC digest and formats it as PHP hex or raw bytes.
fn eval_hash_hmac_result(
    algo: &[u8],
    data: &[u8],
    key: &[u8],
    binary: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let raw = eval_crypto_hmac(algo, data, key)?;
    eval_format_digest_result(&raw, binary, values)
}

/// Calls the elephc-crypto one-shot hash ABI and returns the raw digest bytes.
fn eval_crypto_hash(algo: &[u8], data: &[u8]) -> Result<Vec<u8>, EvalStatus> {
    let mut output = [0_u8; 64];
    let len = unsafe {
        elephc_crypto::elephc_crypto_hash(
            algo.as_ptr(),
            algo.len(),
            data.as_ptr(),
            data.len(),
            output.as_mut_ptr(),
        )
    };
    eval_crypto_digest_bytes(len, &output)
}

/// Calls the elephc-crypto one-shot HMAC ABI and returns the raw digest bytes.
fn eval_crypto_hmac(algo: &[u8], data: &[u8], key: &[u8]) -> Result<Vec<u8>, EvalStatus> {
    let mut output = [0_u8; 64];
    let len = unsafe {
        elephc_crypto::elephc_crypto_hmac(
            algo.as_ptr(),
            algo.len(),
            key.as_ptr(),
            key.len(),
            data.as_ptr(),
            data.len(),
            output.as_mut_ptr(),
        )
    };
    eval_crypto_digest_bytes(len, &output)
}

/// Converts a crypto ABI digest length into an owned digest byte vector.
fn eval_crypto_digest_bytes(len: isize, output: &[u8; 64]) -> Result<Vec<u8>, EvalStatus> {
    let len = usize::try_from(len).map_err(|_| EvalStatus::RuntimeFatal)?;
    if len > output.len() {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(output[..len].to_vec())
}

/// Formats a raw digest using PHP's `$binary` flag convention.
fn eval_format_digest_result(
    raw: &[u8],
    binary: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if binary {
        return values.string_bytes_value(raw);
    }
    values.string(&eval_lower_hex_bytes(raw))
}

/// Evaluates PHP `hash_algos()` with no arguments.
fn eval_builtin_hash_algos(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_hash_algos_result(values)
}

/// Builds the indexed array returned by eval `hash_algos()`.
fn eval_hash_algos_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_static_string_array_result(EVAL_HASH_ALGOS, values)
}

/// Builds one indexed PHP array from a static string slice.
fn eval_static_string_array_result(
    items: &[&str],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.array_new(items.len())?;
    for (index, item) in items.iter().enumerate() {
        let index = i64::try_from(index).map_err(|_| EvalStatus::RuntimeFatal)?;
        let key = values.int(index)?;
        let value = values.string(item)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Evaluates PHP `spl_classes()` with no arguments.
fn eval_builtin_spl_classes(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_spl_classes_result(values)
}

/// Builds the static class-name list returned by eval `spl_classes()`.
fn eval_spl_classes_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_static_string_array_result(EVAL_SPL_CLASS_NAMES, values)
}

/// Evaluates PHP stream introspection list builtins with no arguments.
fn eval_builtin_stream_introspection(
    name: &str,
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_stream_introspection_result(name, values)
}

/// Builds the static list returned by one eval stream introspection builtin.
fn eval_stream_introspection_result(
    name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let items = match name {
        "stream_get_filters" => EVAL_STREAM_FILTERS,
        "stream_get_transports" => EVAL_STREAM_TRANSPORTS,
        "stream_get_wrappers" => EVAL_STREAM_WRAPPERS,
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    eval_static_string_array_result(items, values)
}

/// Evaluates PHP `time()` with no arguments.
fn eval_builtin_time(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_time_result(values)
}

/// Returns the current Unix timestamp as a boxed PHP integer.
fn eval_time_result(values: &mut impl RuntimeValueOps) -> Result<RuntimeCellHandle, EvalStatus> {
    values.int(eval_current_unix_timestamp()?)
}

/// Returns the current Unix timestamp as an integer payload.
fn eval_current_unix_timestamp() -> Result<i64, EvalStatus> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| EvalStatus::RuntimeFatal)?
        .as_secs();
    i64::try_from(timestamp).map_err(|_| EvalStatus::RuntimeFatal)
}

/// Evaluates PHP `date($format, $timestamp = time())` for the eval subset.
fn eval_builtin_date(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [format] => {
            let format = eval_expr(format, context, scope, values)?;
            eval_date_result(format, None, values)
        }
        [format, timestamp] => {
            let format = eval_expr(format, context, scope, values)?;
            let timestamp = eval_expr(timestamp, context, scope, values)?;
            eval_date_result(format, Some(timestamp), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Formats one Unix timestamp through PHP `date()` token rules supported by elephc.
fn eval_date_result(
    format: RuntimeCellHandle,
    timestamp: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let format = values.string_bytes(format)?;
    let timestamp = match timestamp {
        Some(timestamp) => eval_int_value(timestamp, values)?,
        None => eval_current_unix_timestamp()?,
    };
    let tm = eval_localtime(timestamp)?;
    let output = eval_format_date_bytes(&format, &tm, timestamp)?;
    values.string_bytes_value(&output)
}

/// Converts one Unix timestamp to local broken-down time through libc.
fn eval_localtime(timestamp: i64) -> Result<libc::tm, EvalStatus> {
    let raw: libc::time_t = timestamp.try_into().map_err(|_| EvalStatus::RuntimeFatal)?;
    let mut tm = MaybeUninit::<libc::tm>::uninit();
    let result = unsafe { libc::localtime_r(&raw, tm.as_mut_ptr()) };
    if result.is_null() {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(unsafe { tm.assume_init() })
}

/// Applies PHP `date()` tokens to one local broken-down timestamp.
fn eval_format_date_bytes(
    format: &[u8],
    tm: &libc::tm,
    timestamp: i64,
) -> Result<Vec<u8>, EvalStatus> {
    let mut output = Vec::new();
    let mut escaped = false;
    for byte in format {
        if escaped {
            output.push(*byte);
            escaped = false;
            continue;
        }
        if *byte == b'\\' {
            escaped = true;
            continue;
        }
        eval_push_date_token(&mut output, *byte, tm, timestamp)?;
    }
    if escaped {
        output.push(b'\\');
    }
    Ok(output)
}

/// Appends the expansion for one PHP `date()` token, or the token literal.
fn eval_push_date_token(
    output: &mut Vec<u8>,
    token: u8,
    tm: &libc::tm,
    timestamp: i64,
) -> Result<(), EvalStatus> {
    match token {
        b'Y' => eval_push_padded_number(output, i64::from(tm.tm_year) + 1900, 4),
        b'm' => eval_push_padded_number(output, i64::from(tm.tm_mon) + 1, 2),
        b'd' => eval_push_padded_number(output, i64::from(tm.tm_mday), 2),
        b'H' => eval_push_padded_number(output, i64::from(tm.tm_hour), 2),
        b'i' => eval_push_padded_number(output, i64::from(tm.tm_min), 2),
        b's' => eval_push_padded_number(output, i64::from(tm.tm_sec), 2),
        b'l' => output.extend_from_slice(EVAL_WEEKDAY_NAMES[eval_tm_weekday_index(tm)?].as_bytes()),
        b'F' => output.extend_from_slice(EVAL_MONTH_NAMES[eval_tm_month_index(tm)?].as_bytes()),
        b'D' => {
            output.extend_from_slice(EVAL_WEEKDAY_SHORT_NAMES[eval_tm_weekday_index(tm)?].as_bytes())
        }
        b'M' => {
            output.extend_from_slice(EVAL_MONTH_SHORT_NAMES[eval_tm_month_index(tm)?].as_bytes())
        }
        b'N' => {
            let weekday = tm.tm_wday;
            let iso_weekday = if weekday == 0 { 7 } else { weekday };
            output.extend_from_slice(iso_weekday.to_string().as_bytes());
        }
        b'j' => output.extend_from_slice(tm.tm_mday.to_string().as_bytes()),
        b'n' => output.extend_from_slice((tm.tm_mon + 1).to_string().as_bytes()),
        b'G' => output.extend_from_slice(tm.tm_hour.to_string().as_bytes()),
        b'g' => {
            let hour = tm.tm_hour % 12;
            let hour = if hour == 0 { 12 } else { hour };
            output.extend_from_slice(hour.to_string().as_bytes());
        }
        b'A' => output.extend_from_slice(if tm.tm_hour < 12 { b"AM" } else { b"PM" }),
        b'a' => output.extend_from_slice(if tm.tm_hour < 12 { b"am" } else { b"pm" }),
        b'U' => output.extend_from_slice(timestamp.to_string().as_bytes()),
        _ => output.push(token),
    }
    Ok(())
}

/// Returns a checked month index for PHP `date()` name tables.
fn eval_tm_month_index(tm: &libc::tm) -> Result<usize, EvalStatus> {
    let index = usize::try_from(tm.tm_mon).map_err(|_| EvalStatus::RuntimeFatal)?;
    if index >= EVAL_MONTH_NAMES.len() {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(index)
}

/// Returns a checked weekday index for PHP `date()` name tables.
fn eval_tm_weekday_index(tm: &libc::tm) -> Result<usize, EvalStatus> {
    let index = usize::try_from(tm.tm_wday).map_err(|_| EvalStatus::RuntimeFatal)?;
    if index >= EVAL_WEEKDAY_NAMES.len() {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(index)
}

/// Appends one zero-padded decimal value with the requested minimum width.
fn eval_push_padded_number(output: &mut Vec<u8>, value: i64, width: usize) {
    output.extend_from_slice(format!("{value:0width$}").as_bytes());
}

/// Evaluates PHP `mktime(hour, minute, second, month, day, year)`.
fn eval_builtin_mktime(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [hour, minute, second, month, day, year] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let hour = eval_expr(hour, context, scope, values)?;
    let minute = eval_expr(minute, context, scope, values)?;
    let second = eval_expr(second, context, scope, values)?;
    let month = eval_expr(month, context, scope, values)?;
    let day = eval_expr(day, context, scope, values)?;
    let year = eval_expr(year, context, scope, values)?;
    eval_mktime_result(hour, minute, second, month, day, year, values)
}

/// Converts PHP date components to a local Unix timestamp through libc `mktime`.
fn eval_mktime_result(
    hour: RuntimeCellHandle,
    minute: RuntimeCellHandle,
    second: RuntimeCellHandle,
    month: RuntimeCellHandle,
    day: RuntimeCellHandle,
    year: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let timestamp = eval_mktime_timestamp(
        eval_int_cell_as_c_int(hour, values)?,
        eval_int_cell_as_c_int(minute, values)?,
        eval_int_cell_as_c_int(second, values)?,
        eval_int_cell_as_c_int(month, values)?,
        eval_int_cell_as_c_int(day, values)?,
        eval_int_cell_as_c_int(year, values)?,
    )?;
    values.int(timestamp)
}

/// Converts local date components into a Unix timestamp through libc `mktime`.
fn eval_mktime_timestamp(
    hour: libc::c_int,
    minute: libc::c_int,
    second: libc::c_int,
    month: libc::c_int,
    day: libc::c_int,
    year: libc::c_int,
) -> Result<i64, EvalStatus> {
    let mut tm = unsafe { MaybeUninit::<libc::tm>::zeroed().assume_init() };
    tm.tm_hour = hour;
    tm.tm_min = minute;
    tm.tm_sec = second;
    tm.tm_mon = month - 1;
    tm.tm_mday = day;
    tm.tm_year = year - 1900;
    tm.tm_isdst = -1;
    let timestamp = unsafe { libc::mktime(&mut tm) };
    i64::try_from(timestamp).map_err(|_| EvalStatus::RuntimeFatal)
}

/// Casts one eval cell to a PHP int and checks it fits a libc `c_int`.
fn eval_int_cell_as_c_int(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<libc::c_int, EvalStatus> {
    let value = eval_int_value(value, values)?;
    libc::c_int::try_from(value).map_err(|_| EvalStatus::RuntimeFatal)
}

/// Evaluates PHP `strtotime(datetime)` for eval's supported date-string subset.
fn eval_builtin_strtotime(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [datetime] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let datetime = eval_expr(datetime, context, scope, values)?;
    eval_strtotime_result(datetime, values)
}

/// Parses one eval `strtotime()` input and boxes the resulting timestamp.
fn eval_strtotime_result(
    datetime: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(datetime)?;
    let timestamp = eval_strtotime_bytes(&bytes)?;
    values.int(timestamp)
}

/// Parses eval's supported `strtotime()` strings into local Unix timestamps.
fn eval_strtotime_bytes(bytes: &[u8]) -> Result<i64, EvalStatus> {
    let bytes = eval_trim_ascii_whitespace(bytes);
    if bytes.eq_ignore_ascii_case(b"now") {
        return eval_current_unix_timestamp();
    }
    let Some((year, month, day, hour, minute, second)) = eval_parse_iso_datetime(bytes) else {
        return Ok(-1);
    };
    eval_mktime_timestamp(hour, minute, second, month, day, year)
}

/// Trims ASCII whitespace from both ends of one byte slice.
fn eval_trim_ascii_whitespace(bytes: &[u8]) -> &[u8] {
    let mut start = 0;
    let mut end = bytes.len();
    while start < end && bytes[start].is_ascii_whitespace() {
        start += 1;
    }
    while end > start && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    &bytes[start..end]
}

/// Parses fixed-width ISO date and datetime forms supported by eval `strtotime()`.
fn eval_parse_iso_datetime(
    bytes: &[u8],
) -> Option<(
    libc::c_int,
    libc::c_int,
    libc::c_int,
    libc::c_int,
    libc::c_int,
    libc::c_int,
)> {
    if bytes.len() != 10 && bytes.len() != 16 && bytes.len() != 19 {
        return None;
    }
    if bytes.get(4) != Some(&b'-') || bytes.get(7) != Some(&b'-') {
        return None;
    }
    let year = eval_parse_fixed_digits(bytes, 0, 4)?;
    let month = eval_parse_fixed_digits(bytes, 5, 2)?;
    let day = eval_parse_fixed_digits(bytes, 8, 2)?;
    let (hour, minute, second) = if bytes.len() == 10 {
        (0, 0, 0)
    } else {
        if !matches!(bytes.get(10), Some(b' ') | Some(b'T') | Some(b't')) {
            return None;
        }
        if bytes.get(13) != Some(&b':') {
            return None;
        }
        let hour = eval_parse_fixed_digits(bytes, 11, 2)?;
        let minute = eval_parse_fixed_digits(bytes, 14, 2)?;
        let second = if bytes.len() == 19 {
            if bytes.get(16) != Some(&b':') {
                return None;
            }
            eval_parse_fixed_digits(bytes, 17, 2)?
        } else {
            0
        };
        (hour, minute, second)
    };
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || !(0..=23).contains(&hour)
        || !(0..=59).contains(&minute)
        || !(0..=59).contains(&second)
    {
        return None;
    }
    Some((year, month, day, hour, minute, second))
}

/// Parses a fixed-width decimal field as a libc-compatible integer.
fn eval_parse_fixed_digits(bytes: &[u8], start: usize, len: usize) -> Option<libc::c_int> {
    let end = start.checked_add(len)?;
    let field = bytes.get(start..end)?;
    let mut value: libc::c_int = 0;
    for byte in field {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?;
        value = value.checked_add(libc::c_int::from(byte - b'0'))?;
    }
    Some(value)
}

/// Evaluates PHP `microtime()` with an optional ignored argument.
fn eval_builtin_microtime(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [] => eval_microtime_result(values),
        [as_float] => {
            let _ = eval_expr(as_float, context, scope, values)?;
            eval_microtime_result(values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Returns the current Unix timestamp with microsecond precision as a boxed float.
fn eval_microtime_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| EvalStatus::RuntimeFatal)?;
    let seconds = timestamp.as_secs() as f64;
    let micros = f64::from(timestamp.subsec_micros()) / 1_000_000.0;
    values.float(seconds + micros)
}

/// Evaluates PHP `sleep($seconds)` over one eval expression.
fn eval_builtin_sleep(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [seconds] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let seconds = eval_expr(seconds, context, scope, values)?;
    eval_sleep_result(seconds, values)
}

/// Sleeps for a non-negative number of seconds and returns PHP's remaining-seconds value.
fn eval_sleep_result(
    seconds: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let seconds = eval_int_value(seconds, values)?;
    let seconds = u64::try_from(seconds).map_err(|_| EvalStatus::RuntimeFatal)?;
    std::thread::sleep(std::time::Duration::from_secs(seconds));
    values.int(0)
}

/// Evaluates PHP `usleep($microseconds)` over one eval expression.
fn eval_builtin_usleep(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [microseconds] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let microseconds = eval_expr(microseconds, context, scope, values)?;
    eval_usleep_result(microseconds, values)
}

/// Sleeps for a non-negative number of microseconds and returns PHP null.
fn eval_usleep_result(
    microseconds: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let microseconds = eval_int_value(microseconds, values)?;
    let microseconds = u64::try_from(microseconds).map_err(|_| EvalStatus::RuntimeFatal)?;
    std::thread::sleep(std::time::Duration::from_micros(microseconds));
    values.null()
}

/// Evaluates PHP `phpversion()` with no arguments.
fn eval_builtin_phpversion(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_phpversion_result(values)
}

/// Returns the root elephc package version as a boxed PHP string.
fn eval_phpversion_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.string(eval_compiler_php_version())
}

/// Reads the root package version from the workspace manifest used by native `phpversion()`.
fn eval_compiler_php_version() -> &'static str {
    let mut in_package = false;
    for line in EVAL_ROOT_CARGO_TOML.lines() {
        let line = line.trim();
        if line == "[package]" {
            in_package = true;
            continue;
        }
        if in_package && line.starts_with('[') {
            break;
        }
        if in_package {
            if let Some(value) = line.strip_prefix("version = ") {
                return value.trim_matches('"');
            }
        }
    }
    env!("CARGO_PKG_VERSION")
}

/// Evaluates PHP `php_uname($mode = "a")` over zero or one eval expression.
fn eval_builtin_php_uname(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [] => eval_php_uname_result(None, values),
        [mode] => {
            let mode = eval_expr(mode, context, scope, values)?;
            eval_php_uname_result(Some(mode), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Reads the local uname fields and formats the PHP `php_uname()` mode result.
fn eval_php_uname_result(
    mode: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mode = match mode {
        Some(mode) => {
            let bytes = values.string_bytes(mode)?;
            let [mode] = bytes.as_slice() else {
                return Err(EvalStatus::RuntimeFatal);
            };
            *mode
        }
        None => b'a',
    };

    let mut utsname = std::mem::MaybeUninit::<libc::utsname>::zeroed();
    let status = unsafe {
        // libc writes all uname fields into the stack-owned utsname buffer.
        libc::uname(utsname.as_mut_ptr())
    };
    if status != 0 {
        return values.string("");
    }
    let utsname = unsafe {
        // `uname` succeeded, so libc initialized the full `utsname` structure.
        utsname.assume_init()
    };
    let sysname = eval_uname_field_bytes(&utsname.sysname);
    let nodename = eval_uname_field_bytes(&utsname.nodename);
    let release = eval_uname_field_bytes(&utsname.release);
    let version = eval_uname_field_bytes(&utsname.version);
    let machine = eval_uname_field_bytes(&utsname.machine);

    match mode {
        b'a' => {
            let mut output = Vec::new();
            for field in [&sysname, &nodename, &release, &version, &machine] {
                if !output.is_empty() {
                    output.push(b' ');
                }
                output.extend_from_slice(field);
            }
            values.string_bytes_value(&output)
        }
        b's' => values.string_bytes_value(&sysname),
        b'n' => values.string_bytes_value(&nodename),
        b'r' => values.string_bytes_value(&release),
        b'v' => values.string_bytes_value(&version),
        b'm' => values.string_bytes_value(&machine),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Copies one NUL-terminated `utsname` field into raw PHP string bytes.
fn eval_uname_field_bytes(field: &[libc::c_char]) -> Vec<u8> {
    let length = field
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(field.len());
    field[..length].iter().map(|byte| *byte as u8).collect()
}

/// Evaluates PHP `getcwd()` with no arguments.
fn eval_builtin_getcwd(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_getcwd_result(values)
}

/// Returns the process current working directory as a boxed PHP string.
fn eval_getcwd_result(values: &mut impl RuntimeValueOps) -> Result<RuntimeCellHandle, EvalStatus> {
    let cwd = std::env::current_dir().map_err(|_| EvalStatus::RuntimeFatal)?;
    values.string(cwd.to_string_lossy().as_ref())
}

/// Evaluates one PHP filesystem predicate over an eval expression.
fn eval_builtin_file_probe(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filename] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let filename = eval_expr(filename, context, scope, values)?;
    eval_file_probe_result(name, filename, values)
}

/// Computes one local filesystem predicate and returns a PHP boolean.
fn eval_file_probe_result(
    name: &str,
    filename: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    let path = std::path::Path::new(&path);
    let result = match name {
        "file_exists" => path.exists(),
        "is_dir" => path.is_dir(),
        "is_executable" => eval_path_is_executable(path),
        "is_file" => path.is_file(),
        "is_link" => std::fs::symlink_metadata(path)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false),
        "is_readable" => eval_path_is_readable(path),
        "is_writable" | "is_writeable" => eval_path_is_writable(path),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    values.bool_value(result)
}

/// Evaluates one scalar PHP stat metadata builtin over an eval expression.
fn eval_builtin_file_stat_scalar(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filename] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let filename = eval_expr(filename, context, scope, values)?;
    eval_file_stat_scalar_result(name, filename, values)
}

/// Returns scalar stat metadata, using PHP false for failure where native elephc does.
fn eval_file_stat_scalar_result(
    name: &str,
    filename: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(_) if name == "filemtime" => return values.int(0),
        Err(_) => return values.bool_value(false),
    };
    match name {
        "fileatime" => values.int(metadata.atime()),
        "filectime" => values.int(metadata.ctime()),
        "filegroup" => values.int(i64::from(metadata.gid())),
        "fileinode" => {
            values.int(i64::try_from(metadata.ino()).map_err(|_| EvalStatus::RuntimeFatal)?)
        }
        "filemtime" => values.int(metadata.mtime()),
        "fileowner" => values.int(i64::from(metadata.uid())),
        "fileperms" => values.int(i64::from(metadata.mode())),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `file_get_contents($filename)` over one eval expression.
fn eval_builtin_file_get_contents(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filename] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let filename = eval_expr(filename, context, scope, values)?;
    eval_file_get_contents_result(filename, values)
}

/// Reads a local file into a PHP string, or returns false when it cannot be opened.
fn eval_file_get_contents_result(
    filename: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    match std::fs::read(path) {
        Ok(bytes) => values.string_bytes_value(&bytes),
        Err(_) => {
            values.warning("Warning: file_get_contents(): Failed to open stream\n")?;
            values.bool_value(false)
        }
    }
}

/// Evaluates PHP `file($filename)` over one eval expression.
fn eval_builtin_file(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filename] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let filename = eval_expr(filename, context, scope, values)?;
    eval_file_result(filename, values)
}

/// Reads one local file and returns an indexed array of line byte strings.
fn eval_file_result(
    filename: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => {
            values.warning("Warning: file_get_contents(): Failed to open stream\n")?;
            return values.array_new(0);
        }
    };
    eval_file_lines_array(&bytes, values)
}

/// Splits file payload bytes into runtime array entries, preserving trailing newlines.
fn eval_file_lines_array(
    bytes: &[u8],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.array_new(0)?;
    let mut line_start = 0;
    let mut line_index = 0;
    for (index, byte) in bytes.iter().enumerate() {
        if *byte != b'\n' {
            continue;
        }
        result =
            eval_array_set_indexed_bytes(result, line_index, &bytes[line_start..=index], values)?;
        line_start = index + 1;
        line_index += 1;
    }
    if line_start < bytes.len() {
        result = eval_array_set_indexed_bytes(result, line_index, &bytes[line_start..], values)?;
    }
    Ok(result)
}

/// Evaluates PHP `readfile($filename)` over one eval expression.
fn eval_builtin_readfile(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filename] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let filename = eval_expr(filename, context, scope, values)?;
    eval_readfile_result(filename, values)
}

/// Streams one local file to eval output and returns a byte count, false, or -1.
fn eval_readfile_result(
    filename: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    let path = std::path::Path::new(&path);
    if path.is_dir() {
        return values.int(-1);
    }
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => return values.bool_value(false),
    };
    let output = values.string_bytes_value(&bytes)?;
    values.echo(output)?;
    values.int(i64::try_from(bytes.len()).map_err(|_| EvalStatus::RuntimeFatal)?)
}

/// Evaluates PHP `file_put_contents($filename, $data)` over one eval expression.
fn eval_builtin_file_put_contents(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filename, data] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let filename = eval_expr(filename, context, scope, values)?;
    let data = eval_expr(data, context, scope, values)?;
    eval_file_put_contents_result(filename, data, values)
}

/// Writes a PHP string to a local file and returns the written byte count or false.
fn eval_file_put_contents_result(
    filename: RuntimeCellHandle,
    data: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    let data = values.string_bytes(data)?;
    match std::fs::write(path, &data) {
        Ok(()) => values.int(i64::try_from(data.len()).map_err(|_| EvalStatus::RuntimeFatal)?),
        Err(_) => values.bool_value(false),
    }
}

/// Evaluates PHP `filesize($filename)` over one eval expression.
fn eval_builtin_filesize(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filename] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let filename = eval_expr(filename, context, scope, values)?;
    eval_filesize_result(filename, values)
}

/// Returns one local file size in bytes, or zero when stat fails.
fn eval_filesize_result(
    filename: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    let len = std::fs::metadata(path).map(|metadata| metadata.len()).unwrap_or(0);
    values.int(i64::try_from(len).map_err(|_| EvalStatus::RuntimeFatal)?)
}

/// Evaluates PHP `filetype($filename)` over one eval expression.
fn eval_builtin_filetype(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filename] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let filename = eval_expr(filename, context, scope, values)?;
    eval_filetype_result(filename, values)
}

/// Returns the PHP filetype string for one path, or false when lstat fails.
fn eval_filetype_result(
    filename: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    let file_type = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata.file_type(),
        Err(_) => return values.bool_value(false),
    };
    let label = if file_type.is_file() {
        "file"
    } else if file_type.is_dir() {
        "dir"
    } else if file_type.is_symlink() {
        "link"
    } else if file_type.is_char_device() {
        "char"
    } else if file_type.is_block_device() {
        "block"
    } else if file_type.is_fifo() {
        "fifo"
    } else if file_type.is_socket() {
        "socket"
    } else {
        "unknown"
    };
    values.string(label)
}

/// Evaluates PHP `stat($filename)` or `lstat($filename)` over one eval expression.
fn eval_builtin_stat_array(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filename] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let filename = eval_expr(filename, context, scope, values)?;
    eval_stat_array_result(name, filename, values)
}

/// Builds PHP's stat array for one local path, or returns false on stat failure.
fn eval_stat_array_result(
    name: &str,
    filename: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    let metadata = match name {
        "stat" => std::fs::metadata(path),
        "lstat" => std::fs::symlink_metadata(path),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let metadata = match metadata {
        Ok(metadata) => metadata,
        Err(_) => return values.bool_value(false),
    };
    eval_stat_metadata_array(&metadata, values)
}

/// Converts filesystem metadata into PHP's numeric-and-string keyed stat array.
fn eval_stat_metadata_array(
    metadata: &std::fs::Metadata,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let fields = [
        ("dev", eval_u64_to_i64(metadata.dev())?),
        ("ino", eval_u64_to_i64(metadata.ino())?),
        ("mode", i64::from(metadata.mode())),
        ("nlink", eval_u64_to_i64(metadata.nlink())?),
        ("uid", i64::from(metadata.uid())),
        ("gid", i64::from(metadata.gid())),
        ("rdev", eval_u64_to_i64(metadata.rdev())?),
        ("size", eval_u64_to_i64(metadata.size())?),
        ("atime", metadata.atime()),
        ("mtime", metadata.mtime()),
        ("ctime", metadata.ctime()),
        ("blksize", eval_u64_to_i64(metadata.blksize())?),
        ("blocks", eval_u64_to_i64(metadata.blocks())?),
    ];
    let mut result = values.assoc_new(fields.len() * 2)?;
    for (index, (name, value)) in fields.iter().enumerate() {
        result = eval_stat_array_set_int_key(result, index, *value, values)?;
        result = eval_stat_array_set_string_key(result, name, *value, values)?;
    }
    Ok(result)
}

/// Inserts one integer stat field under a numeric PHP array key.
fn eval_stat_array_set_int_key(
    array: RuntimeCellHandle,
    key: usize,
    value: i64,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let key = values.int(i64::try_from(key).map_err(|_| EvalStatus::RuntimeFatal)?)?;
    let value = values.int(value)?;
    values.array_set(array, key, value)
}

/// Inserts one integer stat field under a string PHP array key.
fn eval_stat_array_set_string_key(
    array: RuntimeCellHandle,
    key: &str,
    value: i64,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let key = values.string(key)?;
    let value = values.int(value)?;
    values.array_set(array, key, value)
}

/// Converts unsigned stat metadata into the signed integer payload used by PHP cells.
fn eval_u64_to_i64(value: u64) -> Result<i64, EvalStatus> {
    i64::try_from(value).map_err(|_| EvalStatus::RuntimeFatal)
}

/// Evaluates PHP `disk_free_space($directory)` or `disk_total_space($directory)`.
fn eval_builtin_disk_space(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [directory] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let directory = eval_expr(directory, context, scope, values)?;
    eval_disk_space_result(name, directory, values)
}

/// Reports available or total filesystem bytes as a PHP float, or 0.0 on failure.
fn eval_disk_space_result(
    name: &str,
    directory: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(directory)?;
    let Ok(path) = CString::new(bytes) else {
        return values.float(0.0);
    };
    let mut stats = std::mem::MaybeUninit::<libc::statvfs>::zeroed();
    let status = unsafe {
        // libc writes the statvfs fields for this NUL-terminated local path.
        libc::statvfs(path.as_ptr(), stats.as_mut_ptr())
    };
    if status != 0 {
        return values.float(0.0);
    }
    let stats = unsafe {
        // `statvfs` succeeded, so libc initialized the full stat buffer.
        stats.assume_init()
    };
    let block_size = if stats.f_frsize > 0 {
        stats.f_frsize
    } else {
        stats.f_bsize
    };
    let blocks = match name {
        "disk_free_space" => stats.f_bavail,
        "disk_total_space" => stats.f_blocks,
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    values.float((block_size as f64) * (blocks as f64))
}

/// Evaluates a one-path filesystem operation that returns a PHP boolean.
fn eval_builtin_unary_path_bool(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [path] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let path = eval_expr(path, context, scope, values)?;
    eval_unary_path_bool_result(name, path, values)
}

/// Executes a one-path local filesystem operation and returns whether it succeeded.
fn eval_unary_path_bool_result(
    name: &str,
    path: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(path, values)?;
    let ok = match name {
        "chdir" => std::env::set_current_dir(path).is_ok(),
        "mkdir" => std::fs::create_dir(path).is_ok(),
        "rmdir" => std::fs::remove_dir(path).is_ok(),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    values.bool_value(ok)
}

/// Evaluates a two-path filesystem operation that returns a PHP boolean.
fn eval_builtin_binary_path_bool(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [from, to] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let from = eval_expr(from, context, scope, values)?;
    let to = eval_expr(to, context, scope, values)?;
    eval_binary_path_bool_result(name, from, to, values)
}

/// Executes a two-path local filesystem operation and returns whether it succeeded.
fn eval_binary_path_bool_result(
    name: &str,
    from: RuntimeCellHandle,
    to: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let from = eval_path_string(from, values)?;
    let to = eval_path_string(to, values)?;
    let ok = match name {
        "copy" => std::fs::copy(from, to).is_ok(),
        "link" => std::fs::hard_link(from, to).is_ok(),
        "rename" => std::fs::rename(from, to).is_ok(),
        "symlink" => std::os::unix::fs::symlink(from, to).is_ok(),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    values.bool_value(ok)
}

/// Evaluates PHP `chmod($filename, $permissions)` over eval expressions.
fn eval_builtin_chmod(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filename, permissions] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let filename = eval_expr(filename, context, scope, values)?;
    let permissions = eval_expr(permissions, context, scope, values)?;
    eval_chmod_result(filename, permissions, values)
}

/// Changes one local file's mode and returns whether the operation succeeded.
fn eval_chmod_result(
    filename: RuntimeCellHandle,
    permissions: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    let mode = eval_int_value(permissions, values)? as u32;
    let permissions = std::fs::Permissions::from_mode(mode);
    values.bool_value(std::fs::set_permissions(path, permissions).is_ok())
}

/// Evaluates PHP `scandir($directory)` over one eval expression.
fn eval_builtin_scandir(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [directory] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let directory = eval_expr(directory, context, scope, values)?;
    eval_scandir_result(directory, values)
}

/// Lists one local directory into an indexed string array, or an empty array on failure.
fn eval_scandir_result(
    directory: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(directory, values)?;
    let Ok(entries) = std::fs::read_dir(path) else {
        return values.array_new(0);
    };
    let mut names = vec![".".to_string(), "..".to_string()];
    for entry in entries {
        let entry = entry.map_err(|_| EvalStatus::RuntimeFatal)?;
        names.push(entry.file_name().to_string_lossy().into_owned());
    }
    names.sort();
    let mut result = values.array_new(names.len())?;
    for (index, name) in names.iter().enumerate() {
        result = eval_array_set_indexed_bytes(result, index, name.as_bytes(), values)?;
    }
    Ok(result)
}

/// Evaluates PHP `glob($pattern)` over one eval expression.
fn eval_builtin_glob(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [pattern] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let pattern = eval_expr(pattern, context, scope, values)?;
    eval_glob_result(pattern, values)
}

/// Expands one local glob pattern into a sorted indexed PHP string array.
fn eval_glob_result(
    pattern: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let pattern = eval_path_string(pattern, values)?;
    let matches = eval_glob_matches(&pattern);
    let mut result = values.array_new(matches.len())?;
    for (index, path) in matches.iter().enumerate() {
        result = eval_array_set_indexed_bytes(result, index, path.as_bytes(), values)?;
    }
    Ok(result)
}

/// Collects sorted matches for one local glob pattern.
fn eval_glob_matches(pattern: &str) -> Vec<String> {
    if pattern.is_empty() {
        return Vec::new();
    }
    if !eval_glob_component_has_magic(pattern) {
        return std::path::Path::new(pattern)
            .exists()
            .then(|| pattern.to_string())
            .into_iter()
            .collect();
    }
    let absolute = pattern.starts_with('/');
    let components: Vec<&str> = pattern.split('/').filter(|component| !component.is_empty()).collect();
    let mut matches = Vec::new();
    let base = if absolute {
        std::path::PathBuf::from("/")
    } else {
        std::path::PathBuf::from(".")
    };
    let prefix = if absolute { "/" } else { "" };
    eval_glob_collect(&base, prefix, &components, &mut matches);
    matches.sort();
    matches
}

/// Recursively expands one glob path component at a time.
fn eval_glob_collect(
    base: &std::path::Path,
    prefix: &str,
    components: &[&str],
    matches: &mut Vec<String>,
) {
    let Some((component, rest)) = components.split_first() else {
        if base.exists() && !prefix.is_empty() {
            matches.push(prefix.to_string());
        }
        return;
    };
    if !eval_glob_component_has_magic(component) {
        let next_base = base.join(component);
        if rest.is_empty() {
            if next_base.exists() {
                matches.push(eval_glob_join_output(prefix, component));
            }
        } else if next_base.is_dir() {
            let next_prefix = eval_glob_join_output(prefix, component);
            eval_glob_collect(&next_base, &next_prefix, rest, matches);
        }
        return;
    }
    let Ok(entries) = std::fs::read_dir(base) else {
        return;
    };
    let mut names = Vec::new();
    for entry in entries.flatten() {
        names.push(entry.file_name().to_string_lossy().into_owned());
    }
    names.sort();
    for name in names {
        if !eval_fnmatch_bytes(component.as_bytes(), name.as_bytes(), EVAL_FNM_PERIOD) {
            continue;
        }
        let next_base = base.join(&name);
        if rest.is_empty() {
            matches.push(eval_glob_join_output(prefix, &name));
        } else if next_base.is_dir() {
            let next_prefix = eval_glob_join_output(prefix, &name);
            eval_glob_collect(&next_base, &next_prefix, rest, matches);
        }
    }
}

/// Joins a display path prefix and component while preserving absolute-root output.
fn eval_glob_join_output(prefix: &str, component: &str) -> String {
    if prefix.is_empty() {
        component.to_string()
    } else if prefix == "/" {
        format!("/{component}")
    } else {
        format!("{prefix}/{component}")
    }
}

/// Returns whether a glob component contains wildcard syntax.
fn eval_glob_component_has_magic(component: &str) -> bool {
    component
        .as_bytes()
        .iter()
        .any(|byte| matches!(byte, b'*' | b'?' | b'['))
}

/// Writes one byte-string value into an indexed runtime array at a zero-based position.
fn eval_array_set_indexed_bytes(
    array: RuntimeCellHandle,
    index: usize,
    value: &[u8],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let key = values.int(i64::try_from(index).map_err(|_| EvalStatus::RuntimeFatal)?)?;
    let value = values.string_bytes_value(value)?;
    values.array_set(array, key, value)
}

/// Evaluates PHP `tempnam($directory, $prefix)` over eval expressions.
fn eval_builtin_tempnam(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [directory, prefix] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let directory = eval_expr(directory, context, scope, values)?;
    let prefix = eval_expr(prefix, context, scope, values)?;
    eval_tempnam_result(directory, prefix, values)
}

/// Creates a unique local temporary file and returns its path, or an empty string on failure.
fn eval_tempnam_result(
    directory: RuntimeCellHandle,
    prefix: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let directory = eval_path_string(directory, values)?;
    let prefix = values.string_bytes(prefix)?;
    let prefix = String::from_utf8_lossy(&prefix);
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    for attempt in 0..1000_u32 {
        let candidate =
            std::path::Path::new(&directory).join(eval_tempnam_filename(&prefix, nonce, attempt));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(_) => return values.string(candidate.to_string_lossy().as_ref()),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return values.string(""),
        }
    }
    values.string("")
}

/// Builds one deterministic tempnam candidate basename from prefix, process, and attempt data.
fn eval_tempnam_filename(prefix: &str, nonce: u128, attempt: u32) -> String {
    format!("{}{}_{:x}_{attempt}", prefix, std::process::id(), nonce)
}

/// Evaluates PHP `touch($filename, $mtime = null, $atime = null)` over eval expressions.
fn eval_builtin_touch(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [filename] => {
            let filename = eval_expr(filename, context, scope, values)?;
            eval_touch_result(filename, None, None, values)
        }
        [filename, mtime] => {
            let filename = eval_expr(filename, context, scope, values)?;
            let mtime = eval_expr(mtime, context, scope, values)?;
            eval_touch_result(filename, Some(mtime), None, values)
        }
        [filename, mtime, atime] => {
            let filename = eval_expr(filename, context, scope, values)?;
            let mtime = eval_expr(mtime, context, scope, values)?;
            let atime = eval_expr(atime, context, scope, values)?;
            eval_touch_result(filename, Some(mtime), Some(atime), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Creates or stamps one local file and returns whether the operation succeeded.
fn eval_touch_result(
    filename: RuntimeCellHandle,
    mtime: Option<RuntimeCellHandle>,
    atime: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    let (mtime, atime) = eval_touch_times(mtime, atime, values)?;
    let file = match std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .open(path)
    {
        Ok(file) => file,
        Err(_) => return values.bool_value(false),
    };
    let times = std::fs::FileTimes::new()
        .set_modified(mtime)
        .set_accessed(atime);
    values.bool_value(file.set_times(times).is_ok())
}

/// Resolves PHP touch timestamp defaults into concrete system times.
fn eval_touch_times(
    mtime: Option<RuntimeCellHandle>,
    atime: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<(std::time::SystemTime, std::time::SystemTime), EvalStatus> {
    let now = std::time::SystemTime::now();
    let Some(mtime) = mtime else {
        return Ok((now, now));
    };
    if values.is_null(mtime)? {
        if let Some(atime) = atime {
            if !values.is_null(atime)? {
                return Err(EvalStatus::RuntimeFatal);
            }
        }
        return Ok((now, now));
    }
    let mtime = eval_system_time_from_unix(eval_int_value(mtime, values)?)
        .ok_or(EvalStatus::RuntimeFatal)?;
    let Some(atime) = atime else {
        return Ok((mtime, mtime));
    };
    if values.is_null(atime)? {
        return Ok((mtime, mtime));
    }
    let atime = eval_system_time_from_unix(eval_int_value(atime, values)?)
        .ok_or(EvalStatus::RuntimeFatal)?;
    Ok((mtime, atime))
}

/// Converts a Unix timestamp in seconds into a `SystemTime`.
fn eval_system_time_from_unix(seconds: i64) -> Option<std::time::SystemTime> {
    if seconds >= 0 {
        std::time::UNIX_EPOCH.checked_add(std::time::Duration::from_secs(seconds as u64))
    } else {
        std::time::UNIX_EPOCH.checked_sub(std::time::Duration::from_secs(seconds.unsigned_abs()))
    }
}

/// Evaluates PHP `umask($mask = null)` over an optional eval expression.
fn eval_builtin_umask(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [] => eval_umask_result(None, values),
        [mask] => {
            let mask = eval_expr(mask, context, scope, values)?;
            eval_umask_result(Some(mask), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Applies PHP `umask()` semantics and returns the previous mask.
fn eval_umask_result(
    mask: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let previous = match mask {
        Some(mask) => {
            let mask = eval_int_value(mask, values)? as u32;
            unsafe { umask(mask) }
        }
        None => unsafe {
            let current = umask(0);
            umask(current);
            current
        },
    };
    values.int(i64::from(previous))
}

/// Evaluates PHP `readlink($path)` over one eval expression.
fn eval_builtin_readlink(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [path] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let path = eval_expr(path, context, scope, values)?;
    eval_readlink_result(path, values)
}

/// Reads one symbolic-link target string, or returns PHP false on failure.
fn eval_readlink_result(
    path: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(path, values)?;
    match std::fs::read_link(path) {
        Ok(target) => values.string(target.to_string_lossy().as_ref()),
        Err(_) => values.bool_value(false),
    }
}

/// Evaluates PHP `linkinfo($path)` over one eval expression.
fn eval_builtin_linkinfo(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [path] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let path = eval_expr(path, context, scope, values)?;
    eval_linkinfo_result(path, values)
}

/// Returns one symlink metadata device id, or PHP's `-1` failure sentinel.
fn eval_linkinfo_result(
    path: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(path, values)?;
    let dev = match std::fs::symlink_metadata(path) {
        Ok(metadata) => i64::try_from(metadata.dev()).map_err(|_| EvalStatus::RuntimeFatal)?,
        Err(_) => -1,
    };
    values.int(dev)
}

/// Evaluates `clearstatcache(...)` as an ordered no-op in eval.
fn eval_builtin_clearstatcache(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.len() > 2 {
        return Err(EvalStatus::RuntimeFatal);
    }
    for arg in args {
        eval_expr(arg, context, scope, values)?;
    }
    values.null()
}

/// Evaluates PHP `unlink($filename)` over one eval expression.
fn eval_builtin_unlink(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filename] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let filename = eval_expr(filename, context, scope, values)?;
    eval_unlink_result(filename, values)
}

/// Deletes one local file and returns whether it succeeded.
fn eval_unlink_result(
    filename: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    values.bool_value(std::fs::remove_file(path).is_ok())
}

/// Converts one eval value to a filesystem path string.
fn eval_path_string(
    filename: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let filename = values.string_bytes(filename)?;
    Ok(String::from_utf8_lossy(&filename).into_owned())
}

/// Returns whether a path can be opened for reading by the current process.
fn eval_path_is_readable(path: &std::path::Path) -> bool {
    std::fs::File::open(path).is_ok() || std::fs::read_dir(path).is_ok()
}

/// Returns whether a path has any executable bit set in its Unix mode.
fn eval_path_is_executable(path: &std::path::Path) -> bool {
    std::fs::metadata(path)
        .map(|metadata| metadata.mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// Returns whether a path can be written by the current process.
fn eval_path_is_writable(path: &std::path::Path) -> bool {
    if path.is_file() {
        return std::fs::OpenOptions::new().write(true).open(path).is_ok();
    }
    if !path.is_dir() {
        return false;
    }
    let probe = path.join(format!(".elephc_eval_writable_probe_{}", std::process::id()));
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
    {
        Ok(_) => {
            let _ = std::fs::remove_file(probe);
            true
        }
        Err(_) => false,
    }
}

/// Evaluates PHP `basename($path, $suffix = "")` over one eval expression.
fn eval_builtin_basename(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [path] => {
            let path = eval_expr(path, context, scope, values)?;
            eval_basename_result(path, None, values)
        }
        [path, suffix] => {
            let path = eval_expr(path, context, scope, values)?;
            let suffix = eval_expr(suffix, context, scope, values)?;
            eval_basename_result(path, Some(suffix), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Computes PHP `basename()` bytes and returns them as a runtime string.
fn eval_basename_result(
    path: RuntimeCellHandle,
    suffix: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = values.string_bytes(path)?;
    let suffix = suffix
        .map(|suffix| values.string_bytes(suffix))
        .transpose()?;
    let result = eval_basename_bytes(&path, suffix.as_deref());
    values.string_bytes_value(&result)
}

/// Extracts a PHP basename from one path byte string.
fn eval_basename_bytes(path: &[u8], suffix: Option<&[u8]>) -> Vec<u8> {
    let mut end = path.len();
    while end > 0 && path[end - 1] == b'/' {
        end -= 1;
    }
    if end == 0 {
        return Vec::new();
    }
    let mut start = end;
    while start > 0 && path[start - 1] != b'/' {
        start -= 1;
    }
    let mut result = path[start..end].to_vec();
    if let Some(suffix) = suffix {
        if !suffix.is_empty() && suffix.len() < result.len() && result.ends_with(suffix) {
            result.truncate(result.len() - suffix.len());
        }
    }
    result
}

/// Evaluates PHP `dirname($path, $levels = 1)` over one eval expression.
fn eval_builtin_dirname(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [path] => {
            let path = eval_expr(path, context, scope, values)?;
            eval_dirname_result(path, None, values)
        }
        [path, levels] => {
            let path = eval_expr(path, context, scope, values)?;
            let levels = eval_expr(levels, context, scope, values)?;
            eval_dirname_result(path, Some(levels), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Computes PHP `dirname()` bytes and returns them as a runtime string.
fn eval_dirname_result(
    path: RuntimeCellHandle,
    levels: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = values.string_bytes(path)?;
    let levels = match levels {
        Some(levels) => eval_int_value(levels, values)?,
        None => 1,
    };
    if levels < 1 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let mut current = path;
    for _ in 0..levels {
        current = eval_dirname_once(&current);
    }
    values.string_bytes_value(&current)
}

/// Applies one PHP `dirname()` parent traversal to a path byte string.
fn eval_dirname_once(path: &[u8]) -> Vec<u8> {
    if path.is_empty() {
        return b".".to_vec();
    }
    let mut end = path.len();
    while end > 0 && path[end - 1] == b'/' {
        end -= 1;
    }
    if end == 0 {
        return b"/".to_vec();
    }
    let mut cursor = end;
    while cursor > 0 {
        cursor -= 1;
        if path[cursor] == b'/' {
            let mut parent_end = cursor;
            while parent_end > 0 && path[parent_end - 1] == b'/' {
                parent_end -= 1;
            }
            return if parent_end == 0 {
                b"/".to_vec()
            } else {
                path[..parent_end].to_vec()
            };
        }
    }
    b".".to_vec()
}

/// Evaluates PHP `realpath($path)` over one eval expression.
fn eval_builtin_realpath(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [path] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let path = eval_expr(path, context, scope, values)?;
    eval_realpath_result(path, values)
}

/// Canonicalizes one path or returns PHP false when the path cannot be resolved.
fn eval_realpath_result(
    path: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = values.string_bytes(path)?;
    let path = String::from_utf8_lossy(&path);
    let Ok(canonical) = std::fs::canonicalize(path.as_ref()) else {
        return values.bool_value(false);
    };
    let canonical = canonical.to_string_lossy();
    values.string(canonical.as_ref())
}

/// Evaluates PHP `pathinfo($path, $flags = PATHINFO_ALL)` over one eval expression.
fn eval_builtin_pathinfo(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [path] => {
            let path = eval_expr(path, context, scope, values)?;
            eval_pathinfo_result(path, None, values)
        }
        [path, flags] => {
            let path = eval_expr(path, context, scope, values)?;
            let flags = eval_expr(flags, context, scope, values)?;
            eval_pathinfo_result(path, Some(flags), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Computes PHP `pathinfo()` as either an associative array or one component string.
fn eval_pathinfo_result(
    path: RuntimeCellHandle,
    flags: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = values.string_bytes(path)?;
    let Some(flags) = flags else {
        return eval_pathinfo_array_result(&path, values);
    };
    let flags = eval_int_value(flags, values)?;
    if flags == EVAL_PATHINFO_ALL {
        return eval_pathinfo_array_result(&path, values);
    }
    let component = eval_pathinfo_component_bytes(&path, flags);
    values.string_bytes_value(&component)
}

/// Builds the PHP `pathinfo()` associative-array result for all components.
fn eval_pathinfo_array_result(
    path: &[u8],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.assoc_new(4)?;
    if !path.is_empty() {
        let dirname = eval_pathinfo_dirname_bytes(path);
        result = eval_pathinfo_array_set(result, "dirname", &dirname, values)?;
    }
    let parts = eval_pathinfo_parts(path);
    result = eval_pathinfo_array_set(result, "basename", &parts.basename, values)?;
    if parts.has_extension {
        result = eval_pathinfo_array_set(result, "extension", &parts.extension, values)?;
    }
    eval_pathinfo_array_set(result, "filename", &parts.filename, values)
}

/// Inserts one string component into a PHP `pathinfo()` associative result.
fn eval_pathinfo_array_set(
    array: RuntimeCellHandle,
    key: &str,
    value: &[u8],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let key = values.string(key)?;
    let value = values.string_bytes_value(value)?;
    values.array_set(array, key, value)
}

/// Returns one PHP `pathinfo()` component for a non-all bitmask.
fn eval_pathinfo_component_bytes(path: &[u8], flags: i64) -> Vec<u8> {
    if flags & EVAL_PATHINFO_DIRNAME != 0 {
        return eval_pathinfo_dirname_bytes(path);
    }
    let parts = eval_pathinfo_parts(path);
    if flags & EVAL_PATHINFO_BASENAME != 0 {
        return parts.basename;
    }
    if flags & EVAL_PATHINFO_EXTENSION != 0 {
        return parts.extension;
    }
    if flags & EVAL_PATHINFO_FILENAME != 0 {
        return parts.filename;
    }
    Vec::new()
}

/// Computes the dirname component with `pathinfo("")`'s empty-string exception.
fn eval_pathinfo_dirname_bytes(path: &[u8]) -> Vec<u8> {
    if path.is_empty() {
        Vec::new()
    } else {
        eval_dirname_once(path)
    }
}

/// Splits pathinfo basename, extension, and filename components.
fn eval_pathinfo_parts(path: &[u8]) -> EvalPathInfoParts {
    let basename = eval_basename_bytes(path, None);
    let Some(dot) = basename.iter().rposition(|byte| *byte == b'.') else {
        return EvalPathInfoParts {
            filename: basename.clone(),
            basename,
            extension: Vec::new(),
            has_extension: false,
        };
    };
    EvalPathInfoParts {
        filename: basename[..dot].to_vec(),
        extension: basename[dot + 1..].to_vec(),
        basename,
        has_extension: true,
    }
}

/// Pathinfo components derived from a basename.
struct EvalPathInfoParts {
    /// Full basename component.
    basename: Vec<u8>,
    /// Extension component after the final dot, possibly empty for trailing-dot names.
    extension: Vec<u8>,
    /// Filename component before the final dot.
    filename: Vec<u8>,
    /// Whether the basename contained a dot and therefore has an extension key.
    has_extension: bool,
}

/// Evaluates PHP `fnmatch($pattern, $filename, $flags = 0)` over eval expressions.
fn eval_builtin_fnmatch(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [pattern, filename] => {
            let pattern = eval_expr(pattern, context, scope, values)?;
            let filename = eval_expr(filename, context, scope, values)?;
            eval_fnmatch_result(pattern, filename, None, values)
        }
        [pattern, filename, flags] => {
            let pattern = eval_expr(pattern, context, scope, values)?;
            let filename = eval_expr(filename, context, scope, values)?;
            let flags = eval_expr(flags, context, scope, values)?;
            eval_fnmatch_result(pattern, filename, Some(flags), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Runs PHP-style shell glob matching for one pattern/name pair.
fn eval_fnmatch_result(
    pattern: RuntimeCellHandle,
    filename: RuntimeCellHandle,
    flags: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let pattern = values.string_bytes(pattern)?;
    let filename = values.string_bytes(filename)?;
    let flags = match flags {
        Some(flags) => eval_int_value(flags, values)?,
        None => 0,
    };
    values.bool_value(eval_fnmatch_bytes(&pattern, &filename, flags))
}

/// Matches byte strings using the eval-supported `fnmatch()` grammar and flags.
fn eval_fnmatch_bytes(pattern: &[u8], filename: &[u8], flags: i64) -> bool {
    let mut memo = vec![vec![None; filename.len() + 1]; pattern.len() + 1];
    eval_fnmatch_at(pattern, filename, flags, 0, 0, &mut memo)
}

/// Recursively matches a pattern suffix against a filename suffix with memoization.
fn eval_fnmatch_at(
    pattern: &[u8],
    filename: &[u8],
    flags: i64,
    pattern_index: usize,
    filename_index: usize,
    memo: &mut [Vec<Option<bool>>],
) -> bool {
    if let Some(result) = memo[pattern_index][filename_index] {
        return result;
    }
    let result = if pattern_index == pattern.len() {
        filename_index == filename.len()
    } else {
        match pattern[pattern_index] {
            b'*' => eval_fnmatch_star(pattern, filename, flags, pattern_index, filename_index, memo),
            b'?' => {
                eval_fnmatch_single_wildcard(filename, flags, filename_index)
                    && eval_fnmatch_at(
                        pattern,
                        filename,
                        flags,
                        pattern_index + 1,
                        filename_index + 1,
                        memo,
                    )
            }
            b'[' => eval_fnmatch_class_or_literal(
                pattern,
                filename,
                flags,
                pattern_index,
                filename_index,
                memo,
            ),
            b'\\' if flags & EVAL_FNM_NOESCAPE == 0 => {
                let (literal, next_pattern_index) =
                    eval_fnmatch_escaped_literal(pattern, pattern_index);
                eval_fnmatch_literal(filename, flags, filename_index, literal)
                    && eval_fnmatch_at(
                        pattern,
                        filename,
                        flags,
                        next_pattern_index,
                        filename_index + 1,
                        memo,
                    )
            }
            literal => {
                eval_fnmatch_literal(filename, flags, filename_index, literal)
                    && eval_fnmatch_at(
                        pattern,
                        filename,
                        flags,
                        pattern_index + 1,
                        filename_index + 1,
                        memo,
                    )
            }
        }
    };
    memo[pattern_index][filename_index] = Some(result);
    result
}

/// Handles `*`, including pathname and leading-period restrictions.
fn eval_fnmatch_star(
    pattern: &[u8],
    filename: &[u8],
    flags: i64,
    pattern_index: usize,
    filename_index: usize,
    memo: &mut [Vec<Option<bool>>],
) -> bool {
    let mut next_pattern_index = pattern_index + 1;
    while next_pattern_index < pattern.len() && pattern[next_pattern_index] == b'*' {
        next_pattern_index += 1;
    }
    if eval_fnmatch_at(
        pattern,
        filename,
        flags,
        next_pattern_index,
        filename_index,
        memo,
    ) {
        return true;
    }
    let mut cursor = filename_index;
    while cursor < filename.len() && eval_fnmatch_wildcard_can_consume(filename, flags, cursor) {
        cursor += 1;
        if eval_fnmatch_at(pattern, filename, flags, next_pattern_index, cursor, memo) {
            return true;
        }
    }
    false
}

/// Returns whether `?` can consume the current filename byte.
fn eval_fnmatch_single_wildcard(filename: &[u8], flags: i64, filename_index: usize) -> bool {
    filename_index < filename.len()
        && eval_fnmatch_wildcard_can_consume(filename, flags, filename_index)
}

/// Handles a bracket class, or falls back to a literal `[` when the class is malformed.
fn eval_fnmatch_class_or_literal(
    pattern: &[u8],
    filename: &[u8],
    flags: i64,
    pattern_index: usize,
    filename_index: usize,
    memo: &mut [Vec<Option<bool>>],
) -> bool {
    if filename_index >= filename.len()
        || !eval_fnmatch_wildcard_can_consume(filename, flags, filename_index)
    {
        return false;
    }
    let Some((matches, next_pattern_index)) =
        eval_fnmatch_class_matches(pattern, pattern_index + 1, filename[filename_index], flags)
    else {
        return eval_fnmatch_literal(filename, flags, filename_index, b'[')
            && eval_fnmatch_at(
                pattern,
                filename,
                flags,
                pattern_index + 1,
                filename_index + 1,
                memo,
            );
    };
    matches
        && eval_fnmatch_at(
            pattern,
            filename,
            flags,
            next_pattern_index,
            filename_index + 1,
            memo,
        )
}

/// Matches one bracket class body against the current filename byte.
fn eval_fnmatch_class_matches(
    pattern: &[u8],
    mut index: usize,
    candidate: u8,
    flags: i64,
) -> Option<(bool, usize)> {
    let negated = matches!(pattern.get(index).copied(), Some(b'!' | b'^'));
    if negated {
        index += 1;
    }
    let mut matched = false;
    let mut closed = false;
    while index < pattern.len() {
        if pattern[index] == b']' {
            closed = true;
            index += 1;
            break;
        }
        let start = eval_fnmatch_class_char(pattern, &mut index, flags)?;
        if index + 1 < pattern.len() && pattern[index] == b'-' && pattern[index + 1] != b']' {
            index += 1;
            let end = eval_fnmatch_class_char(pattern, &mut index, flags)?;
            if eval_fnmatch_byte_in_range(candidate, start, end, flags) {
                matched = true;
            }
        } else if eval_fnmatch_byte_eq(candidate, start, flags) {
            matched = true;
        }
    }
    closed.then_some((if negated { !matched } else { matched }, index))
}

/// Reads one character from a bracket class, respecting escapes when enabled.
fn eval_fnmatch_class_char(pattern: &[u8], index: &mut usize, flags: i64) -> Option<u8> {
    if *index >= pattern.len() {
        return None;
    }
    if pattern[*index] == b'\\' && flags & EVAL_FNM_NOESCAPE == 0 && *index + 1 < pattern.len() {
        *index += 2;
        return Some(pattern[*index - 1]);
    }
    let byte = pattern[*index];
    *index += 1;
    Some(byte)
}

/// Returns whether one candidate byte falls within a possibly case-folded range.
fn eval_fnmatch_byte_in_range(candidate: u8, start: u8, end: u8, flags: i64) -> bool {
    let candidate = eval_fnmatch_fold(candidate, flags);
    let start = eval_fnmatch_fold(start, flags);
    let end = eval_fnmatch_fold(end, flags);
    if start <= end {
        candidate >= start && candidate <= end
    } else {
        candidate >= end && candidate <= start
    }
}

/// Reads an escaped literal token outside bracket classes.
fn eval_fnmatch_escaped_literal(pattern: &[u8], pattern_index: usize) -> (u8, usize) {
    if pattern_index + 1 < pattern.len() {
        (pattern[pattern_index + 1], pattern_index + 2)
    } else {
        (b'\\', pattern_index + 1)
    }
}

/// Returns whether one literal pattern byte matches the current filename byte.
fn eval_fnmatch_literal(filename: &[u8], flags: i64, filename_index: usize, literal: u8) -> bool {
    filename_index < filename.len()
        && eval_fnmatch_byte_eq(filename[filename_index], literal, flags)
}

/// Returns whether a wildcard token may consume the current filename byte.
fn eval_fnmatch_wildcard_can_consume(filename: &[u8], flags: i64, filename_index: usize) -> bool {
    if filename_index >= filename.len() {
        return false;
    }
    if flags & EVAL_FNM_PATHNAME != 0 && filename[filename_index] == b'/' {
        return false;
    }
    if flags & EVAL_FNM_PERIOD != 0 && eval_fnmatch_is_leading_period(filename, flags, filename_index) {
        return false;
    }
    true
}

/// Returns whether the current byte is a leading period for `FNM_PERIOD`.
fn eval_fnmatch_is_leading_period(filename: &[u8], flags: i64, filename_index: usize) -> bool {
    filename[filename_index] == b'.'
        && (filename_index == 0
            || (flags & EVAL_FNM_PATHNAME != 0 && filename[filename_index - 1] == b'/'))
}

/// Compares bytes using ASCII case folding when `FNM_CASEFOLD` is present.
fn eval_fnmatch_byte_eq(left: u8, right: u8, flags: i64) -> bool {
    eval_fnmatch_fold(left, flags) == eval_fnmatch_fold(right, flags)
}

/// Applies eval fnmatch's ASCII case folding.
fn eval_fnmatch_fold(byte: u8, flags: i64) -> u8 {
    if flags & EVAL_FNM_CASEFOLD != 0 {
        byte.to_ascii_lowercase()
    } else {
        byte
    }
}

/// Evaluates PHP `gethostbyaddr($ip)` over one eval expression.
fn eval_builtin_gethostbyaddr(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [ip] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let ip = eval_expr(ip, context, scope, values)?;
    eval_gethostbyaddr_result(ip, values)
}

/// Reverse-resolves one IPv4 address, returns the input on miss, or PHP false when malformed.
fn eval_gethostbyaddr_result(
    ip: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let ip_bytes = values.string_bytes(ip)?;
    let ip_text = String::from_utf8_lossy(&ip_bytes);
    let Ok(ipv4) = ip_text.parse::<std::net::Ipv4Addr>() else {
        return values.bool_value(false);
    };
    let octets = ipv4.octets();
    let resolved = unsafe {
        // libc reads the stack-owned IPv4 octets during this call and returns
        // static resolver storage, which is copied before the next resolver call.
        let host = libc_gethostbyaddr(
            octets.as_ptr().cast::<libc::c_void>(),
            octets.len() as libc::socklen_t,
            libc::AF_INET,
        );
        if host.is_null() || (*host).h_name.is_null() {
            None
        } else {
            Some(CStr::from_ptr((*host).h_name).to_bytes().to_vec())
        }
    };
    match resolved {
        Some(name) if !name.is_empty() => values.string_bytes_value(&name),
        _ => values.string(ip_text.as_ref()),
    }
}

/// Evaluates PHP `gethostbyname($hostname)` over one eval expression.
fn eval_builtin_gethostbyname(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [hostname] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let hostname = eval_expr(hostname, context, scope, values)?;
    eval_gethostbyname_result(hostname, values)
}

/// Resolves one host name to an IPv4 string, or returns the original input on failure.
fn eval_gethostbyname_result(
    hostname: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let hostname = values.string_bytes(hostname)?;
    let hostname = String::from_utf8_lossy(&hostname);
    if hostname.parse::<std::net::Ipv4Addr>().is_ok() {
        return values.string(hostname.as_ref());
    }
    let resolved = (hostname.as_ref(), 0_u16)
        .to_socket_addrs()
        .ok()
        .and_then(|addrs| {
            addrs
                .filter_map(|addr| match addr.ip() {
                    std::net::IpAddr::V4(ip) => Some(ip.to_string()),
                    std::net::IpAddr::V6(_) => None,
                })
                .next()
    });
    values.string(resolved.as_deref().unwrap_or_else(|| hostname.as_ref()))
}

/// Evaluates PHP `gethostname()` over one eval expression.
fn eval_builtin_gethostname(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_gethostname_result(values)
}

/// Reads the current host name through libc and returns an empty string on failure.
fn eval_gethostname_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut buffer = [0 as libc::c_char; 256];
    let status = unsafe {
        // libc writes at most buffer.len() bytes into this stack buffer.
        libc::gethostname(buffer.as_mut_ptr(), buffer.len())
    };
    if status != 0 {
        return values.string("");
    }
    let length = buffer
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(buffer.len());
    let hostname = buffer[..length]
        .iter()
        .map(|byte| *byte as u8)
        .collect::<Vec<_>>();
    values.string_bytes_value(&hostname)
}

/// Evaluates PHP `getprotobyname($protocol)` over one eval expression.
fn eval_builtin_getprotobyname(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [protocol] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let protocol = eval_expr(protocol, context, scope, values)?;
    eval_getprotobyname_result(protocol, values)
}

/// Looks up an IP protocol number by name or alias.
fn eval_getprotobyname_result(
    protocol: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(protocol) = eval_lowercase_c_string(protocol, values)? else {
        return values.bool_value(false);
    };
    let entry = unsafe {
        // libc returns a process-global protoent; copy scalar fields before another lookup.
        libc_getprotobyname(protocol.as_ptr())
    };
    if entry.is_null() {
        return values.bool_value(false);
    }
    let number = unsafe { (*entry).p_proto };
    values.int(i64::from(number))
}

/// Evaluates PHP `getprotobynumber($protocol)` over one eval expression.
fn eval_builtin_getprotobynumber(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [protocol] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let protocol = eval_expr(protocol, context, scope, values)?;
    eval_getprotobynumber_result(protocol, values)
}

/// Looks up an IP protocol name by numeric protocol id.
fn eval_getprotobynumber_result(
    protocol: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let protocol = eval_int_value(protocol, values)?;
    let Ok(protocol) = libc::c_int::try_from(protocol) else {
        return values.bool_value(false);
    };
    let entry = unsafe {
        // libc returns a process-global protoent; copy the name before another lookup.
        libc_getprotobynumber(protocol)
    };
    eval_protoent_name_or_false(entry, values)
}

/// Evaluates PHP `getservbyname($service, $protocol)` over two eval expressions.
fn eval_builtin_getservbyname(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [service, protocol] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let service = eval_expr(service, context, scope, values)?;
    let protocol = eval_expr(protocol, context, scope, values)?;
    eval_getservbyname_result(service, protocol, values)
}

/// Looks up an internet service port by service name and protocol.
fn eval_getservbyname_result(
    service: RuntimeCellHandle,
    protocol: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(service) = eval_lowercase_c_string(service, values)? else {
        return values.bool_value(false);
    };
    let Some(protocol) = eval_lowercase_c_string(protocol, values)? else {
        return values.bool_value(false);
    };
    let entry = unsafe {
        // libc returns a process-global servent; copy scalar fields before another lookup.
        libc_getservbyname(service.as_ptr(), protocol.as_ptr())
    };
    if entry.is_null() {
        return values.bool_value(false);
    }
    let port = unsafe { u16::from_be((*entry).s_port as u16) };
    values.int(i64::from(port))
}

/// Evaluates PHP `getservbyport($port, $protocol)` over two eval expressions.
fn eval_builtin_getservbyport(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [port, protocol] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let port = eval_expr(port, context, scope, values)?;
    let protocol = eval_expr(protocol, context, scope, values)?;
    eval_getservbyport_result(port, protocol, values)
}

/// Looks up an internet service name by port and protocol.
fn eval_getservbyport_result(
    port: RuntimeCellHandle,
    protocol: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let port = eval_int_value(port, values)?;
    let Ok(port) = u16::try_from(port) else {
        return values.bool_value(false);
    };
    let Some(protocol) = eval_lowercase_c_string(protocol, values)? else {
        return values.bool_value(false);
    };
    let network_port = port.to_be() as libc::c_int;
    let entry = unsafe {
        // libc returns a process-global servent; copy the name before another lookup.
        libc_getservbyport(network_port, protocol.as_ptr())
    };
    eval_servent_name_or_false(entry, values)
}

/// Converts a PHP value to a NUL-free lowercase C string for libc database lookups.
fn eval_lowercase_c_string(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<CString>, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let bytes = bytes
        .into_iter()
        .map(|byte| byte.to_ascii_lowercase())
        .collect::<Vec<_>>();
    Ok(CString::new(bytes).ok())
}

/// Copies a protoent canonical name into a PHP string or returns PHP false.
fn eval_protoent_name_or_false(
    entry: *mut libc::protoent,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if entry.is_null() {
        return values.bool_value(false);
    }
    let name = unsafe {
        let name = (*entry).p_name;
        if name.is_null() {
            return values.bool_value(false);
        }
        CStr::from_ptr(name).to_bytes().to_vec()
    };
    values.string_bytes_value(&name)
}

/// Copies a servent canonical name into a PHP string or returns PHP false.
fn eval_servent_name_or_false(
    entry: *mut libc::servent,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if entry.is_null() {
        return values.bool_value(false);
    }
    let name = unsafe {
        let name = (*entry).s_name;
        if name.is_null() {
            return values.bool_value(false);
        }
        CStr::from_ptr(name).to_bytes().to_vec()
    };
    values.string_bytes_value(&name)
}

/// Evaluates PHP `long2ip($ip)` over one eval expression.
fn eval_builtin_long2ip(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [ip] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let ip = eval_expr(ip, context, scope, values)?;
    eval_long2ip_result(ip, values)
}

/// Formats one 32-bit IPv4 integer as a dotted-quad string.
fn eval_long2ip_result(
    ip: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let ip = eval_int_value(ip, values)? as u32;
    values.string(&eval_format_ipv4(ip))
}

/// Evaluates PHP `ip2long($ip)` over one eval expression.
fn eval_builtin_ip2long(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [ip] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let ip = eval_expr(ip, context, scope, values)?;
    eval_ip2long_result(ip, values)
}

/// Parses a dotted-quad IPv4 string into an integer or PHP false.
fn eval_ip2long_result(
    ip: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(ip)?;
    match eval_parse_ipv4(&bytes) {
        Some(ip) => values.int(i64::from(ip)),
        None => values.bool_value(false),
    }
}

/// Evaluates PHP `inet_pton($ip)` over one eval expression.
fn eval_builtin_inet_pton(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [ip] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let ip = eval_expr(ip, context, scope, values)?;
    eval_inet_pton_result(ip, values)
}

/// Packs a dotted-quad IPv4 string into four network-order bytes or PHP false.
fn eval_inet_pton_result(
    ip: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(ip)?;
    let Some(ip) = eval_parse_ipv4(&bytes) else {
        return values.bool_value(false);
    };
    values.string_bytes_value(&ip.to_be_bytes())
}

/// Evaluates PHP `inet_ntop($binary)` over one eval expression.
fn eval_builtin_inet_ntop(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [binary] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let binary = eval_expr(binary, context, scope, values)?;
    eval_inet_ntop_result(binary, values)
}

/// Renders a four-byte IPv4 string as dotted-quad text or PHP false.
fn eval_inet_ntop_result(
    binary: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(binary)?;
    let [a, b, c, d] = bytes.as_slice() else {
        return values.bool_value(false);
    };
    let ip = u32::from_be_bytes([*a, *b, *c, *d]);
    values.string(&eval_format_ipv4(ip))
}

/// Parses exactly four decimal IPv4 octets separated by dots.
fn eval_parse_ipv4(bytes: &[u8]) -> Option<u32> {
    let mut octets = [0_u8; 4];
    let mut position = 0_usize;
    let mut index = 0_usize;

    while index < 4 {
        if position >= bytes.len() {
            return None;
        }
        let start = position;
        let mut value = 0_u16;
        while position < bytes.len() && bytes[position].is_ascii_digit() {
            value = value
                .checked_mul(10)?
                .checked_add(u16::from(bytes[position] - b'0'))?;
            position += 1;
            if position - start > 3 || value > 255 {
                return None;
            }
        }
        if position == start {
            return None;
        }
        octets[index] = value as u8;
        index += 1;
        if index == 4 {
            return (position == bytes.len()).then(|| u32::from_be_bytes(octets));
        }
        if bytes.get(position).copied() != Some(b'.') {
            return None;
        }
        position += 1;
    }
    None
}

/// Formats one packed IPv4 integer into dotted-quad text.
fn eval_format_ipv4(ip: u32) -> String {
    let [a, b, c, d] = ip.to_be_bytes();
    format!("{}.{}.{}.{}", a, b, c, d)
}

/// Evaluates PHP `getenv($name)` over one eval expression.
fn eval_builtin_getenv(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [name] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let name = eval_expr(name, context, scope, values)?;
    eval_getenv_result(name, values)
}

/// Reads one environment variable and returns an empty string when it is unset.
fn eval_getenv_result(
    name: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let name = values.string_bytes(name)?;
    let name = String::from_utf8_lossy(&name);
    let value = std::env::var_os(name.as_ref())
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_default();
    values.string(&value)
}

/// Evaluates PHP `putenv($assignment)` over one eval expression.
fn eval_builtin_putenv(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [assignment] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let assignment = eval_expr(assignment, context, scope, values)?;
    eval_putenv_result(assignment, values)
}

/// Applies one `putenv()` assignment to the host environment.
fn eval_putenv_result(
    assignment: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let assignment = values.string_bytes(assignment)?;
    if let Some(separator) = assignment.iter().position(|byte| *byte == b'=') {
        let name = String::from_utf8_lossy(&assignment[..separator]);
        let value = String::from_utf8_lossy(&assignment[separator + 1..]);
        std::env::set_var(name.as_ref(), value.as_ref());
    } else {
        let name = String::from_utf8_lossy(&assignment);
        std::env::remove_var(name.as_ref());
    }
    values.bool_value(true)
}

/// Evaluates PHP `sys_get_temp_dir()` with no arguments.
fn eval_builtin_sys_get_temp_dir(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_sys_get_temp_dir_result(values)
}

/// Returns the same temporary directory literal as the native static builtin.
fn eval_sys_get_temp_dir_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.string("/tmp")
}

/// Evaluates PHP `realpath_cache_get()` with no arguments.
fn eval_builtin_realpath_cache_get(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_realpath_cache_get_result(values)
}

/// Returns elephc's intentionally empty realpath-cache view.
fn eval_realpath_cache_get_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.array_new(0)
}

/// Evaluates PHP `realpath_cache_size()` with no arguments.
fn eval_builtin_realpath_cache_size(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_realpath_cache_size_result(values)
}

/// Returns zero because elephc does not maintain a runtime realpath cache.
fn eval_realpath_cache_size_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.int(0)
}

/// Returns the standard zlib/PHP CRC-32 checksum for a byte slice.
fn eval_crc32_bytes(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff_u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0_u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

/// Casts one eval value to PHP int and returns the scalar payload.
fn eval_int_value(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    let value = values.cast_int(value)?;
    let bytes = values.string_bytes(value)?;
    std::str::from_utf8(&bytes)
        .map_err(|_| EvalStatus::RuntimeFatal)?
        .parse::<i64>()
        .map_err(|_| EvalStatus::RuntimeFatal)
}

/// Evaluates PHP's `bin2hex(...)` over one eval expression.
fn eval_builtin_bin2hex(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_bin2hex_result(value, values)
}

/// Converts one eval value through PHP string conversion and returns lowercase hex bytes.
fn eval_bin2hex_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    values.string(&eval_lower_hex_bytes(&bytes))
}

/// Converts bytes to lowercase hexadecimal text.
fn eval_lower_hex_bytes(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

/// Evaluates PHP's `hex2bin(...)` over one eval expression.
fn eval_builtin_hex2bin(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_hex2bin_result(value, values)
}

/// Converts one eval value through PHP string conversion and decodes hexadecimal bytes.
fn eval_hex2bin_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    if bytes.len() % 2 != 0 {
        values.warning(HEX2BIN_ODD_LENGTH_WARNING)?;
        return values.bool_value(false);
    }
    let mut output = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.chunks_exact(2) {
        let Some(high) = eval_hex_nibble(pair[0]) else {
            values.warning(HEX2BIN_INVALID_WARNING)?;
            return values.bool_value(false);
        };
        let Some(low) = eval_hex_nibble(pair[1]) else {
            values.warning(HEX2BIN_INVALID_WARNING)?;
            return values.bool_value(false);
        };
        output.push((high << 4) | low);
    }
    values.string_bytes_value(&output)
}

/// Returns the four-bit value for one hexadecimal byte.
fn eval_hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Evaluates PHP's `addslashes(...)` or `stripslashes(...)` over one eval expression.
fn eval_builtin_slashes(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_slashes_result(name, value, values)
}

/// Applies PHP byte-string escaping or unescaping for addslashes/stripslashes.
fn eval_slashes_result(
    name: &str,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "addslashes" => eval_addslashes_result(value, values),
        "stripslashes" => eval_stripslashes_result(value, values),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Escapes NUL, quotes, and backslashes using PHP `addslashes()` byte semantics.
fn eval_addslashes_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let mut output = Vec::with_capacity(bytes.len());
    for byte in bytes {
        match byte {
            0 => output.extend_from_slice(b"\\0"),
            b'\'' | b'"' | b'\\' => {
                output.push(b'\\');
                output.push(byte);
            }
            _ => output.push(byte),
        }
    }
    values.string_bytes_value(&output)
}

/// Removes backslash quoting using PHP `stripslashes()` byte semantics.
fn eval_stripslashes_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\\' {
            index += 1;
            if let Some(byte) = bytes.get(index).copied() {
                output.push(if byte == b'0' { 0 } else { byte });
                index += 1;
            }
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    values.string_bytes_value(&output)
}

/// Evaluates PHP's `base64_encode(...)` over one eval expression.
fn eval_builtin_base64_encode(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_base64_encode_result(value, values)
}

/// Converts one eval value through PHP string conversion and returns Base64 text.
fn eval_base64_encode_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let mut output = String::with_capacity(((bytes.len() + 2) / 3) * 4);
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        output.push(ALPHABET[(first >> 2) as usize] as char);
        output.push(ALPHABET[(((first & 0x03) << 4) | (second >> 4)) as usize] as char);
        if chunk.len() > 1 {
            output.push(ALPHABET[(((second & 0x0f) << 2) | (third >> 6)) as usize] as char);
        } else {
            output.push('=');
        }
        if chunk.len() > 2 {
            output.push(ALPHABET[(third & 0x3f) as usize] as char);
        } else {
            output.push('=');
        }
    }
    values.string(&output)
}

/// Evaluates PHP's one-argument `base64_decode(...)` over one eval expression.
fn eval_builtin_base64_decode(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_base64_decode_result(value, values)
}

/// Converts one eval value through PHP string conversion and decodes Base64 bytes.
fn eval_base64_decode_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let input = values.string_bytes(value)?;
    let mut output = Vec::with_capacity((input.len() / 4) * 3);
    let mut quartet = Vec::with_capacity(4);
    for byte in input {
        if byte.is_ascii_whitespace() {
            continue;
        }
        if byte == b'=' {
            quartet.push(None);
        } else if let Some(value) = eval_base64_decode_sextet(byte) {
            quartet.push(Some(value));
        } else {
            continue;
        }
        if quartet.len() == 4 {
            eval_push_base64_decoded_quartet(&quartet, &mut output);
            quartet.clear();
        }
    }
    if !quartet.is_empty() {
        while quartet.len() < 4 {
            quartet.push(None);
        }
        eval_push_base64_decoded_quartet(&quartet, &mut output);
    }
    values.string_bytes_value(&output)
}

/// Returns the six-bit Base64 value for one encoded byte.
fn eval_base64_decode_sextet(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

/// Appends decoded bytes for one padded or unpadded Base64 quartet.
fn eval_push_base64_decoded_quartet(quartet: &[Option<u8>], output: &mut Vec<u8>) {
    let (Some(first), Some(second)) = (quartet[0], quartet[1]) else {
        return;
    };
    output.push((first << 2) | (second >> 4));
    let Some(third) = quartet[2] else {
        return;
    };
    output.push(((second & 0x0f) << 4) | (third >> 2));
    let Some(fourth) = quartet[3] else {
        return;
    };
    output.push(((third & 0x03) << 6) | fourth);
}

/// Evaluates PHP one-argument floating-point math builtins over one eval expression.
fn eval_builtin_float_unary(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_float_unary_result(name, value, values)
}

/// Dispatches an evaluated value through the matching PHP floating-point unary math function.
fn eval_float_unary_result(
    name: &str,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let value = eval_float_value(value, values)?;
    let result = match name {
        "acos" => value.acos(),
        "asin" => value.asin(),
        "atan" => value.atan(),
        "cos" => value.cos(),
        "cosh" => value.cosh(),
        "deg2rad" => value.to_radians(),
        "exp" => value.exp(),
        "log2" => value.log2(),
        "log10" => value.log10(),
        "rad2deg" => value.to_degrees(),
        "sin" => value.sin(),
        "sinh" => value.sinh(),
        "tan" => value.tan(),
        "tanh" => value.tanh(),
        _ => return Err(EvalStatus::UnsupportedConstruct),
    };
    values.float(result)
}

/// Evaluates PHP two-argument floating-point math builtins over eval expressions.
fn eval_builtin_float_pair(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [left, right] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let left = eval_expr(left, context, scope, values)?;
    let right = eval_expr(right, context, scope, values)?;
    eval_float_pair_result(name, left, right, values)
}

/// Dispatches an evaluated pair through PHP `atan2()` or `hypot()`.
fn eval_float_pair_result(
    name: &str,
    left: RuntimeCellHandle,
    right: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let left = eval_float_value(left, values)?;
    let right = eval_float_value(right, values)?;
    let result = match name {
        "atan2" => left.atan2(right),
        "hypot" => left.hypot(right),
        _ => return Err(EvalStatus::UnsupportedConstruct),
    };
    values.float(result)
}

/// Evaluates PHP `log($num, $base = e)` over eval expressions.
fn eval_builtin_log(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [num] => {
            let num = eval_expr(num, context, scope, values)?;
            eval_log_result(num, None, values)
        }
        [num, base] => {
            let num = eval_expr(num, context, scope, values)?;
            let base = eval_expr(base, context, scope, values)?;
            eval_log_result(num, Some(base), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Computes PHP `log()` from already evaluated arguments.
fn eval_log_result(
    num: RuntimeCellHandle,
    base: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let num = eval_float_value(num, values)?;
    let result = match base {
        Some(base) => num.log(eval_float_value(base, values)?),
        None => num.ln(),
    };
    values.float(result)
}

/// Evaluates PHP `intdiv(...)` over two eval expressions.
fn eval_builtin_intdiv(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [left, right] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let left = eval_expr(left, context, scope, values)?;
    let right = eval_expr(right, context, scope, values)?;
    eval_intdiv_result(left, right, values)
}

/// Computes PHP integer division from already evaluated arguments.
fn eval_intdiv_result(
    left: RuntimeCellHandle,
    right: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let left = eval_int_value(left, values)?;
    let right = eval_int_value(right, values)?;
    if right == 0 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let result = left.checked_div(right).ok_or(EvalStatus::RuntimeFatal)?;
    values.int(result)
}

/// Evaluates PHP floating-point binary math builtins over two eval expressions.
fn eval_builtin_float_binary(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [left, right] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let left = eval_expr(left, context, scope, values)?;
    let right = eval_expr(right, context, scope, values)?;
    eval_float_binary_result(name, left, right, values)
}

/// Dispatches an evaluated pair through the matching PHP float math hook.
fn eval_float_binary_result(
    name: &str,
    left: RuntimeCellHandle,
    right: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "fdiv" => values.fdiv(left, right),
        "fmod" => values.fmod(left, right),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Evaluates PHP `clamp($value, $min, $max)` over three eval expressions.
fn eval_builtin_clamp(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value, min, max] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    let min = eval_expr(min, context, scope, values)?;
    let max = eval_expr(max, context, scope, values)?;
    eval_clamp_result(value, min, max, values)
}

/// Selects the inclusive clamp result after validating bound order and NaN bounds.
fn eval_clamp_result(
    value: RuntimeCellHandle,
    min: RuntimeCellHandle,
    max: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if eval_clamp_bound_is_nan(min, values)? || eval_clamp_bound_is_nan(max, values)? {
        return Err(EvalStatus::RuntimeFatal);
    }
    let invalid_bounds = values.compare(EvalBinOp::Gt, min, max)?;
    if values.truthy(invalid_bounds)? {
        return Err(EvalStatus::RuntimeFatal);
    }
    let above_max = values.compare(EvalBinOp::Gt, value, max)?;
    if values.truthy(above_max)? {
        return Ok(max);
    }
    let below_min = values.compare(EvalBinOp::Lt, value, min)?;
    if values.truthy(below_min)? {
        return Ok(min);
    }
    Ok(value)
}

/// Returns whether a clamp bound is a floating-point NaN value.
fn eval_clamp_bound_is_nan(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if values.type_tag(value)? != EVAL_TAG_FLOAT {
        return Ok(false);
    }
    Ok(eval_float_value(value, values)?.is_nan())
}

/// Evaluates PHP numeric `min(...)` and `max(...)` over eval expressions.
fn eval_builtin_min_max(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.len() < 2 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let mut evaluated_args = Vec::with_capacity(args.len());
    for arg in args {
        evaluated_args.push(eval_expr(arg, context, scope, values)?);
    }
    eval_min_max_result(name, &evaluated_args, values)
}

/// Selects the smallest or largest evaluated cell using runtime comparison hooks.
fn eval_min_max_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some((&first, rest)) = evaluated_args.split_first() else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let op = match name {
        "min" => EvalBinOp::Lt,
        "max" => EvalBinOp::Gt,
        _ => return Err(EvalStatus::UnsupportedConstruct),
    };
    let mut selected = first;
    for candidate in rest {
        let better = values.compare(op, *candidate, selected)?;
        if values.truthy(better)? {
            selected = *candidate;
        }
    }
    Ok(selected)
}

/// Evaluates PHP scalar cast builtins over one eval expression.
fn eval_builtin_cast(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_cast_result(name, value, values)
}

/// Dispatches an already evaluated value through the matching PHP cast hook.
fn eval_cast_result(
    name: &str,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "intval" => values.cast_int(value),
        "floatval" => values.cast_float(value),
        "strval" => values.cast_string(value),
        "boolval" => values.cast_bool(value),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Evaluates PHP's `gettype(...)` over one eval expression.
fn eval_builtin_gettype(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_gettype_result(value, values)
}

/// Converts one boxed runtime tag into PHP's `gettype()` spelling.
fn eval_gettype_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let tag = values.type_tag(value)?;
    values.string(eval_gettype_name(tag))
}

/// Returns the PHP-visible type name for a concrete eval runtime tag.
fn eval_gettype_name(tag: u64) -> &'static str {
    match tag {
        EVAL_TAG_INT => "integer",
        EVAL_TAG_FLOAT => "double",
        EVAL_TAG_STRING => "string",
        EVAL_TAG_BOOL => "boolean",
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => "array",
        EVAL_TAG_OBJECT => "object",
        EVAL_TAG_RESOURCE => "resource",
        EVAL_TAG_NULL => "NULL",
        _ => "NULL",
    }
}

/// Evaluates PHP scalar/container type predicate builtins over one eval expression.
fn eval_builtin_type_predicate(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_type_predicate_result(name, value, values)
}

/// Converts a concrete runtime tag into a PHP `is_*` predicate result.
fn eval_type_predicate_result(
    name: &str,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let tag = values.type_tag(value)?;
    let result = match name {
        "is_int" | "is_integer" | "is_long" => tag == EVAL_TAG_INT,
        "is_float" | "is_double" | "is_real" => tag == EVAL_TAG_FLOAT,
        "is_string" => tag == EVAL_TAG_STRING,
        "is_bool" => tag == EVAL_TAG_BOOL,
        "is_null" => tag == EVAL_TAG_NULL,
        "is_array" | "is_iterable" => matches!(tag, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC),
        "is_resource" => tag == EVAL_TAG_RESOURCE,
        "is_nan" => eval_float_value(value, values)?.is_nan(),
        "is_infinite" => eval_float_value(value, values)?.is_infinite(),
        "is_finite" => eval_float_value(value, values)?.is_finite(),
        "is_numeric" => {
            tag == EVAL_TAG_INT
                || tag == EVAL_TAG_FLOAT
                || (tag == EVAL_TAG_STRING && eval_is_numeric_string(&values.string_bytes(value)?))
        }
        _ => return Err(EvalStatus::UnsupportedConstruct),
    };
    values.bool_value(result)
}

/// Matches the static backend's legacy ASCII numeric-string scan.
fn eval_is_numeric_string(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
    }

    let mut index = 0;
    let mut consumed_digits = 0;
    if bytes[index] == b'-' {
        index += 1;
        if index >= bytes.len() {
            return false;
        }
    }

    while index < bytes.len() {
        if bytes[index] == b'.' {
            index += 1;
            break;
        }
        if !bytes[index].is_ascii_digit() {
            return false;
        }
        consumed_digits += 1;
        index += 1;
    }

    while index < bytes.len() {
        if !bytes[index].is_ascii_digit() {
            return false;
        }
        consumed_digits += 1;
        index += 1;
    }

    consumed_digits > 0
}

/// Evaluates PHP's `hash_equals(...)` over two eval expressions.
fn eval_builtin_hash_equals(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [known, user] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let known = eval_expr(known, context, scope, values)?;
    let user = eval_expr(user, context, scope, values)?;
    eval_hash_equals_result(known, user, values)
}

/// Compares two converted strings with PHP `hash_equals()` semantics.
fn eval_hash_equals_result(
    known: RuntimeCellHandle,
    user: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let known = values.string_bytes(known)?;
    let user = values.string_bytes(user)?;
    if known.len() != user.len() {
        return values.bool_value(false);
    }
    let mut diff = 0u8;
    for (known, user) in known.iter().zip(user.iter()) {
        diff |= known ^ user;
    }
    values.bool_value(diff == 0)
}

/// Evaluates PHP string comparison builtins over two eval expressions.
fn eval_builtin_string_compare(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [left, right] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let left = eval_expr(left, context, scope, values)?;
    let right = eval_expr(right, context, scope, values)?;
    eval_string_compare_result(name, left, right, values)
}

/// Compares two converted strings and returns -1, 0, or 1.
fn eval_string_compare_result(
    name: &str,
    left: RuntimeCellHandle,
    right: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut left = values.string_bytes(left)?;
    let mut right = values.string_bytes(right)?;
    match name {
        "strcmp" => {}
        "strcasecmp" => {
            left.make_ascii_lowercase();
            right.make_ascii_lowercase();
        }
        _ => return Err(EvalStatus::UnsupportedConstruct),
    }
    let result = match left.cmp(&right) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    };
    values.int(result)
}

/// Evaluates PHP's byte-string search predicates over two eval expressions.
fn eval_builtin_string_search(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [haystack, needle] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let haystack = eval_expr(haystack, context, scope, values)?;
    let needle = eval_expr(needle, context, scope, values)?;
    eval_string_search_result(name, haystack, needle, values)
}

/// Checks one converted haystack for one converted needle using PHP byte-string semantics.
fn eval_string_search_result(
    name: &str,
    haystack: RuntimeCellHandle,
    needle: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let haystack = values.string_bytes(haystack)?;
    let needle = values.string_bytes(needle)?;
    let matched = match name {
        "str_contains" => {
            needle.is_empty()
                || haystack
                    .windows(needle.len())
                    .any(|window| window == needle)
        }
        "str_starts_with" => haystack.starts_with(&needle),
        "str_ends_with" => haystack.ends_with(&needle),
        _ => return Err(EvalStatus::UnsupportedConstruct),
    };
    values.bool_value(matched)
}

/// Evaluates PHP byte-string position builtins over two eval expressions.
fn eval_builtin_string_position(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [haystack, needle] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let haystack = eval_expr(haystack, context, scope, values)?;
    let needle = eval_expr(needle, context, scope, values)?;
    eval_string_position_result(name, haystack, needle, values)
}

/// Returns the first or last byte offset of a converted needle, or PHP `false`.
fn eval_string_position_result(
    name: &str,
    haystack: RuntimeCellHandle,
    needle: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let haystack = values.string_bytes(haystack)?;
    let needle = values.string_bytes(needle)?;
    let position = match name {
        "strpos" if needle.is_empty() => Some(0),
        "strpos" => haystack
            .windows(needle.len())
            .position(|window| window == needle),
        "strrpos" if needle.is_empty() => Some(haystack.len()),
        "strrpos" => haystack
            .windows(needle.len())
            .rposition(|window| window == needle),
        _ => return Err(EvalStatus::UnsupportedConstruct),
    };
    match position {
        Some(position) => {
            let position = i64::try_from(position).map_err(|_| EvalStatus::RuntimeFatal)?;
            values.int(position)
        }
        None => values.bool_value(false),
    }
}

/// Evaluates PHP `strstr(...)` over haystack, needle, and optional prefix mode.
fn eval_builtin_strstr(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [haystack, needle] => {
            let haystack = eval_expr(haystack, context, scope, values)?;
            let needle = eval_expr(needle, context, scope, values)?;
            eval_strstr_result(haystack, needle, false, values)
        }
        [haystack, needle, before_needle] => {
            let haystack = eval_expr(haystack, context, scope, values)?;
            let needle = eval_expr(needle, context, scope, values)?;
            let before_needle = eval_expr(before_needle, context, scope, values)?;
            let before_needle = values.truthy(before_needle)?;
            eval_strstr_result(haystack, needle, before_needle, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Returns the suffix or prefix selected by PHP `strstr()`, or `false` when absent.
fn eval_strstr_result(
    haystack: RuntimeCellHandle,
    needle: RuntimeCellHandle,
    before_needle: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let haystack = values.string_bytes(haystack)?;
    let needle = values.string_bytes(needle)?;
    let position = if needle.is_empty() {
        Some(0)
    } else {
        eval_find_subslice(&haystack, &needle, 0)
    };
    let Some(position) = position else {
        return values.bool_value(false);
    };
    let result = if before_needle {
        &haystack[..position]
    } else {
        &haystack[position..]
    };
    values.string_bytes_value(result)
}

const PHP_DEFAULT_TRIM_MASK: &[u8] = b" \n\r\t\x0B\x0C\0";

/// Evaluates PHP trim-like string builtins over one eval expression and optional mask.
fn eval_builtin_trim_like(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value] => {
            let value = eval_expr(value, context, scope, values)?;
            eval_trim_like_result(name, value, None, values)
        }
        [value, mask] => {
            let value = eval_expr(value, context, scope, values)?;
            let mask = eval_expr(mask, context, scope, values)?;
            eval_trim_like_result(name, value, Some(mask), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Trims one converted string using PHP's default mask or a caller-provided byte mask.
fn eval_trim_like_result(
    name: &str,
    value: RuntimeCellHandle,
    mask: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let explicit_mask;
    let trim_mask = if let Some(mask) = mask {
        explicit_mask = values.string_bytes(mask)?;
        explicit_mask.as_slice()
    } else {
        PHP_DEFAULT_TRIM_MASK
    };

    let mut start = 0;
    let mut end = bytes.len();
    if matches!(name, "trim" | "ltrim") {
        while start < end && trim_mask.contains(&bytes[start]) {
            start += 1;
        }
    }
    if matches!(name, "trim" | "rtrim" | "chop") {
        while end > start && trim_mask.contains(&bytes[end - 1]) {
            end -= 1;
        }
    }
    if !matches!(name, "trim" | "ltrim" | "rtrim" | "chop") {
        return Err(EvalStatus::UnsupportedConstruct);
    }

    let value =
        String::from_utf8(bytes[start..end].to_vec()).map_err(|_| EvalStatus::RuntimeFatal)?;
    values.string(&value)
}

/// Evaluates PHP ASCII case-conversion string builtins over one eval expression.
fn eval_builtin_string_case(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_string_case_result(name, value, values)
}

/// Converts one eval value through PHP string conversion and ASCII case mapping.
fn eval_string_case_result(
    name: &str,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut bytes = values.string_bytes(value)?;
    match name {
        "strtolower" => {
            for byte in &mut bytes {
                if byte.is_ascii_uppercase() {
                    *byte += b'a' - b'A';
                }
            }
        }
        "strtoupper" => {
            for byte in &mut bytes {
                if byte.is_ascii_lowercase() {
                    *byte -= b'a' - b'A';
                }
            }
        }
        "ucfirst" => {
            if bytes.first().is_some_and(|byte| byte.is_ascii_lowercase()) {
                bytes[0] -= b'a' - b'A';
            }
        }
        "lcfirst" => {
            if bytes.first().is_some_and(|byte| byte.is_ascii_uppercase()) {
                bytes[0] += b'a' - b'A';
            }
        }
        _ => return Err(EvalStatus::UnsupportedConstruct),
    }
    let value = String::from_utf8(bytes).map_err(|_| EvalStatus::RuntimeFatal)?;
    values.string(&value)
}

/// Evaluates PHP `ucwords(...)` over one string and optional separator expression.
fn eval_builtin_ucwords(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value] => {
            let value = eval_expr(value, context, scope, values)?;
            eval_ucwords_result(value, None, values)
        }
        [value, separators] => {
            let value = eval_expr(value, context, scope, values)?;
            let separators = eval_expr(separators, context, scope, values)?;
            eval_ucwords_result(value, Some(separators), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Uppercases ASCII lowercase bytes at the start of words separated by PHP delimiters.
fn eval_ucwords_result(
    value: RuntimeCellHandle,
    separators: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut bytes = values.string_bytes(value)?;
    let separators = match separators {
        Some(separators) => values.string_bytes(separators)?,
        None => b" \t\r\n\x0c\x0b".to_vec(),
    };
    let mut word_start = true;
    for byte in &mut bytes {
        if separators.contains(byte) {
            word_start = true;
        } else if word_start {
            if byte.is_ascii_lowercase() {
                *byte -= b'a' - b'A';
            }
            word_start = false;
        }
    }
    values.string_bytes_value(&bytes)
}

/// Evaluates PHP `wordwrap(...)` over one string and optional wrapping controls.
fn eval_builtin_wordwrap(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value] => {
            let value = eval_expr(value, context, scope, values)?;
            eval_wordwrap_result(value, None, None, None, values)
        }
        [value, width] => {
            let value = eval_expr(value, context, scope, values)?;
            let width = eval_expr(width, context, scope, values)?;
            eval_wordwrap_result(value, Some(width), None, None, values)
        }
        [value, width, break_string] => {
            let value = eval_expr(value, context, scope, values)?;
            let width = eval_expr(width, context, scope, values)?;
            let break_string = eval_expr(break_string, context, scope, values)?;
            eval_wordwrap_result(value, Some(width), Some(break_string), None, values)
        }
        [value, width, break_string, cut] => {
            let value = eval_expr(value, context, scope, values)?;
            let width = eval_expr(width, context, scope, values)?;
            let break_string = eval_expr(break_string, context, scope, values)?;
            let cut = eval_expr(cut, context, scope, values)?;
            eval_wordwrap_result(value, Some(width), Some(break_string), Some(cut), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Wraps a byte string at PHP word boundaries and preserves existing newlines.
fn eval_wordwrap_result(
    value: RuntimeCellHandle,
    width: Option<RuntimeCellHandle>,
    break_string: Option<RuntimeCellHandle>,
    cut: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let width = match width {
        Some(width) => eval_int_value(width, values)?,
        None => 75,
    };
    let break_string = match break_string {
        Some(break_string) => values.string_bytes(break_string)?,
        None => b"\n".to_vec(),
    };
    if break_string.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let cut = match cut {
        Some(cut) => values.truthy(cut)?,
        None => false,
    };
    if width == 0 && cut {
        return Err(EvalStatus::RuntimeFatal);
    }
    if bytes.is_empty() {
        return values.string_bytes_value(&bytes);
    }
    let output = eval_wordwrap_bytes(&bytes, width, &break_string, cut);
    values.string_bytes_value(&output)
}

/// Applies the core PHP word-wrap scan over already converted byte slices.
fn eval_wordwrap_bytes(bytes: &[u8], width: i64, break_string: &[u8], cut: bool) -> Vec<u8> {
    if width < 0 && cut {
        let mut output = Vec::with_capacity(bytes.len() + (bytes.len() * break_string.len()));
        for byte in bytes {
            output.extend_from_slice(break_string);
            output.push(*byte);
        }
        return output;
    }

    let width = width.max(0) as usize;
    let mut output = Vec::with_capacity(bytes.len());
    let mut line_start = 0;
    let mut last_space = None;
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\n' => {
                output.extend_from_slice(&bytes[line_start..=index]);
                index += 1;
                line_start = index;
                last_space = None;
            }
            b' ' => {
                if index.saturating_sub(line_start) >= width {
                    output.extend_from_slice(&bytes[line_start..index]);
                    output.extend_from_slice(break_string);
                    index += 1;
                    line_start = index;
                    last_space = None;
                } else {
                    last_space = Some(index);
                    index += 1;
                }
            }
            _ if index.saturating_sub(line_start) >= width => {
                if let Some(space) = last_space {
                    output.extend_from_slice(&bytes[line_start..space]);
                    output.extend_from_slice(break_string);
                    line_start = space + 1;
                    last_space = None;
                } else if cut && width > 0 {
                    output.extend_from_slice(&bytes[line_start..index]);
                    output.extend_from_slice(break_string);
                    line_start = index;
                } else {
                    index += 1;
                }
            }
            _ => {
                index += 1;
            }
        }
    }
    output.extend_from_slice(&bytes[line_start..]);
    output
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
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    let len = values.array_len(value)?;
    let len = i64::try_from(len).map_err(|_| EvalStatus::RuntimeFatal)?;
    values.int(len)
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
            EvalStmt::ArrayAppendVar { .. }
            | EvalStmt::ArraySetVar { .. }
            | EvalStmt::Break
            | EvalStmt::ClassDecl { .. }
            | EvalStmt::Continue
            | EvalStmt::Echo(_)
            | EvalStmt::Expr(_)
            | EvalStmt::Global { .. }
            | EvalStmt::PropertySet { .. }
            | EvalStmt::ReferenceAssign { .. }
            | EvalStmt::Return(_)
            | EvalStmt::StoreVar { .. }
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
    if let Some(value) = eval_predefined_int_constant(name) {
        return values.int(value);
    }
    let Some(value) = context.constant(name) else {
        return Err(EvalStatus::RuntimeFatal);
    };
    values.retain(value)
}

/// Returns eval-visible predefined integer constants that do not live in dynamic context.
fn eval_predefined_int_constant(name: &str) -> Option<i64> {
    match name {
        "PATHINFO_DIRNAME" => Some(EVAL_PATHINFO_DIRNAME),
        "PATHINFO_BASENAME" => Some(EVAL_PATHINFO_BASENAME),
        "PATHINFO_EXTENSION" => Some(EVAL_PATHINFO_EXTENSION),
        "PATHINFO_FILENAME" => Some(EVAL_PATHINFO_FILENAME),
        "PATHINFO_ALL" => Some(EVAL_PATHINFO_ALL),
        "FNM_NOESCAPE" => Some(EVAL_FNM_NOESCAPE),
        "FNM_PATHNAME" => Some(EVAL_FNM_PATHNAME),
        "FNM_PERIOD" => Some(EVAL_FNM_PERIOD),
        "FNM_CASEFOLD" => Some(EVAL_FNM_CASEFOLD),
        _ => None,
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
mod tests {
    use std::collections::HashMap;
    use std::ffi::c_void;

    use crate::parser::parse_fragment;
    use crate::value::RuntimeCell;

    use super::*;

    /// Test-only array key representation for fake indexed and associative arrays.
    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    enum FakeKey {
        Int(i64),
        String(String),
    }

    /// Test-only runtime value representation used behind opaque cell handles.
    #[derive(Clone, Debug, PartialEq)]
    enum FakeValue {
        Null,
        Bool(bool),
        Int(i64),
        Float(f64),
        String(String),
        Bytes(Vec<u8>),
        Array(Vec<RuntimeCellHandle>),
        Assoc(Vec<(FakeKey, RuntimeCellHandle)>),
        Object(HashMap<String, RuntimeCellHandle>),
        Resource(i64),
    }

    /// Test runtime hooks that allocate stable fake handles and record echo output.
    #[derive(Default)]
    struct FakeOps {
        next_id: usize,
        values: HashMap<usize, FakeValue>,
        output: String,
        releases: Vec<RuntimeCellHandle>,
        warnings: Vec<String>,
    }

    impl FakeOps {
        /// Allocates one fake runtime cell and returns its opaque handle.
        fn alloc(&mut self, value: FakeValue) -> RuntimeCellHandle {
            self.next_id += 1;
            let id = self.next_id;
            self.values.insert(id, value);
            RuntimeCellHandle::from_raw(id as *mut RuntimeCell)
        }

        /// Reads a fake runtime cell by opaque handle.
        fn get(&self, handle: RuntimeCellHandle) -> FakeValue {
            let id = handle.as_ptr() as usize;
            self.values.get(&id).cloned().expect("fake cell missing")
        }

        /// Converts a fake runtime cell into a normalized fake PHP array key.
        fn key(&self, handle: RuntimeCellHandle) -> Result<FakeKey, EvalStatus> {
            let value = self.get(handle);
            match value {
                FakeValue::Int(value) => Ok(FakeKey::Int(value)),
                FakeValue::String(value) => eval_numeric_string_array_key(value.as_bytes())
                    .map(FakeKey::Int)
                    .map_or_else(|| Ok(FakeKey::String(value)), Ok),
                FakeValue::Bytes(value) => eval_numeric_string_array_key(&value)
                    .map(FakeKey::Int)
                    .map_or_else(
                        || Ok(FakeKey::String(String::from_utf8_lossy(&value).into_owned())),
                        Ok,
                    ),
                FakeValue::Null => Ok(FakeKey::String(String::new())),
                value => Ok(FakeKey::Int(self.fake_int(&value))),
            }
        }

        /// Allocates a fake runtime cell for an existing PHP array key.
        fn alloc_key(&mut self, key: &FakeKey) -> Result<RuntimeCellHandle, EvalStatus> {
            match key {
                FakeKey::Int(value) => self.int(*value),
                FakeKey::String(value) => self.string(value),
            }
        }
    }

    impl RuntimeValueOps for FakeOps {
        /// Creates a fake indexed array cell.
        fn array_new(&mut self, capacity: usize) -> Result<RuntimeCellHandle, EvalStatus> {
            Ok(self.alloc(FakeValue::Array(Vec::with_capacity(capacity))))
        }

        /// Creates a fake associative array cell.
        fn assoc_new(&mut self, _capacity: usize) -> Result<RuntimeCellHandle, EvalStatus> {
            Ok(self.alloc(FakeValue::Assoc(Vec::new())))
        }

        /// Reads one fake indexed array element.
        fn array_get(
            &mut self,
            array: RuntimeCellHandle,
            index: RuntimeCellHandle,
        ) -> Result<RuntimeCellHandle, EvalStatus> {
            let key = self.key(index)?;
            match self.get(array) {
                FakeValue::Array(elements) => {
                    let FakeKey::Int(index) = key else {
                        return self.null();
                    };
                    if index < 0 {
                        return self.null();
                    }
                    elements
                        .get(index as usize)
                        .copied()
                        .map_or_else(|| self.null(), Ok)
                }
                FakeValue::Assoc(entries) => entries
                    .iter()
                    .find_map(|(entry_key, value)| (entry_key == &key).then_some(*value))
                    .map_or_else(|| self.null(), Ok),
                _ => self.null(),
            }
        }

        /// Checks whether a fake array has the requested key without reading its value.
        fn array_key_exists(
            &mut self,
            key: RuntimeCellHandle,
            array: RuntimeCellHandle,
        ) -> Result<RuntimeCellHandle, EvalStatus> {
            let key = self.key(key)?;
            let exists = match self.get(array) {
                FakeValue::Array(elements) => {
                    matches!(key, FakeKey::Int(index) if index >= 0 && (index as usize) < elements.len())
                }
                FakeValue::Assoc(entries) => entries.iter().any(|(entry_key, _)| entry_key == &key),
                _ => return Err(EvalStatus::UnsupportedConstruct),
            };
            self.bool_value(exists)
        }

        /// Returns one fake foreach key by insertion-order position.
        fn array_iter_key(
            &mut self,
            array: RuntimeCellHandle,
            position: usize,
        ) -> Result<RuntimeCellHandle, EvalStatus> {
            match self.get(array) {
                FakeValue::Array(elements) if position < elements.len() => {
                    self.int(position as i64)
                }
                FakeValue::Assoc(entries) => {
                    let Some((key, _)) = entries.get(position) else {
                        return self.null();
                    };
                    self.alloc_key(key)
                }
                FakeValue::Array(_) => self.null(),
                _ => Err(EvalStatus::UnsupportedConstruct),
            }
        }

        /// Writes one fake indexed or associative array element.
        fn array_set(
            &mut self,
            array: RuntimeCellHandle,
            index: RuntimeCellHandle,
            value: RuntimeCellHandle,
        ) -> Result<RuntimeCellHandle, EvalStatus> {
            let key = self.key(index)?;
            let id = array.as_ptr() as usize;
            match self.values.get_mut(&id) {
                Some(FakeValue::Array(elements)) => {
                    let FakeKey::Int(index) = key else {
                        return Err(EvalStatus::UnsupportedConstruct);
                    };
                    if index < 0 {
                        return Err(EvalStatus::UnsupportedConstruct);
                    }
                    let index = index as usize;
                    while elements.len() <= index {
                        elements.push(RuntimeCellHandle::from_raw(std::ptr::null_mut()));
                    }
                    elements[index] = value;
                }
                Some(FakeValue::Assoc(entries)) => {
                    if let Some((_, existing_value)) =
                        entries.iter_mut().find(|(entry_key, _)| entry_key == &key)
                    {
                        *existing_value = value;
                    } else {
                        entries.push((key, value));
                    }
                }
                _ => return Err(EvalStatus::UnsupportedConstruct),
            }
            Ok(array)
        }

        /// Reads one fake object property by name.
        fn property_get(
            &mut self,
            object: RuntimeCellHandle,
            property: &str,
        ) -> Result<RuntimeCellHandle, EvalStatus> {
            match self.get(object) {
                FakeValue::Object(properties) => properties
                    .get(property)
                    .copied()
                    .map_or_else(|| self.null(), Ok),
                _ => Err(EvalStatus::UnsupportedConstruct),
            }
        }

        /// Writes one fake object property by name.
        fn property_set(
            &mut self,
            object: RuntimeCellHandle,
            property: &str,
            value: RuntimeCellHandle,
        ) -> Result<(), EvalStatus> {
            let id = object.as_ptr() as usize;
            let Some(FakeValue::Object(properties)) = self.values.get_mut(&id) else {
                return Err(EvalStatus::UnsupportedConstruct);
            };
            properties.insert(property.to_string(), value);
            Ok(())
        }

        /// Calls one fake object method by name.
        fn method_call(
            &mut self,
            object: RuntimeCellHandle,
            method: &str,
            args: Vec<RuntimeCellHandle>,
        ) -> Result<RuntimeCellHandle, EvalStatus> {
            match (self.get(object), method) {
                (FakeValue::Object(_), "answer") if args.is_empty() => self.int(42),
                (FakeValue::Object(properties), "read_x") => {
                    if !args.is_empty() {
                        return Err(EvalStatus::UnsupportedConstruct);
                    }
                    properties.get("x").copied().map_or_else(|| self.null(), Ok)
                }
                (FakeValue::Object(properties), "add_x") => {
                    let [arg] = args.as_slice() else {
                        return Err(EvalStatus::UnsupportedConstruct);
                    };
                    let x = properties
                        .get("x")
                        .copied()
                        .ok_or(EvalStatus::RuntimeFatal)?;
                    let FakeValue::Int(x) = self.get(x) else {
                        return Err(EvalStatus::UnsupportedConstruct);
                    };
                    let FakeValue::Int(arg) = self.get(*arg) else {
                        return Err(EvalStatus::UnsupportedConstruct);
                    };
                    self.int(x + arg)
                }
                (FakeValue::Object(properties), "add2_x") => {
                    let [left, right] = args.as_slice() else {
                        return Err(EvalStatus::UnsupportedConstruct);
                    };
                    let x = properties
                        .get("x")
                        .copied()
                        .ok_or(EvalStatus::RuntimeFatal)?;
                    let FakeValue::Int(x) = self.get(x) else {
                        return Err(EvalStatus::UnsupportedConstruct);
                    };
                    let FakeValue::Int(left) = self.get(*left) else {
                        return Err(EvalStatus::UnsupportedConstruct);
                    };
                    let FakeValue::Int(right) = self.get(*right) else {
                        return Err(EvalStatus::UnsupportedConstruct);
                    };
                    self.int(x + left + right)
                }
                _ => Err(EvalStatus::UnsupportedConstruct),
            }
        }

        /// Creates one fake object for eval `new` unit tests.
        fn new_object(&mut self, _class_name: &str) -> Result<RuntimeCellHandle, EvalStatus> {
            Ok(self.alloc(FakeValue::Object(HashMap::new())))
        }

        /// Applies fake constructor side effects for eval `new` unit tests.
        fn construct_object(
            &mut self,
            object: RuntimeCellHandle,
            args: Vec<RuntimeCellHandle>,
        ) -> Result<(), EvalStatus> {
            let id = object.as_ptr() as usize;
            let Some(FakeValue::Object(properties)) = self.values.get_mut(&id) else {
                return Err(EvalStatus::UnsupportedConstruct);
            };
            if let Some(first) = args.first().copied() {
                properties.insert("x".to_string(), first);
            }
            Ok(())
        }

        /// Reports one fake AOT class for eval `class_exists` unit tests.
        fn class_exists(&mut self, name: &str) -> Result<bool, EvalStatus> {
            Ok(name.eq_ignore_ascii_case("KnownClass"))
        }

        /// Returns the visible element count for fake array values.
        fn array_len(&mut self, array: RuntimeCellHandle) -> Result<usize, EvalStatus> {
            match self.get(array) {
                FakeValue::Array(elements) => Ok(elements.len()),
                FakeValue::Assoc(entries) => Ok(entries.len()),
                _ => Err(EvalStatus::UnsupportedConstruct),
            }
        }

        /// Returns whether a fake runtime cell is an indexed or associative array.
        fn is_array_like(&mut self, value: RuntimeCellHandle) -> Result<bool, EvalStatus> {
            Ok(matches!(
                self.get(value),
                FakeValue::Array(_) | FakeValue::Assoc(_)
            ))
        }

        /// Returns whether a fake runtime cell is null.
        fn is_null(&mut self, value: RuntimeCellHandle) -> Result<bool, EvalStatus> {
            Ok(matches!(self.get(value), FakeValue::Null))
        }

        /// Returns the fake runtime tag corresponding to a test value.
        fn type_tag(&mut self, value: RuntimeCellHandle) -> Result<u64, EvalStatus> {
            Ok(match self.get(value) {
                FakeValue::Int(_) => EVAL_TAG_INT,
                FakeValue::String(_) | FakeValue::Bytes(_) => EVAL_TAG_STRING,
                FakeValue::Float(_) => EVAL_TAG_FLOAT,
                FakeValue::Bool(_) => EVAL_TAG_BOOL,
                FakeValue::Array(_) => EVAL_TAG_ARRAY,
                FakeValue::Assoc(_) => EVAL_TAG_ASSOC,
                FakeValue::Object(_) => EVAL_TAG_OBJECT,
                FakeValue::Resource(_) => EVAL_TAG_RESOURCE,
                FakeValue::Null => EVAL_TAG_NULL,
            })
        }

        /// Records fake releases without freeing handles needed for assertions.
        fn release(&mut self, value: RuntimeCellHandle) -> Result<(), EvalStatus> {
            self.releases.push(value);
            Ok(())
        }

        /// Returns the same fake handle because fake cells do not refcount.
        fn retain(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
            Ok(value)
        }

        /// Records fake PHP warnings without writing to stderr.
        fn warning(&mut self, message: &str) -> Result<(), EvalStatus> {
            self.warnings.push(message.to_string());
            Ok(())
        }

        /// Creates a fake null cell.
        fn null(&mut self) -> Result<RuntimeCellHandle, EvalStatus> {
            Ok(self.alloc(FakeValue::Null))
        }

        /// Creates a fake bool cell.
        fn bool_value(&mut self, value: bool) -> Result<RuntimeCellHandle, EvalStatus> {
            Ok(self.alloc(FakeValue::Bool(value)))
        }

        /// Creates a fake int cell.
        fn int(&mut self, value: i64) -> Result<RuntimeCellHandle, EvalStatus> {
            Ok(self.alloc(FakeValue::Int(value)))
        }

        /// Creates a fake float cell.
        fn float(&mut self, value: f64) -> Result<RuntimeCellHandle, EvalStatus> {
            Ok(self.alloc(FakeValue::Float(value)))
        }

        /// Creates a fake string cell.
        fn string(&mut self, value: &str) -> Result<RuntimeCellHandle, EvalStatus> {
            Ok(self.alloc(FakeValue::String(value.to_string())))
        }

        /// Creates a fake string cell from raw PHP bytes.
        fn string_bytes_value(&mut self, value: &[u8]) -> Result<RuntimeCellHandle, EvalStatus> {
            match std::str::from_utf8(value) {
                Ok(value) => self.string(value),
                Err(_) => Ok(self.alloc(FakeValue::Bytes(value.to_vec()))),
            }
        }

        /// Casts a fake runtime cell to a fake integer cell.
        fn cast_int(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
            let value = self.get(value);
            let value = self.fake_int(&value);
            self.int(value)
        }

        /// Casts a fake runtime cell to a fake float cell.
        fn cast_float(
            &mut self,
            value: RuntimeCellHandle,
        ) -> Result<RuntimeCellHandle, EvalStatus> {
            let value = self.get(value);
            let value = self.fake_numeric(&value);
            self.float(value)
        }

        /// Casts a fake runtime cell to a fake string cell.
        fn cast_string(
            &mut self,
            value: RuntimeCellHandle,
        ) -> Result<RuntimeCellHandle, EvalStatus> {
            let value = self.stringify(value);
            self.string(&value)
        }

        /// Casts a fake runtime cell to a fake boolean cell.
        fn cast_bool(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
            let value = self.get(value);
            let value = self.fake_truthy(&value);
            self.bool_value(value)
        }

        /// Computes fake PHP absolute value while preserving float payloads.
        fn abs(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
            match self.get(value) {
                FakeValue::Float(value) => self.float(value.abs()),
                value => self.int(self.fake_int(&value).wrapping_abs()),
            }
        }

        /// Computes fake PHP ceiling through numeric conversion as a float result.
        fn ceil(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
            let value = self.get(value);
            self.float(self.fake_numeric(&value).ceil())
        }

        /// Computes fake PHP floor through numeric conversion as a float result.
        fn floor(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
            let value = self.get(value);
            self.float(self.fake_numeric(&value).floor())
        }

        /// Computes fake PHP square root through numeric conversion as a float result.
        fn sqrt(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
            let value = self.get(value);
            self.float(self.fake_numeric(&value).sqrt())
        }

        /// Reverses a fake string byte-wise for interpreter tests.
        fn strrev(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
            let mut bytes = self.stringify(value).into_bytes();
            bytes.reverse();
            let value = String::from_utf8(bytes).map_err(|_| EvalStatus::RuntimeFatal)?;
            self.string(&value)
        }

        /// Divides fake numeric cells with PHP `fdiv()` zero handling.
        fn fdiv(
            &mut self,
            left: RuntimeCellHandle,
            right: RuntimeCellHandle,
        ) -> Result<RuntimeCellHandle, EvalStatus> {
            let left = self.fake_numeric(&self.get(left));
            let right = self.fake_numeric(&self.get(right));
            self.float(left / right)
        }

        /// Computes fake floating-point modulo for interpreter tests.
        fn fmod(
            &mut self,
            left: RuntimeCellHandle,
            right: RuntimeCellHandle,
        ) -> Result<RuntimeCellHandle, EvalStatus> {
            let left = self.fake_numeric(&self.get(left));
            let right = self.fake_numeric(&self.get(right));
            self.float(left % right)
        }

        /// Adds fake numeric cells for interpreter tests.
        fn add(
            &mut self,
            left: RuntimeCellHandle,
            right: RuntimeCellHandle,
        ) -> Result<RuntimeCellHandle, EvalStatus> {
            match (self.get(left), self.get(right)) {
                (FakeValue::Int(left), FakeValue::Int(right)) => self.int(left + right),
                (left, right) => self.float(self.fake_numeric(&left) + self.fake_numeric(&right)),
            }
        }

        /// Subtracts fake numeric cells for interpreter tests.
        fn sub(
            &mut self,
            left: RuntimeCellHandle,
            right: RuntimeCellHandle,
        ) -> Result<RuntimeCellHandle, EvalStatus> {
            match (self.get(left), self.get(right)) {
                (FakeValue::Int(left), FakeValue::Int(right)) => self.int(left - right),
                (left, right) => self.float(self.fake_numeric(&left) - self.fake_numeric(&right)),
            }
        }

        /// Multiplies fake numeric cells for interpreter tests.
        fn mul(
            &mut self,
            left: RuntimeCellHandle,
            right: RuntimeCellHandle,
        ) -> Result<RuntimeCellHandle, EvalStatus> {
            match (self.get(left), self.get(right)) {
                (FakeValue::Int(left), FakeValue::Int(right)) => self.int(left * right),
                (left, right) => self.float(self.fake_numeric(&left) * self.fake_numeric(&right)),
            }
        }

        /// Divides fake numeric cells for interpreter tests.
        fn div(
            &mut self,
            left: RuntimeCellHandle,
            right: RuntimeCellHandle,
        ) -> Result<RuntimeCellHandle, EvalStatus> {
            let right = self.fake_numeric(&self.get(right));
            if right == 0.0 {
                return Err(EvalStatus::RuntimeFatal);
            }
            let left = self.fake_numeric(&self.get(left));
            self.float(left / right)
        }

        /// Computes fake integer modulo for interpreter tests.
        fn modulo(
            &mut self,
            left: RuntimeCellHandle,
            right: RuntimeCellHandle,
        ) -> Result<RuntimeCellHandle, EvalStatus> {
            let right = self.fake_int(&self.get(right));
            if right == 0 {
                return Err(EvalStatus::RuntimeFatal);
            }
            let left = self.fake_int(&self.get(left));
            self.int(left % right)
        }

        /// Raises fake numeric cells for interpreter tests.
        fn pow(
            &mut self,
            left: RuntimeCellHandle,
            right: RuntimeCellHandle,
        ) -> Result<RuntimeCellHandle, EvalStatus> {
            let left = self.fake_numeric(&self.get(left));
            let right = self.fake_numeric(&self.get(right));
            self.float(left.powf(right))
        }

        /// Rounds fake numeric cells with PHP's optional decimal precision.
        fn round(
            &mut self,
            value: RuntimeCellHandle,
            precision: Option<RuntimeCellHandle>,
        ) -> Result<RuntimeCellHandle, EvalStatus> {
            let value = self.fake_numeric(&self.get(value));
            let precision = precision
                .map(|precision| self.fake_int(&self.get(precision)))
                .unwrap_or(0);
            let multiplier = 10_f64.powf(precision as f64);
            self.float((value * multiplier).round() / multiplier)
        }

        /// Applies fake integer bitwise and shift operations for interpreter tests.
        fn bitwise(
            &mut self,
            op: EvalBinOp,
            left: RuntimeCellHandle,
            right: RuntimeCellHandle,
        ) -> Result<RuntimeCellHandle, EvalStatus> {
            let left = self.fake_int(&self.get(left));
            let right = self.fake_int(&self.get(right));
            let value = match op {
                EvalBinOp::BitAnd => left & right,
                EvalBinOp::BitOr => left | right,
                EvalBinOp::BitXor => left ^ right,
                EvalBinOp::ShiftLeft => {
                    if right < 0 {
                        return Err(EvalStatus::RuntimeFatal);
                    }
                    left.wrapping_shl(right as u32)
                }
                EvalBinOp::ShiftRight => {
                    if right < 0 {
                        return Err(EvalStatus::RuntimeFatal);
                    }
                    left.wrapping_shr(right as u32)
                }
                _ => return Err(EvalStatus::UnsupportedConstruct),
            };
            self.int(value)
        }

        /// Applies fake integer bitwise NOT for interpreter tests.
        fn bit_not(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
            let value = self.fake_int(&self.get(value));
            self.int(!value)
        }

        /// Concatenates fake cells with byte-preserving string conversion for interpreter tests.
        fn concat(
            &mut self,
            left: RuntimeCellHandle,
            right: RuntimeCellHandle,
        ) -> Result<RuntimeCellHandle, EvalStatus> {
            let mut left = self.string_bytes_for_value(&self.get(left));
            let right = self.string_bytes_for_value(&self.get(right));
            left.extend_from_slice(&right);
            self.string_bytes_value(&left)
        }

        /// Compares fake scalar cells and returns a fake PHP boolean.
        fn compare(
            &mut self,
            op: EvalBinOp,
            left: RuntimeCellHandle,
            right: RuntimeCellHandle,
        ) -> Result<RuntimeCellHandle, EvalStatus> {
            let result = match op {
                EvalBinOp::LooseEq => self.loose_eq(left, right),
                EvalBinOp::LooseNotEq => !self.loose_eq(left, right),
                EvalBinOp::StrictEq => self.strict_eq(left, right),
                EvalBinOp::StrictNotEq => !self.strict_eq(left, right),
                EvalBinOp::Lt => self.numeric(left)? < self.numeric(right)?,
                EvalBinOp::LtEq => self.numeric(left)? <= self.numeric(right)?,
                EvalBinOp::Gt => self.numeric(left)? > self.numeric(right)?,
                EvalBinOp::GtEq => self.numeric(left)? >= self.numeric(right)?,
                EvalBinOp::Add
                | EvalBinOp::Sub
                | EvalBinOp::Mul
                | EvalBinOp::Div
                | EvalBinOp::Mod
                | EvalBinOp::Pow
                | EvalBinOp::BitAnd
                | EvalBinOp::BitOr
                | EvalBinOp::BitXor
                | EvalBinOp::ShiftLeft
                | EvalBinOp::ShiftRight
                | EvalBinOp::Concat
                | EvalBinOp::Spaceship
                | EvalBinOp::LogicalAnd
                | EvalBinOp::LogicalOr
                | EvalBinOp::LogicalXor => {
                    return Err(EvalStatus::UnsupportedConstruct);
                }
            };
            self.bool_value(result)
        }

        /// Compares fake numeric cells and returns a PHP spaceship integer.
        fn spaceship(
            &mut self,
            left: RuntimeCellHandle,
            right: RuntimeCellHandle,
        ) -> Result<RuntimeCellHandle, EvalStatus> {
            let left = self.numeric(left)?;
            let right = self.numeric(right)?;
            let value = if left < right {
                -1
            } else if left > right {
                1
            } else {
                0
            };
            self.int(value)
        }

        /// Appends fake echo output for interpreter tests.
        fn echo(&mut self, value: RuntimeCellHandle) -> Result<(), EvalStatus> {
            let value = self.stringify(value);
            self.output.push_str(&value);
            Ok(())
        }

        /// Casts one fake runtime cell to bytes for nested eval parsing.
        fn string_bytes(&mut self, value: RuntimeCellHandle) -> Result<Vec<u8>, EvalStatus> {
            Ok(self.string_bytes_for_value(&self.get(value)))
        }

        /// Returns PHP-like truthiness for fake runtime cells.
        fn truthy(&mut self, value: RuntimeCellHandle) -> Result<bool, EvalStatus> {
            Ok(match self.get(value) {
                FakeValue::Null => false,
                FakeValue::Bool(value) => value,
                FakeValue::Int(value) => value != 0,
                FakeValue::Float(value) => value != 0.0,
                FakeValue::String(value) => !value.is_empty() && value != "0",
                FakeValue::Bytes(value) => !value.is_empty() && value.as_slice() != b"0",
                FakeValue::Array(value) => !value.is_empty(),
                FakeValue::Assoc(value) => !value.is_empty(),
                FakeValue::Object(_) => true,
                FakeValue::Resource(_) => true,
            })
        }
    }

    impl FakeOps {
        /// Compares fake scalar values with the same loose rules covered by eval tests.
        fn loose_eq(&self, left: RuntimeCellHandle, right: RuntimeCellHandle) -> bool {
            match (self.get(left), self.get(right)) {
                (FakeValue::Bool(left), right) => left == self.fake_truthy(&right),
                (left, FakeValue::Bool(right)) => self.fake_truthy(&left) == right,
                (FakeValue::Null, FakeValue::Null) => true,
                (FakeValue::Null, FakeValue::String(value))
                | (FakeValue::String(value), FakeValue::Null) => value.is_empty(),
                (FakeValue::Null, FakeValue::Bytes(value))
                | (FakeValue::Bytes(value), FakeValue::Null) => value.is_empty(),
                (FakeValue::String(left), FakeValue::String(right)) => {
                    match (left.parse::<f64>(), right.parse::<f64>()) {
                        (Ok(left), Ok(right)) => left == right,
                        _ => left == right,
                    }
                }
                (FakeValue::Bytes(left), FakeValue::Bytes(right)) => left == right,
                (FakeValue::String(left), FakeValue::Bytes(right))
                | (FakeValue::Bytes(right), FakeValue::String(left)) => left.as_bytes() == right,
                (FakeValue::String(left), right) => left
                    .parse::<f64>()
                    .is_ok_and(|left| left == self.fake_numeric(&right)),
                (FakeValue::Bytes(left), right) => std::str::from_utf8(&left)
                    .ok()
                    .and_then(|left| left.parse::<f64>().ok())
                    .is_some_and(|left| left == self.fake_numeric(&right)),
                (left, FakeValue::String(right)) => right
                    .parse::<f64>()
                    .is_ok_and(|right| self.fake_numeric(&left) == right),
                (left, FakeValue::Bytes(right)) => std::str::from_utf8(&right)
                    .ok()
                    .and_then(|right| right.parse::<f64>().ok())
                    .is_some_and(|right| self.fake_numeric(&left) == right),
                (left, right) => self.fake_numeric(&left) == self.fake_numeric(&right),
            }
        }

        /// Compares fake scalar values by PHP strict tag and payload equality.
        fn strict_eq(&self, left: RuntimeCellHandle, right: RuntimeCellHandle) -> bool {
            match (self.get(left), self.get(right)) {
                (FakeValue::Null, FakeValue::Null) => true,
                (FakeValue::Bool(left), FakeValue::Bool(right)) => left == right,
                (FakeValue::Int(left), FakeValue::Int(right)) => left == right,
                (FakeValue::Float(left), FakeValue::Float(right)) => left == right,
                (FakeValue::String(left), FakeValue::String(right)) => left == right,
                (FakeValue::Bytes(left), FakeValue::Bytes(right)) => left == right,
                (FakeValue::String(left), FakeValue::Bytes(right))
                | (FakeValue::Bytes(right), FakeValue::String(left)) => left.as_bytes() == right,
                (FakeValue::Resource(left), FakeValue::Resource(right)) => left == right,
                _ => false,
            }
        }

        /// Converts one fake scalar cell to a numeric value for comparison tests.
        fn numeric(&self, handle: RuntimeCellHandle) -> Result<f64, EvalStatus> {
            Ok(self.fake_numeric(&self.get(handle)))
        }

        /// Converts a fake value to the numeric scalar used by comparison tests.
        fn fake_numeric(&self, value: &FakeValue) -> f64 {
            match value {
                FakeValue::Null => 0.0,
                FakeValue::Bool(false) => 0.0,
                FakeValue::Bool(true) => 1.0,
                FakeValue::Int(value) => *value as f64,
                FakeValue::Float(value) => *value,
                FakeValue::String(value) => value.parse::<f64>().unwrap_or(0.0),
                FakeValue::Bytes(value) => std::str::from_utf8(value)
                    .ok()
                    .and_then(|value| value.parse::<f64>().ok())
                    .unwrap_or(0.0),
                FakeValue::Array(value) => value.len() as f64,
                FakeValue::Assoc(value) => value.len() as f64,
                FakeValue::Object(_) => 1.0,
                FakeValue::Resource(value) => (*value + 1) as f64,
            }
        }

        /// Converts a fake value to the integer scalar used by modulo tests.
        fn fake_int(&self, value: &FakeValue) -> i64 {
            self.fake_numeric(value) as i64
        }

        /// Returns fake PHP truthiness for already-loaded test values.
        fn fake_truthy(&self, value: &FakeValue) -> bool {
            match value {
                FakeValue::Null => false,
                FakeValue::Bool(value) => *value,
                FakeValue::Int(value) => *value != 0,
                FakeValue::Float(value) => *value != 0.0,
                FakeValue::String(value) => !value.is_empty() && value != "0",
                FakeValue::Bytes(value) => !value.is_empty() && value.as_slice() != b"0",
                FakeValue::Array(value) => !value.is_empty(),
                FakeValue::Assoc(value) => !value.is_empty(),
                FakeValue::Object(_) => true,
                FakeValue::Resource(_) => true,
            }
        }

        /// Converts a fake runtime cell to a PHP-like string for test echo/concat.
        fn stringify(&self, handle: RuntimeCellHandle) -> String {
            match self.get(handle) {
                FakeValue::Null => String::new(),
                FakeValue::Bool(false) => String::new(),
                FakeValue::Bool(true) => "1".to_string(),
                FakeValue::Int(value) => value.to_string(),
                FakeValue::Float(value) => value.to_string(),
                FakeValue::String(value) => value,
                FakeValue::Bytes(value) => String::from_utf8_lossy(&value).into_owned(),
                FakeValue::Array(_) => "Array".to_string(),
                FakeValue::Assoc(_) => "Array".to_string(),
                FakeValue::Object(_) => "Object".to_string(),
                FakeValue::Resource(value) => format!("Resource id #{}", value + 1),
            }
        }

        /// Converts a fake PHP value to string bytes while preserving binary strings.
        fn string_bytes_for_value(&self, value: &FakeValue) -> Vec<u8> {
            match value {
                FakeValue::String(value) => value.as_bytes().to_vec(),
                FakeValue::Bytes(value) => value.clone(),
                value => self.stringify_value(value).into_bytes(),
            }
        }

        /// Converts one loaded fake PHP value to display text for byte coercions.
        fn stringify_value(&self, value: &FakeValue) -> String {
            match value {
                FakeValue::Null => String::new(),
                FakeValue::Bool(false) => String::new(),
                FakeValue::Bool(true) => "1".to_string(),
                FakeValue::Int(value) => value.to_string(),
                FakeValue::Float(value) => value.to_string(),
                FakeValue::String(value) => value.clone(),
                FakeValue::Bytes(value) => String::from_utf8_lossy(value).into_owned(),
                FakeValue::Array(_) | FakeValue::Assoc(_) => "Array".to_string(),
                FakeValue::Object(_) => "Object".to_string(),
                FakeValue::Resource(value) => format!("Resource id #{}", value + 1),
            }
        }
    }

    /// Test native invoker that returns the descriptor pointer as a runtime cell.
    unsafe extern "C" fn fake_native_return_descriptor(
        descriptor: *mut c_void,
        _args: *mut RuntimeCell,
    ) -> *mut RuntimeCell {
        descriptor.cast()
    }

    /// Verifies assignment writes a named scope entry and return reads it back.
    #[test]
    fn execute_program_stores_and_returns_scope_value() {
        let program = parse_fragment(b"$x = 3; return $x + 4;").expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
        let x = scope.visible_cell("x").expect("scope should contain x");

        assert_eq!(values.get(x), FakeValue::Int(3));
        assert_eq!(values.get(result), FakeValue::Int(7));
    }

    /// Verifies reference assignment aliases variable names and writes through the alias.
    #[test]
    fn execute_program_reference_assignment_updates_source_variable() {
        let program = parse_fragment(b"$x = 1; $alias =& $x; $alias = 5; return $x;")
            .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
        let x = scope.visible_cell("x").expect("scope should contain x");
        let alias = scope
            .visible_cell("alias")
            .expect("scope should contain alias");

        assert_eq!(x, alias);
        assert_eq!(values.get(x), FakeValue::Int(5));
        assert_eq!(values.get(result), FakeValue::Int(5));
    }

    /// Verifies simple variable compound assignments read, compute, and write the scope value.
    #[test]
    fn execute_program_evaluates_compound_assignments() {
        let program =
            parse_fragment(br#"$x = 2; $x += 3; $x *= 4; $x -= 5; $s = "v"; $s .= $x; echo $s;"#)
                .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
        let x = scope.visible_cell("x").expect("scope should contain x");

        assert_eq!(values.output, "v15");
        assert_eq!(values.get(x), FakeValue::Int(15));
    }

    /// Verifies division and modulo evaluate through fake runtime numeric hooks.
    #[test]
    fn execute_program_evaluates_division_and_modulo() {
        let program = parse_fragment(br#"$x = 20; $x /= 2; $x %= 6; echo $x; return 9 / 2;"#)
            .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
        let x = scope.visible_cell("x").expect("scope should contain x");

        assert_eq!(values.output, "4");
        assert_eq!(values.get(x), FakeValue::Int(4));
        assert_eq!(values.get(result), FakeValue::Float(4.5));
    }

    /// Verifies exponentiation evaluates through fake runtime numeric hooks.
    #[test]
    fn execute_program_evaluates_exponentiation() {
        let program = parse_fragment(
            br#"$x = 2; $x **= 3; echo $x; echo ":"; echo -2 ** 2; return 2 ** 3 ** 2;"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
        let x = scope.visible_cell("x").expect("scope should contain x");

        assert_eq!(values.output, "8:-4");
        assert_eq!(values.get(x), FakeValue::Float(8.0));
        assert_eq!(values.get(result), FakeValue::Float(512.0));
    }

    /// Verifies bitwise and shift operators evaluate through fake runtime hooks.
    #[test]
    fn execute_program_evaluates_bitwise_and_shift_ops() {
        let program = parse_fragment(
            br#"$x = 6; $x &= 3; echo $x; echo ":";
$x = 4; $x |= 1; echo $x; echo ":";
$x = 7; $x ^= 3; echo $x; echo ":";
$x = 1; $x <<= 5; echo $x; echo ":";
$x = 64; $x >>= 3; echo $x; echo ":";
echo ~0; echo ":"; echo -16 >> 2;
return (1 << 4) | ((16 >> 2) ^ (3 & 1));"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "2:5:4:32:8:-1:-4");
        assert_eq!(values.get(result), FakeValue::Int(21));
    }

    /// Verifies simple variable increment and decrement statements update the scope value.
    #[test]
    fn execute_program_evaluates_inc_dec_statements() {
        let program = parse_fragment(br#"$i = 1; $i++; ++$i; $i--; --$i; echo $i;"#)
            .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
        let i = scope.visible_cell("i").expect("scope should contain i");

        assert_eq!(values.output, "1");
        assert_eq!(values.get(i), FakeValue::Int(1));
    }

    /// Verifies echo and unset operate through runtime hooks and scope metadata.
    #[test]
    fn execute_program_echoes_and_unsets_scope_value() {
        let program =
            parse_fragment(br#"echo "hi" . $name; unset($name);"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let name = values.string(" Ada").expect("create fake string");
        scope.set("name", name, ScopeCellOwnership::Owned);

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "hi Ada");
        assert_eq!(values.get(result), FakeValue::Null);
        assert!(scope.entry("name").expect("unset marker").flags().unset);
    }

    /// Verifies comma-separated echo expressions are executed in source order.
    #[test]
    fn execute_program_echoes_comma_list() {
        let program = parse_fragment(br#"echo "a", $b, "c";"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let b = values.string("b").expect("create fake string");
        scope.set("b", b, ScopeCellOwnership::Owned);

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "abc");
    }

    /// Verifies print writes output and returns integer 1.
    #[test]
    fn execute_program_print_returns_one() {
        let program = parse_fragment(br#"return print "p";"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "p");
        assert_eq!(values.get(result), FakeValue::Int(1));
    }

    /// Verifies eval property reads and writes dispatch through runtime hooks.
    #[test]
    fn execute_program_reads_and_writes_object_property() {
        let program = parse_fragment(br#"$this->x = $this->x + 1; return $this->x;"#)
            .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let x = values.int(1).expect("create fake int");
        let mut properties = HashMap::new();
        properties.insert("x".to_string(), x);
        let object = values.alloc(FakeValue::Object(properties));
        scope.set("this", object, ScopeCellOwnership::Borrowed);

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::Int(2));
        assert_eq!(
            values
                .property_get(object, "x")
                .map(|value| values.get(value))
                .expect("property should be readable"),
            FakeValue::Int(2)
        );
    }

    /// Verifies eval method calls dispatch through the runtime method hook.
    #[test]
    fn execute_program_calls_object_method() {
        let program = parse_fragment(br#"return $this->answer();"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let object = values.alloc(FakeValue::Object(HashMap::new()));
        scope.set("this", object, ScopeCellOwnership::Borrowed);

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::Int(42));
    }

    /// Verifies eval method calls forward evaluated arguments to the runtime hook.
    #[test]
    fn execute_program_calls_object_method_with_argument() {
        let program = parse_fragment(br#"return $this->add_x(5);"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let x = values.int(7).expect("create fake int");
        let mut properties = HashMap::new();
        properties.insert("x".to_string(), x);
        let object = values.alloc(FakeValue::Object(properties));
        scope.set("this", object, ScopeCellOwnership::Borrowed);

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::Int(12));
    }

    /// Verifies eval method calls forward multiple evaluated arguments to the runtime hook.
    #[test]
    fn execute_program_calls_object_method_with_two_arguments() {
        let program =
            parse_fragment(br#"return $this->add2_x(5, 6);"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let x = values.int(7).expect("create fake int");
        let mut properties = HashMap::new();
        properties.insert("x".to_string(), x);
        let object = values.alloc(FakeValue::Object(properties));
        scope.set("this", object, ScopeCellOwnership::Borrowed);

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::Int(18));
    }

    /// Verifies eval method calls forward numerically unpacked arguments.
    #[test]
    fn execute_program_calls_object_method_with_spread_arguments() {
        let program =
            parse_fragment(br#"return $this->add2_x(...[5, 6]);"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let x = values.int(7).expect("create fake int");
        let mut properties = HashMap::new();
        properties.insert("x".to_string(), x);
        let object = values.alloc(FakeValue::Object(properties));
        scope.set("this", object, ScopeCellOwnership::Borrowed);

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::Int(18));
    }

    /// Verifies eval object construction dispatches through runtime hooks.
    #[test]
    fn execute_program_constructs_named_object() {
        let program = parse_fragment(br#"return new Box();"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::Object(HashMap::new()));
    }

    /// Verifies eval object construction passes constructor arguments through runtime hooks.
    #[test]
    fn execute_program_constructs_named_object_with_args() {
        let program = parse_fragment(br#"return new Box(1);"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
        let FakeValue::Object(properties) = values.get(result) else {
            panic!("expected fake object");
        };

        assert_eq!(values.get(properties["x"]), FakeValue::Int(1));
    }

    /// Verifies if/else executes only the PHP-truthy branch.
    #[test]
    fn execute_program_if_else_uses_php_truthiness() {
        let program = parse_fragment(br#"if ($flag) { $x = "then"; } else { $x = "else"; }"#)
            .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let flag = values.int(0).expect("create fake int");
        scope.set("flag", flag, ScopeCellOwnership::Owned);

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
        let x = scope.visible_cell("x").expect("scope should contain x");

        assert_eq!(values.get(x), FakeValue::String("else".to_string()));
    }

    /// Verifies elseif chains execute the first truthy branch and skip later branches.
    #[test]
    fn execute_program_elseif_uses_first_truthy_branch() {
        let program = parse_fragment(
            br#"if ($a) { $x = "a"; } elseif ($b) { $x = "b"; } else { $x = "c"; }"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let a = values.bool_value(false).expect("create fake bool");
        let b = values.bool_value(true).expect("create fake bool");
        scope.set("a", a, ScopeCellOwnership::Owned);
        scope.set("b", b, ScopeCellOwnership::Owned);

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
        let x = scope.visible_cell("x").expect("scope should contain x");

        assert_eq!(values.get(x), FakeValue::String("b".to_string()));
    }

    /// Verifies while repeats while the condition remains truthy and propagates writes.
    #[test]
    fn execute_program_while_uses_php_truthiness() {
        let program = parse_fragment(br#"while ($flag) { echo $flag; $flag = false; }"#)
            .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let flag = values.int(2).expect("create fake int");
        scope.set("flag", flag, ScopeCellOwnership::Owned);

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
        let flag = scope
            .visible_cell("flag")
            .expect("scope should contain flag");

        assert_eq!(values.output, "2");
        assert_eq!(values.get(flag), FakeValue::Bool(false));
    }

    /// Verifies do/while runs the body before testing the condition.
    #[test]
    fn execute_program_do_while_runs_body_before_condition() {
        let program = parse_fragment(br#"do { echo $i; $i = $i + 1; } while (false);"#)
            .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let i = values.int(0).expect("create fake int");
        scope.set("i", i, ScopeCellOwnership::Owned);

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
        let i = scope.visible_cell("i").expect("scope should contain i");

        assert_eq!(values.output, "0");
        assert_eq!(values.get(i), FakeValue::Int(1));
    }

    /// Verifies switch uses loose matching and falls through after the matching case.
    #[test]
    fn execute_program_switch_matches_and_falls_through() {
        let program =
            parse_fragment(br#"switch ($x) { case 1: echo "one"; break; case 2: echo "two"; default: echo "default"; }"#)
                .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let x = values.int(2).expect("create fake int");
        scope.set("x", x, ScopeCellOwnership::Owned);

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "twodefault");
    }

    /// Verifies for loops run init, condition, update, and body in PHP order.
    #[test]
    fn execute_program_for_loop_updates_after_body() {
        let program = parse_fragment(br#"for ($i = 3; $i; $i = $i - 1) { echo $i; }"#)
            .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
        let i = scope.visible_cell("i").expect("scope should contain i");

        assert_eq!(values.output, "321");
        assert_eq!(values.get(i), FakeValue::Int(0));
    }

    /// Verifies `continue` in a for loop still runs the update clause.
    #[test]
    fn execute_program_for_continue_runs_update_clause() {
        let program = parse_fragment(
            br#"for ($i = 3; $i; $i = $i - 1) { if ($i - 1) { continue; } echo "done"; }"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
        let i = scope.visible_cell("i").expect("scope should contain i");

        assert_eq!(values.output, "done");
        assert_eq!(values.get(i), FakeValue::Int(0));
    }

    /// Verifies comparison operators return boolean cells usable by echo and branches.
    #[test]
    fn execute_program_comparisons_return_bool_cells() {
        let program = parse_fragment(
            br#"echo 2 < 3; echo 3 <= 3; echo 4 > 3; echo 4 >= 4; if ("10" == 10) { echo "n"; } if ("a" != "b") { echo "s"; }"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "1111ns");
    }

    /// Verifies spaceship comparisons return PHP -1/0/1 integer cells.
    #[test]
    fn execute_program_spaceship_returns_int_cells() {
        let program =
            parse_fragment(br#"echo 1 <=> 2; echo ":"; echo 2 <=> 2; echo ":"; echo 3 <=> 2;"#)
                .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "-1:0:1");
    }

    /// Verifies strict equality keeps PHP type identity distinct from loose equality.
    #[test]
    fn execute_program_strict_equality_uses_type_identity() {
        let program = parse_fragment(
            br#"echo "10" == 10; echo "10" === 10; echo "10" === "10"; echo "10" !== 10;"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "111");
    }

    /// Verifies logical AND skips an unsupported right-hand expression after a false left side.
    #[test]
    fn execute_program_short_circuits_logical_and() {
        let program =
            parse_fragment(br#"return false && missing();"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::Bool(false));
    }

    /// Verifies logical OR skips an unsupported right-hand expression after a true left side.
    #[test]
    fn execute_program_short_circuits_logical_or() {
        let program = parse_fragment(br#"return true || missing();"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies PHP keyword logical operators use PHP precedence and short-circuiting.
    #[test]
    fn execute_program_evaluates_keyword_logical_operators() {
        let program = parse_fragment(
            br#"echo (false || true and false) ? "T" : "F"; return true or missing();"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "F");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies PHP keyword `xor` evaluates both operands and returns a boolean cell.
    #[test]
    fn execute_program_evaluates_keyword_xor() {
        let program = parse_fragment(
            br#"echo (true xor false) ? "T" : "F"; echo (true xor true) ? "T" : "F";"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "TF");
    }

    /// Verifies ternary expressions evaluate only the selected branch.
    #[test]
    fn execute_program_ternary_short_circuits_unselected_branch() {
        let program =
            parse_fragment(br#"echo true ? "yes" : missing(); echo false ? missing() : "no";"#)
                .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "yesno");
    }

    /// Verifies the short ternary form returns the condition value when it is truthy.
    #[test]
    fn execute_program_short_ternary_reuses_truthy_condition() {
        let program = parse_fragment(br#"echo "x" ?: "fallback"; echo false ?: "fallback";"#)
            .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "xfallback");
    }

    /// Verifies null coalescing uses the default for missing or null values.
    #[test]
    fn execute_program_null_coalesce_uses_default_for_missing_or_null() {
        let program =
            parse_fragment(br#"echo $missing ?? "fallback"; echo $x ?? "null-fallback";"#)
                .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let x = values.null().expect("create fake null");
        scope.set("x", x, ScopeCellOwnership::Owned);

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "fallbacknull-fallback");
    }

    /// Verifies null coalescing skips the default expression for non-null values.
    #[test]
    fn execute_program_null_coalesce_short_circuits_non_null_value() {
        let program = parse_fragment(br#"echo "set" ?? missing();"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "set");
    }

    /// Verifies logical negation returns boolean cells using PHP truthiness.
    #[test]
    fn execute_program_evaluates_logical_not() {
        let program = parse_fragment(br#"echo !false; echo !"x";"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "1");
    }

    /// Verifies unary numeric operators delegate to PHP numeric runtime operations.
    #[test]
    fn execute_program_evaluates_unary_numeric_ops() {
        let program = parse_fragment(br#"return -$x + +2;"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let x = values.int(5).expect("create fake int");
        scope.set("x", x, ScopeCellOwnership::Owned);

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::Int(-3));
    }

    /// Verifies foreach assigns each indexed element to the value variable.
    #[test]
    fn execute_program_foreach_iterates_indexed_values() {
        let program = parse_fragment(br#"foreach (["a", "b"] as $item) { echo $item; }"#)
            .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
        let item = scope
            .visible_cell("item")
            .expect("scope should contain last foreach item");

        assert_eq!(values.output, "ab");
        assert_eq!(values.get(item), FakeValue::String("b".to_string()));
    }

    /// Verifies foreach key-value targets receive indexed integer keys and values.
    #[test]
    fn execute_program_foreach_assigns_indexed_keys() {
        let program =
            parse_fragment(br#"foreach (["a", "b"] as $key => $item) { echo $key . $item; }"#)
                .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
        let key = scope.visible_cell("key").expect("scope should contain key");
        let item = scope
            .visible_cell("item")
            .expect("scope should contain last foreach item");

        assert_eq!(values.output, "0a1b");
        assert_eq!(values.get(key), FakeValue::Int(1));
        assert_eq!(values.get(item), FakeValue::String("b".to_string()));
    }

    /// Verifies foreach over associative arrays preserves insertion-order keys and values.
    #[test]
    fn execute_program_foreach_iterates_assoc_keys_and_values() {
        let program = parse_fragment(
            br#"foreach (["a" => 1, "b" => 2] as $key => $item) { echo $key . ":" . $item . ";"; }"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "a:1;b:2;");
    }

    /// Verifies value-only foreach over associative arrays still yields values in insertion order.
    #[test]
    fn execute_program_foreach_iterates_assoc_values_only() {
        let program = parse_fragment(br#"foreach (["a" => 1, "b" => 2] as $item) { echo $item; }"#)
            .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "12");
    }

    /// Verifies break and continue control foreach execution inside eval.
    #[test]
    fn execute_program_foreach_honors_break_and_continue() {
        let program = parse_fragment(
            br#"foreach ([1, 2, 3] as $item) { if ($item == 1) { continue; } echo $item; break; }"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "2");
    }

    /// Verifies indexed array literals and reads execute through runtime hooks.
    #[test]
    fn execute_program_reads_indexed_array_literal() {
        let program = parse_fragment(br#"return ["a", "b"][1];"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::String("b".to_string()));
    }

    /// Verifies associative array literals and string-key reads execute through runtime hooks.
    #[test]
    fn execute_program_reads_assoc_array_literal() {
        let program =
            parse_fragment(br#"return ["name" => "Ada"]["name"];"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::String("Ada".to_string()));
    }

    /// Verifies unkeyed assoc literal entries start at zero after string keys.
    #[test]
    fn execute_program_assoc_array_literal_unkeyed_after_string_key_starts_at_zero() {
        let program = parse_fragment(br#"return ["name" => "Ada", "Grace"][0];"#)
            .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::String("Grace".to_string()));
    }

    /// Verifies unkeyed assoc literal entries use one plus the largest integer key.
    #[test]
    fn execute_program_assoc_array_literal_unkeyed_after_positive_int_key() {
        let program =
            parse_fragment(br#"return [2 => "two", "tail"][3];"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::String("tail".to_string()));
    }

    /// Verifies unkeyed assoc literal entries preserve PHP's negative-key rule.
    #[test]
    fn execute_program_assoc_array_literal_unkeyed_after_negative_int_key() {
        let program =
            parse_fragment(br#"return [-2 => "minus", "tail"][-1];"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::String("tail".to_string()));
    }

    /// Verifies numeric string literal keys update the next automatic key.
    #[test]
    fn execute_program_assoc_array_literal_unkeyed_after_numeric_string_key() {
        let program =
            parse_fragment(br#"return ["2" => "two", "tail"][3];"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::String("tail".to_string()));
    }

    /// Verifies leading-zero string literal keys do not update the automatic key.
    #[test]
    fn execute_program_assoc_array_literal_unkeyed_after_leading_zero_string_key() {
        let program =
            parse_fragment(br#"return ["02" => "two", "tail"][0];"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::String("tail".to_string()));
    }

    /// Verifies null literal keys normalize to empty strings without advancing automatic keys.
    #[test]
    fn execute_program_assoc_array_literal_unkeyed_after_null_key() {
        let program = parse_fragment(br#"return [null => "empty", "tail"][0];"#)
            .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::String("tail".to_string()));
    }

    /// Verifies null literal keys are readable through the empty-string key.
    #[test]
    fn execute_program_assoc_array_literal_reads_null_key_as_empty_string() {
        let program =
            parse_fragment(br#"return [null => "empty"][""];"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::String("empty".to_string()));
    }

    /// Verifies boolean literal keys update the next automatic key after integer normalization.
    #[test]
    fn execute_program_assoc_array_literal_unkeyed_after_bool_key() {
        let program =
            parse_fragment(br#"return [true => "yes", "tail"][2];"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::String("tail".to_string()));
    }

    /// Verifies false literal keys update the next automatic key from zero.
    #[test]
    fn execute_program_assoc_array_literal_unkeyed_after_false_key() {
        let program =
            parse_fragment(br#"return [false => "no", "tail"][1];"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::String("tail".to_string()));
    }

    /// Verifies float literal keys update the next automatic key after truncation.
    #[test]
    fn execute_program_assoc_array_literal_unkeyed_after_float_key() {
        let program =
            parse_fragment(br#"return [2.7 => "two", "tail"][3];"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::String("tail".to_string()));
    }

    /// Verifies nested eval calls parse and execute against the same dynamic scope.
    #[test]
    fn execute_program_nested_eval_uses_same_scope() {
        let program =
            parse_fragment(br#"eval("$x = $x + 4;"); return $x;"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let x = values.int(1).expect("create fake int");
        scope.set("x", x, ScopeCellOwnership::Owned);

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::Int(5));
    }

    /// Verifies `__LINE__` inside eval uses the source line within the fragment.
    #[test]
    fn execute_program_magic_line_uses_fragment_line() {
        let program = parse_fragment(b"\nreturn __LINE__;").expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::Int(2));
    }

    /// Verifies file-dependent eval magic constants use call-site metadata from the context.
    #[test]
    fn execute_program_magic_file_and_dir_use_context_call_site() {
        let program =
            parse_fragment(br#"return __FILE__ . "|" . __DIR__;"#).expect("parse eval fragment");
        let mut context = ElephcEvalContext::new();
        context.set_call_site("/tmp/main.php", "/tmp", 17);
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
            .expect("execute eval ir");

        assert_eq!(
            values.get(result),
            FakeValue::String("/tmp/main.php(17) : eval()'d code|/tmp".to_string())
        );
    }

    /// Verifies eval class, namespace, and trait magic constants are empty in eval scope.
    #[test]
    fn execute_program_scope_magic_constants_are_empty_strings() {
        let program = parse_fragment(
            br#"return "[" . __CLASS__ . "|" . __NAMESPACE__ . "|" . __TRAIT__ . "]";"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::String("[||]".to_string()));
    }

    /// Verifies eval-declared functions can be called by the same fragment.
    #[test]
    fn execute_program_calls_declared_function() {
        let program = parse_fragment(br#"function dyn($x) { return $x + 1; } return dyn(4);"#)
            .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::Int(5));
    }

    /// Verifies eval-declared functions bind named arguments by parameter name.
    #[test]
    fn execute_program_calls_declared_function_with_named_args() {
        let program = parse_fragment(
            br#"function dyn($x, $y) { return ($x * 10) + $y; } return dyn(y: 2, x: 1);"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::Int(12));
    }

    /// Verifies eval-declared functions unpack indexed arrays as positional arguments.
    #[test]
    fn execute_program_calls_declared_function_with_spread_args() {
        let program = parse_fragment(
            br#"function dyn($x, $y) { return ($x * 10) + $y; } return dyn(...[1, 2]);"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::Int(12));
    }

    /// Verifies string keys unpack as named arguments for eval-declared functions.
    #[test]
    fn execute_program_calls_declared_function_with_named_spread_args() {
        let program = parse_fragment(
            br#"function dyn($x, $y) { return ($x * 10) + $y; } return dyn(...["y" => 2], x: 1);"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::Int(12));
    }

    /// Verifies eval-declared function static locals persist between calls.
    #[test]
    fn execute_program_static_var_persists_in_declared_function() {
        let program = parse_fragment(
            br#"function dyn() { for ($i = 0; $i < 2; $i++) { static $n = 0; $n++; } return $n; }
return (dyn() * 10) + dyn();"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::Int(24));
    }

    /// Verifies top-level eval static declarations reinitialize on each eval execution.
    #[test]
    fn execute_program_top_level_static_var_reinitializes_per_eval() {
        let program =
            parse_fragment(br#"static $n = 0; $n++; return $n;"#).expect("parse eval fragment");
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let first = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
            .expect("execute first eval ir");
        let second = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
            .expect("execute second eval ir");

        assert_eq!(values.get(first), FakeValue::Int(1));
        assert_eq!(values.get(second), FakeValue::Int(1));
    }

    /// Verifies `global` declarations read and write the context global scope.
    #[test]
    fn execute_program_global_alias_writes_context_global_scope() {
        let program =
            parse_fragment(br#"global $g; $g = $g + 1; return $g;"#).expect("parse eval fragment");
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let mut global_scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let initial = values.int(1).expect("allocate initial global");
        global_scope.set("g", initial, ScopeCellOwnership::Owned);
        context.set_global_scope(&mut global_scope);

        let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
            .expect("execute eval ir");

        let global = global_scope
            .visible_cell("g")
            .expect("global scope should contain g");
        assert_eq!(values.get(result), FakeValue::Int(2));
        assert_eq!(values.get(global), FakeValue::Int(2));
    }

    /// Verifies references to global aliases write the source global variable.
    #[test]
    fn execute_program_reference_alias_to_global_updates_source_global() {
        let program = parse_fragment(br#"global $g; $alias =& $g; $alias = 4; return $g;"#)
            .expect("parse eval fragment");
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let mut global_scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let initial = values.int(1).expect("allocate initial global");
        global_scope.set("g", initial, ScopeCellOwnership::Owned);
        context.set_global_scope(&mut global_scope);

        let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
            .expect("execute eval ir");

        let global = global_scope
            .visible_cell("g")
            .expect("global scope should contain g");
        assert_eq!(values.get(result), FakeValue::Int(4));
        assert_eq!(values.get(global), FakeValue::Int(4));
        assert!(global_scope.visible_cell("alias").is_none());
    }

    /// Verifies named calls reject positional arguments that follow named arguments.
    #[test]
    fn execute_program_rejects_positional_after_named_arg() {
        let program = parse_fragment(
            br#"function dyn($x, $y) { return $x + $y; } return dyn(x: 1, print "late");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values);

        assert_eq!(result, Err(EvalStatus::RuntimeFatal));
        assert_eq!(values.output, "");
    }

    /// Verifies named calls reject argument unpacking after named arguments.
    #[test]
    fn execute_program_rejects_spread_after_named_arg() {
        let program = parse_fragment(
            br#"function dyn($x, $y) { return $x + $y; } return dyn(x: 1, ...[2]);"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values);

        assert_eq!(result, Err(EvalStatus::RuntimeFatal));
    }

    /// Verifies function-scope magic constants keep the eval declaration spelling.
    #[test]
    fn execute_program_magic_function_and_method_use_eval_declared_name() {
        let program = parse_fragment(
            br#"function DynMagicCase() { return __FUNCTION__ . ":" . __METHOD__; } return dynmagiccase();"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(
            values.get(result),
            FakeValue::String("DynMagicCase:DynMagicCase".to_string())
        );
    }

    /// Verifies eval-declared functions persist in a shared eval context.
    #[test]
    fn execute_program_context_keeps_declared_function() {
        let define =
            parse_fragment(br#"function dyn($x) { return $x + 1; }"#).expect("parse eval fragment");
        let call = parse_fragment(br#"return dyn(4);"#).expect("parse eval fragment");
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let _ = execute_program_with_context(&mut context, &define, &mut scope, &mut values)
            .expect("execute eval ir");
        let result = execute_program_with_context(&mut context, &call, &mut scope, &mut values)
            .expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::Int(5));
    }

    /// Verifies `call_user_func` inside eval can dispatch an eval-declared function.
    #[test]
    fn execute_program_call_user_func_dispatches_declared_function() {
        let program = parse_fragment(
            br#"function dyn($x) { return $x + 1; }
return call_user_func("dyn", 4);"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::Int(5));
    }

    /// Verifies `call_user_func` inside eval can dispatch a supported builtin.
    #[test]
    fn execute_program_call_user_func_dispatches_builtin() {
        let program = parse_fragment(br#"return call_user_func("strlen", "abcd");"#)
            .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::Int(4));
    }

    /// Verifies `call_user_func` inside eval can dispatch a registered native function.
    #[test]
    fn execute_program_call_user_func_dispatches_registered_native_function() {
        let program = parse_fragment(br#"return call_user_func("native_answer");"#)
            .expect("parse eval fragment");
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let expected = values.int(42).expect("allocate fake result");
        let native =
            NativeFunction::new(expected.as_ptr().cast(), fake_native_return_descriptor, 0);
        assert!(context
            .define_native_function("native_answer", native)
            .is_ok());

        let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
            .expect("execute eval ir");

        assert_eq!(result, expected);
    }

    /// Verifies `call_user_func_array` inside eval can dispatch an eval-declared function.
    #[test]
    fn execute_program_call_user_func_array_dispatches_declared_function() {
        let program = parse_fragment(
            br#"function dyn($x, $y) { return $x + $y; }
return call_user_func_array("dyn", [4, 5]);"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::Int(9));
    }

    /// Verifies `call_user_func_array` string keys bind eval-declared parameters by name.
    #[test]
    fn execute_program_call_user_func_array_binds_declared_named_args() {
        let program = parse_fragment(
            br#"function dyn($x, $y) { return ($x * 10) + $y; }
return call_user_func_array("dyn", ["y" => 2, "x" => 1]);"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::Int(12));
    }

    /// Verifies context-level `call_user_func_array` dispatch binds eval-declared named args.
    #[test]
    fn execute_context_function_call_array_binds_declared_named_args() {
        let program = parse_fragment(br#"function dyn($x, $y) { return ($x * 10) + $y; }"#)
            .expect("parse eval fragment");
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let _ = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
            .expect("execute eval ir");
        let arg_array = values.assoc_new(2).expect("allocate argument array");
        let key_y = values.string("y").expect("allocate y key");
        let value_y = values.int(2).expect("allocate y value");
        let _ = values
            .array_set(arg_array, key_y, value_y)
            .expect("store y argument");
        let key_x = values.string("x").expect("allocate x key");
        let value_x = values.int(1).expect("allocate x value");
        let _ = values
            .array_set(arg_array, key_x, value_x)
            .expect("store x argument");

        let result =
            execute_context_function_call_array(&mut context, "dyn", arg_array, &mut values)
                .expect("execute context function call array");

        assert_eq!(values.get(result), FakeValue::Int(12));
    }

    /// Verifies `call_user_func_array` rejects positional values after named keys.
    #[test]
    fn execute_program_call_user_func_array_rejects_positional_after_named_arg() {
        let program = parse_fragment(
            br#"function dyn($x, $y) { return $x + $y; }
return call_user_func_array("dyn", ["y" => 2, 1]);"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values);

        assert_eq!(result, Err(EvalStatus::RuntimeFatal));
    }

    /// Verifies `call_user_func_array` inside eval can dispatch a supported builtin.
    #[test]
    fn execute_program_call_user_func_array_dispatches_builtin() {
        let program = parse_fragment(br#"return call_user_func_array("strlen", ["abcd"]);"#)
            .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::Int(4));
    }

    /// Verifies `call_user_func_array` inside eval can dispatch a registered native function.
    #[test]
    fn execute_program_call_user_func_array_dispatches_registered_native_function() {
        let program = parse_fragment(br#"return call_user_func_array("native_answer", [4, 5]);"#)
            .expect("parse eval fragment");
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let expected = values.int(42).expect("allocate fake result");
        let native =
            NativeFunction::new(expected.as_ptr().cast(), fake_native_return_descriptor, 2);
        assert!(context
            .define_native_function("native_answer", native)
            .is_ok());

        let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
            .expect("execute eval ir");

        assert_eq!(result, expected);
    }

    /// Verifies `call_user_func_array` named keys can bind registered native parameters.
    #[test]
    fn execute_program_call_user_func_array_binds_registered_native_named_args() {
        let program = parse_fragment(
            br#"return call_user_func_array("native_answer", ["right" => 2, "left" => 1]);"#,
        )
        .expect("parse eval fragment");
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let expected = values.int(42).expect("allocate fake result");
        let mut native =
            NativeFunction::new(expected.as_ptr().cast(), fake_native_return_descriptor, 2);
        assert!(native.set_param_name(0, "left"));
        assert!(native.set_param_name(1, "right"));
        assert!(context
            .define_native_function("native_answer", native)
            .is_ok());

        let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
            .expect("execute eval ir");

        assert_eq!(result, expected);
    }

    /// Verifies duplicate eval-declared function names fail in a shared context.
    #[test]
    fn execute_program_rejects_duplicate_declared_function() {
        let define =
            parse_fragment(br#"function dyn() { return 1; }"#).expect("parse eval fragment");
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let _ = execute_program_with_context(&mut context, &define, &mut scope, &mut values)
            .expect("execute first declaration");
        let err = execute_program_with_context(&mut context, &define, &mut scope, &mut values)
            .expect_err("duplicate function declaration should fail");

        assert_eq!(err, EvalStatus::RuntimeFatal);
    }

    /// Verifies dynamic builtin calls inside eval dispatch through runtime value hooks.
    #[test]
    fn execute_program_dispatches_simple_builtins() {
        let program = parse_fragment(br#"return strlen("abc") + count([1, 2, 3]);"#)
            .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::Int(6));
    }

    /// Verifies direct eval builtin calls bind named and unpacked arguments.
    #[test]
    fn execute_program_dispatches_named_and_spread_builtins() {
        let program = parse_fragment(
            br#"echo strlen(string: "abcd");
echo ":" . (array_key_exists(array: ["name" => 1], key: "name") ? "Y" : "N");
echo ":" . (str_contains(...["haystack" => "abc", "needle" => "b"]) ? "Y" : "N");
return round(precision: 1, num: 3.14);"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "4:Y:Y");
        assert_eq!(values.get(result), FakeValue::Float(3.1));
    }

    /// Verifies eval `ord()` returns the first byte and supports callable dispatch.
    #[test]
    fn execute_program_dispatches_ord_builtin() {
        let program = parse_fragment(
            br#"echo ord("A");
echo ":" . ord("");
echo ":" . call_user_func("ord", "B");
echo ":" . call_user_func_array("ord", ["C"]);
echo ":"; echo function_exists("ord");
return ord("Z");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "65:0:66:67:1");
        assert_eq!(values.get(result), FakeValue::Int(90));
    }

    /// Verifies eval array aggregate builtins iterate array values and support callable dispatch.
    #[test]
    fn execute_program_dispatches_array_aggregate_builtins() {
        let program = parse_fragment(
            br#"echo array_sum([1, 2, 3]);
echo ":" . array_product([2, 3, 4]);
echo ":" . array_sum([]);
echo ":" . array_product([]);
echo ":" . array_sum(["a" => 2, "b" => 5]);
echo ":" . call_user_func("array_sum", [3, 4]);
echo ":" . call_user_func_array("array_product", [[2, 5]]);
echo ":"; echo function_exists("array_sum");
return function_exists("array_product");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "6:24:0:1:7:7:10:1");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `array_combine()` converts key values through PHP string-key rules.
    #[test]
    fn execute_program_dispatches_array_combine_builtin() {
        let program = parse_fragment(
            br#"$pairs = array_combine(["a", "b"], [10, 20]);
echo $pairs["a"] . ":" . $pairs["b"];
$numeric = array_combine(["1", "01"], ["n", "z"]);
echo ":" . $numeric[1] . $numeric["01"];
$scalar = array_combine([null, true, false, 2.8], ["n", "t", "f", "d"]);
echo ":" . $scalar[""] . $scalar[1] . $scalar["2.8"];
$named = array_combine(keys: ["k"], values: ["v"]);
echo ":" . $named["k"];
$call = call_user_func("array_combine", ["x"], [7]);
echo ":" . $call["x"];
$spread = call_user_func_array("array_combine", [["y"], [8]]);
echo ":" . $spread["y"] . ":";
return function_exists("array_combine");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "10:20:nz:ftd:v:7:8:");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `array_column()` extracts present row columns and reindexes them.
    #[test]
    fn execute_program_dispatches_array_column_builtin() {
        let program = parse_fragment(
            br#"$rows = [["name" => "Ada", "score" => 10], ["score" => 20], ["name" => "Lin", "score" => 30], 42];
$names = array_column($rows, "name");
echo count($names) . ":" . $names[0] . ":" . $names[1];
$scores = array_column($rows, "score");
echo ":" . count($scores) . ":" . $scores[0] . $scores[2];
$numeric = array_column([[0 => "zero", 1 => "one"], [1 => "uno"]], 1);
echo ":" . count($numeric) . ":" . $numeric[0] . ":" . $numeric[1];
$named = array_column(array: $rows, column_key: "score");
echo ":" . $named[1];
$call = call_user_func("array_column", [["x" => 5], ["x" => 6]], "x");
echo ":" . $call[1];
$spread = call_user_func_array("array_column", [[["y" => 7], ["z" => 0], ["y" => 9]], "y"]);
echo ":" . count($spread) . ":" . $spread[1] . ":";
return function_exists("array_column");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "2:Ada:Lin:3:1030:2:one:uno:20:6:2:9:");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `array_pad()` and `array_chunk()` build reindexed array shapes.
    #[test]
    fn execute_program_dispatches_array_shape_builtins() {
        let program = parse_fragment(
            br#"$right = array_pad([1, 2], 5, 0);
echo count($right) . ":" . $right[0] . $right[1] . $right[2] . $right[4];
$left = array_pad([1, 2], -4, 9);
echo ":" . $left[0] . $left[1] . $left[2] . $left[3];
$copy = array_pad([7, 8], 1, 0);
echo ":" . count($copy) . ":" . $copy[0] . $copy[1];
$chunks = array_chunk([1, 2, 3, 4, 5], 2);
echo ":" . count($chunks) . ":" . $chunks[0][1] . $chunks[2][0];
$named = array_pad(array: ["a"], length: 2, value: "b");
echo ":" . $named[1];
$call = call_user_func("array_chunk", [6, 7, 8], 2);
echo ":" . $call[1][0];
$spread = call_user_func_array("array_pad", [[1], 3, 2]);
echo ":" . $spread[2] . ":";
return function_exists("array_pad") && function_exists("array_chunk");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "5:1200:9912:2:78:3:25:b:8:2:");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `array_slice()` observes PHP offset and length bounds.
    #[test]
    fn execute_program_dispatches_array_slice_builtin() {
        let program = parse_fragment(
            br#"$mid = array_slice([10, 20, 30, 40, 50], 1, 3);
echo count($mid) . ":" . $mid[0] . $mid[1] . $mid[2];
$tail = array_slice([10, 20, 30, 40], -2, 1);
echo ":" . $tail[0];
$open = array_slice([10, 20, 30, 40, 50], 2);
echo ":" . count($open) . ":" . $open[0] . $open[2];
$null_len = array_slice([5, 6, 7], 1, null);
echo ":" . $null_len[0] . $null_len[1];
$negative_len = array_slice([10, 20, 30, 40, 50], 1, -1);
echo ":" . count($negative_len) . ":" . $negative_len[0] . $negative_len[2];
$named = array_slice(array: [1, 2, 3], offset: 1, length: 1);
echo ":" . $named[0];
$call = call_user_func("array_slice", [6, 7, 8], 1, 2);
echo ":" . $call[1];
$spread = call_user_func_array("array_slice", [[9, 10, 11], 1]);
echo ":" . $spread[0] . ":";
return function_exists("array_slice");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "3:203040:30:3:3050:67:3:2040:2:8:10:");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `array_merge()` appends numeric keys and overwrites string keys.
    #[test]
    fn execute_program_dispatches_array_merge_builtin() {
        let program = parse_fragment(
            br#"$merged = array_merge([1, 2], [3, 4]);
echo count($merged) . ":" . $merged[0] . $merged[1] . $merged[2] . $merged[3];
$left = [1, 2];
$right = [3];
$copy = array_merge($left, $right);
echo ":" . count($left) . ":" . $left[0] . ":" . $copy[2];
$assoc = array_merge(["a" => 1, 2 => "x"], ["a" => 9, 5 => "y", "b" => 3]);
echo ":" . $assoc["a"] . ":" . $assoc[0] . ":" . $assoc[1] . ":" . $assoc["b"];
$call = call_user_func("array_merge", [6], [7, 8]);
echo ":" . $call[2];
$spread = call_user_func_array("array_merge", [[9], [10]]);
echo ":" . $spread[1] . ":";
return function_exists("array_merge");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "4:1234:2:1:3:9:x:y:3:8:10:");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `array_diff()` and `array_intersect()` compare values as strings.
    #[test]
    fn execute_program_dispatches_array_value_set_builtins() {
        let program = parse_fragment(
            br#"$diff = array_diff(["a" => 1, "b" => 2, "c" => "2", "d" => 3], [2]);
echo count($diff) . ":" . $diff["a"] . ":" . $diff["d"];
echo ":" . (array_key_exists("b", $diff) ? "bad" : "no-b");
echo ":" . (array_key_exists("c", $diff) ? "bad" : "no-c");
$inter = array_intersect(["a" => 1, "b" => 2, "c" => "2", "d" => 3], ["2", 4]);
echo ":" . count($inter) . ":" . $inter["b"] . ":" . $inter["c"];
$call = call_user_func("array_diff", [1, 2, 3], [2]);
echo ":" . count($call) . ":" . $call[0] . $call[2];
$spread = call_user_func_array("array_intersect", [[1, 2, 3], [3]]);
echo ":" . count($spread) . ":" . $spread[2] . ":";
return function_exists("array_diff") && function_exists("array_intersect");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "2:1:3:no-b:no-c:2:2:2:2:13:1:3:");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `array_diff_key()` and `array_intersect_key()` preserve first-array keys.
    #[test]
    fn execute_program_dispatches_array_key_set_builtins() {
        let program = parse_fragment(
            br#"$diff = array_diff_key(["a" => 1, "b" => 2, 4 => 3], ["a" => 0, 5 => 0]);
echo count($diff) . ":" . $diff["b"] . ":" . $diff[4];
echo ":" . (array_key_exists("a", $diff) ? "bad" : "no-a");
$inter = array_intersect_key(["a" => 1, "b" => 2, 4 => 3], ["b" => 0, 4 => 0]);
echo ":" . count($inter) . ":" . $inter["b"] . ":" . $inter[4];
$call = call_user_func("array_diff_key", [10, 20, 30], [1 => 0]);
echo ":" . count($call) . ":" . $call[0] . $call[2];
$spread = call_user_func_array("array_intersect_key", [["x" => 7, "y" => 8], ["y" => 0]]);
echo ":" . count($spread) . ":" . $spread["y"] . ":";
return function_exists("array_diff_key") && function_exists("array_intersect_key");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "2:2:3:no-a:2:2:3:2:1030:1:8:");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `range()` builds inclusive ascending and descending integer arrays.
    #[test]
    fn execute_program_dispatches_range_builtin() {
        let program = parse_fragment(
            br#"$up = range(1, 4);
echo count($up) . ":" . $up[0] . $up[3];
$down = range(4, 1);
echo ":" . count($down) . ":" . $down[0] . $down[3];
$single = range(3, 3);
echo ":" . count($single) . ":" . $single[0];
$named = range(start: 2, end: 4);
echo ":" . $named[0] . $named[2];
$call = call_user_func("range", 5, 7);
echo ":" . $call[2];
$spread = call_user_func_array("range", [8, 6]);
echo ":" . count($spread) . ":" . $spread[0] . $spread[2] . ":";
return function_exists("range");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "4:14:4:41:1:3:24:7:3:86:");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `array_rand()` returns a key that exists in the source array.
    #[test]
    fn execute_program_dispatches_array_rand_builtin() {
        let program = parse_fragment(
            br#"$nums = [10, 20, 30];
$idx = array_rand($nums);
echo ($idx >= 0 && $idx < 3 && array_key_exists($idx, $nums)) ? "idx" : "bad";
$assoc = ["a" => 1, "b" => 2];
$key = array_rand($assoc);
echo ":" . (array_key_exists($key, $assoc) ? "assoc" : "bad");
$named = array_rand(array: [5, 6]);
echo ":" . (($named >= 0 && $named < 2) ? "named" : "bad");
$call = call_user_func("array_rand", [7, 8]);
echo ":" . (($call >= 0 && $call < 2) ? "call" : "bad");
$spread = call_user_func_array("array_rand", [["x" => 1, "y" => 2]]);
echo ":" . (array_key_exists($spread, ["x" => 1, "y" => 2]) ? "spread" : "bad") . ":";
return function_exists("array_rand");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "idx:assoc:named:call:spread:");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `rand()` and `mt_rand()` return values inside PHP inclusive ranges.
    #[test]
    fn execute_program_dispatches_rand_builtins() {
        let program = parse_fragment(
            br#"$plain = rand();
echo ($plain >= 0 && $plain <= 2147483647) ? "plain" : "bad";
$bounded = rand(2, 4);
echo ":" . (($bounded >= 2 && $bounded <= 4) ? "range" : "bad");
$same = mt_rand(max: 6, min: 6);
echo ":" . ($same === 6 ? "same" : "bad");
$swapped = rand(10, 1);
echo ":" . (($swapped >= 1 && $swapped <= 10) ? "swap" : "bad");
$call = call_user_func("mt_rand", 1, 1);
echo ":" . ($call === 1 ? "call" : "bad");
$spread = call_user_func_array("rand", ["min" => 3, "max" => 3]);
echo ":" . ($spread === 3 ? "spread" : "bad") . ":";
echo function_exists("rand");
return function_exists("mt_rand");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "plain:range:same:swap:call:spread:1");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `array_fill()` and `array_fill_keys()` create arrays with PHP key rules.
    #[test]
    fn execute_program_dispatches_array_fill_builtins() {
        let program = parse_fragment(
            br#"$filled = array_fill(2, 3, "x");
echo count($filled) . ":" . $filled[2] . $filled[4];
$negative = array_fill(-2, 3, 7);
echo ":" . $negative[-2] . $negative[-1] . $negative[0];
$empty = array_fill(5, 0, "x");
echo ":" . count($empty);
$map = array_fill_keys(["a", "1", "01"], 8);
echo ":" . $map["a"] . ":" . $map[1] . ":" . $map["01"];
$named = array_fill(start_index: 1, count: 2, value: "n");
echo ":" . $named[1] . $named[2];
$call = call_user_func("array_fill", 0, 2, "c");
echo ":" . $call[0] . $call[1];
$spread = call_user_func_array("array_fill_keys", [["x", "y"], "z"]);
echo ":" . $spread["x"] . $spread["y"] . ":";
return function_exists("array_fill") && function_exists("array_fill_keys");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "3:xx:777:0:8:8:8:nn:cc:zz:");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `array_flip()` swaps valid values into PHP-normalized keys.
    #[test]
    fn execute_program_dispatches_array_flip_builtin() {
        let program = parse_fragment(
            br#"$flipped = array_flip(["a" => "x", "b" => "y", "c" => "x", "d" => 1, "e" => "01", "skip" => null, "truth" => true]);
echo $flipped["x"] . ":" . $flipped["y"] . ":" . $flipped[1] . ":" . $flipped["01"] . ":" . count($flipped);
$named = array_flip(array: ["k" => "v"]);
echo ":" . $named["v"];
$call = call_user_func("array_flip", ["left" => "right"]);
echo ":" . $call["right"];
$spread = call_user_func_array("array_flip", [["n" => 9]]);
echo ":" . $spread[9] . ":";
return function_exists("array_flip");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "c:b:d:e:4:k:left:n:");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `array_unique()` preserves first keys using default string comparison.
    #[test]
    fn execute_program_dispatches_array_unique_builtin() {
        let program = parse_fragment(
            br#"$unique = array_unique(["a", "b", "a", "2", 2]);
echo count($unique) . ":" . $unique[0] . $unique[1] . $unique[3];
$assoc = array_unique(["x" => "a", "y" => "b", "z" => "a"]);
echo ":" . count($assoc) . ":" . $assoc["x"] . $assoc["y"];
$scalar = array_unique([1, "1", 1.0, true, false, null, ""]);
echo ":" . count($scalar) . ":" . $scalar[0] . ":";
echo $scalar[4] ? "bad" : "F";
$named = array_unique(array: ["k" => "v", "l" => "v"]);
echo ":" . $named["k"] . ":" . count($named);
$call = call_user_func("array_unique", ["q", "q", "r"]);
echo ":" . $call[0] . $call[2];
$spread = call_user_func_array("array_unique", [["s", "s", "t"]]);
echo ":" . $spread[0] . $spread[2] . ":";
return function_exists("array_unique");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "3:ab2:2:ab:2:1:F:v:1:qr:st:");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval array projection builtins produce indexed key/value arrays.
    #[test]
    fn execute_program_dispatches_array_projection_builtins() {
        let program = parse_fragment(
            br#"$values = array_values(["a" => 10, "b" => 20]);
echo $values[0] . ":" . $values[1];
$keys = array_keys(["a" => 10, "b" => 20]);
echo ":" . $keys[0] . ":" . $keys[1];
echo ":" . count(array_values([]));
$call_keys = call_user_func("array_keys", ["z" => 7]);
echo ":" . $call_keys[0];
$call_values = call_user_func_array("array_values", [["q" => 8]]);
echo ":" . $call_values[0];
echo ":"; echo function_exists("array_keys");
return function_exists("array_values");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "10:20:a:b:0:z:8:1");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `array_reverse()` handles PHP key preservation rules.
    #[test]
    fn execute_program_dispatches_array_reverse_builtin() {
        let program = parse_fragment(
            br#"$indexed = array_reverse([1, 2, 3]);
echo $indexed[0]; echo $indexed[1]; echo $indexed[2]; echo ":";
$mixed = array_reverse([2 => "a", "k" => "b", 5 => "c"]);
echo $mixed[0]; echo $mixed["k"]; echo $mixed[1]; echo ":";
$preserved = array_reverse([2 => "a", "k" => "b", 5 => "c"], true);
echo $preserved[5]; echo $preserved["k"]; echo $preserved[2]; echo ":";
$named = array_reverse(array: ["x", "y"], preserve_keys: true);
echo $named[1]; echo $named[0]; echo ":";
$call = call_user_func("array_reverse", [4, 5]);
echo $call[0]; echo $call[1]; echo ":";
$spread = call_user_func_array("array_reverse", [[6, 7]]);
echo $spread[0]; echo $spread[1]; echo ":";
return function_exists("array_reverse");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "321:cba:cba:yx:54:76:");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `array_key_exists()` distinguishes present null values from missing keys.
    #[test]
    fn execute_program_dispatches_array_key_exists_builtin() {
        let program = parse_fragment(
            br#"$map = ["name" => null, "age" => 30];
echo array_key_exists("name", $map) ? "Y" : "N"; echo ":";
echo array_key_exists("missing", $map) ? "bad" : "N"; echo ":";
echo array_key_exists(1, [10, null]) ? "Y" : "N"; echo ":";
echo array_key_exists(2, [10, null]) ? "bad" : "N"; echo ":";
echo call_user_func("array_key_exists", "age", $map) ? "Y" : "N"; echo ":";
echo call_user_func_array("array_key_exists", ["age", $map]) ? "Y" : "N"; echo ":";
return function_exists("array_key_exists");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "Y:N:Y:N:Y:Y:");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval array search builtins use loose comparison and return keys or booleans.
    #[test]
    fn execute_program_dispatches_array_search_builtins() {
        let program = parse_fragment(
            br#"echo in_array(2, [1, 2, 3]) ? "Y" : "bad";
echo ":"; echo in_array(4, [1, 2, 3]) ? "bad" : "N";
echo ":" . array_search(20, [10, 20, 30]);
echo ":" . array_search("Grace", ["name" => "Grace"]);
echo ":"; echo array_search("x", ["name" => "Grace"]) === false ? "F" : "bad";
echo ":"; echo call_user_func("in_array", "b", ["a", "b"]) ? "C" : "bad";
$found = call_user_func_array("array_search", ["v", ["k" => "v"]]);
echo ":" . $found;
echo ":"; echo function_exists("in_array");
return function_exists("array_search");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "Y:N:1:name:F:C:k:1");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `explode()` and `implode()` bridge byte strings and arrays.
    #[test]
    fn execute_program_dispatches_explode_implode_builtins() {
        let program = parse_fragment(
            br#"$parts = explode(",", "a,b,");
echo count($parts); echo ":" . $parts[0] . ":" . $parts[1] . ":" . $parts[2];
echo ":" . implode("|", $parts);
echo ":" . implode(separator: "-", array: ["x", 2, true, null]);
$call_parts = call_user_func("explode", ":", "m:n");
echo ":" . $call_parts[1];
echo ":" . call_user_func_array("implode", ["separator" => "/", "array" => ["p", "q"]]);
echo ":"; echo function_exists("explode");
return function_exists("implode");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "3:a:b::a|b|:x-2-1-:n:p/q:1");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `str_split()` builds indexed arrays of fixed-width chunks.
    #[test]
    fn execute_program_dispatches_str_split_builtin() {
        let program = parse_fragment(
            br#"$letters = str_split("abc");
echo count($letters) . ":" . $letters[0] . $letters[1] . $letters[2]; echo ":";
$pairs = str_split(string: "abcd", length: 2);
echo $pairs[0] . "-" . $pairs[1]; echo ":";
$empty = str_split("");
echo count($empty); echo ":";
$call = call_user_func("str_split", "xyz", 2);
echo $call[0] . "-" . $call[1]; echo ":";
$named = call_user_func_array("str_split", ["string" => "pqrs", "length" => 3]);
echo $named[0] . "-" . $named[1]; echo ":";
return function_exists("str_split");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "3:abc:ab-cd:0:xy-z:pqr-s:");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `str_pad()` supports PHP left, right, and both-side padding modes.
    #[test]
    fn execute_program_dispatches_str_pad_builtin() {
        let program = parse_fragment(
            br#"echo "[" . str_pad("hi", 5) . "]"; echo ":";
echo "[" . str_pad(string: "hi", length: 5, pad_string: "_", pad_type: 0) . "]"; echo ":";
echo "[" . str_pad("x", 6, "ab", 2) . "]"; echo ":";
echo call_user_func("str_pad", "42", 5, "0", 0); echo ":";
echo call_user_func_array("str_pad", ["string" => "x", "length" => 3, "pad_string" => "."]); echo ":";
return function_exists("str_pad");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "[hi   ]:[___hi]:[abxaba]:00042:x..:");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval string replacement builtins support direct, named, and callable dispatch.
    #[test]
    fn execute_program_dispatches_string_replace_builtins() {
        let program = parse_fragment(
            br#"echo str_replace("o", "0", "Hello World"); echo ":";
echo str_replace(search: "aa", replace: "b", subject: "aaaa"); echo ":";
echo str_replace("", "x", "abc"); echo ":";
echo str_ireplace("HE", "ye", "Hello he"); echo ":";
echo call_user_func("str_replace", "l", "L", "hello"); echo ":";
echo call_user_func_array("str_ireplace", ["search" => "x", "replace" => "Y", "subject" => "xX"]); echo ":";
echo function_exists("str_replace");
return function_exists("str_ireplace");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "Hell0 W0rld:bb:abc:yello ye:heLLo:YY:1");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval HTML entity builtins encode, decode, and dispatch as callables.
    #[test]
    fn execute_program_dispatches_html_entity_builtins() {
        let program = parse_fragment(
            br#"echo htmlspecialchars("<b>\"Hi\" & 'bye'</b>"); echo ":";
echo htmlentities(string: "<a>"); echo ":";
echo html_entity_decode("&lt;b&gt;hi&lt;/b&gt;"); echo ":";
echo call_user_func("htmlspecialchars", "<x>"); echo ":";
echo call_user_func_array("html_entity_decode", ["string" => "&quot;q&quot;"]); echo ":";
echo function_exists("htmlspecialchars"); echo function_exists("htmlentities");
return function_exists("html_entity_decode");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(
            values.output,
            "&lt;b&gt;&quot;Hi&quot; &amp; &#039;bye&#039;&lt;/b&gt;:&lt;a&gt;:<b>hi</b>:&lt;x&gt;:\"q\":11"
        );
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval URL codec builtins dispatch through direct, named, and callable paths.
    #[test]
    fn execute_program_dispatches_url_codec_builtins() {
        let program = parse_fragment(
            br#"echo urlencode("a b&=~"); echo ":";
echo rawurlencode(string: "a b&=~"); echo ":";
echo urldecode("a+b%26%3D%7E"); echo ":";
echo rawurldecode("a+b%26%3D%7E"); echo ":";
echo call_user_func("urlencode", "%zz"); echo ":";
echo call_user_func_array("rawurldecode", ["string" => "x%2By%zz"]); echo ":";
echo function_exists("urlencode"); echo function_exists("rawurlencode");
echo function_exists("urldecode");
return function_exists("rawurldecode");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(
            values.output,
            "a+b%26%3D%7E:a%20b%26%3D~:a b&=~:a+b&=~:%25zz:x+y%zz:111"
        );
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `ctype_*` predicates dispatch through direct, named, and callable paths.
    #[test]
    fn execute_program_dispatches_ctype_builtins() {
        let program = parse_fragment(
            br#"echo ctype_alpha("abc") ? "A" : "-"; echo ":";
echo ctype_digit(text: "123") ? "D" : "-"; echo ":";
echo ctype_alnum("a1") ? "N" : "-"; echo ":";
echo ctype_space(" \t\n" . chr(11) . chr(12) . "\r") ? "S" : "-"; echo ":";
echo ctype_alpha("") ? "bad" : "empty"; echo ":";
echo call_user_func("ctype_digit", "12x") ? "bad" : "not-digit"; echo ":";
echo call_user_func_array("ctype_space", ["text" => " x"]) ? "bad" : "not-space"; echo ":";
echo function_exists("ctype_alpha"); echo function_exists("ctype_digit");
echo function_exists("ctype_alnum");
return function_exists("ctype_space");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(
            values.output,
            "A:D:N:S:empty:not-digit:not-space:111"
        );
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `crc32()` returns PHP-compatible non-negative checksums.
    #[test]
    fn execute_program_dispatches_crc32_builtin() {
        let program = parse_fragment(
            br#"echo crc32(""); echo ":";
echo crc32(string: "123456789"); echo ":";
echo call_user_func("crc32", "hello"); echo ":";
echo call_user_func_array("crc32", ["string" => "The quick brown fox jumps over the lazy dog"]); echo ":";
return function_exists("crc32");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "0:3421780262:907060870:1095738169:");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `hash_algos()` returns supported hash names through callable dispatch too.
    #[test]
    fn execute_program_dispatches_hash_algos_builtin() {
        let program = parse_fragment(
            br#"$algos = hash_algos();
echo count($algos) . ":" . $algos[0] . ":" . $algos[5] . ":";
echo in_array("crc32c", $algos) ? "crc" : "bad";
$call = call_user_func("hash_algos");
echo ":" . $call[18];
$spread = call_user_func_array("hash_algos", []);
echo ":" . $spread[27] . ":";
echo function_exists("hash_algos") ? "exists" : "missing";
return count($algos);"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(
            values.output,
            "28:md2:sha256:crc:whirlpool:joaat:exists"
        );
        assert_eq!(values.get(result), FakeValue::Int(28));
    }

    /// Verifies eval one-shot hash digest builtins use the crypto bridge and dispatch dynamically.
    #[test]
    fn execute_program_dispatches_hash_digest_builtins() {
        let program = parse_fragment(
            br#"echo md5("abc"); echo ":";
echo sha1(string: "abc"); echo ":";
echo hash("sha256", "abc"); echo ":";
echo hash_hmac(algo: "sha256", data: "data", key: "key"); echo ":";
echo bin2hex(md5("abc", true)); echo ":";
echo bin2hex(call_user_func("sha1", "abc", true)); echo ":";
echo call_user_func_array("hash", ["algo" => "md5", "data" => "abc"]); echo ":";
echo call_user_func_array("hash_hmac", ["algo" => "sha256", "data" => "data", "key" => "key"]); echo ":";
echo function_exists("md5"); echo function_exists("sha1"); echo function_exists("hash");
return function_exists("hash_hmac");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(
            values.output,
            concat!(
                "900150983cd24fb0d6963f7d28e17f72:",
                "a9993e364706816aba3e25717850c26c9cd0d89d:",
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad:",
                "5031fe3d989c6d1537a013fa6e739da23463fdaec3b70137d828e36ace221bd0:",
                "900150983cd24fb0d6963f7d28e17f72:",
                "a9993e364706816aba3e25717850c26c9cd0d89d:",
                "900150983cd24fb0d6963f7d28e17f72:",
                "5031fe3d989c6d1537a013fa6e739da23463fdaec3b70137d828e36ace221bd0:",
                "111"
            )
        );
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval zero-argument system builtins return native-compatible values.
    #[test]
    fn execute_program_dispatches_zero_arg_system_builtins() {
        let program = parse_fragment(
            br#"echo time() > 1000000000 ? "time" : "bad"; echo ":";
echo phpversion(); echo ":";
echo sys_get_temp_dir(); echo ":";
echo strlen(getcwd()) > 0 ? "cwd" : "bad"; echo ":";
echo call_user_func("time") > 1000000000 ? "call-time" : "bad"; echo ":";
echo call_user_func("phpversion"); echo ":";
echo call_user_func_array("getcwd", []) !== "" ? "call-cwd" : "bad"; echo ":";
echo call_user_func_array("sys_get_temp_dir", []); echo ":";
echo function_exists("time"); echo function_exists("phpversion"); echo function_exists("getcwd");
return function_exists("sys_get_temp_dir");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(
            values.output,
            format!(
                "time:{}:/tmp:cwd:call-time:{}:call-cwd:/tmp:111",
                eval_compiler_php_version(),
                eval_compiler_php_version()
            )
        );
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `date()` formats libc local timestamps and `mktime()` builds them.
    #[test]
    fn execute_program_dispatches_date_mktime_builtins() {
        let program = parse_fragment(
            br#"$ts = mktime(13, 2, 3, 1, 2, 2024);
echo date("Y-m-d H:i:s", $ts);
echo ":" . date("j-n-G-g-A-a-N-D-M-l-F", $ts);
echo ":" . (date("U", $ts) === strval($ts) ? "U" : "bad");
echo ":" . call_user_func("date", "Y", $ts);
$named = call_user_func_array("mktime", ["hour" => 0, "minute" => 0, "second" => 0, "month" => 1, "day" => 1, "year" => 2000]);
echo ":" . date(format: "Y", timestamp: $named);
echo ":"; echo function_exists("date");
return function_exists("mktime");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(
            values.output,
            "2024-01-02 13:02:03:2-1-13-1-PM-pm-2-Tue-Jan-Tuesday-January:U:2024:2000:1"
        );
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `strtotime()` parses supported ISO date strings and rejects others.
    #[test]
    fn execute_program_dispatches_strtotime_builtin() {
        let program = parse_fragment(
            br#"$date = strtotime("2024-06-15");
echo date("Y-m-d H:i:s", $date);
$full = strtotime("2024-06-15 12:30:45");
echo ":" . date("Y-m-d H:i:s", $full);
$short = strtotime("2024-06-15T12:30");
echo ":" . date("Y-m-d H:i:s", $short);
echo ":" . (strtotime("2024/06/15") === -1 ? "bad" : "wrong");
$call = call_user_func("strtotime", "2024-01-02 03:04:05");
echo ":" . date("Y-m-d H:i:s", $call);
$spread = call_user_func_array("strtotime", ["datetime" => "2024-01-02"]);
echo ":" . date("Y-m-d", $spread) . ":";
return function_exists("strtotime");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(
            values.output,
            "2024-06-15 00:00:00:2024-06-15 12:30:45:2024-06-15 12:30:00:bad:2024-01-02 03:04:05:2024-01-02:"
        );
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `microtime()` returns a plausible float timestamp by all call paths.
    #[test]
    fn execute_program_dispatches_microtime_builtin() {
        let program = parse_fragment(
            br#"echo microtime() > 1000000000 ? "now" : "bad"; echo ":";
echo microtime(as_float: false) > 1000000000 ? "named" : "bad"; echo ":";
echo call_user_func("microtime", true) > 1000000000 ? "call" : "bad"; echo ":";
echo call_user_func_array("microtime", ["as_float" => true]) > 1000000000 ? "array" : "bad";
echo ":";
return function_exists("microtime");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "now:named:call:array:");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval realpath-cache stubs match elephc's empty-cache runtime view.
    #[test]
    fn execute_program_dispatches_realpath_cache_builtins() {
        let program = parse_fragment(
            br#"$cache = realpath_cache_get();
echo count($cache) . ":" . realpath_cache_size() . ":";
$call_cache = call_user_func("realpath_cache_get");
echo count($call_cache) . ":";
echo call_user_func_array("realpath_cache_size", []) . ":";
echo function_exists("realpath_cache_get");
return function_exists("realpath_cache_size");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "0:0:0:0:1");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval stream introspection builtins return native-compatible static lists.
    #[test]
    fn execute_program_dispatches_stream_introspection_builtins() {
        let program = parse_fragment(
            br#"$wrappers = stream_get_wrappers();
$transports = stream_get_transports();
$filters = stream_get_filters();
echo count($wrappers) . ":" . $wrappers[0] . ":" . $wrappers[5] . ":";
echo count($transports) . ":" . $transports[0] . ":" . $transports[8] . ":";
echo count($filters) . ":" . $filters[2] . ":";
$call_wrappers = call_user_func("stream_get_wrappers");
echo $call_wrappers[10] . ":";
$call_transports = call_user_func_array("stream_get_transports", []);
echo $call_transports[11] . ":";
$call_filters = call_user_func_array("stream_get_filters", []);
echo $call_filters[13] . ":";
echo function_exists("stream_get_wrappers"); echo function_exists("stream_get_transports");
return function_exists("stream_get_filters");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(
            values.output,
            "11:file:https:12:tcp:tlsv1.0:14:string.rot13:glob:tlsv1.3:bzip2.decompress:11"
        );
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `spl_classes()` returns the native-compatible SPL type snapshot.
    #[test]
    fn execute_program_dispatches_spl_classes_builtin() {
        let program = parse_fragment(
            br#"$names = spl_classes();
echo count($names) . ":" . $names[0] . ":" . $names[55] . ":";
echo (in_array("Exception", $names) ? "exception" : "bad") . ":";
echo (in_array("SplDoublyLinkedList", $names) ? "list" : "bad") . ":";
$call = call_user_func("spl_classes");
echo (in_array("Throwable", $call) ? "call" : "bad") . ":";
$spread = call_user_func_array("spl_classes", []);
echo (count($spread) === count($names) ? "spread" : "bad") . ":";
echo function_exists("spl_classes");
return is_callable("spl_classes");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(
            values.output,
            "61:AppendIterator:Throwable:exception:list:call:spread:1"
        );
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval environment builtins read, write, unset, and dispatch dynamically.
    #[test]
    fn execute_program_dispatches_environment_builtins() {
        let program = parse_fragment(
            br#"putenv("ELEPHC_EVAL_ENV_TEST=direct");
echo getenv("ELEPHC_EVAL_ENV_TEST") . ":";
putenv(assignment: "ELEPHC_EVAL_ENV_TEST=named");
echo getenv(name: "ELEPHC_EVAL_ENV_TEST") . ":";
echo call_user_func("getenv", "ELEPHC_EVAL_ENV_TEST") . ":";
echo call_user_func_array("putenv", ["assignment" => "ELEPHC_EVAL_ENV_TEST=spread"]) ? "set" : "bad";
echo ":" . getenv("ELEPHC_EVAL_ENV_TEST") . ":";
putenv("ELEPHC_EVAL_ENV_TEST");
echo getenv("ELEPHC_EVAL_ENV_TEST") === "" ? "empty" : "bad";
echo ":"; echo function_exists("getenv");
return function_exists("putenv");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "direct:named:named:set:spread:empty:1");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval sleep builtins dispatch without delaying focused tests.
    #[test]
    fn execute_program_dispatches_sleep_builtins() {
        let program = parse_fragment(
            br#"echo sleep(0) . ":";
echo sleep(seconds: 0) . ":";
usleep(0);
echo "u:";
echo call_user_func("sleep", 0) . ":";
echo call_user_func_array("usleep", ["microseconds" => 0]) === null ? "null" : "bad";
echo ":"; echo function_exists("sleep");
return function_exists("usleep");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "0:0:u:0:null:1");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `php_uname()` dispatches default, named, mode, and callable calls.
    #[test]
    fn execute_program_dispatches_php_uname_builtin() {
        let program = parse_fragment(
            br#"echo strlen(php_uname()) > 0 ? "all" : "empty"; echo ":";
echo php_uname() === php_uname("a") ? "same" : "different"; echo ":";
echo strlen(php_uname(mode: "s")) > 0 ? "sys" : "empty"; echo ":";
echo strlen(php_uname("n")) > 0 ? "node" : "empty"; echo ":";
echo strlen(php_uname("r")) > 0 ? "release" : "empty"; echo ":";
echo strlen(php_uname("v")) > 0 ? "version" : "empty"; echo ":";
echo strlen(php_uname("m")) > 0 ? "machine" : "empty"; echo ":";
echo strlen(call_user_func("php_uname", "m")) > 0 ? "call" : "empty"; echo ":";
echo strlen(call_user_func_array("php_uname", ["mode" => "n"])) > 0 ? "spread" : "empty"; echo ":";
return function_exists("php_uname");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(
            values.output,
            "all:same:sys:node:release:version:machine:call:spread:"
        );
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `gethostbyname()` handles IPv4 literals and failed lookups.
    #[test]
    fn execute_program_dispatches_gethostbyname_builtin() {
        let program = parse_fragment(
            br#"echo gethostbyname("127.0.0.1") . ":";
echo gethostbyname(hostname: "not a host") . ":";
echo call_user_func("gethostbyname", "127.0.0.1") . ":";
echo call_user_func_array("gethostbyname", ["hostname" => "not a host"]) . ":";
return function_exists("gethostbyname");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "127.0.0.1:not a host:127.0.0.1:not a host:");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `gethostname()` dispatches direct and callable zero-arg calls.
    #[test]
    fn execute_program_dispatches_gethostname_builtin() {
        let program = parse_fragment(
            br#"echo strlen(gethostname()) > 0 ? "host" : "empty"; echo ":";
echo strlen(call_user_func("gethostname")) > 0 ? "call" : "empty"; echo ":";
echo strlen(call_user_func_array("gethostname", [])) > 0 ? "spread" : "empty"; echo ":";
return function_exists("gethostname");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "host:call:spread:");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `gethostbyaddr()` handles valid, malformed, and callable calls.
    #[test]
    fn execute_program_dispatches_gethostbyaddr_builtin() {
        let program = parse_fragment(
            br#"echo strlen(gethostbyaddr("127.0.0.1")) > 0 ? "direct" : "empty"; echo ":";
echo strlen(gethostbyaddr(ip: "127.0.0.1")) > 0 ? "named" : "empty"; echo ":";
echo gethostbyaddr("not-an-ip-address") === false ? "false" : "bad"; echo ":";
echo strlen(call_user_func("gethostbyaddr", "127.0.0.1")) > 0 ? "call" : "empty"; echo ":";
echo call_user_func_array("gethostbyaddr", ["ip" => "not-an-ip-address"]) === false ? "spread" : "bad"; echo ":";
return function_exists("gethostbyaddr");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "direct:named:false:call:spread:");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval protocol and service database lookups dispatch dynamically.
    #[test]
    fn execute_program_dispatches_protocol_service_builtins() {
        let program = parse_fragment(
            br#"echo getprotobyname("TCP") . ":";
echo getprotobynumber(6) . ":";
echo getprotobyname("no_such_protocol") === false ? "missing-proto" : "bad"; echo ":";
echo getprotobynumber(999) === false ? "missing-number" : "bad"; echo ":";
echo getservbyname("www", "tcp") . ":";
echo getservbyport(80, "tcp") . ":";
echo getservbyname("no_such_service", "tcp") === false ? "missing-service" : "bad"; echo ":";
echo getservbyport(80, "no_such_proto") === false ? "missing-port" : "bad"; echo ":";
echo call_user_func("getprotobyname", "udp") . ":";
echo call_user_func_array("getprotobynumber", ["protocol" => 17]) . ":";
echo call_user_func("getservbyname", "https", "tcp") . ":";
echo call_user_func_array("getservbyport", ["port" => 443, "protocol" => "tcp"]) . ":";
echo function_exists("getprotobyname"); echo function_exists("getprotobynumber"); echo function_exists("getservbyname");
return function_exists("getservbyport");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(
            values.output,
            "6:tcp:missing-proto:missing-number:80:http:missing-service:missing-port:17:udp:443:https:111"
        );
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval IPv4 conversion builtins handle scalar and raw-byte paths.
    #[test]
    fn execute_program_dispatches_ip_conversion_builtins() {
        let program = parse_fragment(
            br#"echo long2ip(3232235777) . ":";
echo long2ip(ip: 4294967295) . ":";
echo ip2long("192.168.1.1") . ":";
echo ip2long(ip: "1.2.3") === false ? "bad-ip" : "bad"; echo ":";
$packed = inet_pton("1.2.3.4");
echo bin2hex($packed) . ":";
echo inet_pton(ip: "nonsense") === false ? "bad-pton" : "bad"; echo ":";
echo inet_ntop($packed) . ":";
echo inet_ntop(ip: "xx") === false ? "bad-ntop" : "bad"; echo ":";
echo call_user_func("long2ip", 2130706433) . ":";
echo call_user_func_array("ip2long", ["ip" => "0.0.0.0"]) . ":";
echo function_exists("long2ip"); echo function_exists("ip2long");
echo function_exists("inet_pton");
return function_exists("inet_ntop");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(
            values.output,
            "192.168.1.1:255.255.255.255:3232235777:bad-ip:01020304:bad-pton:1.2.3.4:bad-ntop:127.0.0.1:0:111"
        );
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval path component builtins mirror static basename/dirname edge cases.
    #[test]
    fn execute_program_dispatches_path_component_builtins() {
        let program = parse_fragment(
            br#"echo basename("/var/log/syslog.log", ".log") . ":";
echo basename(path: "/usr///") . ":";
echo basename("/", "x") === "" ? "root" : "bad"; echo ":";
echo dirname("/usr/local/bin/tool", 2) . ":";
echo dirname(path: "/usr///local///bin") . ":";
echo call_user_func("basename", "foo.tar.gz", ".bz2") . ":";
echo call_user_func_array("dirname", ["path" => "/usr", "levels" => 3]) . ":";
echo function_exists("basename");
return function_exists("dirname");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(
            values.output,
            "syslog:usr:root:/usr/local:/usr///local:foo.tar.gz:/:1"
        );
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `realpath()` resolves existing paths and returns false for misses.
    #[test]
    fn execute_program_dispatches_realpath_builtin() {
        let program = parse_fragment(
            br#"echo realpath(".") !== false ? "resolved" : "bad"; echo ":";
echo realpath(path: "elephc-eval-missing-path") === false ? "false" : "bad"; echo ":";
echo call_user_func("realpath", ".") !== false ? "call" : "bad"; echo ":";
echo call_user_func_array("realpath", ["path" => "elephc-eval-missing-path"]) === false ? "array-false" : "bad";
echo ":";
return function_exists("realpath");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "resolved:false:call:array-false:");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `fnmatch()` supports wildcards, classes, flags, constants, and callables.
    #[test]
    fn execute_program_dispatches_fnmatch_builtin() {
        let program = parse_fragment(
            br#"echo fnmatch("*.log", "system.log") ? "match" : "bad"; echo ":";
echo fnmatch("*.log", "logs/system.log", FNM_PATHNAME) ? "bad" : "path"; echo ":";
echo fnmatch("*.LOG", "system.log", FNM_CASEFOLD) ? "case" : "bad"; echo ":";
echo fnmatch("*", ".env", FNM_PERIOD) ? "bad" : "period"; echo ":";
echo fnmatch("[!abc]oo", "doo") && !fnmatch("[!abc]oo", "boo") ? "class" : "bad"; echo ":";
echo fnmatch('a\\*b', 'a*b') ? "escape" : "bad"; echo ":";
echo fnmatch('a\\*b', 'a\\xxb', FNM_NOESCAPE) ? "noescape" : "bad"; echo ":";
$flags = FNM_PATHNAME | FNM_CASEFOLD;
echo fnmatch("dir/*.TXT", "dir/file.txt", $flags) ? "flags" : "bad"; echo ":";
echo call_user_func("fnmatch", "*.txt", "report.txt") ? "call" : "bad"; echo ":";
echo call_user_func_array("fnmatch", ["pattern" => "*.TXT", "filename" => "report.txt", "flags" => FNM_CASEFOLD]) ? "callarray" : "bad"; echo ":";
echo function_exists("fnmatch"); echo defined("FNM_CASEFOLD");
return FNM_CASEFOLD;"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(
            values.output,
            "match:path:case:period:class:escape:noescape:flags:call:callarray:11"
        );
        assert_eq!(values.get(result), FakeValue::Int(EVAL_FNM_CASEFOLD));
    }

    /// Verifies eval `pathinfo()` handles arrays, component flags, constants, and callables.
    #[test]
    fn execute_program_dispatches_pathinfo_builtin() {
        let program = parse_fragment(
            br#"$info = pathinfo("/var/log/syslog.log");
echo $info["dirname"] . "|" . $info["basename"] . "|" . $info["extension"] . "|" . $info["filename"] . ":";
echo pathinfo("archive.tar.gz", PATHINFO_EXTENSION) . ":";
echo pathinfo(".bashrc", PATHINFO_FILENAME) === "" ? "dotfile" : "bad"; echo ":";
echo pathinfo("file.", PATHINFO_EXTENSION) === "" ? "trail" : "bad"; echo ":";
echo pathinfo("", PATHINFO_DIRNAME) === "" ? "empty-dir" : "bad"; echo ":";
$plain = pathinfo("/etc/hosts");
echo array_key_exists("extension", $plain) ? "bad" : "no-ext"; echo ":";
echo pathinfo("/a/b.php", PATHINFO_BASENAME | PATHINFO_FILENAME) . ":";
$call = call_user_func("pathinfo", "foo.txt", PATHINFO_ALL);
echo $call["basename"] . ":";
echo call_user_func_array("pathinfo", ["path" => "foo.txt", "flags" => 0]) === "" ? "zero" : "bad";
echo ":"; echo function_exists("pathinfo"); echo defined("PATHINFO_ALL");
return PATHINFO_ALL;"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(
            values.output,
            "/var/log|syslog.log|log|syslog:gz:dotfile:trail:empty-dir:no-ext:b.php:foo.txt:zero:11"
        );
        assert_eq!(values.get(result), FakeValue::Int(EVAL_PATHINFO_ALL));
    }

    /// Verifies eval local filesystem builtins read, write, stat, delete, and dispatch.
    #[test]
    fn execute_program_dispatches_filesystem_builtins() {
        let filename = format!("elephc_eval_fs_probe_{}.txt", std::process::id());
        let missing = format!("elephc_eval_fs_missing_{}.txt", std::process::id());
        let source = format!(
            r#"echo file_put_contents("{filename}", "hello") . ":";
echo file_get_contents("{filename}") . ":";
echo file_exists("{filename}") ? "exists" : "missing"; echo ":";
echo is_file(filename: "{filename}") ? "file" : "bad"; echo ":";
echo is_dir(".") ? "dir" : "bad"; echo ":";
echo is_readable("{filename}") ? "readable" : "bad"; echo ":";
echo is_writable("{filename}") ? "writable" : "bad"; echo ":";
echo is_writeable("{filename}") ? "writeable" : "bad"; echo ":";
echo filesize("{filename}") . ":";
echo file_get_contents("{missing}") === false ? "missing-false" : "bad"; echo ":";
echo call_user_func("file_exists", "{filename}") ? "call-exists" : "bad"; echo ":";
echo call_user_func_array("filesize", ["filename" => "{filename}"]) . ":";
echo unlink("{filename}") ? "unlinked" : "bad"; echo ":";
echo file_exists("{filename}") ? "bad" : "gone"; echo ":";
echo function_exists("file_get_contents"); echo function_exists("file_put_contents");
echo function_exists("file_exists"); echo function_exists("is_file"); echo function_exists("is_dir");
echo function_exists("is_readable"); echo function_exists("is_writable"); echo function_exists("is_writeable");
echo function_exists("filesize");
return function_exists("unlink");"#
        );
        let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        let _ = std::fs::remove_file(&filename);
        assert_eq!(
            values.output,
            "5:hello:exists:file:dir:readable:writable:writeable:5:missing-false:call-exists:5:unlinked:gone:111111111"
        );
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval disk-space builtins query local filesystem capacity and dispatch dynamically.
    #[test]
    fn execute_program_dispatches_disk_space_builtins() {
        let program = parse_fragment(
            br#"echo disk_free_space(".") > 0 ? "free" : "bad"; echo ":";
echo disk_total_space(directory: ".") > 0 ? "total" : "bad"; echo ":";
echo disk_total_space(".") >= disk_free_space(".") ? "ordered" : "bad"; echo ":";
echo disk_free_space("no/such/path/elephc-eval") === 0.0 ? "missing" : "bad"; echo ":";
echo call_user_func("disk_free_space", ".") > 0 ? "call" : "bad"; echo ":";
echo call_user_func_array("disk_total_space", ["directory" => "."]) > 0 ? "spread" : "bad"; echo ":";
echo function_exists("disk_free_space");
return function_exists("disk_total_space");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "free:total:ordered:missing:call:spread:1");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval stat metadata builtins expose scalar file metadata and link probes.
    #[test]
    fn execute_program_dispatches_stat_metadata_builtins() {
        let filename = format!("elephc_eval_stat_probe_{}.txt", std::process::id());
        let missing = format!("elephc_eval_stat_missing_{}.txt", std::process::id());
        let link = format!("elephc_eval_stat_link_{}.txt", std::process::id());
        let source = format!(
            r#"echo filemtime("{filename}") > 0 ? "mtime" : "bad"; echo ":";
echo fileatime("{filename}") > 0 ? "atime" : "bad"; echo ":";
echo filectime("{filename}") > 0 ? "ctime" : "bad"; echo ":";
echo fileperms("{filename}") > 0 ? "perms" : "bad"; echo ":";
echo fileowner("{filename}") >= 0 ? "owner" : "bad"; echo ":";
echo filegroup("{filename}") >= 0 ? "group" : "bad"; echo ":";
echo fileinode("{filename}") > 0 ? "inode" : "bad"; echo ":";
echo filetype("{filename}") . ":";
echo filetype(".") . ":";
echo filetype("{link}") . ":";
echo is_executable("{filename}") ? "bad" : "noexec"; echo ":";
echo is_link("{link}") ? "link" : "bad"; echo ":";
echo fileatime("{missing}") === false ? "missing-atime" : "bad"; echo ":";
echo filectime("{missing}") === false ? "missing-ctime" : "bad"; echo ":";
echo fileperms("{missing}") === false ? "missing-perms" : "bad"; echo ":";
echo fileowner("{missing}") === false ? "missing-owner" : "bad"; echo ":";
echo filegroup("{missing}") === false ? "missing-group" : "bad"; echo ":";
echo fileinode("{missing}") === false ? "missing-inode" : "bad"; echo ":";
echo filetype("{missing}") === false ? "missing-type" : "bad"; echo ":";
echo filemtime("{missing}") === 0 ? "missing-mtime" : "bad"; echo ":";
echo call_user_func("filetype", "{filename}") . ":";
echo call_user_func_array("fileinode", ["filename" => "{filename}"]) > 0 ? "callinode" : "bad"; echo ":";
echo function_exists("filemtime"); echo function_exists("fileatime");
echo function_exists("filectime"); echo function_exists("fileperms");
echo function_exists("fileowner"); echo function_exists("filegroup");
echo function_exists("fileinode"); echo function_exists("filetype");
echo function_exists("is_executable"); echo function_exists("is_link");
return true;"#
        );
        let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
        let _ = std::fs::remove_file(&filename);
        let _ = std::fs::remove_file(&link);
        std::fs::write(&filename, b"hello").expect("write stat fixture");
        std::os::unix::fs::symlink(&filename, &link).expect("create stat symlink");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        let _ = std::fs::remove_file(&filename);
        let _ = std::fs::remove_file(&link);
        assert_eq!(
            values.output,
            "mtime:atime:ctime:perms:owner:group:inode:file:dir:link:noexec:link:missing-atime:missing-ctime:missing-perms:missing-owner:missing-group:missing-inode:missing-type:missing-mtime:file:callinode:1111111111"
        );
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `stat()` and `lstat()` build PHP-compatible metadata arrays.
    #[test]
    fn execute_program_dispatches_stat_array_builtins() {
        let pid = std::process::id();
        let filename = format!("elephc_eval_stat_array_{pid}.txt");
        let link = format!("elephc_eval_lstat_array_{pid}.txt");
        let missing = format!("elephc_eval_stat_array_missing_{pid}.txt");
        let source = format!(
            r#"$stat = stat("{filename}");
$lstat = lstat("{link}");
echo $stat["size"] === 5 && $stat[7] === $stat["size"] ? "stat" : "bad"; echo ":";
echo ($stat["mode"] & 61440) === 32768 ? "mode" : "bad"; echo ":";
echo ($lstat["mode"] & 61440) === 40960 ? "lstat" : "bad"; echo ":";
echo stat("{missing}") === false && lstat("{missing}") === false ? "missing" : "bad"; echo ":";
$call = call_user_func("stat", "{filename}");
echo $call["mtime"] === filemtime("{filename}") ? "callstat" : "bad"; echo ":";
$call_lstat = call_user_func_array("lstat", ["filename" => "{link}"]);
echo $call_lstat["ino"] > 0 ? "calllstat" : "bad"; echo ":";
echo unlink("{link}") && unlink("{filename}") ? "cleanup" : "bad"; echo ":";
echo function_exists("stat"); echo function_exists("lstat");
return true;"#
        );
        let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
        let _ = std::fs::remove_file(&filename);
        let _ = std::fs::remove_file(&link);
        std::fs::write(&filename, b"hello").expect("write stat array fixture");
        std::os::unix::fs::symlink(&filename, &link).expect("create stat array symlink");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        let _ = std::fs::remove_file(&filename);
        let _ = std::fs::remove_file(&link);
        assert_eq!(
            values.output,
            "stat:mode:lstat:missing:callstat:calllstat:cleanup:11"
        );
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval local path operation builtins mutate filesystem state.
    #[test]
    fn execute_program_dispatches_path_operation_builtins() {
        let pid = std::process::id();
        let dir = format!("elephc_eval_ops_dir_{pid}");
        let call_dir = format!("elephc_eval_ops_call_dir_{pid}");
        let src = format!("elephc_eval_ops_src_{pid}.txt");
        let copy = format!("elephc_eval_ops_copy_{pid}.txt");
        let moved = format!("elephc_eval_ops_moved_{pid}.txt");
        let symlink = format!("elephc_eval_ops_symlink_{pid}.txt");
        let hardlink = format!("elephc_eval_ops_hardlink_{pid}.txt");
        let source = format!(
            r#"file_put_contents("{src}", "hello");
echo mkdir("{dir}") ? "mkdir" : "bad"; echo ":";
echo is_dir("{dir}") ? "dir" : "bad"; echo ":";
echo copy("{src}", "{copy}") && file_get_contents("{copy}") === "hello" ? "copy" : "bad"; echo ":";
echo rename("{copy}", "{moved}") && file_exists("{moved}") && !file_exists("{copy}") ? "rename" : "bad"; echo ":";
echo symlink("{src}", "{symlink}") ? "symlink" : "bad"; echo ":";
echo readlink("{symlink}") === "{src}" ? "readlink" : "bad"; echo ":";
echo linkinfo("{symlink}") >= 0 ? "linkinfo" : "bad"; echo ":";
echo readlink("{src}") === false ? "readlink-false" : "bad"; echo ":";
echo linkinfo("{missing}") === -1 ? "linkinfo-missing" : "bad"; echo ":";
echo link("{src}", "{hardlink}") && file_get_contents("{hardlink}") === "hello" ? "hardlink" : "bad"; echo ":";
echo clearstatcache() === null ? "cache" : "bad"; echo ":";
echo unlink("{symlink}") && unlink("{hardlink}") && unlink("{moved}") && unlink("{src}") && rmdir("{dir}") ? "cleanup" : "bad"; echo ":";
echo call_user_func("mkdir", "{call_dir}") ? "callmkdir" : "bad"; echo ":";
echo call_user_func_array("rmdir", ["directory" => "{call_dir}"]) ? "callrmdir" : "bad"; echo ":";
echo function_exists("mkdir"); echo function_exists("rmdir"); echo function_exists("copy");
echo function_exists("rename"); echo function_exists("symlink"); echo function_exists("link");
echo function_exists("readlink"); echo function_exists("linkinfo"); echo function_exists("clearstatcache");
return true;"#,
            missing = format!("elephc_eval_ops_missing_{pid}.txt"),
        );
        let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
        for path in [&symlink, &hardlink, &moved, &copy, &src] {
            let _ = std::fs::remove_file(path);
        }
        for path in [&call_dir, &dir] {
            let _ = std::fs::remove_dir(path);
        }
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        for path in [&symlink, &hardlink, &moved, &copy, &src] {
            let _ = std::fs::remove_file(path);
        }
        for path in [&call_dir, &dir] {
            let _ = std::fs::remove_dir(path);
        }
        assert_eq!(
            values.output,
            "mkdir:dir:copy:rename:symlink:readlink:linkinfo:readlink-false:linkinfo-missing:hardlink:cache:cleanup:callmkdir:callrmdir:111111111"
        );
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval file-listing builtins build arrays, stream files, and dispatch dynamically.
    #[test]
    fn execute_program_dispatches_file_listing_builtins() {
        let pid = std::process::id();
        let lines = format!("elephc_eval_listing_lines_{pid}.txt");
        let empty = format!("elephc_eval_listing_empty_{pid}.txt");
        let missing = format!("elephc_eval_listing_missing_{pid}.txt");
        let dir = format!("elephc_eval_listing_dir_{pid}");
        let source = format!(
            r#"file_put_contents("{lines}", "one\ntwo");
file_put_contents("{empty}", "");
$lines = file("{lines}");
echo count($lines) . ":";
echo $lines[0] === "one\n" ? "line0" : "bad"; echo ":";
echo $lines[1] === "two" ? "line1" : "bad"; echo ":";
echo "[";
$bytes = readfile(filename: "{empty}");
echo "]" . $bytes . ":";
echo readfile("{missing}") === false ? "missing-readfile" : "bad"; echo ":";
echo count(file("{missing}")) === 0 ? "missing-file" : "bad"; echo ":";
mkdir("{dir}");
file_put_contents("{dir}/a.txt", "a");
file_put_contents("{dir}/b.txt", "b");
$scan = scandir(directory: "{dir}");
echo count($scan) . ":";
echo in_array(".", $scan) && in_array("..", $scan) && in_array("a.txt", $scan) && in_array("b.txt", $scan) ? "scan" : "bad"; echo ":";
$call_lines = call_user_func("file", "{lines}");
echo $call_lines[0] === "one\n" ? "callfile" : "bad"; echo ":";
$call_scan = call_user_func_array("scandir", ["directory" => "{dir}"]);
echo count($call_scan) . ":";
echo unlink("{dir}/a.txt") && unlink("{dir}/b.txt") && rmdir("{dir}") && unlink("{lines}") && unlink("{empty}") ? "cleanup" : "bad"; echo ":";
echo function_exists("file"); echo function_exists("readfile"); echo function_exists("scandir");
return true;"#
        );
        let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
        for path in [&lines, &empty, &missing] {
            let _ = std::fs::remove_file(path);
        }
        let _ = std::fs::remove_file(format!("{dir}/a.txt"));
        let _ = std::fs::remove_file(format!("{dir}/b.txt"));
        let _ = std::fs::remove_dir(&dir);
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        for path in [&lines, &empty, &missing] {
            let _ = std::fs::remove_file(path);
        }
        let _ = std::fs::remove_file(format!("{dir}/a.txt"));
        let _ = std::fs::remove_file(format!("{dir}/b.txt"));
        let _ = std::fs::remove_dir(&dir);
        assert_eq!(
            values.output,
            "2:line0:line1:[]0:missing-readfile:missing-file:4:scan:callfile:4:cleanup:111"
        );
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `glob()` expands local patterns and dispatches dynamically.
    #[test]
    fn execute_program_dispatches_glob_builtin() {
        let pid = std::process::id();
        let dir = format!("elephc_eval_glob_dir_{pid}");
        let source = format!(
            r#"mkdir("{dir}");
file_put_contents("{dir}/a.txt", "a");
file_put_contents("{dir}/b.log", "b");
file_put_contents("{dir}/c.txt", "c");
file_put_contents("{dir}/.hidden.txt", "h");
$matches = glob("{dir}/*.txt");
echo count($matches) === 2 && basename($matches[0]) === "a.txt" && basename($matches[1]) === "c.txt" ? "glob" : "bad"; echo ":";
echo count(glob("{dir}/*.none")) === 0 ? "empty" : "bad"; echo ":";
$literal = glob("{dir}/a.txt");
echo count($literal) === 1 && $literal[0] === "{dir}/a.txt" ? "literal" : "bad"; echo ":";
$all = glob("{dir}/*");
echo in_array("{dir}/.hidden.txt", $all) ? "bad" : "hidden"; echo ":";
$call = call_user_func("glob", "{dir}/*.log");
echo count($call) === 1 && basename($call[0]) === "b.log" ? "callglob" : "bad"; echo ":";
$call_array = call_user_func_array("glob", ["pattern" => "{dir}/*.txt"]);
echo count($call_array) === 2 ? "callarray" : "bad"; echo ":";
unlink("{dir}/.hidden.txt");
unlink("{dir}/c.txt");
unlink("{dir}/b.log");
unlink("{dir}/a.txt");
echo rmdir("{dir}") ? "cleanup" : "bad"; echo ":";
echo function_exists("glob");
return true;"#
        );
        let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
        let _ = std::fs::remove_file(format!("{dir}/.hidden.txt"));
        let _ = std::fs::remove_file(format!("{dir}/c.txt"));
        let _ = std::fs::remove_file(format!("{dir}/b.log"));
        let _ = std::fs::remove_file(format!("{dir}/a.txt"));
        let _ = std::fs::remove_dir(&dir);
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        let _ = std::fs::remove_file(format!("{dir}/.hidden.txt"));
        let _ = std::fs::remove_file(format!("{dir}/c.txt"));
        let _ = std::fs::remove_file(format!("{dir}/b.log"));
        let _ = std::fs::remove_file(format!("{dir}/a.txt"));
        let _ = std::fs::remove_dir(&dir);
        assert_eq!(
            values.output,
            "glob:empty:literal:hidden:callglob:callarray:cleanup:1"
        );
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval file-modification builtins update modes, masks, temp files, and dispatch.
    #[test]
    fn execute_program_dispatches_file_modify_builtins() {
        let pid = std::process::id();
        let filename = format!("elephc_eval_modify_{pid}.txt");
        let missing = format!("elephc_eval_modify_missing_{pid}.txt");
        let prefix = format!("evm{pid}_");
        let call_prefix = format!("evc{pid}_");
        let source = format!(
            r#"file_put_contents("{filename}", "x");
echo chmod(filename: "{filename}", permissions: 384) ? "chmod" : "bad"; echo ":";
echo (fileperms("{filename}") & 511) === 384 ? "mode" : "bad"; echo ":";
echo chmod("{missing}", 384) ? "bad" : "chmod-false"; echo ":";
$tmp = tempnam(directory: ".", prefix: "{prefix}");
echo file_exists($tmp) && str_starts_with(basename($tmp), "{prefix}") ? "tempnam" : "bad"; echo ":";
unlink($tmp);
$previous = umask(mask: 18);
$set = umask($previous);
echo $set === 18 ? "umask" : "bad"; echo ":";
$before = umask(18);
$probe = umask();
$restore = umask($before);
echo $probe === 18 && $restore === 18 ? "probe" : "bad"; echo ":";
echo call_user_func("chmod", "{filename}", 420) ? "callchmod" : "bad"; echo ":";
$call_tmp = call_user_func_array("tempnam", ["directory" => ".", "prefix" => "{call_prefix}"]);
echo file_exists($call_tmp) && str_starts_with(basename($call_tmp), "{call_prefix}") ? "calltempnam" : "bad"; echo ":";
unlink($call_tmp);
echo unlink("{filename}") ? "cleanup" : "bad"; echo ":";
echo function_exists("chmod"); echo function_exists("tempnam"); echo function_exists("umask");
return true;"#
        );
        let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
        let _ = std::fs::remove_file(&filename);
        let _ = std::fs::remove_file(&missing);
        for entry in std::fs::read_dir(".").expect("read eval test cwd") {
            let entry = entry.expect("read eval temp entry");
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with(&prefix) || name.starts_with(&call_prefix) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        let _ = std::fs::remove_file(&filename);
        let _ = std::fs::remove_file(&missing);
        for entry in std::fs::read_dir(".").expect("read eval test cwd") {
            let entry = entry.expect("read eval temp entry");
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with(&prefix) || name.starts_with(&call_prefix) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
        assert_eq!(
            values.output,
            "chmod:mode:chmod-false:tempnam:umask:probe:callchmod:calltempnam:cleanup:111"
        );
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `touch()` creates files, stamps mtimes, and dispatches dynamically.
    #[test]
    fn execute_program_dispatches_touch_builtin() {
        let pid = std::process::id();
        let created = format!("elephc_eval_touch_created_{pid}.txt");
        let stamped = format!("elephc_eval_touch_stamped_{pid}.txt");
        let missing = format!("elephc_eval_touch_missing_{pid}/x.txt");
        let source = format!(
            r#"echo touch(filename: "{created}") && file_exists("{created}") ? "create" : "bad"; echo ":";
file_put_contents("{stamped}", "x");
echo touch("{stamped}", 1000000000) ? "mtime" : "bad"; echo ":";
echo filemtime("{stamped}") === 1000000000 ? "readmtime" : "bad"; echo ":";
echo touch("{stamped}", 1000000001, null) && filemtime("{stamped}") === 1000000001 ? "nullatime" : "bad"; echo ":";
echo touch("{stamped}", 1000000002, 1000000003) && filemtime("{stamped}") === 1000000002 ? "both" : "bad"; echo ":";
echo touch("{missing}") ? "bad" : "touch-false"; echo ":";
echo call_user_func("touch", "{created}", 1000000004) ? "calltouch" : "bad"; echo ":";
echo call_user_func_array("touch", ["filename" => "{stamped}", "mtime" => 1000000005]) ? "callarray" : "bad"; echo ":";
echo unlink("{created}") && unlink("{stamped}") ? "cleanup" : "bad"; echo ":";
echo function_exists("touch");
return true;"#
        );
        let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
        let _ = std::fs::remove_file(&created);
        let _ = std::fs::remove_file(&stamped);
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        let _ = std::fs::remove_file(&created);
        let _ = std::fs::remove_file(&stamped);
        assert_eq!(
            values.output,
            "create:mtime:readmtime:nullatime:both:touch-false:calltouch:callarray:cleanup:1"
        );
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval ASCII string case builtins work directly and through callable dispatch.
    #[test]
    fn execute_program_dispatches_string_case_builtins() {
        let program = parse_fragment(
            br#"echo strtoupper("Hello World"); echo ":";
echo strtolower("LOUD"); echo ":";
echo ucfirst("eval"); echo ":";
echo lcfirst("LOUD"); echo ":";
echo call_user_func("strtoupper", "xy"); echo ":";
echo call_user_func_array("strtolower", ["ZZ"]); echo ":";
echo call_user_func("ucfirst", "case"); echo ":";
echo call_user_func_array("lcfirst", ["CASE"]);
echo ":"; echo function_exists("strtoupper"); echo function_exists("strtolower"); echo function_exists("ucfirst");
return function_exists("lcfirst");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(
            values.output,
            "HELLO WORLD:loud:Eval:lOUD:XY:zz:Case:cASE:111"
        );
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `ucwords()` capitalizes word starts directly and by callable dispatch.
    #[test]
    fn execute_program_dispatches_ucwords_builtin() {
        let program = parse_fragment(
            br#"echo ucwords("hello world"); echo ":";
echo ucwords(string: "hello-world", separators: "-"); echo ":";
echo ucwords("hello\tworld"); echo ":";
echo call_user_func("ucwords", "a b"); echo ":";
echo call_user_func_array("ucwords", ["string" => "a-b", "separators" => "-"]); echo ":";
return function_exists("ucwords");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "Hello World:Hello-World:Hello\tWorld:A B:A-B:");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `wordwrap()` wraps at word boundaries and can cut long words.
    #[test]
    fn execute_program_dispatches_wordwrap_builtin() {
        let program = parse_fragment(
            br#"echo wordwrap("The quick brown fox", 10, "|"); echo ":";
echo wordwrap(string: "A verylongword here", width: 8, break: "|"); echo ":";
echo wordwrap("abcdefghij", 4, "|", true); echo ":";
echo wordwrap("preserve\nnewlines here ok", 10, "|"); echo ":";
echo call_user_func("wordwrap", "aaa bbb ccc", 3, "<br>"); echo ":";
echo call_user_func_array("wordwrap", ["string" => "hello world", "width" => 5, "break" => "|"]);
echo ":";
return function_exists("wordwrap");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(
            values.output,
            "The quick|brown fox:A|verylongword|here:abcd|efgh|ij:preserve\nnewlines|here ok:aaa<br>bbb<br>ccc:hello|world:"
        );
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `str_contains()` uses byte-string search and supports callable dispatch.
    #[test]
    fn execute_program_dispatches_str_contains_builtin() {
        let program = parse_fragment(
            br#"echo str_contains("Hello World", "World") ? "Y" : "N";
echo str_contains("Hello", "z") ? "bad" : ":N";
echo str_contains("Hello", "") ? ":E" : "bad";
echo call_user_func("str_contains", "abc", "b") ? ":C" : "bad";
echo call_user_func_array("str_contains", ["abc", "x"]) ? "bad" : ":A";
return function_exists("str_contains");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "Y:N:E:C:A");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval string position builtins return byte offsets or PHP false.
    #[test]
    fn execute_program_dispatches_string_position_builtins() {
        let program = parse_fragment(
            br#"echo strpos("banana", "na");
echo ":" . strrpos("banana", "na");
echo ":"; echo strpos("abc", "z") === false ? "F" : "bad";
echo ":" . strpos("abc", "");
echo ":" . strrpos("abc", "");
echo ":" . call_user_func("strpos", "abc", "b");
echo ":" . call_user_func_array("strrpos", ["ababa", "ba"]);
echo ":"; echo function_exists("strpos");
return function_exists("strrpos");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "2:4:F:0:3:1:3:1");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `strstr()` returns suffixes, prefixes, or false for misses.
    #[test]
    fn execute_program_dispatches_strstr_builtin() {
        let program = parse_fragment(
            br#"echo strstr("user@example.com", "@"); echo ":";
echo strstr(haystack: "hello world", needle: "lo", before_needle: true); echo ":";
echo strstr("hello", "x") === false ? "F" : "bad"; echo ":";
echo strstr("hello", ""); echo ":";
echo call_user_func("strstr", "abcabc", "bc"); echo ":";
echo call_user_func_array("strstr", ["haystack" => "abcabc", "needle" => "bc", "before_needle" => true]); echo ":";
return function_exists("strstr");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(
            values.output,
            "@example.com:hel:F:hello:bcabc:a:"
        );
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval prefix/suffix string search builtins use byte-string semantics.
    #[test]
    fn execute_program_dispatches_string_boundary_builtins() {
        let program = parse_fragment(
            br#"echo str_starts_with("Hello World", "Hello") ? "S" : "bad";
echo str_starts_with("Hello", "World") ? "bad" : ":s";
echo str_starts_with("Hello", "") ? ":se" : "bad";
echo str_ends_with("Hello World", "World") ? ":E" : "bad";
echo str_ends_with("Hello", "World") ? "bad" : ":e";
echo str_ends_with("Hello", "") ? ":ee" : "bad";
echo call_user_func("str_starts_with", "abc", "a") ? ":CS" : "bad";
echo call_user_func_array("str_ends_with", ["abc", "c"]) ? ":CE" : "bad";
echo ":"; echo function_exists("str_starts_with");
return function_exists("str_ends_with");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "S:s:se:E:e:ee:CS:CE:1");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval string comparison builtins return PHP-compatible scalar results.
    #[test]
    fn execute_program_dispatches_string_compare_builtins() {
        let program = parse_fragment(
            br#"echo strcmp("abc", "abc");
echo ":"; echo strcmp("abc", "abd") < 0 ? "lt" : "bad";
echo ":"; echo strcasecmp("Hello", "hello");
echo ":"; echo call_user_func("strcmp", "b", "a") > 0 ? "gt" : "bad";
echo ":"; echo call_user_func_array("strcasecmp", ["A", "a"]) === 0 ? "ci" : "bad";
echo ":"; echo hash_equals("abc", "abc") ? "heq" : "bad";
echo ":"; echo hash_equals("abc", "abcd") ? "bad" : "hlen";
echo ":"; echo call_user_func("hash_equals", "abc", "abd") ? "bad" : "hneq";
echo ":"; echo function_exists("strcmp"); echo function_exists("strcasecmp");
return function_exists("hash_equals");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "0:lt:0:gt:ci:heq:hlen:hneq:11");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval trim-like builtins strip default and explicit byte masks.
    #[test]
    fn execute_program_dispatches_trim_like_builtins() {
        let program = parse_fragment(
            br#"echo "[" . trim("  hello  ") . "]";
echo ":[" . ltrim("  left") . "]";
echo ":[" . rtrim("right  ") . "]";
echo ":[" . chop("tail... ", " .") . "]";
echo ":[" . trim("**boxed**", "*") . "]";
echo ":[" . call_user_func("trim", "  cuf  ") . "]";
echo ":[" . call_user_func_array("ltrim", ["0007", "0"]) . "]";
echo ":"; echo function_exists("trim"); echo function_exists("ltrim"); echo function_exists("rtrim");
return function_exists("chop");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(
            values.output,
            "[hello]:[left]:[right]:[tail]:[boxed]:[cuf]:[7]:111"
        );
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval type-predicate builtins inspect boxed runtime tags directly and by callable.
    #[test]
    fn execute_program_dispatches_type_predicate_builtins() {
        let program = parse_fragment(
            br#"echo is_int(1); echo is_integer(1); echo is_long(1);
echo is_float(1.5); echo is_double(1.5); echo is_real(1.5);
echo is_string("x"); echo is_bool(false); echo is_null(null);
echo is_array([1]); echo is_array(["a" => 1]);
echo is_iterable([1]); echo is_iterable(["a" => 1]);
echo is_iterable(1) ? "bad" : "T";
echo is_array(1) ? "bad" : "ok";
echo is_numeric(42); echo is_numeric(3.14); echo is_numeric("42");
echo is_numeric("-5"); echo is_numeric("3.14");
echo is_numeric("abc") ? "bad" : "N";
echo is_numeric(true) ? "bad" : "B";
echo is_resource(1) ? "bad" : "R";
echo is_nan(fdiv(0, 0)) ? "N" : "bad";
echo is_infinite(fdiv(1, 0)) ? "I" : "bad";
echo is_infinite(fdiv(-1, 0)) ? "i" : "bad";
echo is_finite(42) ? "F" : "bad";
echo is_finite(fdiv(1, 0)) ? "bad" : "f";
echo ":"; echo call_user_func("is_string", "x");
echo call_user_func_array("is_array", [[1]]);
echo call_user_func("is_numeric", "12");
echo call_user_func("is_iterable", [1]);
echo call_user_func_array("is_iterable", ["value" => 1]) ? "bad" : "t";
echo function_exists("is_numeric"); echo function_exists("is_resource");
echo function_exists("is_double"); echo function_exists("is_nan"); echo function_exists("is_finite");
echo function_exists("is_iterable");
return function_exists("is_infinite");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(
            values.output,
            "1111111111111Tok11111NBRNIiFf:1111t111111"
        );
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `is_resource()` recognizes resource-tagged runtime cells from scope.
    #[test]
    fn execute_program_dispatches_is_resource_true() {
        let program = parse_fragment(
            br#"echo is_resource($handle) ? "R" : "bad";
echo ":" . gettype($handle);
return call_user_func("is_resource", $handle);"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let handle = values.alloc(FakeValue::Resource(6));
        scope.set("handle".to_string(), handle, ScopeCellOwnership::Borrowed);

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "R:resource");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval cast builtins return boxed scalar cells directly and by callable.
    #[test]
    fn execute_program_dispatches_cast_builtins() {
        let program = parse_fragment(
            br#"echo intval("42"); echo ":";
echo floatval("3.5"); echo ":";
echo strval(12); echo ":";
echo boolval("0") ? "bad" : "false";
echo ":"; echo call_user_func("strval", 7);
return call_user_func_array("intval", ["9"]);"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "42:3.5:12:false:7");
        assert_eq!(values.get(result), FakeValue::Int(9));
    }

    /// Verifies eval `gettype()` maps runtime tags to PHP type names directly and by callable.
    #[test]
    fn execute_program_dispatches_gettype_builtin() {
        let program = parse_fragment(
            br#"echo gettype(1); echo ":";
echo gettype(1.5); echo ":";
echo gettype("x"); echo ":";
echo gettype(false); echo ":";
echo gettype(null); echo ":";
echo gettype([1]); echo ":";
echo gettype(["a" => 1]); echo ":";
echo call_user_func("gettype", true); echo ":";
echo call_user_func_array("gettype", [null]);
return function_exists("gettype");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(
            values.output,
            "integer:double:string:boolean:NULL:array:array:boolean:NULL"
        );
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `abs()` dispatches through runtime numeric hooks directly and by callable.
    #[test]
    fn execute_program_dispatches_abs_builtin() {
        let program = parse_fragment(
            br#"echo abs(-5); echo ":";
echo abs(-2.5); echo ":";
echo gettype(abs(-2.5)); echo ":";
echo call_user_func("abs", -7); echo ":";
echo call_user_func_array("abs", [-9]);
return function_exists("abs");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "5:2.5:double:7:9");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `floor()` and `ceil()` dispatch as double-returning math builtins.
    #[test]
    fn execute_program_dispatches_floor_and_ceil_builtins() {
        let program = parse_fragment(
            br#"echo floor(3.7); echo ":";
echo gettype(floor(3)); echo ":";
echo ceil(3.2); echo ":";
echo gettype(ceil(3)); echo ":";
echo call_user_func("floor", 4.9); echo ":";
echo call_user_func_array("ceil", [4.1]);
echo ":"; echo function_exists("floor");
return function_exists("ceil");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "3:double:4:double:4:5:1");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `fdiv()` and `fmod()` dispatch as floating-point binary builtins.
    #[test]
    fn execute_program_dispatches_float_binary_builtins() {
        let program = parse_fragment(
            br#"echo round(fdiv(10, 4), 2); echo ":";
echo gettype(fdiv(10, 4)); echo ":";
echo round(fmod(10.5, 3.2), 1); echo ":";
echo round(call_user_func("fdiv", 9, 2), 1); echo ":";
echo round(call_user_func_array("fmod", [10.5, 3.2]), 1); echo ":";
echo function_exists("fdiv");
return function_exists("fmod");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
        assert_eq!(values.output, "2.5:double:0.9:4.5:0.9:1");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval extended scalar math builtins support direct, named, callable, and probe paths.
    #[test]
    fn execute_program_dispatches_extended_math_builtins() {
        let program = parse_fragment(
            br#"echo sin(0); echo ":";
echo cos(0); echo ":";
echo tan(0); echo ":";
echo round(asin(1), 2); echo ":";
echo acos(1); echo ":";
echo round(atan(1), 2); echo ":";
echo sinh(0); echo ":";
echo cosh(0); echo ":";
echo tanh(0); echo ":";
echo log2(8); echo ":";
echo log10(100); echo ":";
echo exp(0); echo ":";
echo round(deg2rad(180), 2); echo ":";
echo round(rad2deg(pi()), 0); echo ":";
echo log(num: 8, base: 2); echo ":";
echo atan2(y: 0, x: 1); echo ":";
echo hypot(3, 4); echo ":";
echo intdiv(7, 2); echo ":";
echo round(call_user_func("sin", pi() / 2), 0); echo ":";
echo call_user_func_array("intdiv", ["num1" => 9, "num2" => 2]); echo ":";
echo function_exists("sin"); echo function_exists("log"); echo function_exists("intdiv");
return function_exists("hypot");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(
            values.output,
            "0:1:0:1.57:0:0.79:0:1:0:3:2:1:3.14:180:3:0:5:3:1:4:111"
        );
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `pow()` dispatches through the existing exponentiation runtime hook.
    #[test]
    fn execute_program_dispatches_pow_builtin() {
        let program = parse_fragment(
            br#"echo pow(2, 3); echo ":";
echo gettype(pow(2, 3)); echo ":";
echo call_user_func("pow", 2, 5); echo ":";
echo call_user_func_array("pow", [3, 3]);
return function_exists("pow");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "8:double:32:27");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `round()` supports default and explicit precision through callable paths.
    #[test]
    fn execute_program_dispatches_round_builtin() {
        let program = parse_fragment(
            br#"echo round(3.5); echo ":";
echo round(3.14159, 2); echo ":";
echo gettype(round(3)); echo ":";
echo call_user_func("round", 2.5); echo ":";
echo call_user_func_array("round", [1.55, 1]);
return function_exists("round");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "4:3.14:double:3:1.6");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `number_format()` groups and rounds numbers through callable paths.
    #[test]
    fn execute_program_dispatches_number_format_builtin() {
        let program = parse_fragment(
            br#"echo number_format(1234567); echo ":";
echo number_format(1234.5678, 2); echo ":";
echo number_format(num: 1234567.89, decimals: 2, decimal_separator: ",", thousands_separator: "."); echo ":";
echo number_format(1234567.89, 2, ".", ""); echo ":";
echo call_user_func("number_format", -1234.5, 1); echo ":";
echo call_user_func_array("number_format", ["num" => 1234, "decimals" => 0, "decimal_separator" => ".", "thousands_separator" => " "]); echo ":";
return function_exists("number_format");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(
            values.output,
            "1,234,567:1,234.57:1.234.567,89:1234567.89:-1,234.5:1 234:"
        );
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `min()` and `max()` select numeric values directly and by callable.
    #[test]
    fn execute_program_dispatches_min_max_builtins() {
        let program = parse_fragment(
            br#"echo min(3, 1, 2); echo ":";
echo max(1, 3, 2); echo ":";
echo min(2.5, 1.5); echo ":";
echo max(1.5, 2.5); echo ":";
echo call_user_func("min", 9, 4, 7); echo ":";
echo call_user_func_array("max", [4, 8, 6]); echo ":";
echo function_exists("min");
return function_exists("max");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "1:3:1.5:2.5:4:8:1");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `clamp()` selects numeric values through direct, named, and callable paths.
    #[test]
    fn execute_program_dispatches_clamp_builtin() {
        let program = parse_fragment(
            br#"echo clamp(5, 0, 10); echo ":";
echo clamp(15, 0, 10); echo ":";
echo clamp(-5, 0, 10); echo ":";
echo clamp(2.75, 1.5, 2.5); echo ":";
echo clamp(value: 8, min: 0, max: 5); echo ":";
echo call_user_func("clamp", -1, 0, 10); echo ":";
echo call_user_func_array("clamp", ["value" => 9, "min" => 0, "max" => 7]); echo ":";
echo function_exists("clamp");
return is_callable("clamp");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "5:10:0:2.5:5:0:7:1");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `clamp()` rejects a lower bound greater than the upper bound.
    #[test]
    fn execute_program_rejects_clamp_invalid_bounds() {
        let program =
            parse_fragment(br#"return clamp(5, 10, 0);"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let err = execute_program(&program, &mut scope, &mut values)
            .expect_err("invalid clamp bounds should fail");

        assert_eq!(err, EvalStatus::RuntimeFatal);
    }

    /// Verifies eval `pi()` returns a double constant directly and through callable paths.
    #[test]
    fn execute_program_dispatches_pi_builtin() {
        let program = parse_fragment(
            br#"echo round(pi(), 2); echo ":";
echo gettype(pi()); echo ":";
echo round(call_user_func("pi"), 3); echo ":";
echo round(call_user_func_array("pi", []), 4); echo ":";
return function_exists("pi");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "3.14:double:3.142:3.1416:");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `sqrt()` dispatches through runtime float hooks directly and by callable.
    #[test]
    fn execute_program_dispatches_sqrt_builtin() {
        let program = parse_fragment(
            br#"echo sqrt(16); echo ":";
echo gettype(sqrt(9)); echo ":";
echo call_user_func("sqrt", 25); echo ":";
echo call_user_func_array("sqrt", [36]);
return function_exists("sqrt");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "4:double:5:6");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `strrev()` dispatches through direct and callable paths.
    #[test]
    fn execute_program_dispatches_strrev_builtin() {
        let program = parse_fragment(
            br#"echo strrev("Hello"); echo ":";
echo strrev(123); echo ":";
echo call_user_func("strrev", "ABC"); echo ":";
echo call_user_func_array("strrev", ["def"]); echo ":";
return function_exists("strrev");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
        assert_eq!(values.output, "olleH:321:CBA:fed:");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `chr()` dispatches through direct, named, and callable paths.
    #[test]
    fn execute_program_dispatches_chr_builtin() {
        let program = parse_fragment(
            br#"echo chr(65); echo ":";
echo bin2hex(chr(codepoint: 256)); echo ":";
echo bin2hex(call_user_func("chr", 257)); echo ":";
echo call_user_func_array("chr", ["codepoint" => 321]); echo ":";
return function_exists("chr");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "A:00:01:A:");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `str_repeat()` dispatches through direct, named, and callable paths.
    #[test]
    fn execute_program_dispatches_str_repeat_builtin() {
        let program = parse_fragment(
            br#"echo str_repeat("ha", 3); echo ":";
echo strlen(str_repeat(string: "x", times: 0)); echo ":";
echo call_user_func("str_repeat", "ab", 2); echo ":";
echo call_user_func_array("str_repeat", ["string" => "z", "times" => 3]); echo ":";
return function_exists("str_repeat");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "hahaha:0:abab:zzz:");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `substr()` dispatches through direct, named, and callable paths.
    #[test]
    fn execute_program_dispatches_substr_builtin() {
        let program = parse_fragment(
            br#"echo substr("abcdef", 2); echo ":";
echo substr(string: "abcdef", offset: 1, length: -1); echo ":";
echo substr("abcdef", -2); echo ":";
echo call_user_func("substr", "abcdef", 2, -2); echo ":";
echo call_user_func_array("substr", ["string" => "abcdef", "offset" => -4, "length" => 2]); echo ":";
return function_exists("substr");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "cdef:bcde:ef:cd:cd:");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `substr_replace()` dispatches through direct, named, and callable paths.
    #[test]
    fn execute_program_dispatches_substr_replace_builtin() {
        let program = parse_fragment(
            br#"echo substr_replace("hello world", "PHP", 6, 5); echo ":";
echo substr_replace(string: "abcdef", replace: "X", offset: 1, length: -1); echo ":";
echo substr_replace("abcdef", "X", -2); echo ":";
echo call_user_func("substr_replace", "abcdef", "X", 99, 1); echo ":";
echo call_user_func_array("substr_replace", ["string" => "abcdef", "replace" => "X", "offset" => -99, "length" => 2]); echo ":";
return function_exists("substr_replace");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "hello PHP:aXf:abcdX:abcdefX:Xcdef:");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `nl2br()` dispatches through direct, named, and callable paths.
    #[test]
    fn execute_program_dispatches_nl2br_builtin() {
        let program = parse_fragment(
            br#"echo bin2hex(nl2br("a\nb")); echo ":";
echo bin2hex(nl2br(string: "a\nb", use_xhtml: false)); echo ":";
echo bin2hex(call_user_func("nl2br", "a\r\nb")); echo ":";
echo bin2hex(call_user_func_array("nl2br", ["string" => "a\n\rb", "use_xhtml" => false])); echo ":";
return function_exists("nl2br");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(
            values.output,
            "613c6272202f3e0a62:613c62723e0a62:613c6272202f3e0d0a62:613c62723e0a0d62:"
        );
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `bin2hex()` dispatches through direct, named, and callable paths.
    #[test]
    fn execute_program_dispatches_bin2hex_builtin() {
        let program = parse_fragment(
            br#"echo bin2hex("Az"); echo ":";
echo bin2hex(string: "A\n"); echo ":";
echo call_user_func("bin2hex", "!?"); echo ":";
echo call_user_func_array("bin2hex", ["string" => "ok"]); echo ":";
return function_exists("bin2hex");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "417a:410a:213f:6f6b:");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `hex2bin()` dispatches through direct, named, and callable paths.
    #[test]
    fn execute_program_dispatches_hex2bin_builtin() {
        let program = parse_fragment(
            br#"echo hex2bin("417a"); echo ":";
echo bin2hex(hex2bin(string: "410a")); echo ":";
echo call_user_func("hex2bin", "213f"); echo ":";
echo call_user_func_array("hex2bin", ["string" => "6f6b"]); echo ":";
echo hex2bin("4") ? "bad" : "false"; echo ":";
return function_exists("hex2bin");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "Az:410a:!?:ok:false:");
        assert_eq!(values.get(result), FakeValue::Bool(true));
        assert_eq!(
            values.warnings,
            vec![HEX2BIN_ODD_LENGTH_WARNING.to_string()]
        );
    }

    /// Verifies eval slash escaping builtins use PHP byte-string semantics.
    #[test]
    fn execute_program_dispatches_slash_escape_builtins() {
        let program = parse_fragment(
            br#"$escaped = addslashes($source);
echo bin2hex($escaped); echo ":";
echo bin2hex(stripslashes($escaped)); echo ":";
echo call_user_func("addslashes", "x\"y"); echo ":";
echo call_user_func_array("stripslashes", [addslashes("o\"k")]); echo ":";
return function_exists("addslashes") && function_exists("stripslashes");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let source = values.string("a\0b\\c\"d'").expect("create source");
        scope.set("source", source, ScopeCellOwnership::Owned);

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "615c30625c5c635c22645c27:6100625c63226427:x\\\"y:o\"k:");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `base64_encode()` dispatches through direct, named, and callable paths.
    #[test]
    fn execute_program_dispatches_base64_encode_builtin() {
        let program = parse_fragment(
            br#"echo base64_encode("Hello"); echo ":";
echo base64_encode(string: "Hi"); echo ":";
echo call_user_func("base64_encode", "Test 123!"); echo ":";
echo call_user_func_array("base64_encode", ["string" => ""]); echo ":";
return function_exists("base64_encode");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "SGVsbG8=:SGk=:VGVzdCAxMjMh::");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `base64_decode()` dispatches through direct, named, and callable paths.
    #[test]
    fn execute_program_dispatches_base64_decode_builtin() {
        let program = parse_fragment(
            br#"echo base64_decode("SGVsbG8="); echo ":";
echo base64_decode(string: "SGk="); echo ":";
echo call_user_func("base64_decode", "VGVzdCAxMjMh"); echo ":";
echo call_user_func_array("base64_decode", ["string" => ""]); echo ":";
return function_exists("base64_decode");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "Hello:Hi:Test 123!::");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies `isset` distinguishes missing, null, and other falsey values.
    #[test]
    fn execute_program_isset_distinguishes_missing_null_and_falsey_values() {
        let program = parse_fragment(
            br#"if (isset($missing)) { echo "1"; } else { echo "0"; }
if (isset($nullish)) { echo "1"; } else { echo "0"; }
if (isset($zero)) { echo "1"; } else { echo "0"; }
if (isset($empty)) { echo "1"; } else { echo "0"; }
if (isset($zero, $empty)) { echo "1"; } else { echo "0"; }
if (isset($zero, $nullish)) { echo "1"; } else { echo "0"; }"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let nullish = values.null().expect("create fake null");
        let zero = values.int(0).expect("create fake int");
        let empty = values.string("").expect("create fake string");
        scope.set("nullish", nullish, ScopeCellOwnership::Owned);
        scope.set("zero", zero, ScopeCellOwnership::Owned);
        scope.set("empty", empty, ScopeCellOwnership::Owned);

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "001110");
        assert_eq!(values.get(result), FakeValue::Null);
    }

    /// Verifies `empty` treats missing, null, and falsey values as empty.
    #[test]
    fn execute_program_empty_uses_php_truthiness_without_missing_warnings() {
        let program = parse_fragment(
            br#"if (empty($missing)) { echo "1"; } else { echo "0"; }
if (empty($nullish)) { echo "1"; } else { echo "0"; }
if (empty($zero)) { echo "1"; } else { echo "0"; }
if (empty($empty_string)) { echo "1"; } else { echo "0"; }
if (empty($zero_string)) { echo "1"; } else { echo "0"; }
if (empty($value)) { echo "1"; } else { echo "0"; }"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let nullish = values.null().expect("create fake null");
        let zero = values.int(0).expect("create fake int");
        let empty_string = values.string("").expect("create fake empty string");
        let zero_string = values.string("0").expect("create fake zero string");
        let value = values.string("x").expect("create fake non-empty string");
        scope.set("nullish", nullish, ScopeCellOwnership::Owned);
        scope.set("zero", zero, ScopeCellOwnership::Owned);
        scope.set("empty_string", empty_string, ScopeCellOwnership::Owned);
        scope.set("zero_string", zero_string, ScopeCellOwnership::Owned);
        scope.set("value", value, ScopeCellOwnership::Owned);

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "111110");
        assert_eq!(values.get(result), FakeValue::Null);
    }

    /// Verifies `isset` and `empty` use PHP offset semantics for array reads.
    #[test]
    fn execute_program_isset_and_empty_support_array_offsets() {
        let program = parse_fragment(
            br#"$map = [
    "present" => "x",
    "nullish" => null,
    "zero" => 0,
    "empty" => "",
    "child" => ["leaf" => "ok", "null" => null],
];
echo isset($map["present"]) ? "1" : "0";
echo isset($map["nullish"]) ? "1" : "0";
echo isset($map["missing"]) ? "1" : "0";
echo isset($map["zero"]) ? "1" : "0";
echo isset($map["child"]["leaf"]) ? "1" : "0";
echo isset($map["child"]["null"]) ? "1" : "0";
echo isset($map["missing"]["leaf"]) ? "1" : "0";
echo ":";
echo empty($map["present"]) ? "1" : "0";
echo empty($map["nullish"]) ? "1" : "0";
echo empty($map["missing"]) ? "1" : "0";
echo empty($map["zero"]) ? "1" : "0";
echo empty($map["empty"]) ? "1" : "0";
echo empty($map["child"]["leaf"]) ? "1" : "0";
echo empty($map["child"]["null"]) ? "1" : "0";
echo empty($map["missing"]["leaf"]) ? "1" : "0";"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "1001100:01111011");
        assert_eq!(values.get(result), FakeValue::Null);
    }

    /// Verifies eval builtin probes see dynamic functions and supported PHP-visible builtins.
    #[test]
    fn execute_program_function_probes_use_eval_context() {
        let program = parse_fragment(
            br#"function dyn_probe() { return 1; }
echo function_exists("DYN_PROBE") . "x";
echo is_callable("dyn_probe") . "x";
echo function_exists("strlen") . "x";
echo function_exists("native_probe") . "x";
echo function_exists("eval") . "x";
echo function_exists("missing_probe") . "x";"#,
        )
        .expect("parse eval fragment");
        let native = NativeFunction::new(1usize as *mut c_void, fake_native_return_descriptor, 0);
        let mut context = ElephcEvalContext::new();
        assert!(context
            .define_native_function("native_probe", native)
            .is_ok());
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let _ = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
            .expect("execute eval ir");

        assert_eq!(values.output, "1x1x1x1xxx");
    }

    /// Verifies eval `define()` and `defined()` share a dynamic constant-name table.
    #[test]
    fn execute_program_define_and_defined_use_dynamic_constant_table() {
        let program = parse_fragment(
            br#"echo define("DynEvalConst", "ok") ? "Y" : "N";
echo DynEvalConst;
echo \DynEvalConst;
echo defined("DynEvalConst") ? "Y" : "N";
echo defined("\\DynEvalConst") ? "Y" : "N";
echo defined("dynevalconst") ? "Y" : "N";
echo define("DynEvalConst", 2) ? "Y" : "N";
echo call_user_func("defined", "DynEvalConst") ? "Y" : "N";
echo call_user_func_array("defined", ["constant_name" => "\\DynEvalConst"]) ? "Y" : "N";"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "YokokYYNNYY");
        assert_eq!(
            values.warnings,
            vec![DEFINE_ALREADY_DEFINED_WARNING.to_string()]
        );
    }

    /// Verifies missing eval dynamic constants fail through runtime status.
    #[test]
    fn execute_program_missing_constant_fetch_fails() {
        let program = parse_fragment(br#"return MissingEvalConst;"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let err = execute_program(&program, &mut scope, &mut values)
            .expect_err("missing constant should fail");

        assert_eq!(err, EvalStatus::RuntimeFatal);
    }

    /// Verifies eval class probes use the runtime class-name table.
    #[test]
    fn execute_program_class_exists_uses_runtime_probe() {
        let program = parse_fragment(
            br#"class DynProbe {}
echo class_exists("DynProbe") ? "Y" : "N";
echo class_exists("\dynprobe") ? "Y" : "N";
echo class_exists("KnownClass") ? "Y" : "N";
echo class_exists("\knownclass") ? "Y" : "N";
echo class_exists(class: "MissingClass", autoload: false) ? "Y" : "N";"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "YYYYN");
    }

    /// Verifies duplicate eval-declared class names fail through runtime status.
    #[test]
    fn execute_program_duplicate_class_declaration_fails() {
        let program = parse_fragment(
            br#"class DynProbeDup {}
class dynprobedup {}"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let err = execute_program(&program, &mut scope, &mut values).expect_err("duplicate fails");

        assert_eq!(err, EvalStatus::RuntimeFatal);
    }

    /// Verifies eval fragments can dispatch registered native AOT functions.
    #[test]
    fn execute_program_calls_registered_native_function() {
        let program = parse_fragment(br#"return native_answer();"#).expect("parse eval fragment");
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let expected = values.int(42).expect("allocate fake result");
        let native =
            NativeFunction::new(expected.as_ptr().cast(), fake_native_return_descriptor, 0);
        assert!(context
            .define_native_function("native_answer", native)
            .is_ok());

        let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
            .expect("execute eval ir");

        assert_eq!(result, expected);
    }

    /// Verifies direct eval calls can bind registered native parameters by name.
    #[test]
    fn execute_program_calls_registered_native_function_with_named_args() {
        let program = parse_fragment(br#"return native_answer(right: 2, left: 1);"#)
            .expect("parse eval fragment");
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let expected = values.int(42).expect("allocate fake result");
        let mut native =
            NativeFunction::new(expected.as_ptr().cast(), fake_native_return_descriptor, 2);
        assert!(native.set_param_name(0, "left"));
        assert!(native.set_param_name(1, "right"));
        assert!(context
            .define_native_function("native_answer", native)
            .is_ok());

        let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
            .expect("execute eval ir");

        assert_eq!(result, expected);
    }

    /// Verifies direct eval calls can unpack arrays into registered native parameters.
    #[test]
    fn execute_program_calls_registered_native_function_with_spread_args() {
        let program =
            parse_fragment(br#"return native_answer(...[1, 2]);"#).expect("parse eval fragment");
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let expected = values.int(42).expect("allocate fake result");
        let mut native =
            NativeFunction::new(expected.as_ptr().cast(), fake_native_return_descriptor, 2);
        assert!(native.set_param_name(0, "left"));
        assert!(native.set_param_name(1, "right"));
        assert!(context
            .define_native_function("native_answer", native)
            .is_ok());

        let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
            .expect("execute eval ir");

        assert_eq!(result, expected);
    }

    /// Verifies indexed array writes mutate an existing scope array.
    #[test]
    fn execute_program_writes_indexed_scope_array() {
        let program = parse_fragment(br#"$items = ["a"]; $items[1] = "b"; return $items[1];"#)
            .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::String("b".to_string()));
    }

    /// Verifies indexed array append writes use the next visible index.
    #[test]
    fn execute_program_appends_indexed_scope_array() {
        let program = parse_fragment(br#"$items = ["a"]; $items[] = "b"; return $items[1];"#)
            .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::String("b".to_string()));
    }

    /// Verifies associative append starts at key zero when only string keys exist.
    #[test]
    fn execute_program_appends_assoc_scope_array_with_string_keys() {
        let program =
            parse_fragment(br#"$items = ["name" => "Ada"]; $items[] = "Grace"; return $items[0];"#)
                .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::String("Grace".to_string()));
    }

    /// Verifies associative append uses one plus the largest existing integer key.
    #[test]
    fn execute_program_appends_assoc_scope_array_after_positive_int_key() {
        let program = parse_fragment(
            br#"$items = [2 => "two", "name" => "Ada"]; $items[] = "tail"; return $items[3];"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::String("tail".to_string()));
    }

    /// Verifies associative append preserves PHP's largest-negative-key behavior.
    #[test]
    fn execute_program_appends_assoc_scope_array_after_negative_int_key() {
        let program =
            parse_fragment(br#"$items = [-2 => "minus"]; $items[] = "tail"; return $items[-1];"#)
                .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::String("tail".to_string()));
    }

    /// Verifies mutating a borrowed scope array does not make the eval scope own it.
    #[test]
    fn execute_program_preserves_borrowed_array_ownership() {
        let program = parse_fragment(br#"$items[0] = "b";"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let array = values.array_new(1).expect("create fake array");
        scope.set("items", array, ScopeCellOwnership::Borrowed);

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
        let entry = scope.entry("items").expect("scope should contain items");

        assert_eq!(entry.cell(), array);
        assert_eq!(entry.flags().ownership, ScopeCellOwnership::Borrowed);
        assert!(values.releases.is_empty());
    }

    /// Verifies replacing an eval-owned scope value releases the old cell.
    #[test]
    fn execute_program_releases_replaced_scope_value() {
        let program = parse_fragment(br#"$x = "old"; $x = "new";"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.releases.len(), 1);
        assert_eq!(
            values.get(values.releases[0]),
            FakeValue::String("old".to_string())
        );
    }

    /// Verifies unsetting an eval-owned scope value releases the old cell.
    #[test]
    fn execute_program_releases_unset_scope_value() {
        let program = parse_fragment(br#"$x = "old"; unset($x);"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.releases.len(), 1);
        assert_eq!(
            values.get(values.releases[0]),
            FakeValue::String("old".to_string())
        );
    }

    /// Verifies break exits a runtime eval loop before later statements run.
    #[test]
    fn execute_program_break_exits_loop() {
        let program = parse_fragment(br#"while ($flag) { echo "a"; break; echo "b"; }"#)
            .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let flag = values.bool_value(true).expect("create fake bool");
        scope.set("flag", flag, ScopeCellOwnership::Owned);

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "a");
    }

    /// Verifies continue restarts a runtime eval loop and observes later scope updates.
    #[test]
    fn execute_program_continue_restarts_loop() {
        let program = parse_fragment(
            br#"while ($flag) { $flag = false; continue; echo "unreachable"; } echo "done";"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let flag = values.bool_value(true).expect("create fake bool");
        scope.set("flag", flag, ScopeCellOwnership::Owned);

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "done");
    }
}
