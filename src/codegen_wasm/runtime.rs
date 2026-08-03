//! Purpose:
//! Emits the hand-authored WebAssembly text (WAT) runtime for the wasm32-wasi
//! backend: the WASI imports a command module needs and the `__rt_*` helper
//! routines (currently integer echo). Runtime helpers are added to a `WatModule`
//! as raw `(func ...)` blocks.
//!
//! Called from:
//! - `crate::codegen_wasm::generate()` for command (main-bearing) modules.
//!
//! Key details:
//! - Low linear memory is reserved as runtime scratch:
//!     [0, 8)        iovec for `fd_write`: { buf_ptr @0 (i32), buf_len @4 (i32) }
//!     [8, 16)       `nwritten` cell for `fd_write` / `args_sizes_get` scratch (i32)
//!     [16, 64)      number-formatting buffer (itoa/ftoa), written back-to-front
//!     [64, 65600)   legacy concat reservation, retained for stable static-data offsets
//!   Compile-time data segments and the heap start at `RT_SCRATCH_END`.
//! - `__rt_concat` is heap-backed and bounds-checked. The legacy
//!   `$__concat_off` cursor remains as an ABI-compatible no-op for existing
//!   `ConcatReset` lowering; live strings never occupy the shared reservation.
//!   WASI imports and the echo/exit helpers are "command" runtime, emitted only
//!   for main-bearing modules (importing WASI forces `_start`-command semantics).

use super::wat::{DataSegment, FuncImport, Global, ValType, WatModule};

/// Base offset of the legacy string-concatenation reservation.
const CONCAT_BASE: u32 = 64;
/// Size of the legacy reservation retained to keep static-data addresses stable.
const CONCAT_SIZE: u32 = 65536;

/// First linear-memory offset available to data segments / the heap; everything
/// below this is reserved runtime scratch (number buffer + concat buffer).
pub(super) const RT_SCRATCH_END: u32 = CONCAT_BASE + CONCAT_SIZE;

/// Base of the dedicated float<->string scratch region. The strtod bignum buffers
/// (`__rt_digits_to_f64` / `__rt_str_to_f64`) and the later ftoa/itoa scratch live
/// here, above the concat buffer, so a parse or format never collides with an
/// in-flight string concatenation whose cursor would otherwise run through 0x4000.
/// Callers reach this base via the immutable `$__float_scratch` global.
pub(super) const FLOAT_SCRATCH_BASE: u32 = RT_SCRATCH_END;

/// Size of the float<->string scratch region. The strtod path uses offsets
/// 0..0x1200 (four 96-limb bignums at +0/+1024/+2048/+3072 and the digit buffer at
/// +4096); the ftoa/itoa scratch lands at +0x2000..+0x3000. 16 KiB bounds both.
pub(super) const FLOAT_SCRATCH_SIZE: u32 = 0x4000;

/// First byte reserved for command-runtime fatal diagnostics.
const COMMAND_DATA_BASE: u32 = FLOAT_SCRATCH_BASE + FLOAT_SCRATCH_SIZE;
const ERR_DIV_ZERO: &[u8] =
    b"PHP Fatal error: Uncaught DivisionByZeroError: Division by zero\n";
const ERR_MOD_ZERO: &[u8] =
    b"PHP Fatal error: Uncaught DivisionByZeroError: Modulo by zero\n";
const ERR_NEG_SHIFT: &[u8] =
    b"PHP Fatal error: Uncaught ArithmeticError: Bit shift by negative number\n";
const ERR_INTDIV_OVERFLOW: &[u8] = b"PHP Fatal error: Uncaught ArithmeticError: Division of PHP_INT_MIN by -1 is not an integer\n";
const ERR_WASI: &[u8] = b"PHP Fatal error: WASI operation failed\n";
const ERR_OOM: &[u8] = b"PHP Fatal error: Allowed memory size exhausted\n";
const ERR_HASH_APPEND_OCCUPIED: &[u8] =
    b"PHP Fatal error: Uncaught Error: Cannot add element to the array as the next element is already occupied\n";
const ERR_CALLABLE_DISPATCH: &[u8] =
    b"PHP Fatal error: Uncaught Error: Invalid callable dispatch\n";
const ERR_MIXED_HEAP_TYPE: &[u8] =
    b"PHP Fatal error: Uncaught TypeError: Value does not match the required heap type\n";
/// A PHP exception that reaches the top of `main` with no `catch` to receive it.
///
/// KNOWN DIVERGENCE: reference PHP names the class and message and prints the file, line and
/// stack trace (`Uncaught Exception: boom in /path.php:4`). Reproducing that needs the built-in
/// Throwable accessors, which this target does not lower yet, so the diagnostic is currently
/// class-agnostic. The EXIT STATUS is PHP's — 255 — which is the observable most callers act on.
const ERR_UNCAUGHT_EXCEPTION: &[u8] = b"PHP Fatal error: Uncaught exception\n";
const ERR_METHOD_CALL_PREFIX: &[u8] =
    b"PHP Fatal error: Uncaught Error: Call to a member function ";
const ERR_METHOD_CALL_SUFFIX: &[u8] = b"() on ";
const PHP_TYPE_INT: &[u8] = b"int\n";
const PHP_TYPE_STRING: &[u8] = b"string\n";
const PHP_TYPE_FLOAT: &[u8] = b"float\n";
const PHP_TYPE_BOOL: &[u8] = b"bool\n";
const PHP_TYPE_ARRAY: &[u8] = b"array\n";
const PHP_TYPE_NULL: &[u8] = b"null\n";
const PHP_TYPE_RESOURCE: &[u8] = b"resource\n";
const PHP_TYPE_CALLABLE: &[u8] = b"callable\n";
const PHP_TYPE_UNKNOWN: &[u8] = b"unknown\n";
const ERR_UNDEFINED_METHOD_PREFIX: &[u8] =
    b"PHP Fatal error: Uncaught Error: Call to undefined method ";
const ERR_UNDEFINED_METHOD_SEPARATOR: &[u8] = b"::";
const ERR_UNDEFINED_METHOD_SUFFIX: &[u8] = b"()\n";
const WARN_UNDEFINED_ARRAY_KEY_PREFIX: &[u8] = b"Warning: Undefined array key ";
const WARN_QUOTE: &[u8] = b"\"";
const WARN_SUFFIX: &[u8] = b"\n";
/// PHP 8.5 alone diagnoses a float whose value no integer can represent. The
/// rendered float sits between the two fragments, formatted by `__rt_ftoa`.
const WARN_FLOAT_NOT_REPRESENTABLE_PREFIX: &[u8] = b"Warning: The float ";
const WARN_FLOAT_NOT_REPRESENTABLE_SUFFIX: &[u8] =
    b" is not representable as an int, cast occurred\n";
/// Arithmetic on a string carrying only a numeric prefix warns and uses the prefix.
const WARN_NON_NUMERIC_VALUE: &[u8] = b"Warning: A non-numeric value encountered\n";
/// The ONE diagnostic in PHP's whole scalar-cast family: `(string)` of an array. Measured —
/// `(int)`, `(float)` and `(bool)` of an array are all silent.
const WARN_ARRAY_TO_STRING: &[u8] = b"Warning: Array to string conversion\n";
/// PHP reports an object reaching a numeric cast, then uses 1. The class name sits between
/// the prefix and the per-target suffix.
const WARN_OBJECT_TO_SCALAR_PREFIX: &[u8] = b"Warning: Object of class ";
const WARN_OBJECT_TO_INT_SUFFIX: &[u8] = b" could not be converted to int\n";
const WARN_OBJECT_TO_FLOAT_SUFFIX: &[u8] = b" could not be converted to float\n";
/// `(string)` of an object without `__toString` is a FATAL, not a warning — the one place
/// this family stops at a diagnostic and terminates.
const ERR_OBJECT_TO_STRING_PREFIX: &[u8] = b"PHP Fatal error: Uncaught Error: Object of class ";
const ERR_OBJECT_TO_STRING_SUFFIX: &[u8] = b" could not be converted to string\n";
/// Arithmetic on a wholly non-numeric string is a PHP `TypeError`. Reported as an
/// uncaught fatal until this target gains exception support.
const ERR_UNSUPPORTED_OPERAND: &[u8] =
    b"PHP Fatal error: Uncaught TypeError: Unsupported operand types\n";
/// `str_repeat()` with a negative count is a PHP `ValueError`, catchable like any other.
const ERR_STR_REPEAT_NEGATIVE: &[u8] = b"PHP Fatal error: Uncaught ValueError: str_repeat(): Argument #2 ($times) must be greater than or equal to 0\n";
/// `str_pad()` with an empty pad string is a PHP `ValueError` when padding is actually needed.
const ERR_STR_PAD_EMPTY: &[u8] = b"PHP Fatal error: Uncaught ValueError: str_pad(): Argument #3 ($pad_string) must not be empty\n";
/// `explode()` with an empty separator is a PHP `ValueError`; without it the split loops.
const ERR_EXPLODE_EMPTY_SEP: &[u8] = b"PHP Fatal error: Uncaught ValueError: explode(): Argument #1 ($separator) must not be empty\n";
/// `str_split()` with a non-positive chunk length is a PHP `ValueError`.
const ERR_STR_SPLIT_LENGTH: &[u8] = b"PHP Fatal error: Uncaught ValueError: str_split(): Argument #2 ($length) must be greater than 0\n";
/// `chr()` outside `[0, 255]` still answers, wrapping modulo 256, but is deprecated since 8.5.
const DEPRECATED_CHR_RANGE: &[u8] = b"Deprecated: chr(): Providing a value not in-between 0 and 255 is deprecated, this is because a byte value must be in the [0, 255] interval. The value used will be constrained using % 256\n";
/// `ord()` on anything but exactly one byte still answers, but is deprecated since 8.5.
const DEPRECATED_ORD_LENGTH: &[u8] =
    b"Deprecated: ord(): Providing a string that is not one byte long is deprecated. Use ord($str[0]) instead\n";

/// First byte available to PHP string literals in a command module.
pub(super) const COMMAND_DATA_END: u32 = COMMAND_DATA_BASE
    + ERR_DIV_ZERO.len() as u32
    + ERR_MOD_ZERO.len() as u32
    + ERR_NEG_SHIFT.len() as u32
    + ERR_INTDIV_OVERFLOW.len() as u32
    + ERR_WASI.len() as u32
    + ERR_OOM.len() as u32
    + ERR_HASH_APPEND_OCCUPIED.len() as u32
    + ERR_CALLABLE_DISPATCH.len() as u32
    + ERR_MIXED_HEAP_TYPE.len() as u32
    + ERR_UNCAUGHT_EXCEPTION.len() as u32
    + ERR_STR_REPEAT_NEGATIVE.len() as u32
    + ERR_STR_PAD_EMPTY.len() as u32
    + ERR_EXPLODE_EMPTY_SEP.len() as u32
    + ERR_STR_SPLIT_LENGTH.len() as u32
    + ERR_METHOD_CALL_PREFIX.len() as u32
    + ERR_METHOD_CALL_SUFFIX.len() as u32
    + PHP_TYPE_INT.len() as u32
    + PHP_TYPE_STRING.len() as u32
    + PHP_TYPE_FLOAT.len() as u32
    + PHP_TYPE_BOOL.len() as u32
    + PHP_TYPE_ARRAY.len() as u32
    + PHP_TYPE_NULL.len() as u32
    + PHP_TYPE_RESOURCE.len() as u32
    + PHP_TYPE_CALLABLE.len() as u32
    + PHP_TYPE_UNKNOWN.len() as u32
    + ERR_UNDEFINED_METHOD_PREFIX.len() as u32
    + ERR_UNDEFINED_METHOD_SEPARATOR.len() as u32
    + ERR_UNDEFINED_METHOD_SUFFIX.len() as u32
    + WARN_UNDEFINED_ARRAY_KEY_PREFIX.len() as u32
    + WARN_QUOTE.len() as u32
    + WARN_SUFFIX.len() as u32
    + crate::ir::ARRAY_OFFSET_ON_NULL_WARNING_PHP82.len() as u32
    + crate::ir::ARRAY_OFFSET_ON_NULL_WARNING.len() as u32
    + WARN_FLOAT_NOT_REPRESENTABLE_PREFIX.len() as u32
    + WARN_FLOAT_NOT_REPRESENTABLE_SUFFIX.len() as u32
    + WARN_NON_NUMERIC_VALUE.len() as u32
    + WARN_OBJECT_TO_SCALAR_PREFIX.len() as u32
    + WARN_OBJECT_TO_INT_SUFFIX.len() as u32
    + WARN_OBJECT_TO_FLOAT_SUFFIX.len() as u32
    + ERR_UNSUPPORTED_OPERAND.len() as u32
    + DEPRECATED_CHR_RANGE.len() as u32
    + DEPRECATED_ORD_LENGTH.len() as u32
    + WARN_ARRAY_TO_STRING.len() as u32
    + ERR_OBJECT_TO_STRING_PREFIX.len() as u32
    + ERR_OBJECT_TO_STRING_SUFFIX.len() as u32;

/// Adds the import-free runtime every module needs: the compatibility concat
/// cursor global and the heap-backed `__rt_concat` helper.
pub(super) fn emit_common_runtime(wm: &mut WatModule) {
    wm.add_global(Global {
        name: "__concat_off".to_string(),
        ty: ValType::I32,
        mutable: true,
        init: CONCAT_BASE as i64,
    });
    wm.add_raw_func(RT_CONCAT);
}

/// Adds the WASI imports and `__rt_*` helpers a command (main-bearing) module needs.
///
/// Imports `proc_exit` and `fd_write` from `wasi_snapshot_preview1` and registers
/// the echo helpers. Must be called before functions that reference these symbols
/// are rendered.
pub(super) fn emit_command_runtime(wm: &mut WatModule) {
    wm.import_func(FuncImport {
        module: "wasi_snapshot_preview1".to_string(),
        field: "proc_exit".to_string(),
        internal: "wasi_proc_exit".to_string(),
        params: vec![ValType::I32],
        results: vec![],
    });
    wm.import_func(FuncImport {
        module: "wasi_snapshot_preview1".to_string(),
        field: "fd_write".to_string(),
        internal: "wasi_fd_write".to_string(),
        // fd, iovs_ptr, iovs_len, nwritten_ptr -> errno
        params: vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32],
        results: vec![ValType::I32],
    });
    wm.import_func(FuncImport {
        module: "wasi_snapshot_preview1".to_string(),
        field: "args_sizes_get".to_string(),
        internal: "wasi_args_sizes_get".to_string(),
        // argc_ptr, argv_buf_size_ptr -> errno
        params: vec![ValType::I32, ValType::I32],
        results: vec![ValType::I32],
    });
    wm.import_func(FuncImport {
        module: "wasi_snapshot_preview1".to_string(),
        field: "args_get".to_string(),
        internal: "wasi_args_get".to_string(),
        // argv_ptr_array, argv_buf -> errno
        params: vec![ValType::I32, ValType::I32],
        results: vec![ValType::I32],
    });
    emit_failure_runtime(wm);
    wm.add_raw_func(RT_WASI_WRITE_ALL);
    wm.add_raw_func(RT_WASI_WRITE_OR_FAIL);
    wm.add_raw_func(RT_ECHO_I64);
    wm.add_raw_func(RT_ECHO_F64);
    wm.add_raw_func(RT_ECHO_STR);
    wm.add_raw_func(RT_ECHO_BOOL);
    wm.add_raw_func(RT_ARGC);
    wm.add_raw_func(RT_STRLEN_C);
    wm.add_raw_func(RT_ARGV);
    wm.add_raw_func(RT_MIXED_WRITE_STDOUT);
}

/// Emits immutable diagnostic data and the command-runtime failure dispatcher.
///
/// Error code 1 is division by zero, 2 modulo by zero, 3 a negative shift,
/// 4 `PHP_INT_MIN / -1` for integer division, 5 a WASI boundary failure, and
/// 6 allocator exhaustion or arithmetic overflow, 7 an occupied saturated
/// array append key, 8 a rejected callable dispatch, 9 a runtime Mixed
/// heap-kind mismatch, and 10 a PHP exception that reached the top of `main`
/// uncaught.
/// The helper writes the selected message to stderr, exits with status 255, and
/// ends in `unreachable` so validation does not treat `proc_exit` as returning.
/// The same data region also owns the warning fragments used by the non-fatal
/// undefined-index diagnostic.
fn emit_failure_runtime(wm: &mut WatModule) {
    let fixed_messages = [
        ERR_DIV_ZERO,
        ERR_MOD_ZERO,
        ERR_NEG_SHIFT,
        ERR_INTDIV_OVERFLOW,
        ERR_WASI,
        ERR_OOM,
        ERR_HASH_APPEND_OCCUPIED,
        ERR_CALLABLE_DISPATCH,
        ERR_MIXED_HEAP_TYPE,
        ERR_UNCAUGHT_EXCEPTION,
        ERR_STR_REPEAT_NEGATIVE,
        ERR_STR_PAD_EMPTY,
        ERR_EXPLODE_EMPTY_SEP,
        ERR_STR_SPLIT_LENGTH,
    ];
    let method_messages = [
        ERR_METHOD_CALL_PREFIX,
        ERR_METHOD_CALL_SUFFIX,
        PHP_TYPE_INT,
        PHP_TYPE_STRING,
        PHP_TYPE_FLOAT,
        PHP_TYPE_BOOL,
        PHP_TYPE_ARRAY,
        PHP_TYPE_NULL,
        PHP_TYPE_RESOURCE,
        PHP_TYPE_CALLABLE,
        PHP_TYPE_UNKNOWN,
        ERR_UNDEFINED_METHOD_PREFIX,
        ERR_UNDEFINED_METHOD_SEPARATOR,
        ERR_UNDEFINED_METHOD_SUFFIX,
    ];
    let warning_messages = [
        WARN_UNDEFINED_ARRAY_KEY_PREFIX,
        WARN_QUOTE,
        WARN_SUFFIX,
        crate::ir::ARRAY_OFFSET_ON_NULL_WARNING_PHP82.as_bytes(),
        crate::ir::ARRAY_OFFSET_ON_NULL_WARNING.as_bytes(),
        WARN_FLOAT_NOT_REPRESENTABLE_PREFIX,
        WARN_FLOAT_NOT_REPRESENTABLE_SUFFIX,
        WARN_NON_NUMERIC_VALUE,
        ERR_UNSUPPORTED_OPERAND,
        WARN_OBJECT_TO_SCALAR_PREFIX,
        WARN_OBJECT_TO_INT_SUFFIX,
        WARN_OBJECT_TO_FLOAT_SUFFIX,
        DEPRECATED_CHR_RANGE,
        DEPRECATED_ORD_LENGTH,
        WARN_ARRAY_TO_STRING,
        ERR_OBJECT_TO_STRING_PREFIX,
        ERR_OBJECT_TO_STRING_SUFFIX,
    ];
    let mut offsets = Vec::with_capacity(fixed_messages.len());
    let mut cursor = COMMAND_DATA_BASE;
    for message in fixed_messages {
        offsets.push((cursor, message.len() as u32));
        wm.add_data(DataSegment {
            offset: cursor,
            bytes: message.to_vec(),
        });
        cursor += message.len() as u32;
    }
    let mut method_offsets = Vec::with_capacity(method_messages.len());
    for message in method_messages {
        method_offsets.push((cursor, message.len() as u32));
        wm.add_data(DataSegment {
            offset: cursor,
            bytes: message.to_vec(),
        });
        cursor += message.len() as u32;
    }
    let mut warning_offsets = Vec::with_capacity(warning_messages.len());
    for message in warning_messages {
        warning_offsets.push((cursor, message.len() as u32));
        wm.add_data(DataSegment {
            offset: cursor,
            bytes: message.to_vec(),
        });
        cursor += message.len() as u32;
    }
    debug_assert_eq!(cursor, COMMAND_DATA_END);

    let mut wat = String::from(
        "(func $__rt_fail (param $code i32)\n  (local $ptr i32) (local $len i32)\n",
    );
    for (index, (offset, len)) in offsets.iter().enumerate() {
        wat.push_str(&format!(
            "  (if (i32.eq (local.get $code) (i32.const {}))\n    (then\n      (local.set $ptr (i32.const {}))\n      (local.set $len (i32.const {}))))\n",
            index + 1,
            offset,
            len
        ));
    }
    wat.push_str(
        "  (drop (call $__rt_wasi_write_all (i32.const 2) (local.get $ptr) (local.get $len)))\n  (call $wasi_proc_exit (i32.const 255))\n  unreachable ;; elephc-trap:post-noreturn:runtime-fatal-exit\n)",
    );
    wm.add_raw_func(&wat);
    emit_method_call_failure_runtime(wm, &method_offsets);
    emit_undefined_array_key_warning_runtime(wm, &warning_offsets);
}

/// Emits the fatal path used when a `Mixed` receiver is not an object.
///
/// The helper composes the PHP-visible method name with the runtime Mixed tag,
/// writes the diagnostic to stderr, and terminates with PHP's fatal status 255.
fn emit_method_call_failure_runtime(wm: &mut WatModule, offsets: &[(u32, u32)]) {
    debug_assert_eq!(offsets.len(), 14);
    let (prefix_ptr, prefix_len) = offsets[0];
    let (suffix_ptr, suffix_len) = offsets[1];
    let type_offsets = &offsets[2..11];
    let mut wat = format!(
        "(func $__rt_fail_method_call_non_object (param $method_ptr i32) (param $method_len i32) (param $tag i32)\n  (local $type_ptr i32) (local $type_len i32)\n  (drop (call $__rt_wasi_write_all (i32.const 2) (i32.const {prefix_ptr}) (i32.const {prefix_len})))\n  (drop (call $__rt_wasi_write_all (i32.const 2) (local.get $method_ptr) (local.get $method_len)))\n  (drop (call $__rt_wasi_write_all (i32.const 2) (i32.const {suffix_ptr}) (i32.const {suffix_len})))\n  (local.set $type_ptr (i32.const {}))\n  (local.set $type_len (i32.const {}))\n",
        type_offsets[8].0, type_offsets[8].1
    );
    for (tag, type_index) in [
        (0, 0),
        (1, 1),
        (2, 2),
        (3, 3),
        (4, 4),
        (5, 4),
        (8, 5),
        (9, 6),
        (10, 7),
    ] {
        let (type_ptr, type_len) = type_offsets[type_index];
        wat.push_str(&format!(
            "  (if (i32.eq (local.get $tag) (i32.const {tag}))\n    (then\n      (local.set $type_ptr (i32.const {type_ptr}))\n      (local.set $type_len (i32.const {type_len}))))\n"
        ));
    }
    wat.push_str(
        "  (drop (call $__rt_wasi_write_all (i32.const 2) (local.get $type_ptr) (local.get $type_len)))\n  (call $wasi_proc_exit (i32.const 255))\n  unreachable ;; elephc-trap:post-noreturn:method-type-fatal-exit\n)",
    );
    wm.add_raw_func(&wat);
    emit_undefined_method_failure_runtime(wm, &offsets[11..14]);
}

/// Emits the fatal path used when an object has no matching method dispatch arm.
///
/// The runtime class-name table supplies the concrete class name while the
/// instruction's interned method-name bytes complete PHP's undefined-method text.
fn emit_undefined_method_failure_runtime(wm: &mut WatModule, offsets: &[(u32, u32)]) {
    debug_assert_eq!(offsets.len(), 3);
    let (prefix_ptr, prefix_len) = offsets[0];
    let (separator_ptr, separator_len) = offsets[1];
    let (suffix_ptr, suffix_len) = offsets[2];
    wm.add_raw_func(&format!(
        r#"(func $__rt_fail_undefined_method (param $cid i64) (param $method_ptr i32) (param $method_len i32)
  (local $class_ptr i32) (local $class_len i64)
  (call $__rt_class_name_by_cid (local.get $cid))
  (local.set $class_len)
  (local.set $class_ptr)
  (drop (call $__rt_wasi_write_all (i32.const 2) (i32.const {prefix_ptr}) (i32.const {prefix_len})))
  (drop (call $__rt_wasi_write_all (i32.const 2) (local.get $class_ptr) (i32.wrap_i64 (local.get $class_len))))
  (drop (call $__rt_wasi_write_all (i32.const 2) (i32.const {separator_ptr}) (i32.const {separator_len})))
  (drop (call $__rt_wasi_write_all (i32.const 2) (local.get $method_ptr) (local.get $method_len)))
  (drop (call $__rt_wasi_write_all (i32.const 2) (i32.const {suffix_ptr}) (i32.const {suffix_len})))
  (call $wasi_proc_exit (i32.const 255))
  unreachable ;; elephc-trap:post-noreturn:undefined-method-fatal-exit
)"#
    ));
}

/// Emits PHP's non-fatal warning for a missing integer array index.
///
/// The key is formatted through the shared signed `__rt_itoa` helper, including
/// `i64::MIN`, and every stderr fragment uses the checked WASI write path. The
/// helper returns normally so the caller can continue with the already-produced
/// null value. A no-argument companion emits the exact offset-on-null warning.
fn emit_undefined_array_key_warning_runtime(
    wm: &mut WatModule,
    offsets: &[(u32, u32)],
) {
    debug_assert_eq!(offsets.len(), 17);
    let (prefix_ptr, prefix_len) = offsets[0];
    let (quote_ptr, quote_len) = offsets[1];
    let (suffix_ptr, suffix_len) = offsets[2];
    let (chr_range_ptr, chr_range_len) = offsets[12];
    let (ord_length_ptr, ord_length_len) = offsets[13];
    let (array_to_string_ptr, array_to_string_len) = offsets[14];
    let (object_string_prefix_ptr, object_string_prefix_len) = offsets[15];
    let (object_string_suffix_ptr, object_string_suffix_len) = offsets[16];
    let (float_prefix_ptr, float_prefix_len) = offsets[5];
    let (float_suffix_ptr, float_suffix_len) = offsets[6];
    let (non_numeric_ptr, non_numeric_len) = offsets[7];
    let (operand_ptr, operand_len) = offsets[8];
    let (object_prefix_ptr, object_prefix_len) = offsets[9];
    let (object_int_ptr, object_int_len) = offsets[10];
    let (object_float_ptr, object_float_len) = offsets[11];
    let (offset_on_null_ptr, offset_on_null_len) =
        if crate::codegen_support::runtime::array_offset_on_null_warning()
            == crate::ir::ARRAY_OFFSET_ON_NULL_WARNING_PHP82
        {
            offsets[3]
        } else {
            offsets[4]
        };
    wm.add_raw_func(&format!(
        r#"(func $__rt_warn_undefined_array_key_int (param $key i64)
  (local $key_ptr i32) (local $key_len i32)
  (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {prefix_ptr}) (i32.const {prefix_len}))
  (call $__rt_itoa (local.get $key) (global.get $__float_scratch))
  (local.set $key_len)
  (local.set $key_ptr)
  (call $__rt_wasi_write_or_fail (i32.const 2) (local.get $key_ptr) (local.get $key_len))
  (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {suffix_ptr}) (i32.const {suffix_len})))"#
    ));
    wm.add_raw_func(&format!(
        r#"(func $__rt_warn_undefined_array_key_str (param $key_ptr i32) (param $key_len i32)
  (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {prefix_ptr}) (i32.const {prefix_len}))
  (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {quote_ptr}) (i32.const {quote_len}))
  (call $__rt_wasi_write_or_fail (i32.const 2) (local.get $key_ptr) (local.get $key_len))
  (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {quote_ptr}) (i32.const {quote_len}))
  (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {suffix_ptr}) (i32.const {suffix_len})))"#
    ));
    wm.add_raw_func(&format!(
        r#"(func $__rt_fail_object_to_string (param $cid i64)
  (local $ptr i32) (local $len i64)
  (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {object_string_prefix_ptr}) (i32.const {object_string_prefix_len}))
  (call $__rt_class_name_by_cid (local.get $cid))                 ;; resolve the class name -> (ptr, len)
  (local.set $len)
  (local.set $ptr)
  (call $__rt_wasi_write_or_fail (i32.const 2) (local.get $ptr) (i32.wrap_i64 (local.get $len)))
  (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {object_string_suffix_ptr}) (i32.const {object_string_suffix_len}))
  (call $wasi_proc_exit (i32.const 255))
  unreachable ;; elephc-trap:post-noreturn:object-to-string-fatal
)"#
    ));
    wm.add_raw_func(
        r#"(func $__rt_echo_array_word
  ;; The five bytes of "Array" are written into the float scratch rather than carried as a
  ;; data segment, so this stays independent of the module's static-data layout.
  (i32.store8 (global.get $__float_scratch) (i32.const 65))
  (i32.store8 (i32.add (global.get $__float_scratch) (i32.const 1)) (i32.const 114))
  (i32.store8 (i32.add (global.get $__float_scratch) (i32.const 2)) (i32.const 114))
  (i32.store8 (i32.add (global.get $__float_scratch) (i32.const 3)) (i32.const 97))
  (i32.store8 (i32.add (global.get $__float_scratch) (i32.const 4)) (i32.const 121))
  (call $__rt_echo_str (global.get $__float_scratch) (i64.const 5)))"#,
    );
    wm.add_raw_func(&format!(
        r#"(func $__rt_warn_array_to_string
  (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {array_to_string_ptr}) (i32.const {array_to_string_len})))"#
    ));
    wm.add_raw_func(&format!(
        r#"(func $__rt_warn_array_offset_on_null
  (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {offset_on_null_ptr}) (i32.const {offset_on_null_len})))"#
    ));
    wm.add_raw_func(&format!(
        r#"(func $__rt_warn_float_not_representable (param $bits i64)
  (local $ptr i32) (local $len i32)
  (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {float_prefix_ptr}) (i32.const {float_prefix_len}))  ;; "Warning: The float "
  (call $__rt_ftoa (local.get $bits) (i32.add (global.get $__float_scratch) (i32.const 1024)) (i32.const 80) (i32.add (global.get $__float_scratch) (i32.const 2048)) (i32.const 792) (i32.add (global.get $__float_scratch) (i32.const 4096)))  ;; render the offending float exactly as PHP prints it
  (local.set $len)                                                ;; ftoa returns (ptr, len): pop the length first
  (local.set $ptr)                                                ;; then the pointer
  (call $__rt_wasi_write_or_fail (i32.const 2) (local.get $ptr) (local.get $len))  ;; the float text itself
  (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {float_suffix_ptr}) (i32.const {float_suffix_len})))  ;; " is not representable as an int, cast occurred\n""#
    ));
    // The class name is looked up from the runtime class id, so one helper per target type
    // covers every class rather than needing a per-class message.
    wm.add_raw_func(&format!(
        r#"(func $__rt_warn_object_to_int (param $cid i64)
  (local $ptr i32) (local $len i64)
  (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {object_prefix_ptr}) (i32.const {object_prefix_len}))  ;; "Warning: Object of class "
  (call $__rt_class_name_by_cid (local.get $cid))                 ;; resolve the class name -> (ptr, len)
  (local.set $len)                                                ;; pop the name length
  (local.set $ptr)                                                ;; pop the name pointer
  (call $__rt_wasi_write_or_fail (i32.const 2) (local.get $ptr) (i32.wrap_i64 (local.get $len)))  ;; the class name itself
  (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {object_int_ptr}) (i32.const {object_int_len})))  ;; " could not be converted to int\n""#
    ));
    wm.add_raw_func(&format!(
        r#"(func $__rt_warn_object_to_float (param $cid i64)
  (local $ptr i32) (local $len i64)
  (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {object_prefix_ptr}) (i32.const {object_prefix_len}))  ;; "Warning: Object of class "
  (call $__rt_class_name_by_cid (local.get $cid))                 ;; resolve the class name -> (ptr, len)
  (local.set $len)                                                ;; pop the name length
  (local.set $ptr)                                                ;; pop the name pointer
  (call $__rt_wasi_write_or_fail (i32.const 2) (local.get $ptr) (i32.wrap_i64 (local.get $len)))  ;; the class name itself
  (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {object_float_ptr}) (i32.const {object_float_len})))  ;; " could not be converted to float\n""#
    ));
    wm.add_raw_func(&format!(
        r#"(func $__rt_warn_non_numeric_value
  (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {non_numeric_ptr}) (i32.const {non_numeric_len})))"#
    ));
    wm.add_raw_func(&format!(
        r#"(func $__rt_fatal_unsupported_operand
  (drop (call $__rt_wasi_write_all (i32.const 2) (i32.const {operand_ptr}) (i32.const {operand_len})))
  (call $wasi_proc_exit (i32.const 255))
  unreachable ;; elephc-trap:post-noreturn:unsupported-operand-fatal-exit
)"#
    ));
    if matches!(
        crate::codegen_support::compile_php_version(),
        crate::web_prelude::PhpVersion::Php85
    ) {
        // Registered here, not in `emit_float_runtime`: the diagnosing conversion
        // depends on the warning helper above, which only command modules carry.
        wm.add_raw_func(super::float::RT_FLOAT_TO_INT_WARN);
        // PHP 8.5 alone deprecates a `chr()` argument outside a byte and an `ord()` argument
        // that is not exactly one byte. Both still ANSWER — the value is unchanged, only the
        // diagnostic is new — so earlier profiles get the same result with no message.
        wm.add_raw_func(&format!(
            r#"(func $__rt_deprecated_chr_range
  (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {chr_range_ptr}) (i32.const {chr_range_len})))"#
        ));
        wm.add_raw_func(&format!(
            r#"(func $__rt_deprecated_ord_length
  (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {ord_length_ptr}) (i32.const {ord_length_len})))"#
        ));
    }
    super::mixed_numeric::emit_mixed_numeric_runtime(wm);
}

/// Repeatedly invokes WASI `fd_write` until every requested byte is written.
///
/// Returns the first host errno. A zero-progress write or an impossible
/// `nwritten > remaining` response returns WASI `ERRNO_IO` (29), preventing an
/// infinite loop or pointer underflow. The single iovec and `nwritten` cell use
/// the reserved low-memory scratch region.
const RT_WASI_WRITE_ALL: &str =
    r#"(func $__rt_wasi_write_all (param $fd i32) (param $ptr i32) (param $len i32) (result i32)
  (local $remaining i32) (local $cursor i32) (local $errno i32) (local $written i32)
  (local.set $remaining (local.get $len))                         ;; bytes still to write
  (local.set $cursor (local.get $ptr))                            ;; next byte address
  (block $done
    (loop $write
      (br_if $done (i32.eqz (local.get $remaining)))              ;; all bytes written
      (i32.store (i32.const 0) (local.get $cursor))               ;; iovec.buf_ptr
      (i32.store (i32.const 4) (local.get $remaining))            ;; iovec.buf_len
      (local.set $errno
        (call $wasi_fd_write (local.get $fd) (i32.const 0) (i32.const 1) (i32.const 8))) ;; host write
      (if (i32.ne (local.get $errno) (i32.const 0))
        (then (return (local.get $errno))))                       ;; propagate host errno
      (local.set $written (i32.load (i32.const 8)))               ;; bytes accepted by host
      (if (i32.or
            (i32.eqz (local.get $written))
            (i32.gt_u (local.get $written) (local.get $remaining)))
        (then (return (i32.const 29))))                           ;; ERRNO_IO on no/invalid progress
      (local.set $cursor (i32.add (local.get $cursor) (local.get $written))) ;; advance source
      (local.set $remaining (i32.sub (local.get $remaining) (local.get $written))) ;; shrink tail
      (br $write)))
  (i32.const 0))                                                  ;; success"#;

/// Writes every requested byte or converts a WASI host error into the command
/// runtime's deterministic fatal diagnostic and exit status.
///
/// `__rt_fail` deliberately calls `__rt_wasi_write_all` directly for its
/// best-effort stderr diagnostic, avoiding recursion if stderr itself fails.
const RT_WASI_WRITE_OR_FAIL: &str =
    r#"(func $__rt_wasi_write_or_fail (param $fd i32) (param $ptr i32) (param $len i32)
  (if (i32.ne
        (call $__rt_wasi_write_all (local.get $fd) (local.get $ptr) (local.get $len))
        (i32.const 0))
    (then
      (call $__rt_fail (i32.const 5))
      unreachable))) ;; elephc-trap:post-noreturn:wasi-write-failure
"#;

/// `__rt_argc`: returns PHP's `$argc` (the process argument count) via WASI
/// `args_sizes_get`, which writes the count to the number-buffer scratch region.
const RT_ARGC: &str = r#"(func $__rt_argc (result i64)
  (local $errno i32)
  (local.set $errno (call $wasi_args_sizes_get (i32.const 16) (i32.const 20))) ;; argc@16, argv_buf_size@20
  (if (i32.ne (local.get $errno) (i32.const 0))
    (then
      (call $__rt_fail (i32.const 5))
      unreachable))                                               ;; elephc-trap:post-noreturn:argc-sizes-failure args_sizes_get failed
  (i64.extend_i32_u (i32.load (i32.const 16))))                    ;; return argc as i64"#;

/// `__rt_strlen_c`: byte length of a NUL-terminated C string (used to measure the
/// WASI argv entries before copying them into PHP strings).
const RT_STRLEN_C: &str = r#"(func $__rt_strlen_c (param $p i32) (result i32)
  (local $n i32)
  (local.set $n (i32.const 0))                               ;; n = 0
  (block $end (loop $scan
    (br_if $end (i32.eqz (i32.load8_u (i32.add (local.get $p) (local.get $n)))))  ;; stop at the NUL terminator
    (local.set $n (i32.add (local.get $n) (i32.const 1)))    ;; n++
    (br $scan)))                                             ;; continue scanning
  (local.get $n))                                            ;; return byte count"#;

/// `__rt_argv`: builds PHP's `$argv` as an indexed string array via WASI
/// `args_sizes_get` + `args_get`. Temporary heap buffers hold the WASI pointer
/// array and argument byte buffer; each argument is copied (persisted) into the
/// array via `__rt_array_push_str`, after which the temporaries are freed.
const RT_ARGV: &str = r#"(func $__rt_argv (result i32)
  (local $argc i32)
  (local $bufsize i32)
  (local $ptrs i32)
  (local $buf i32)
  (local $arr i32)
  (local $i i32)
  (local $argp i32)
  (local $len i32)
  (local $errno i32)
  (local.set $errno (call $wasi_args_sizes_get (i32.const 16) (i32.const 20))) ;; query argc and byte size
  (if (i32.ne (local.get $errno) (i32.const 0))
    (then
      (call $__rt_fail (i32.const 5))
      unreachable))                                                 ;; elephc-trap:post-noreturn:argv-sizes-failure args_sizes_get failed
  (local.set $argc (i32.load (i32.const 16)))                        ;; load argc from scratch
  (local.set $bufsize (i32.load (i32.const 20)))                     ;; load argv byte-buffer size
  (if (i32.gt_u (local.get $argc) (i32.const 1073741823))
    (then
      (call $__rt_fail (i32.const 5))
      unreachable))                                                 ;; elephc-trap:post-noreturn:argv-count-overflow argc * 4 must not wrap wasm32
  (local.set $ptrs (call $__rt_heap_alloc (i32.mul (local.get $argc) (i32.const 4))))  ;; argc i32 pointers
  (local.set $buf (call $__rt_heap_alloc (local.get $bufsize)))      ;; argv byte buffer
  (local.set $errno (call $wasi_args_get (local.get $ptrs) (local.get $buf))) ;; fill pointer array + buffer
  (if (i32.ne (local.get $errno) (i32.const 0))
    (then
      (call $__rt_heap_free (local.get $ptrs))
      (call $__rt_heap_free (local.get $buf))
      (call $__rt_fail (i32.const 5))
      unreachable))                                                 ;; elephc-trap:post-noreturn:argv-get-failure args_get failed after balanced cleanup
  (local.set $arr (call $__rt_array_new (i64.extend_i32_u (local.get $argc)) (i64.const 16)))  ;; string array
  (local.set $i (i32.const 0))                                       ;; i = 0
  (block $end (loop $loop
    (br_if $end (i32.ge_u (local.get $i) (local.get $argc)))         ;; exit loop when all args processed
    (local.set $argp (i32.load (i32.add (local.get $ptrs) (i32.mul (local.get $i) (i32.const 4)))))  ;; argv[i] (C string)
    (local.set $len (call $__rt_strlen_c (local.get $argp)))         ;; its byte length
    (local.set $arr (call $__rt_array_push_str (local.get $arr) (local.get $argp) (i64.extend_i32_u (local.get $len))))  ;; append a persisted copy
    (local.set $i (i32.add (local.get $i) (i32.const 1)))            ;; i++
    (br $loop)))                                                     ;; next arg
  (call $__rt_heap_free (local.get $ptrs))                          ;; temporaries no longer needed (args were copied)
  (call $__rt_heap_free (local.get $buf))                            ;; free the argument byte buffer
  (local.get $arr))                                                  ;; return the argv array"#;

/// `__rt_mixed_write_stdout`: echoes a boxed Mixed value by dispatching on its tag:
/// int (0) via `__rt_echo_i64`, float (2) via `__rt_echo_f64` (`%.14G`), string (1)
/// via `__rt_echo_str`, bool (3) via `__rt_echo_bool`; null (8) and non-scalar tags
/// print nothing (PHP semantics).
const RT_MIXED_WRITE_STDOUT: &str = r#"(func $__rt_mixed_write_stdout (param $ptr i32)
  (local $tag i64)
  (local $sptr i32)
  (local $len i32)
  (if (i32.eqz (local.get $ptr))
    (then (return)))                                                ;; null pointer -> nothing
  (local.set $tag (i64.load (local.get $ptr)))                      ;; tag @ +0
  (if (i64.eqz (local.get $tag))                                    ;; tag 0 = int
    (then
      (call $__rt_echo_i64 (i64.load (i32.add (local.get $ptr) (i32.const 8)))) ;; echo int payload (lo @ +8)
      (return)))                                                     ;; done
  (if (i64.eq (local.get $tag) (i64.const 1))                       ;; tag 1 = string
    (then
      (call $__rt_echo_str
        (i32.wrap_i64 (i64.load (i32.add (local.get $ptr) (i32.const 8))))
        (i64.load (i32.add (local.get $ptr) (i32.const 16))))        ;; echo string (ptr, len)
      (return)))                                                     ;; done
  (if (i64.eq (local.get $tag) (i64.const 2))                       ;; tag 2 = float
    (then
      (call $__rt_echo_f64 (f64.load (i32.add (local.get $ptr) (i32.const 8)))) ;; %.14G text via __rt_ftoa + fd_write
      (return)))                                                     ;; done
  (if (i64.eq (local.get $tag) (i64.const 3))                       ;; tag 3 = bool
    (then
      (call $__rt_echo_bool (i64.load (i32.add (local.get $ptr) (i32.const 8)))) ;; echo bool payload
      (return)))
  (if (i32.or (i64.eq (local.get $tag) (i64.const 4)) (i64.eq (local.get $tag) (i64.const 5)))
    (then
      ;; PHP prints the literal text "Array" and warns; the cast helper owns both, and its
      ;; persisted result is released here because echoing keeps nothing.
      (call $__rt_mixed_cast_string (local.get $ptr))
      (local.set $len)                                               ;; pop (ptr, len)
      (local.set $sptr)
      (call $__rt_echo_str (local.get $sptr) (i64.extend_i32_u (local.get $len)))  ;; "Array"
      (call $__rt_heap_free_safe (local.get $sptr))                  ;; the echo owns nothing
      (return))))                                                    ;; done
"#;

/// `__rt_concat`: allocates an owned string and copies `a` then `b` into it.
///
/// Length addition and wasm32 narrowing are checked in i64 before allocation.
/// The returned block is stamped with runtime kind 1 so ordinary ownership
/// release frees intermediate concatenations. This avoids shared-buffer overlap
/// across recursion, calls, and strings larger than 64 KiB.
const RT_CONCAT: &str = r#"(func $__rt_concat (param $aptr i32) (param $alen i64) (param $bptr i32) (param $blen i64) (result i32) (result i64)
  (local $total i64) (local $al i32) (local $bl i32) (local $result i32)
  (if (i32.or
        (i64.lt_s (local.get $alen) (i64.const 0))
        (i64.lt_s (local.get $blen) (i64.const 0)))
    (then
      (call $__rt_oom)
      unreachable))                                          ;; elephc-trap:deterministic-oom:concat-negative-length malformed negative length
  (local.set $total (i64.add (local.get $alen) (local.get $blen))) ;; widened total length
  (if (i32.or
        (i64.lt_u (local.get $total) (local.get $alen))
        (i64.gt_u (local.get $total) (i64.const 4294900736)))
    (then
      (call $__rt_oom)
      unreachable))                                          ;; elephc-trap:deterministic-oom:concat-length-overflow overflow or unaddressable wasm32 length
  (local.set $al (i32.wrap_i64 (local.get $alen)))           ;; safe after total-length bound
  (local.set $bl (i32.wrap_i64 (local.get $blen)))           ;; safe after total-length bound
  (local.set $result (call $__rt_heap_alloc (i32.wrap_i64 (local.get $total)))) ;; owned result bytes
  (i64.store (i32.sub (local.get $result) (i32.const 8)) (i64.const 1)) ;; runtime kind = string
  (memory.copy (local.get $result) (local.get $aptr) (local.get $al)) ;; copy lhs bytes
  (memory.copy
    (i32.add (local.get $result) (local.get $al))
    (local.get $bptr)
    (local.get $bl))                                          ;; append rhs bytes
  (local.get $result)                                         ;; owned result pointer
  (local.get $total))                                         ;; result length"#;

/// `__rt_echo_bool`: PHP `echo` of a boolean writes "1" for true and nothing for
/// false. The value is the i64 boolean (0 or 1).
const RT_ECHO_BOOL: &str = r#"(func $__rt_echo_bool (param $v i64)
  (if (i64.ne (local.get $v) (i64.const 0))
    (then
      (i32.store8 (i32.const 16) (i32.const 49))            ;; '1' into the number buffer
      (call $__rt_wasi_write_or_fail (i32.const 1) (i32.const 16) (i32.const 1))))) ;; write "1""#;

/// `__rt_echo_str`: writes a string (a linear-memory pointer + byte length) to
/// stdout via `fd_write`. The length is an i64 (PHP int) wrapped to the i32 the
/// iovec field requires.
const RT_ECHO_STR: &str = r#"(func $__rt_echo_str (param $ptr i32) (param $len i64)
  (if (i64.gt_u (local.get $len) (i64.const 4294967295))
    (then
      (call $__rt_fail (i32.const 5))
      unreachable))                                          ;; elephc-trap:post-noreturn:echo-string-length-overflow wasm32 cannot address a larger byte range
  (call $__rt_wasi_write_or_fail (i32.const 1) (local.get $ptr) (i32.wrap_i64 (local.get $len)))) ;; write to stdout"#;

/// `__rt_echo_i64`: writes a signed 64-bit integer to stdout as decimal text.
///
/// Formats the value back-to-front into the scratch number buffer [16, 64), then
/// points the iovec at the written bytes and calls `fd_write(1, ...)`. The
/// magnitude is taken as unsigned (`0 - v`), which wraps correctly for `i64::MIN`
/// so `div_u`/`rem_u` produce its true digits.
const RT_ECHO_I64: &str = r#"(func $__rt_echo_i64 (param $v i64)
  (local $ptr i32)   ;; back-to-front write cursor into the number buffer
  (local $neg i32)   ;; 1 if the value is negative
  (local $u i64)     ;; magnitude (unsigned)
  (local $len i32)   ;; number of bytes written
  (local.set $ptr (i32.const 64))                              ;; buffer end (exclusive)
  (if (i64.eqz (local.get $v))
    (then
      (local.set $ptr (i32.sub (local.get $ptr) (i32.const 1))) ;; back up one byte for '0'
      (i32.store8 (local.get $ptr) (i32.const 48)))            ;; '0'
    (else
      (local.set $neg (i64.lt_s (local.get $v) (i64.const 0))) ;; sign
      (if (local.get $neg)
        (then (local.set $u (i64.sub (i64.const 0) (local.get $v)))) ;; magnitude (MIN wraps -> correct unsigned)
        (else (local.set $u (local.get $v))))                  ;; positive: magnitude = v
      (block $done
        (loop $digit
          (br_if $done (i64.eqz (local.get $u)))               ;; stop when no digits left
          (local.set $ptr (i32.sub (local.get $ptr) (i32.const 1))) ;; back up one byte for digit
          (i32.store8 (local.get $ptr)
            (i32.add (i32.const 48)
              (i32.wrap_i64 (i64.rem_u (local.get $u) (i64.const 10))))) ;; '0' + (u % 10)
          (local.set $u (i64.div_u (local.get $u) (i64.const 10)))      ;; u /= 10
          (br $digit)))                                        ;; next digit
      (if (local.get $neg)
        (then
          (local.set $ptr (i32.sub (local.get $ptr) (i32.const 1))) ;; back up one byte for '-'
          (i32.store8 (local.get $ptr) (i32.const 45))))))     ;; '-'
  (local.set $len (i32.sub (i32.const 64) (local.get $ptr)))   ;; byte count
  (call $__rt_wasi_write_or_fail (i32.const 1) (local.get $ptr) (local.get $len))) ;; write to stdout"#;

/// `__rt_echo_f64`: writes a PHP float to stdout as `%.14G` text. The float arrives
/// as a wasm `f64`; its bits are reinterpreted to an `i64` for `__rt_ftoa`, which
/// renders into the float-scratch output region (scratch+4096) and returns
/// `(ptr, len)`. The iovec at [0, 16) is then pointed at those bytes and `fd_write`
/// flushes them to stdout. Mirrors `__rt_echo_str` once the text is materialized.
const RT_ECHO_F64: &str = r#"(func $__rt_echo_f64 (param $v f64)
  (local $bits i64)                                         ;; f64 bits handed to __rt_ftoa
  (local $ptr i32)                                          ;; formatted text pointer (from __rt_ftoa)
  (local $len i32)                                          ;; formatted text length (from __rt_ftoa)
  (local.set $bits (i64.reinterpret_f64 (local.get $v)))    ;; f64 value -> raw bits for __rt_ftoa
  (call $__rt_ftoa (local.get $bits) (i32.add (global.get $__float_scratch) (i32.const 1024)) (i32.const 80) (i32.add (global.get $__float_scratch) (i32.const 2048)) (i32.const 792) (i32.add (global.get $__float_scratch) (i32.const 4096))) ;; format into scratch+4096 -> (ptr,len)
  (local.set $len)                                          ;; pop ftoa length (result 1, on top)
  (local.set $ptr)                                          ;; pop ftoa pointer (result 0)
  (call $__rt_wasi_write_or_fail (i32.const 1) (local.get $ptr) (local.get $len))) ;; write to stdout"#;

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Runtime regression tests for heap-backed string concatenation.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Modules are import-free reactors containing the common runtime and heap.
    //! - Tests validate the bytes with `wasmparser` and execute under Wasmer when
    //!   available, including strings larger than the former 64 KiB buffer.

    use super::{
        emit_common_runtime, ERR_DIV_ZERO, ERR_INTDIV_OVERFLOW, ERR_MOD_ZERO, ERR_NEG_SHIFT,
        ERR_EXPLODE_EMPTY_SEP, ERR_STR_PAD_EMPTY, ERR_STR_REPEAT_NEGATIVE, ERR_STR_SPLIT_LENGTH,
        RT_ARGV, RT_ECHO_BOOL, RT_ECHO_F64, RT_ECHO_I64, RT_ECHO_STR, RT_WASI_WRITE_OR_FAIL,
    };
    use super::super::heap::emit_heap_runtime;
    use super::super::objects::CATCHABLE_RUNTIME_ERRORS;
    use super::super::wat::{DataSegment, WatModule};
    use std::sync::atomic::{AtomicU32, Ordering};

    static TMP_SEQ: AtomicU32 = AtomicU32::new(0);

    /// Verifies a raised runtime error and its uncaught fatal name the same class and message.
    ///
    /// These two halves live apart on purpose: the raise site builds the object a `catch` will
    /// match, while `__rt_fail` owns the text `main` prints when no clause matched. Nothing in
    /// the emitter forces them to agree, so changing one and not the other would let a program
    /// catch a `DivisionByZeroError` yet report an `ArithmeticError` when it does not.
    #[test]
    fn raised_runtime_errors_agree_with_their_uncaught_diagnostics() {
        let fatals = [
            (1, ERR_DIV_ZERO),
            (2, ERR_MOD_ZERO),
            (3, ERR_NEG_SHIFT),
            (4, ERR_INTDIV_OVERFLOW),
            (11, ERR_STR_REPEAT_NEGATIVE),
            (12, ERR_STR_PAD_EMPTY),
            (13, ERR_EXPLODE_EMPTY_SEP),
            (14, ERR_STR_SPLIT_LENGTH),
        ];
        for (code, class_name, message) in CATCHABLE_RUNTIME_ERRORS {
            let (_, fatal) = fatals
                .iter()
                .find(|(fatal_code, _)| *fatal_code == code)
                .unwrap_or_else(|| panic!("failure code {code} has no registered fatal message"));
            let fatal = String::from_utf8_lossy(fatal).to_string();
            assert_eq!(
                fatal,
                format!("PHP Fatal error: Uncaught {class_name}: {message}\n"),
                "failure code {code} raises a different class or message than it reports"
            );
        }
    }

    /// Verifies every PHP stdout helper converts a non-zero WASI errno into the
    /// shared fatal path and that `$argv` rejects pointer-table multiplication
    /// overflow before allocating.
    #[test]
    fn command_runtime_propagates_write_errors_and_guards_argv_size() {
        for echo in [RT_ECHO_BOOL, RT_ECHO_STR, RT_ECHO_I64, RT_ECHO_F64] {
            assert!(
                echo.contains("call $__rt_wasi_write_or_fail"),
                "echo helper bypasses the checked WASI write path:\n{echo}"
            );
            assert!(
                !echo.contains("drop (call $__rt_wasi_write_all"),
                "echo helper still discards the WASI errno:\n{echo}"
            );
        }
        assert!(RT_WASI_WRITE_OR_FAIL.contains("call $__rt_fail (i32.const 5)"));
        assert!(
            RT_ARGV.contains("i32.gt_u (local.get $argc) (i32.const 1073741823)"),
            "$argv must reject argc * 4 overflow before heap allocation"
        );
    }

    /// Returns whether the Wasmer CLI is available for runtime assertions.
    fn wasmer_available() -> bool {
        std::process::Command::new("wasmer")
            .arg("--version")
            .output()
            .is_ok()
    }

    /// Builds, validates, and invokes an import-free runtime driver.
    fn run_concat_driver(
        pages: u32,
        heap_base: u32,
        segments: &[(u32, Vec<u8>)],
        driver: &str,
    ) -> Option<String> {
        let mut module = WatModule::new();
        module.set_memory(pages, Some("memory"));
        emit_common_runtime(&mut module);
        emit_heap_runtime(&mut module, heap_base, pages * 65536);
        for (offset, bytes) in segments {
            module.add_data(DataSegment {
                offset: *offset,
                bytes: bytes.clone(),
            });
        }
        module.add_raw_func(driver);
        let wat = module.render();
        let bytes =
            ::wat::parse_str(&wat).unwrap_or_else(|error| panic!("invalid WAT: {error}\n{wat}"));
        wasmparser::validate(&bytes)
            .unwrap_or_else(|error| panic!("invalid WASM: {error}\n{wat}"));
        if !wasmer_available() {
            return None;
        }
        let sequence = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "elephc_wasm_concat_{}_{}",
            std::process::id(),
            sequence
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("m.wasm");
        std::fs::write(&path, bytes).expect("write wasm");
        let output = std::process::Command::new("wasmer")
            .arg("run")
            .arg("--invoke")
            .arg("t")
            .arg(&path)
            .output()
            .expect("run wasmer");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            output.status.success(),
            "concat driver failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Verifies a concatenation larger than 64 KiB grows the heap and preserves bytes.
    #[test]
    fn concat_grows_beyond_the_legacy_buffer() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $ptr i32) (local $len i64)
  (call $__rt_concat (i32.const 90000) (i64.const 70000) (i32.const 160000) (i64.const 1))
  (local.set $len)
  (local.set $ptr)
  (i64.add
    (i64.mul (local.get $len) (i64.const 100000))
    (i64.add
      (i64.mul (i64.extend_i32_u (i32.load8_u (local.get $ptr))) (i64.const 100))
      (i64.extend_i32_u (i32.load8_u (i32.add (local.get $ptr) (i32.const 70000)))))))"#;
        let segments = [
            (90000, vec![b'a'; 70000]),
            (160000, vec![b'Z']),
        ];
        if let Some(output) = run_concat_driver(4, 170000, &segments, driver) {
            assert_eq!(output, "7000109790");
        }
    }

    /// Verifies a later concatenation cannot overwrite an earlier live result.
    #[test]
    fn concurrent_live_concat_results_do_not_alias() {
        let driver = r#"(func $t (export "t") (result i64)
  (local $first i32) (local $second i32)
  (call $__rt_concat (i32.const 90000) (i64.const 2) (i32.const 90002) (i64.const 2))
  drop
  (local.set $first)
  (call $__rt_concat (i32.const 90004) (i64.const 2) (i32.const 90006) (i64.const 2))
  drop
  (local.set $second)
  (drop (local.get $second))
  (i64.or
    (i64.shl (i64.extend_i32_u (i32.load8_u (local.get $first))) (i64.const 24))
    (i64.or
      (i64.shl (i64.extend_i32_u (i32.load8_u (i32.add (local.get $first) (i32.const 1)))) (i64.const 16))
      (i64.or
        (i64.shl (i64.extend_i32_u (i32.load8_u (i32.add (local.get $first) (i32.const 2)))) (i64.const 8))
        (i64.extend_i32_u (i32.load8_u (i32.add (local.get $first) (i32.const 3))))))))"#;
        let segments = [
            (90000, b"AB".to_vec()),
            (90002, b"CD".to_vec()),
            (90004, b"xy".to_vec()),
            (90006, b"zz".to_vec()),
        ];
        if let Some(output) = run_concat_driver(2, 100000, &segments, driver) {
            assert_eq!(output, "1094861636");
        }
    }
}
