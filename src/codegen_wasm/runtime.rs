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
/// +4096); the ftoa/itoa scratch lands at +0x2000..+0x3000; syscall scratch and the
/// per-fd stream tables sit at +0x3000..+0x4000; the per-fd stream METADATA table
/// (16 bytes per fd, 256 fds — mode and uri as persisted-string ptr/len pairs
/// recorded by `fopen`) fills +0x4000..+0x5000; the `parse_url` parts table (eight
/// `(start, len)` pairs, a presence mask and the port) sits at +0x5000..+0x5100.
/// `output_buffer::OB_LEVELS` (eight 32-byte level records) fills +0x6700..+0x6800, the top. It once sat
/// at +0x5100, on top of the scanf scratch and the clock out-parameter; `scratch_ranges_are_disjoint` pins that.
/// Grown again for directory iteration: the per-fd state (16 bytes per fd, 256 fds — the
/// listing snapshot's pointer and length, the cursor's byte offset into it, and the cursor
/// phase) fills +0x5200..+0x6200, and the two syscall out-parameters plus the `.`/`..`
/// names +0x6200..+0x620C. The listing itself is a heap block, not scratch.
///
/// Grown once more for the per-fd CHUNK SIZE `stream_set_chunk_size()` records (4 bytes per
/// fd, 256 fds), which fills +0x6300..+0x6700. An in-memory stream has no fd to index, so its
/// size rides in its own descriptor instead.
///
/// `dirs` and `streamtune` each assert their last bound against this size at compile time,
/// because getting it wrong is silent: a table simply runs past the reserved region into the
/// module's static data, and the walk reads back whatever was written over it.
pub(super) const FLOAT_SCRATCH_SIZE: u32 = 0x6800;

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
/// A call to a public function name whose body would have arrived from an include that never
/// ran. The name is composed at RUNTIME because which variant is live is runtime state.
/// `var_dump`'s fixed fragments. php writes the TYPE and, for a string, its BYTE LENGTH, so
/// only the punctuation is fixed; the numbers are rendered at runtime.
/// php's `Error` for a property written through a non-object. The NAME is composed at runtime
/// because one lowering serves every property.
const ERR_ASSIGN_ON_NULL_PREFIX: &[u8] = b"PHP Fatal error: Uncaught Error: Attempt to assign property \"";
const ERR_ASSIGN_ON_NULL_SUFFIX: &[u8] = b"\" on null\n";
/// php's `Error` for a property ALIASED through a non-object — `$r = &$null->prop;`. Measured
/// on 8.5.6: the verb is "modify" where a plain write says "assign", and nothing else differs.
const ERR_MODIFY_ON_NULL_PREFIX: &[u8] =
    b"PHP Fatal error: Uncaught Error: Attempt to modify property \"";
const DUMP_INT_PREFIX: &[u8] = b"int(";
const DUMP_FLOAT_PREFIX: &[u8] = b"float(";
const DUMP_STRING_PREFIX: &[u8] = b"string(";
const DUMP_STRING_MIDDLE: &[u8] = b") \"";
const DUMP_STRING_SUFFIX: &[u8] = b"\"\n";
const DUMP_CLOSE: &[u8] = b")\n";
const DUMP_TRUE: &[u8] = b"bool(true)\n";
const DUMP_FALSE: &[u8] = b"bool(false)\n";
const DUMP_NULL: &[u8] = b"NULL\n";
/// The container form. php indents two spaces per level, and writes the key and the value on
/// SEPARATE lines at the same indent — measured on php-src 8.5.6.
const DUMP_ARRAY_PREFIX: &[u8] = b"array(";
const DUMP_ARRAY_OPEN: &[u8] = b") {\n";
const DUMP_ARRAY_CLOSE: &[u8] = b"}\n";
const DUMP_KEY_OPEN: &[u8] = b"[";
const DUMP_KEY_CLOSE: &[u8] = b"]=>\n";
const DUMP_QUOTE: &[u8] = b"\"";
const DUMP_INDENT: &[u8] = b"  ";

/// The message of a `throw_error_value` that no frame can catch. The text after the prefix is
/// composed at RUNTIME (it names the receiver's method, or the class php-src would report), so
/// it arrives as a pointer/length pair rather than a baked literal.
const ERR_ERROR_VALUE_PREFIX: &[u8] = b"PHP Fatal error: Uncaught Error: ";
const ERR_ERROR_VALUE_SUFFIX: &[u8] = b"\n";
const ERR_UNDEFINED_FUNCTION_PREFIX: &[u8] =
    b"PHP Fatal error: Uncaught Error: Call to undefined function ";
const ERR_UNDEFINED_FUNCTION_SUFFIX: &[u8] = b"()\n";
/// A dynamic dispatch selecting a class php-src would not enter for lack of arguments. The class
/// and method names come from the runtime table and the call site, the two counts are rendered by
/// `__rt_itoa`, and the wording between them is php-src's own: `exactly` when every declared
/// parameter is required, `at least` when a default makes the counts differ. Measured on 8.5.6 —
/// a variadic tail does NOT soften the word.
///
/// KNOWN DIVERGENCE: php-src continues `, 0 passed in /path.php on line 9 and …` and closes with
/// a stack trace. This target reports no location tail, the same convention its internal-function
/// `TypeError`s already follow; the class, the message and the 255 exit status are PHP's.
const ERR_TOO_FEW_ARGS_PREFIX: &[u8] =
    b"PHP Fatal error: Uncaught ArgumentCountError: Too few arguments to function ";
const ERR_TOO_FEW_ARGS_PASSED: &[u8] = b"(), ";
const ERR_TOO_FEW_ARGS_EXACTLY: &[u8] = b" passed and exactly ";
const ERR_TOO_FEW_ARGS_AT_LEAST: &[u8] = b" passed and at least ";
const ERR_TOO_FEW_ARGS_SUFFIX: &[u8] = b" expected\n";
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
/// `stream_set_chunk_size()` with a non-positive size is a PHP `ValueError`, and php leaves the
/// recorded size untouched when it raises.
const ERR_STREAM_CHUNK_SIZE: &[u8] = b"PHP Fatal error: Uncaught ValueError: stream_set_chunk_size(): Argument #2 ($size) must be greater than 0\n";
/// A bcmath operand that is not a well-formed decimal is a PHP `ValueError`. php names the
/// function and the argument position; this table holds one string per code, so the wording is
/// generic while the class and the catchability are php's.
const ERR_BC_MALFORMED: &[u8] = b"PHP Fatal error: Uncaught ValueError: bcmath: an argument is not well-formed\n";
/// A compressed stream this target cannot allocate room for. php's `gzinflate` answers
/// `false` for a CORRUPT stream and does not throw, so this covers only the memory case.
const ERR_Z_CORRUPT: &[u8] = b"PHP Fatal error: Uncaught Error: gzinflate: out of memory\n";
/// php's own refusal to rewind a generator a consumer has already advanced. An `Exception`
/// rather than an `Error`, which is what php-src raises here.
const ERR_REWIND_ADVANCED: &[u8] =
    b"PHP Fatal error: Uncaught Exception: Cannot rewind a generator that was already run\n";
/// Code 21: a second `fclose()` of an in-memory stream. php rejects any call on a closed
/// stream resource with a `TypeError`.
const ERR_FCLOSE_CLOSED: &[u8] =
    b"PHP Fatal error: Uncaught TypeError: fclose(): Argument #1 ($stream) must be an open stream resource\n";
/// Code 22: `stream_set_chunk_size()` past `INT_MAX`.
const ERR_STREAM_CHUNK_TOO_LARGE: &[u8] =
    b"PHP Fatal error: Uncaught ValueError: stream_set_chunk_size(): Argument #2 ($size) is too large\n";
/// Code 23: `bcscale()` with a scale outside `[0, INT_MAX]`.
const ERR_BCSCALE_RANGE: &[u8] =
    b"PHP Fatal error: Uncaught ValueError: bcscale(): Argument #1 ($scale) must be between 0 and 2147483647\n";
/// Code 24: an explicit scale argument of a served bcmath function outside `[0, INT_MAX]`. php
/// names the function and argument; this table holds one string per code, so the wording is
/// generic while the class and the catchability are php's.
const ERR_BC_SCALE_RANGE: &[u8] =
    b"PHP Fatal error: Uncaught ValueError: bcmath: scale must be between 0 and 2147483647\n";
/// Code 25: `Generator::getReturn()` on a generator that has not returned.
const ERR_GENERATOR_NO_RETURN: &[u8] =
    b"PHP Fatal error: Uncaught Exception: Cannot get return value of a generator that hasn't returned\n";
/// Code 26: the `preg_match()` pattern VM ran out of backtrack frames or steps. php answers
/// `false` and sets `PREG_BACKTRACK_LIMIT_ERROR`; this target's `preg_match` returns an int, so
/// answering `0` would silently report "no match" for a subject that does match. Stopping loudly
/// is the honest spelling of "the limit was hit".
pub(super) const ERR_PREG_LIMIT: &[u8] =
    b"PHP Fatal error: preg_match(): the pattern VM's backtrack limit was exhausted (php answers false here)\n";
/// Code 27: a `foreach` over a generator that has already finished. php raises it from the
/// iterator constructor, as an `Exception` rather than an `Error`.
const ERR_GENERATOR_CLOSED: &[u8] =
    b"PHP Fatal error: Uncaught Exception: Cannot traverse an already closed generator\n";
/// Codes 28-31: an output-buffer operation attempted from inside an output handler. php raises a
/// non-catchable E_ERROR naming the function that was called, so one message per function.
pub(super) const ERR_OB_START_IN_HANDLER: &[u8] =
    b"PHP Fatal error: ob_start(): Cannot use output buffering in output buffering display handlers\n";
/// See `ERR_OB_START_IN_HANDLER`.
pub(super) const ERR_OB_END_FLUSH_IN_HANDLER: &[u8] =
    b"PHP Fatal error: ob_end_flush(): Cannot use output buffering in output buffering display handlers\n";
/// See `ERR_OB_START_IN_HANDLER`.
pub(super) const ERR_OB_END_CLEAN_IN_HANDLER: &[u8] =
    b"PHP Fatal error: ob_end_clean(): Cannot use output buffering in output buffering display handlers\n";
/// See `ERR_OB_START_IN_HANDLER`.
pub(super) const ERR_OB_GET_CLEAN_IN_HANDLER: &[u8] =
    b"PHP Fatal error: ob_get_clean(): Cannot use output buffering in output buffering display handlers\n";
/// Code 32: `array_pad()` with a size whose magnitude exceeds php's array limit (2^30), which
/// includes `PHP_INT_MIN`, whose negation wraps. php raises a catchable `ValueError`.
const ERR_ARRAY_PAD_SIZE: &[u8] = b"PHP Fatal error: Uncaught ValueError: array_pad(): Argument #2 ($length) must not exceed the maximum allowed array size\n";
/// Code 33: `null + array`, where the left operand of a statically array-typed `+` is a null
/// pointer (a missed element read). php raises a catchable `TypeError`.
const ERR_UNION_NULL_LEFT: &[u8] = b"PHP Fatal error: Uncaught TypeError: Unsupported operand types: null + array\n";
/// Code 34: `array + null`; see code 33.
const ERR_UNION_NULL_RIGHT: &[u8] = b"PHP Fatal error: Uncaught TypeError: Unsupported operand types: array + null\n";
/// Code 35: `array_merge()` with a null first argument.
const ERR_MERGE_NULL_FIRST: &[u8] = b"PHP Fatal error: Uncaught TypeError: array_merge(): Argument #1 must be of type array, null given\n";
/// Code 36: `array_merge()` with a null second argument.
const ERR_MERGE_NULL_SECOND: &[u8] = b"PHP Fatal error: Uncaught TypeError: array_merge(): Argument #2 must be of type array, null given\n";
/// Code 37: `array_replace()` with a null first argument.
const ERR_REPLACE_NULL_FIRST: &[u8] = b"PHP Fatal error: Uncaught TypeError: array_replace(): Argument #1 ($array) must be of type array, null given\n";
/// Code 38: `array_replace()` with a null second argument.
const ERR_REPLACE_NULL_SECOND: &[u8] = b"PHP Fatal error: Uncaught TypeError: array_replace(): Argument #2 must be of type array, null given\n";
/// Code 39: `null + null` over two statically array-typed operands. php answers the INTEGER 0 here,
/// which a result typed as an array cannot hold, so it stops by name instead of guessing.
const ERR_UNION_NULL_BOTH: &[u8] = b"PHP Fatal error: `+` of two null operands evaluates to int 0 in php, which an array-typed result cannot hold on wasm32-wasi\n";
/// `str_split()` with a non-positive chunk length is a PHP `ValueError`.
const ERR_STR_SPLIT_LENGTH: &[u8] = b"PHP Fatal error: Uncaught ValueError: str_split(): Argument #2 ($length) must be greater than 0\n";
/// Code 15 (its position in `fixed_messages`): `print_r` reached a tag its renderer
/// does not implement (an object or a resource nested inside a container the audit
/// could not see through). Failing LOUDLY here is the runtime counterpart of the
/// compile-time refusal — never a truncated rendering php would not print.
const ERR_PRINT_R_UNRENDERABLE: &[u8] =
    b"print_r: rendering an object or resource is not implemented on wasm32-wasi\n";
/// `chr()` outside `[0, 255]` still answers, wrapping modulo 256, but is deprecated since 8.5.
/// Code 16: `new static()` reached with a called-class id no candidate arm matches. The ladder
/// covers every class in the module that descends from the method's own, so this is the shape a
/// hierarchy compiled without one of its members would take — loud, not a null object.
const ERR_LATE_STATIC_UNRESOLVED: &[u8] =
    b"PHP Fatal error: Uncaught Error: new static() reached an unregistered called class\n";
const DEPRECATED_CHR_RANGE: &[u8] = b"Deprecated: chr(): Providing a value not in-between 0 and 255 is deprecated, this is because a byte value must be in the [0, 255] interval. The value used will be constrained using % 256\n";
/// `ord()` on anything but exactly one byte still answers, but is deprecated since 8.5.
const DEPRECATED_ORD_LENGTH: &[u8] =
    b"Deprecated: ord(): Providing a string that is not one byte long is deprecated. Use ord($str[0]) instead\n";
/// PHP's IMPLICIT coercion at a declared `int` return, when the value still converts but
/// loses its fraction on the way. Measured on 8.5.6: a float and a float-shaped string get
/// different wordings, and both render the offending value between the fragments.
const DEPRECATED_FLOAT_TO_INT_PREFIX: &[u8] = b"Deprecated: Implicit conversion from float ";
const DEPRECATED_FLOAT_STR_TO_INT_PREFIX: &[u8] =
    b"Deprecated: Implicit conversion from float-string \"";
const DEPRECATED_TO_INT_SUFFIX: &[u8] = b" to int loses precision\n";
const DEPRECATED_STR_TO_INT_SUFFIX: &[u8] = b"\" to int loses precision\n";
/// A value no `int` can hold, returned from a function declared `int`, is a `TypeError`
/// naming the FUNCTION — `f()` for a plain function, `C::m()` for a method, which is
/// already how the EIR names both — and the type word that arrived.
const ERR_RETURN_TYPE_PREFIX: &[u8] = b"PHP Fatal error: Uncaught TypeError: ";
const ERR_RETURN_TYPE_MIDDLE: &[u8] = b"(): Return value must be of type ";
const ERR_RETURN_TYPE_SEPARATOR: &[u8] = b", ";
const ERR_RETURN_TYPE_SUFFIX: &[u8] = b" returned\n";
/// The same `TypeError` at an internal function's declared PARAMETER, which php-src words
/// differently and positions by argument number: `strtoupper(): Argument #1 ($string) must be of
/// type string, array given`. The function and parameter names travel from the call site, and the
/// word after the comma comes from `__rt_type_word_for_tag`, exactly as the return one does.
const ERR_ARGUMENT_TYPE_MIDDLE: &[u8] = b"(): Argument #";
const ERR_ARGUMENT_NAME_PREFIX: &[u8] = b" ($";
const ERR_ARGUMENT_TYPE_MUST_BE: &[u8] = b") must be of type ";
const ERR_ARGUMENT_TYPE_SUFFIX: &[u8] = b" given\n";
/// `null` at that same boundary is NOT a `TypeError`: measured on php-src 8.5.6, it still
/// converts — to `""` for a `string` parameter — after this deprecation. Only a value with no
/// conversion at all (array, object, resource) raises.
const DEPRECATED_ARGUMENT_NULL_PREFIX: &[u8] = b"Deprecated: ";
const DEPRECATED_ARGUMENT_NULL_MIDDLE: &[u8] = b"(): Passing null to parameter #";
const DEPRECATED_ARGUMENT_NULL_OF_TYPE: &[u8] = b") of type ";
const DEPRECATED_ARGUMENT_NULL_SUFFIX: &[u8] = b" is deprecated\n";
/// `foreach` over something that is neither an array nor an object. Measured on php-src 8.5.6:
/// it WARNS, names the type that arrived, and runs the body zero times — it does not raise.
const WARN_FOREACH_NON_ITERABLE_PREFIX: &[u8] =
    b"Warning: foreach() argument must be of type array|object, ";
const WARN_FOREACH_NON_ITERABLE_SUFFIX: &[u8] = b" given\n";
/// Arithmetic on an operand with no numeric meaning. php-src names BOTH operand types and the
/// operator — `Unsupported operand types: string % int` — where this target's older fixed
/// message named neither. The left word comes from `__rt_type_word_for_tag`; the operator and
/// the right word travel from the call site, which knows them statically.
const ERR_OPERAND_TYPES_PREFIX: &[u8] =
    b"PHP Fatal error: Uncaught TypeError: Unsupported operand types: ";
const ERR_OPERAND_TYPES_SPACE: &[u8] = b" ";
/// PHP names a closure by its CLASS in that message — measured: `Closure returned`, not
/// `callable returned`. Every first-class closure PHP builds is a `Closure`, so the word is
/// fixed even though this target keeps a callable as a descriptor rather than an object.
const PHP_CLASS_CLOSURE: &[u8] = b"Closure";
/// `count()` names the VALUE for a boolean — `true given`, not `bool given` — which is the one
/// place its word table parts ways with the declared-return one. Measured on php-src 8.5.6.
const PHP_VALUE_TRUE: &[u8] = b"true";
const PHP_VALUE_FALSE: &[u8] = b"false";
/// The message carries no tail at all: an internal function's `TypeError` stops at the word.
const ERR_COUNT_PREFIX: &[u8] =
    b"PHP Fatal error: Uncaught TypeError: count(): Argument #1 ($value) must be of type Countable|array, ";
const ERR_COUNT_SUFFIX: &[u8] = b" given\n";
/// `$s[$i]` outside the string answers the EMPTY string after this warning, which names the
/// index AS WRITTEN — a negative one is reported negative, not resolved from the end first.
/// Exact php-src 8.5 diagnostic for a property read through a null receiver. The property name
/// is written between the two fragments, so a live data segment is not needed for the message.
const WARN_PROPERTY_ON_NULL_PREFIX: &[u8] = b"Warning: Attempt to read property \"";
const WARN_PROPERTY_ON_NULL_SUFFIX: &[u8] = b"\" on null\n";
/// Reading a DYNAMIC property the object does not carry. php names the class and the property,
/// and both are runtime `(pointer, length)` pairs here: the class comes from the receiver's id
/// so one helper serves every class, and the property name is the very value `$obj->$name` read.
const WARN_UNDEFINED_PROPERTY_PREFIX: &[u8] = b"Warning: Undefined property: ";
const WARN_UNDEFINED_PROPERTY_SCOPE: &[u8] = b"::$";
/// The middle of `Attempt to read property "n" on int`, which the null form spells in one piece.
const WARN_PROPERTY_ON_MIDDLE: &[u8] = b"\" on ";
/// php names the VALUE for a bool receiver — `on true`, not `on bool` — so these two cannot come
/// from the `PHP_TYPE_*` table every other tag reads.
const WARN_PROPERTY_ON_TRUE: &[u8] = b"true\n";
const WARN_PROPERTY_ON_FALSE: &[u8] = b"false\n";
/// `array_sum`/`array_product` over an element that is no number. Measured on php 8.5.10: the
/// fold WARNS naming the element's type — a class name for an object — and goes on, folding a
/// string's or a resource's integer value and skipping an array or an object.
const WARN_ARRAY_SUM_UNSUPPORTED: &[u8] =
    b"Warning: array_sum(): Addition is not supported on type ";
const WARN_ARRAY_PRODUCT_UNSUPPORTED: &[u8] =
    b"Warning: array_product(): Multiplication is not supported on type ";
const WARN_ARRAY_FOLD_NEWLINE: &[u8] = b"\n";
/// Indexing a NON-container scalar warns and answers null. PHP 8.3 renamed the value in this
/// message the same way it did for null, and for a boolean it names the VALUE rather than the
/// type — measured on 8.5.6: `on true`, not `on bool`. Each case is one complete message rather
/// than a prefix plus a word, because the two profiles do not agree on how many pieces there are.
const WARN_OFFSET_ON_TRUE: &[u8] = b"Warning: Trying to access array offset on true\n";
const WARN_OFFSET_ON_FALSE: &[u8] = b"Warning: Trying to access array offset on false\n";
const WARN_OFFSET_ON_INT: &[u8] = b"Warning: Trying to access array offset on int\n";
const WARN_OFFSET_ON_FLOAT: &[u8] = b"Warning: Trying to access array offset on float\n";
const WARN_OFFSET_ON_TYPE_BOOL: &[u8] =
    b"Warning: Trying to access array offset on value of type bool\n";
const WARN_OFFSET_ON_TYPE_INT: &[u8] =
    b"Warning: Trying to access array offset on value of type int\n";
const WARN_OFFSET_ON_TYPE_FLOAT: &[u8] =
    b"Warning: Trying to access array offset on value of type float\n";
/// A path that cannot be opened warns before answering `false`. php-src names the path and the
/// errno — `fopen(nope.txt): Failed to open stream: No such file or directory` — which needs a
/// strerror table this backend has no data for; these are the NATIVE backend's own wording, so
/// the two Elephc targets agree and both stop short of php-src's detail.
const WARN_FOPEN_FAILED: &[u8] = b"Warning: fopen(): Failed to open stream\n";
const WARN_FILE_GET_CONTENTS_FAILED: &[u8] =
    b"Warning: file_get_contents(): Failed to open stream\n";
/// A duplicate `define()`. php-src NAMES the constant — `Warning: Constant X already defined` —
/// which the native backend does not; the name is available here, so php's wording is used.
///
/// 8.3 added the deprecation clause, measured on 8.5.6. Both suffixes are laid out and the
/// emitter picks by profile, the same way the offset-on-null warning does, so the data layout
/// does not depend on the target version.
const WARN_CONSTANT_DEFINED_PREFIX: &[u8] = b"Warning: Constant ";
const WARN_CONSTANT_DEFINED_SUFFIX: &[u8] = b" already defined\n";
const WARN_CONSTANT_DEFINED_SUFFIX_PHP9: &[u8] =
    b" already defined, this will be an error in PHP 9\n";
const WARN_UNINIT_STRING_OFFSET: &[u8] = b"Warning: Uninitialized string offset ";
/// The newline closing that warning, in this emitter's own data group.
const WARN_OFFSET_NEWLINE: &[u8] = b"\n";
/// Reaching `string` or `bool` from a NaN still converts, but PHP 8.5 WARNS first — measured
/// raw, since an error handler hides the level. There is no notice on the way to `float`, where
/// NaN is an ordinary value.
const WARN_NAN_TO_STRING: &[u8] = b"Warning: unexpected NAN value was coerced to string\n";
const WARN_NAN_TO_BOOL: &[u8] = b"Warning: unexpected NAN value was coerced to bool\n";

/// `stream_get_meta_data`'s nine keys and its constant values, php-src 8.5.6's own
/// spellings and ORDER (the array is insertion-ordered, so print_r shows these
/// exactly as laid out here). `plainfile`/`STDIO` describe a real file; `PHP` with
/// `MEMORY` or `TEMP` describe the two in-memory streams.
const META_KEY_TIMED_OUT: &[u8] = b"timed_out";
const META_KEY_BLOCKED: &[u8] = b"blocked";
const META_KEY_EOF: &[u8] = b"eof";
const META_KEY_WRAPPER_TYPE: &[u8] = b"wrapper_type";
const META_KEY_STREAM_TYPE: &[u8] = b"stream_type";
const META_KEY_MODE: &[u8] = b"mode";
const META_KEY_UNREAD_BYTES: &[u8] = b"unread_bytes";
const META_KEY_SEEKABLE: &[u8] = b"seekable";
const META_KEY_URI: &[u8] = b"uri";
const META_WRAPPER_PLAINFILE: &[u8] = b"plainfile";
const META_STREAM_STDIO: &[u8] = b"STDIO";
const META_WRAPPER_PHP: &[u8] = b"PHP";
const META_STREAM_MEMORY: &[u8] = b"MEMORY";
const META_STREAM_TEMP: &[u8] = b"TEMP";
/// The `stream_type` of a `php://output` handle.
const META_STREAM_OUTPUT: &[u8] = b"Output";

/// `print_r`'s two multi-byte fragments; the parens, brackets, newlines and the
/// bool's "1" are single bytes the renderer stores directly.
const PRINT_R_ARRAY_HEAD: &[u8] = b"Array\n";
const PRINT_R_ARROW: &[u8] = b"] => ";

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
    + ERR_REWIND_ADVANCED.len() as u32
    + ERR_FCLOSE_CLOSED.len() as u32
    + ERR_STREAM_CHUNK_TOO_LARGE.len() as u32
    + ERR_BCSCALE_RANGE.len() as u32
    + ERR_BC_SCALE_RANGE.len() as u32
    + ERR_GENERATOR_NO_RETURN.len() as u32
    + ERR_PREG_LIMIT.len() as u32
    + ERR_GENERATOR_CLOSED.len() as u32
    + ERR_OB_START_IN_HANDLER.len() as u32
    + ERR_OB_END_FLUSH_IN_HANDLER.len() as u32
    + ERR_OB_END_CLEAN_IN_HANDLER.len() as u32
    + ERR_OB_GET_CLEAN_IN_HANDLER.len() as u32
    + ERR_ARRAY_PAD_SIZE.len() as u32
    + ERR_UNION_NULL_LEFT.len() as u32
    + ERR_UNION_NULL_RIGHT.len() as u32
    + ERR_MERGE_NULL_FIRST.len() as u32
    + ERR_MERGE_NULL_SECOND.len() as u32
    + ERR_REPLACE_NULL_FIRST.len() as u32
    + ERR_REPLACE_NULL_SECOND.len() as u32
    + ERR_UNION_NULL_BOTH.len() as u32
    + ERR_MIXED_HEAP_TYPE.len() as u32
    + ERR_UNCAUGHT_EXCEPTION.len() as u32
    + ERR_STR_REPEAT_NEGATIVE.len() as u32
    + ERR_STR_PAD_EMPTY.len() as u32
    + ERR_EXPLODE_EMPTY_SEP.len() as u32
    + ERR_STR_SPLIT_LENGTH.len() as u32
    + ERR_PRINT_R_UNRENDERABLE.len() as u32
    + ERR_LATE_STATIC_UNRESOLVED.len() as u32
    + ERR_STREAM_CHUNK_SIZE.len() as u32
    + ERR_BC_MALFORMED.len() as u32
    + ERR_Z_CORRUPT.len() as u32
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
    + ERR_UNDEFINED_FUNCTION_PREFIX.len() as u32
    + ERR_UNDEFINED_FUNCTION_SUFFIX.len() as u32
    + ERR_ERROR_VALUE_PREFIX.len() as u32
    + ERR_ERROR_VALUE_SUFFIX.len() as u32
    + ERR_ASSIGN_ON_NULL_PREFIX.len() as u32
    + ERR_ASSIGN_ON_NULL_SUFFIX.len() as u32
    + ERR_MODIFY_ON_NULL_PREFIX.len() as u32
    + DUMP_INT_PREFIX.len() as u32
    + DUMP_FLOAT_PREFIX.len() as u32
    + DUMP_STRING_PREFIX.len() as u32
    + DUMP_STRING_MIDDLE.len() as u32
    + DUMP_STRING_SUFFIX.len() as u32
    + DUMP_CLOSE.len() as u32
    + DUMP_TRUE.len() as u32
    + DUMP_FALSE.len() as u32
    + DUMP_NULL.len() as u32
    + DUMP_ARRAY_PREFIX.len() as u32
    + DUMP_ARRAY_OPEN.len() as u32
    + DUMP_ARRAY_CLOSE.len() as u32
    + DUMP_KEY_OPEN.len() as u32
    + DUMP_KEY_CLOSE.len() as u32
    + DUMP_QUOTE.len() as u32
    + DUMP_INDENT.len() as u32
    + ERR_TOO_FEW_ARGS_PREFIX.len() as u32
    + ERR_TOO_FEW_ARGS_PASSED.len() as u32
    + ERR_TOO_FEW_ARGS_EXACTLY.len() as u32
    + ERR_TOO_FEW_ARGS_AT_LEAST.len() as u32
    + ERR_TOO_FEW_ARGS_SUFFIX.len() as u32
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
    + ERR_OBJECT_TO_STRING_SUFFIX.len() as u32
    + DEPRECATED_FLOAT_TO_INT_PREFIX.len() as u32
    + DEPRECATED_FLOAT_STR_TO_INT_PREFIX.len() as u32
    + DEPRECATED_TO_INT_SUFFIX.len() as u32
    + DEPRECATED_STR_TO_INT_SUFFIX.len() as u32
    + ERR_RETURN_TYPE_PREFIX.len() as u32
    + ERR_RETURN_TYPE_MIDDLE.len() as u32
    + ERR_RETURN_TYPE_SUFFIX.len() as u32
    + PHP_CLASS_CLOSURE.len() as u32
    + ERR_RETURN_TYPE_SEPARATOR.len() as u32
    + ERR_ARGUMENT_TYPE_MIDDLE.len() as u32
    + ERR_ARGUMENT_NAME_PREFIX.len() as u32
    + ERR_ARGUMENT_TYPE_MUST_BE.len() as u32
    + ERR_ARGUMENT_TYPE_SUFFIX.len() as u32
    + DEPRECATED_ARGUMENT_NULL_PREFIX.len() as u32
    + DEPRECATED_ARGUMENT_NULL_MIDDLE.len() as u32
    + DEPRECATED_ARGUMENT_NULL_OF_TYPE.len() as u32
    + DEPRECATED_ARGUMENT_NULL_SUFFIX.len() as u32
    + WARN_FOREACH_NON_ITERABLE_PREFIX.len() as u32
    + WARN_FOREACH_NON_ITERABLE_SUFFIX.len() as u32
    + ERR_OPERAND_TYPES_PREFIX.len() as u32
    + ERR_OPERAND_TYPES_SPACE.len() as u32
    + WARN_NAN_TO_STRING.len() as u32
    + WARN_NAN_TO_BOOL.len() as u32
    + PHP_VALUE_TRUE.len() as u32
    + PHP_VALUE_FALSE.len() as u32
    + ERR_COUNT_PREFIX.len() as u32
    + ERR_COUNT_SUFFIX.len() as u32
    + WARN_UNINIT_STRING_OFFSET.len() as u32
    + WARN_OFFSET_NEWLINE.len() as u32
    + WARN_PROPERTY_ON_NULL_PREFIX.len() as u32
    + WARN_PROPERTY_ON_NULL_SUFFIX.len() as u32
    + WARN_OFFSET_ON_TRUE.len() as u32
    + WARN_OFFSET_ON_FALSE.len() as u32
    + WARN_OFFSET_ON_INT.len() as u32
    + WARN_OFFSET_ON_FLOAT.len() as u32
    + WARN_OFFSET_ON_TYPE_BOOL.len() as u32
    + WARN_OFFSET_ON_TYPE_INT.len() as u32
    + WARN_OFFSET_ON_TYPE_FLOAT.len() as u32
    + WARN_FOPEN_FAILED.len() as u32
    + WARN_FILE_GET_CONTENTS_FAILED.len() as u32
    + WARN_CONSTANT_DEFINED_PREFIX.len() as u32
    + WARN_CONSTANT_DEFINED_SUFFIX.len() as u32
    + WARN_CONSTANT_DEFINED_SUFFIX_PHP9.len() as u32
    + WARN_INC_BOOL.len() as u32
    + WARN_DEC_BOOL.len() as u32
    + WARN_DEC_NULL.len() as u32
    + DEPRECATED_INC_STR.len() as u32
    + DEPRECATED_DEC_STR.len() as u32
    + DEPRECATED_DEC_EMPTY.len() as u32
    + DEPRECATED_FLOAT_KEY_PREFIX.len() as u32
    + DEPRECATED_FLOAT_KEY_SUFFIX.len() as u32
    + DEPRECATED_NULL_KEY.len() as u32
    + META_KEY_TIMED_OUT.len() as u32
    + META_KEY_BLOCKED.len() as u32
    + META_KEY_EOF.len() as u32
    + META_KEY_WRAPPER_TYPE.len() as u32
    + META_KEY_STREAM_TYPE.len() as u32
    + META_KEY_MODE.len() as u32
    + META_KEY_UNREAD_BYTES.len() as u32
    + META_KEY_SEEKABLE.len() as u32
    + META_KEY_URI.len() as u32
    + META_WRAPPER_PLAINFILE.len() as u32
    + META_STREAM_STDIO.len() as u32
    + META_WRAPPER_PHP.len() as u32
    + META_STREAM_MEMORY.len() as u32
    + META_STREAM_TEMP.len() as u32
    + PRINT_R_ARRAY_HEAD.len() as u32
    + PRINT_R_ARROW.len() as u32
    + WARN_UNDEFINED_PROPERTY_PREFIX.len() as u32
    + WARN_UNDEFINED_PROPERTY_SCOPE.len() as u32
    + WARN_PROPERTY_ON_MIDDLE.len() as u32
    + WARN_PROPERTY_ON_TRUE.len() as u32
    + WARN_PROPERTY_ON_FALSE.len() as u32
    + WARN_ARRAY_SUM_UNSUPPORTED.len() as u32
    + WARN_ARRAY_PRODUCT_UNSUPPORTED.len() as u32
    + WARN_ARRAY_FOLD_NEWLINE.len() as u32
    + META_STREAM_OUTPUT.len() as u32;

/// PHP's `++`/`--` diagnostics, measured on php-src 8.5.6. A bool or null operand
/// keeps its value and WARNS; a non-numeric string keeps (or perl-increments) its
/// value and DEPRECATES. The empty string is its own case in each direction.
const WARN_INC_BOOL: &[u8] =
    b"Warning: Increment on type bool has no effect, this will change in the next major version of PHP\n";
const WARN_DEC_BOOL: &[u8] =
    b"Warning: Decrement on type bool has no effect, this will change in the next major version of PHP\n";
const WARN_DEC_NULL: &[u8] =
    b"Warning: Decrement on type null has no effect, this will change in the next major version of PHP\n";
const DEPRECATED_INC_STR: &[u8] =
    b"Deprecated: Increment on non-numeric string is deprecated, use str_increment() instead\n";
const DEPRECATED_DEC_STR: &[u8] =
    b"Deprecated: Decrement on non-numeric string has no effect and is deprecated\n";
const DEPRECATED_DEC_EMPTY: &[u8] =
    b"Deprecated: Decrement on empty string is deprecated as non-numeric\n";

/// Array-key conversion deprecations, measured on php 8.5.6. The float one carries the
/// rendered value between its fragments; BOTH fire even on the silent read path — only
/// the undefined-key warning is silenced there.
const DEPRECATED_FLOAT_KEY_PREFIX: &[u8] = b"Deprecated: Implicit conversion from float ";
const DEPRECATED_FLOAT_KEY_SUFFIX: &[u8] = b" to int loses precision\n";
const DEPRECATED_NULL_KEY: &[u8] =
    b"Deprecated: Using null as an array offset is deprecated, use an empty string instead\n";

/// Name of the mutable global holding the `@` suppression DEPTH.
pub(super) const DIAG_SUPPRESS_GLOBAL: &str = "__diag_suppress";

/// The guard every `__rt_warn_*` helper opens with.
///
/// Placed at the TOP of each warning helper rather than around the individual
/// `fd_write` calls: a warning is emitted as several writes (prefix, value,
/// suffix), and gating them one by one would risk printing a half-formed
/// diagnostic if a single write were missed. All-or-nothing per warning.
#[cfg(test)]
pub(super) const DIAG_SUPPRESS_GUARD: &str =
    "  (if (global.get $__diag_suppress) (then (return)))";

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
    emit_diag_suppression_runtime(wm);
}

/// Emits the `@` operator's suppression state.
///
/// A DEPTH, not a flag: `@foo(@bar())` nests, and popping to a boolean would
/// un-suppress the outer expression as soon as the inner one finished. This
/// mirrors the native backend, where `Op::ErrorSuppressBegin` and
/// `Op::ErrorSuppressEnd` call `__rt_diag_push_suppression` / `__rt_diag_pop_suppression`.
///
/// Only WARNINGS consult it. PHP 8 no longer lets `@` swallow a fatal error, so
/// the `__rt_fail_*` helpers deliberately keep writing and exiting.
/// Emits the warning a duplicate `define()` produces.
///
/// Measured on php-src 8.5.6: `Warning: Constant NAME already defined`, and the call answers
/// false. `@define(...)` silences it, which is why this opens with the suppression guard like
/// every other `__rt_warn_*` helper.
fn emit_constant_redefinition_warning(wm: &mut WatModule, offsets: &[(u32, u32)]) {
    debug_assert_eq!(offsets.len(), 3);
    let (prefix_ptr, prefix_len) = offsets[0];
    let php82 = matches!(
        crate::codegen_support::compile_php_version(),
        crate::web_prelude::PhpVersion::Php82
    );
    let (suffix_ptr, suffix_len) = if php82 { offsets[1] } else { offsets[2] };
    wm.add_raw_func(&format!(
        r#"(func $__rt_warn_constant_already_defined (param $name_ptr i32) (param $name_len i32)
  (if (global.get $__diag_suppress) (then (return)))
  (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {prefix_ptr}) (i32.const {prefix_len}))
  (call $__rt_wasi_write_or_fail (i32.const 2) (local.get $name_ptr) (local.get $name_len))
  (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {suffix_ptr}) (i32.const {suffix_len})))"#
    ));
}

/// Adds the diagnostic-suppression support: the depth global and the push/pop helpers that
/// the warning emitters consult before printing.
fn emit_diag_suppression_runtime(wm: &mut WatModule) {
    wm.add_global(Global {
        name: DIAG_SUPPRESS_GLOBAL.to_string(),
        ty: ValType::I32,
        mutable: true,
        init: 0,
    });
    wm.add_raw_func(&format!(
        r#"(func $__rt_diag_push_suppression
  (global.set ${DIAG_SUPPRESS_GLOBAL} (i32.add (global.get ${DIAG_SUPPRESS_GLOBAL}) (i32.const 1))))"#
    ));
    // Saturating at zero: an unbalanced pop must not wrap the depth to ~4e9 and
    // silence every later warning in the program.
    wm.add_raw_func(&format!(
        r#"(func $__rt_diag_pop_suppression
  (if (i32.gt_u (global.get ${DIAG_SUPPRESS_GLOBAL}) (i32.const 0))
    (then (global.set ${DIAG_SUPPRESS_GLOBAL} (i32.sub (global.get ${DIAG_SUPPRESS_GLOBAL}) (i32.const 1))))))"#
    ));
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
        field: "fd_read".to_string(),
        internal: "wasi_fd_read".to_string(),
        // fd, iovs_ptr, iovs_len, nread_ptr -> errno
        params: vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32],
        results: vec![ValType::I32],
    });
    wm.import_func(FuncImport {
        module: "wasi_snapshot_preview1".to_string(),
        field: "fd_seek".to_string(),
        internal: "wasi_fd_seek".to_string(),
        // fd, offset, whence, newoffset_ptr -> errno
        params: vec![ValType::I32, ValType::I64, ValType::I32, ValType::I32],
        results: vec![ValType::I32],
    });
    // The path family. WASI Preview 1 is capability-based: every one of these takes the fd of a
    // directory the host preopened, which `__rt_wasi_dirfd` finds by probing from fd 3.
    wm.import_func(FuncImport {
        module: "wasi_snapshot_preview1".to_string(),
        field: "fd_prestat_get".to_string(),
        internal: "wasi_fd_prestat_get".to_string(),
        // fd, prestat_buf -> errno
        params: vec![ValType::I32, ValType::I32],
        results: vec![ValType::I32],
    });
    wm.import_func(FuncImport {
        module: "wasi_snapshot_preview1".to_string(),
        field: "fd_close".to_string(),
        internal: "wasi_fd_close".to_string(),
        params: vec![ValType::I32],
        results: vec![ValType::I32],
    });
    wm.import_func(FuncImport {
        module: "wasi_snapshot_preview1".to_string(),
        field: "path_open".to_string(),
        internal: "wasi_path_open".to_string(),
        // dirfd, dirflags, path, path_len, oflags, rights_base, rights_inheriting,
        // fdflags, opened_fd_out -> errno
        params: vec![
            ValType::I32,
            ValType::I32,
            ValType::I32,
            ValType::I32,
            ValType::I32,
            ValType::I64,
            ValType::I64,
            ValType::I32,
            ValType::I32,
        ],
        results: vec![ValType::I32],
    });
    wm.import_func(FuncImport {
        module: "wasi_snapshot_preview1".to_string(),
        field: "clock_time_get".to_string(),
        internal: "wasi_clock_time_get".to_string(),
        // clock_id, precision, time_out -> errno
        params: vec![ValType::I32, ValType::I64, ValType::I32],
        results: vec![ValType::I32],
    });
    wm.import_func(FuncImport {
        module: "wasi_snapshot_preview1".to_string(),
        field: "fd_readdir".to_string(),
        internal: "wasi_fd_readdir".to_string(),
        // fd, buf, buf_len, cookie, bufused_out -> errno
        params: vec![
            ValType::I32,
            ValType::I32,
            ValType::I32,
            ValType::I64,
            ValType::I32,
        ],
        results: vec![ValType::I32],
    });
    wm.import_func(FuncImport {
        module: "wasi_snapshot_preview1".to_string(),
        field: "path_filestat_get".to_string(),
        internal: "wasi_path_filestat_get".to_string(),
        // dirfd, flags, path, path_len, filestat_buf -> errno
        params: vec![
            ValType::I32,
            ValType::I32,
            ValType::I32,
            ValType::I32,
            ValType::I32,
        ],
        results: vec![ValType::I32],
    });
    wm.import_func(FuncImport {
        module: "wasi_snapshot_preview1".to_string(),
        field: "path_unlink_file".to_string(),
        internal: "wasi_path_unlink_file".to_string(),
        // dirfd, path, path_len -> errno
        params: vec![ValType::I32, ValType::I32, ValType::I32],
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
    wm.import_func(FuncImport {
        module: "wasi_snapshot_preview1".to_string(),
        field: "environ_sizes_get".to_string(),
        internal: "wasi_environ_sizes_get".to_string(),
        // count_ptr, buf_size_ptr -> errno
        params: vec![ValType::I32, ValType::I32],
        results: vec![ValType::I32],
    });
    wm.import_func(FuncImport {
        module: "wasi_snapshot_preview1".to_string(),
        field: "environ_get".to_string(),
        internal: "wasi_environ_get".to_string(),
        // environ_ptr_array, environ_buf -> errno
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
    wm.add_raw_func(RT_READLINE);
    wm.add_raw_func(RT_GETENV);
    super::files::emit_file_runtime(wm);
    // The three stream tuners. `__rt_fd_meta_clear` above calls `__rt_fd_chunk_clear`, and WAT
    // requires the target to be defined, so this is not optional for a command module.
    super::streamtune::emit_stream_tuning_runtime(wm);
    // `fscanf`'s line reader and tokenizer. They need the heap, `fd_read` and the memstream
    // layout, all of which the file runtime above provides.
    super::scanf::emit_scanf_runtime(wm);
    // Decimal arithmetic. It needs the heap and the string runtime, nothing else — there is no
    // bridge to link on this target, so the digits are the implementation.
    super::bcmath::emit_bcmath_runtime(wm);
    // DEFLATE. It needs the heap, the string runtime and the tables the planner laid out.
    super::zlib::emit_zlib_runtime(wm);
    // The pattern VM. It reads programs the planner laid out and needs only the heap.
    super::regex_vm::emit_regex_runtime(wm);
    // The wall clock. One WASI import and two conversions — `time()` was the corpus's most
    // cited blocker only because this import had never been declared.
    super::clock::emit_clock_runtime(wm);
    // Directory iteration rides on the same preopen probe and resource boxing the file runtime
    // establishes, so it is registered right after it.
    super::dirs::emit_dir_runtime(wm);
}

/// `__rt_getenv`: PHP's single-argument `getenv`, boxed as `string|false`.
///
/// The environment is read fresh on every call through `environ_sizes_get`/`environ_get`
/// into two transient heap blocks, scanned as NUL-terminated `NAME=value` entries, and
/// freed before returning — `__rt_mixed_from_value` persists a tag-1 string internally,
/// so boxing the value slice before the free is safe. The comparison is byte-exact and
/// case-SENSITIVE, php's unix rule. A variable set to the empty string answers `""`
/// (tag 1), never `false`; only a missing name — or a host that provides no environment
/// at all — answers the boxed `false`. The sizes land in the same transient low-scratch
/// words `__rt_readline` uses for its iovec.
const RT_GETENV: &str = r#"(func $__rt_getenv (param $name i32) (param $name_len i64) (result i32)
  (local $count i32) (local $bufsz i32) (local $ptrs i32) (local $buf i32)
  (local $i i32) (local $entry i32) (local $p i32) (local $c i32) (local $j i32)
  (local $klen i32) (local $vlen i32) (local $m i32) (local $out i32)
  (if (i32.ne (call $wasi_environ_sizes_get (i32.const 0) (i32.const 4)) (i32.const 0))
    (then (return (call $__rt_mixed_from_value (i64.const 3) (i64.const 0) (i64.const 0)))))
  (local.set $count (i32.load (i32.const 0)))
  (local.set $bufsz (i32.load (i32.const 4)))
  (if (i32.eqz (local.get $count))
    (then (return (call $__rt_mixed_from_value (i64.const 3) (i64.const 0) (i64.const 0)))))
  (local.set $ptrs (call $__rt_heap_alloc (i32.mul (local.get $count) (i32.const 4))))
  (local.set $buf (call $__rt_heap_alloc (local.get $bufsz)))
  (if (i32.ne (call $wasi_environ_get (local.get $ptrs) (local.get $buf)) (i32.const 0))
    (then
      (call $__rt_heap_free (local.get $buf))
      (call $__rt_heap_free (local.get $ptrs))
      (return (call $__rt_mixed_from_value (i64.const 3) (i64.const 0) (i64.const 0)))))
  (block $done (loop $each
    (br_if $done (i32.ge_u (local.get $i) (local.get $count)))
    (local.set $entry (i32.load (i32.add (local.get $ptrs) (i32.mul (local.get $i) (i32.const 4)))))
    (local.set $klen (i32.const 0))
    (block $kdone (loop $kscan                                    ;; key = bytes before '='
      (local.set $c (i32.load8_u (i32.add (local.get $entry) (local.get $klen))))
      (br_if $kdone (i32.or (i32.eq (local.get $c) (i32.const 61)) (i32.eqz (local.get $c))))
      (local.set $klen (i32.add (local.get $klen) (i32.const 1)))
      (br $kscan)))
    (if (i32.and (i64.eq (i64.extend_i32_u (local.get $klen)) (local.get $name_len))
                 (i32.eq (local.get $c) (i32.const 61)))          ;; a NUL-ended entry has no value
      (then
        (local.set $m (i32.const 1))
        (local.set $j (i32.const 0))
        (block $cdone (loop $cmp
          (br_if $cdone (i32.ge_u (local.get $j) (local.get $klen)))
          (if (i32.ne (i32.load8_u (i32.add (local.get $entry) (local.get $j)))
                      (i32.load8_u (i32.add (local.get $name) (local.get $j))))
            (then
              (local.set $m (i32.const 0))
              (br $cdone)))
          (local.set $j (i32.add (local.get $j) (i32.const 1)))
          (br $cmp)))
        (if (local.get $m)
          (then
            (local.set $p (i32.add (i32.add (local.get $entry) (local.get $klen)) (i32.const 1)))
            (local.set $vlen (i32.const 0))
            (block $vdone (loop $vscan                            ;; value = bytes before the NUL
              (br_if $vdone (i32.eqz (i32.load8_u (i32.add (local.get $p) (local.get $vlen)))))
              (local.set $vlen (i32.add (local.get $vlen) (i32.const 1)))
              (br $vscan)))
            (local.set $out (call $__rt_mixed_from_value (i64.const 1)
              (i64.extend_i32_u (local.get $p)) (i64.extend_i32_u (local.get $vlen))))
            (call $__rt_heap_free (local.get $buf))
            (call $__rt_heap_free (local.get $ptrs))
            (return (local.get $out))))))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $each)))
  (call $__rt_heap_free (local.get $buf))
  (call $__rt_heap_free (local.get $ptrs))
  (call $__rt_mixed_from_value (i64.const 3) (i64.const 0) (i64.const 0)))
"#;

/// Emits the `print_r` renderer and its heap string builder.
///
/// `offsets` carries the two multi-byte fragments ("Array\n", "] => ") laid out by
/// `emit_failure_runtime`. The format is php-src 8.5.6's, measured byte-for-byte on
/// nested arrays: a container at depth `d` renders `Array\n`, its parens indented
/// `8*d`, each entry `4 + 8*d`, and the entry's own trailing newline lands right
/// after a nested container's `)\n` — which is where php's blank line comes from.
/// `print_r(true)` renders "1", `false` and `null` render nothing, an int/float
/// render exactly as `echo` does. An OBJECT or resource reaching the renderer fails
/// LOUDLY through `__rt_fail` (code 15) rather than printing a truncated form.
fn emit_print_r_runtime(wm: &mut WatModule, offsets: &[(u32, u32)]) {
    let [head, arrow] = offsets else {
        unreachable!("emit_failure_runtime hands exactly the two print_r fragments");
    };
    // A tiny heap string builder: 12-byte descriptor (+0 buf, +4 cap, +8 len).
    wm.add_raw_func(
        r#"(func $__rt_sb_reserve (param $sb i32) (param $extra i32)
  (local $len i32) (local $cap i32) (local $buf i32) (local $want i32) (local $new i32)
  (local.set $len (i32.load (i32.add (local.get $sb) (i32.const 8))))
  (local.set $cap (i32.load (i32.add (local.get $sb) (i32.const 4))))
  (local.set $want (i32.add (local.get $len) (local.get $extra)))
  (if (i32.le_u (local.get $want) (local.get $cap))
    (then (return)))
  (if (i32.lt_u (local.get $cap) (i32.const 64))
    (then (local.set $cap (i32.const 64))))
  (block $sized (loop $double
    (br_if $sized (i32.ge_u (local.get $cap) (local.get $want)))
    (local.set $cap (i32.shl (local.get $cap) (i32.const 1)))
    (br $double)))
  (local.set $new (call $__rt_heap_alloc (local.get $cap)))
  (local.set $buf (i32.load (local.get $sb)))
  (memory.copy (local.get $new) (local.get $buf) (local.get $len))
  (if (local.get $buf)
    (then (call $__rt_heap_free (local.get $buf))))
  (i32.store (local.get $sb) (local.get $new))
  (i32.store (i32.add (local.get $sb) (i32.const 4)) (local.get $cap)))
"#,
    );
    wm.add_raw_func(
        r#"(func $__rt_sb_append (param $sb i32) (param $ptr i32) (param $len i32)
  (local $cur i32)
  (call $__rt_sb_reserve (local.get $sb) (local.get $len))
  (local.set $cur (i32.load (i32.add (local.get $sb) (i32.const 8))))
  (memory.copy (i32.add (i32.load (local.get $sb)) (local.get $cur)) (local.get $ptr) (local.get $len))
  (i32.store (i32.add (local.get $sb) (i32.const 8)) (i32.add (local.get $cur) (local.get $len))))
"#,
    );
    wm.add_raw_func(
        r#"(func $__rt_sb_byte (param $sb i32) (param $b i32)
  (local $cur i32)
  (call $__rt_sb_reserve (local.get $sb) (i32.const 1))
  (local.set $cur (i32.load (i32.add (local.get $sb) (i32.const 8))))
  (i32.store8 (i32.add (i32.load (local.get $sb)) (local.get $cur)) (local.get $b))
  (i32.store (i32.add (local.get $sb) (i32.const 8)) (i32.add (local.get $cur) (i32.const 1))))
"#,
    );
    wm.add_raw_func(
        r#"(func $__rt_sb_spaces (param $sb i32) (param $n i32)
  (local $cur i32)
  (call $__rt_sb_reserve (local.get $sb) (local.get $n))
  (local.set $cur (i32.load (i32.add (local.get $sb) (i32.const 8))))
  (memory.fill (i32.add (i32.load (local.get $sb)) (local.get $cur)) (i32.const 32) (local.get $n))
  (i32.store (i32.add (local.get $sb) (i32.const 8)) (i32.add (local.get $cur) (local.get $n))))
"#,
    );
    wm.add_raw_func(&format!(
        r#"(func $__rt_print_r_render (param $tag i64) (param $lo i64) (param $hi i64) (param $level i64) (param $sb i32)
  (local $p i32) (local $l i32) (local $arr i32) (local $len i64) (local $esz i64)
  (local $vt i32) (local $i i64) (local $slot i32) (local $cell i32)
  (local $ctag i64) (local $clo i64) (local $chi i64)
  (local $cursor i64) (local $more i64) (local $entry i32) (local $klo i64) (local $khi i64)
  (if (i64.eqz (local.get $tag))                                  ;; int: echo's own decimal form
    (then
      (call $__rt_itoa (local.get $lo) (global.get $__float_scratch))
      (local.set $l)
      (local.set $p)
      (call $__rt_sb_append (local.get $sb) (local.get $p) (local.get $l))
      (return)))
  (if (i64.eq (local.get $tag) (i64.const 1))                     ;; string: raw bytes
    (then
      (call $__rt_sb_append (local.get $sb)
        (i32.wrap_i64 (local.get $lo)) (i32.wrap_i64 (local.get $hi)))
      (return)))
  (if (i64.eq (local.get $tag) (i64.const 2))                     ;; float: echo's %.14G form
    (then
      (call $__rt_ftoa (local.get $lo)
        (i32.add (global.get $__float_scratch) (i32.const 1024)) (i32.const 80)
        (i32.add (global.get $__float_scratch) (i32.const 2048)) (i32.const 792)
        (i32.add (global.get $__float_scratch) (i32.const 4096)))
      (local.set $l)
      (local.set $p)
      (call $__rt_sb_append (local.get $sb) (local.get $p) (local.get $l))
      (return)))
  (if (i64.eq (local.get $tag) (i64.const 3))                     ;; bool: "1" or nothing
    (then
      (if (i64.ne (local.get $lo) (i64.const 0))
        (then (call $__rt_sb_byte (local.get $sb) (i32.const 49))))
      (return)))
  (if (i64.eq (local.get $tag) (i64.const 8))                     ;; null renders nothing
    (then (return)))
  (if (i64.eq (local.get $tag) (i64.const 7))                     ;; nested cell: unwrap and recurse
    (then
      (local.set $cell (i32.wrap_i64 (local.get $lo)))
      (call $__rt_mixed_unbox (local.get $cell))
      (local.set $chi)
      (local.set $clo)
      (local.set $ctag)
      (call $__rt_print_r_render (local.get $ctag) (local.get $clo) (local.get $chi) (local.get $level) (local.get $sb))
      (return)))
  (if (i64.eq (local.get $tag) (i64.const 4))                     ;; indexed array
    (then
      (local.set $arr (i32.wrap_i64 (local.get $lo)))
      (call $__rt_sb_append (local.get $sb) (i32.const {head_p}) (i32.const {head_l}))
      (call $__rt_sb_spaces (local.get $sb) (i32.wrap_i64 (i64.mul (local.get $level) (i64.const 8))))
      (call $__rt_sb_byte (local.get $sb) (i32.const 40))         ;; '('
      (call $__rt_sb_byte (local.get $sb) (i32.const 10))
      (local.set $len (i64.load (local.get $arr)))
      (local.set $esz (i64.load (i32.add (local.get $arr) (i32.const 16))))
      (local.set $vt (i32.and (i32.wrap_i64 (i64.shr_u
        (i64.load (i32.sub (local.get $arr) (i32.const 8))) (i64.const 8))) (i32.const 127)))
      (local.set $i (i64.const 0))
      (block $done (loop $each
        (br_if $done (i64.ge_s (local.get $i) (local.get $len)))
        (call $__rt_sb_spaces (local.get $sb) (i32.wrap_i64
          (i64.add (i64.mul (local.get $level) (i64.const 8)) (i64.const 4))))
        (call $__rt_sb_byte (local.get $sb) (i32.const 91))       ;; '['
        (call $__rt_itoa (local.get $i) (global.get $__float_scratch))
        (local.set $l)
        (local.set $p)
        (call $__rt_sb_append (local.get $sb) (local.get $p) (local.get $l))
        (call $__rt_sb_append (local.get $sb) (i32.const {arrow_p}) (i32.const {arrow_l}))
        (local.set $slot (i32.add (i32.add (local.get $arr) (i32.const 24))
                                  (i32.wrap_i64 (i64.mul (local.get $i) (local.get $esz)))))
        (if (i32.or (i32.eqz (local.get $vt))
            (i32.or (i32.eq (local.get $vt) (i32.const 2)) (i32.eq (local.get $vt) (i32.const 3))))
          (then                                                   ;; scalar slot: one i64 payload
            (call $__rt_print_r_render (i64.extend_i32_u (local.get $vt))
              (i64.load (local.get $slot)) (i64.const 0)
              (i64.add (local.get $level) (i64.const 1)) (local.get $sb)))
          (else (if (i32.eq (local.get $vt) (i32.const 1))        ;; string slot: (ptr, len)
            (then
              (call $__rt_print_r_render (i64.const 1)
                (i64.load (local.get $slot)) (i64.load (i32.add (local.get $slot) (i32.const 8)))
                (i64.add (local.get $level) (i64.const 1)) (local.get $sb)))
            (else (if (i32.eq (local.get $vt) (i32.const 7))      ;; mixed-cell slot
              (then
                (call $__rt_mixed_unbox (i32.wrap_i64 (i64.load (local.get $slot))))
                (local.set $chi)
                (local.set $clo)
                (local.set $ctag)
                (call $__rt_print_r_render (local.get $ctag) (local.get $clo) (local.get $chi)
                  (i64.add (local.get $level) (i64.const 1)) (local.get $sb)))
              (else                                               ;; container slot: its pointer
                (call $__rt_print_r_render (i64.extend_i32_u (local.get $vt))
                  (i64.load (local.get $slot)) (i64.const 0)
                  (i64.add (local.get $level) (i64.const 1)) (local.get $sb))))))))
        (call $__rt_sb_byte (local.get $sb) (i32.const 10))       ;; the entry's own newline
        (local.set $i (i64.add (local.get $i) (i64.const 1)))
        (br $each)))
      (call $__rt_sb_spaces (local.get $sb) (i32.wrap_i64 (i64.mul (local.get $level) (i64.const 8))))
      (call $__rt_sb_byte (local.get $sb) (i32.const 41))         ;; ')'
      (call $__rt_sb_byte (local.get $sb) (i32.const 10))
      (return)))
  (if (i64.eq (local.get $tag) (i64.const 5))                     ;; hash, in insertion order
    (then
      (local.set $arr (i32.wrap_i64 (local.get $lo)))
      (call $__rt_sb_append (local.get $sb) (i32.const {head_p}) (i32.const {head_l}))
      (call $__rt_sb_spaces (local.get $sb) (i32.wrap_i64 (i64.mul (local.get $level) (i64.const 8))))
      (call $__rt_sb_byte (local.get $sb) (i32.const 40))         ;; '('
      (call $__rt_sb_byte (local.get $sb) (i32.const 10))
      (local.set $cursor (i64.const -2))
      (block $hdone (loop $heach
        (call $__rt_hash_iter_next (local.get $arr) (local.get $cursor))
        (local.set $more)
        (local.set $cursor)
        (br_if $hdone (i64.eqz (local.get $more)))
        (local.set $entry (i32.add (i32.add (local.get $arr) (i32.const 40))
                                   (i32.wrap_i64 (i64.mul (local.get $cursor) (i64.const 72)))))
        (call $__rt_sb_spaces (local.get $sb) (i32.wrap_i64
          (i64.add (i64.mul (local.get $level) (i64.const 8)) (i64.const 4))))
        (call $__rt_sb_byte (local.get $sb) (i32.const 91))       ;; '['
        (local.set $klo (i64.load (i32.add (local.get $entry) (i32.const 8))))
        (local.set $khi (i64.load (i32.add (local.get $entry) (i32.const 16))))
        (if (i64.eq (local.get $khi) (i64.const -1))
          (then                                                   ;; int key
            (call $__rt_itoa (local.get $klo) (global.get $__float_scratch))
            (local.set $l)
            (local.set $p)
            (call $__rt_sb_append (local.get $sb) (local.get $p) (local.get $l)))
          (else                                                   ;; string key, raw bytes
            (call $__rt_sb_append (local.get $sb)
              (i32.wrap_i64 (local.get $klo)) (i32.wrap_i64 (local.get $khi)))))
        (call $__rt_sb_append (local.get $sb) (i32.const {arrow_p}) (i32.const {arrow_l}))
        (call $__rt_print_r_render
          (i64.load (i32.add (local.get $entry) (i32.const 40)))  ;; value_tag
          (i64.load (i32.add (local.get $entry) (i32.const 24)))  ;; value_lo
          (i64.load (i32.add (local.get $entry) (i32.const 32)))  ;; value_hi
          (i64.add (local.get $level) (i64.const 1)) (local.get $sb))
        (call $__rt_sb_byte (local.get $sb) (i32.const 10))       ;; the entry's own newline
        (br $heach)))
      (call $__rt_sb_spaces (local.get $sb) (i32.wrap_i64 (i64.mul (local.get $level) (i64.const 8))))
      (call $__rt_sb_byte (local.get $sb) (i32.const 41))         ;; ')'
      (call $__rt_sb_byte (local.get $sb) (i32.const 10))
      (return)))
  (call $__rt_fail (i32.const 15))                                ;; object/resource: fail LOUDLY
  unreachable)                                                    ;; elephc-trap:post-noreturn:print-r-unrenderable __rt_fail exits the process
"#,
        head_p = head.0,
        head_l = head.1,
        arrow_p = arrow.0,
        arrow_l = arrow.1,
    ));
    wm.add_raw_func(
        r#"(func $__rt_print_r (param $cell i32) (param $ret i64) (result i32)
  (local $sb i32) (local $tag i64) (local $lo i64) (local $hi i64)
  (local $buf i32) (local $len i32) (local $out i32)
  (local.set $sb (call $__rt_heap_alloc (i32.const 12)))
  (i32.store (local.get $sb) (i32.const 0))
  (i32.store (i32.add (local.get $sb) (i32.const 4)) (i32.const 0))
  (i32.store (i32.add (local.get $sb) (i32.const 8)) (i32.const 0))
  (call $__rt_mixed_unbox (local.get $cell))
  (local.set $hi)
  (local.set $lo)
  (local.set $tag)
  (call $__rt_print_r_render (local.get $tag) (local.get $lo) (local.get $hi) (i64.const 0) (local.get $sb))
  (local.set $buf (i32.load (local.get $sb)))
  (local.set $len (i32.load (i32.add (local.get $sb) (i32.const 8))))
  (if (result i32) (i64.ne (local.get $ret) (i64.const 0))
    (then                                                         ;; capture mode: the rendered string
      (call $__rt_mixed_from_value (i64.const 1)
        (i64.extend_i32_u (local.get $buf)) (i64.extend_i32_u (local.get $len))))
    (else                                                         ;; echo mode: write, answer true
      (call $__rt_stdout_write (local.get $buf) (local.get $len))
      (call $__rt_mixed_from_value (i64.const 3) (i64.const 1) (i64.const 0))))
  (local.set $out)
  (if (local.get $buf)
    (then (call $__rt_heap_free (local.get $buf))))
  (call $__rt_heap_free (local.get $sb))
  (local.get $out))
"#,
    );
}

/// `__rt_readline`: reads one line from stdin, WITHOUT its terminating newline.
///
/// Two details come from measuring php-src 8.5.6 rather than from the native backend, which
/// gets both wrong (`printf 'Ada\n' | ...` answers `Hello Ada\n!` there, and prints the prompt):
///
/// - The newline is NOT part of the result. `readline()` strips it; keeping it puts a line break
///   in the middle of `"Hello " . $name . "!"`.
/// - The PROMPT is not written to stdout. php-src hands it to the terminal, so with stdout
///   redirected it does not appear at all — writing it here would add bytes php-src never emits.
///   The argument is therefore accepted and ignored, which is exactly what a captured run sees.
///
/// Bytes land in the legacy concat reservation, which is dead space kept only so static-data
/// offsets stay stable, so nothing else can be occupying it. Reading one byte at a time keeps the
/// helper free of any buffering that could swallow input a later read needs.
///
/// EOF answers php's `false`, and only when NOTHING was read: a line ended by EOF rather than by
/// a newline is still that line (`printf 'Ada'` answers `"Ada"`), and an empty line is the empty
/// string, not `false`. The result is therefore a boxed cell rather than a raw string — which is
/// what the checker types it, `string|false`.
const RT_READLINE: &str = r#"(func $__rt_readline (param $prompt_ptr i32) (param $prompt_len i64) (result i32)
  (local $n i32)
  (local $rc i32)
  (local $ended_at_eof i32)
  (local $ptr i32)
  (local $len i64)
  (block $done (loop $next
    (br_if $done (i32.ge_u (local.get $n) (i32.const 65535)))     ;; bound by the reservation
    (i32.store (i32.const 0) (i32.add (i32.const 64) (local.get $n)))  ;; iovec buf = base + n
    (i32.store (i32.const 4) (i32.const 1))                       ;; iovec len = 1 byte
    (local.set $rc (call $wasi_fd_read (i32.const 0) (i32.const 0) (i32.const 1) (i32.const 8)))
    (if (i32.ne (local.get $rc) (i32.const 0))
      (then (local.set $ended_at_eof (i32.const 1)) (br $done)))  ;; read error -> end the line
    (if (i32.eqz (i32.load (i32.const 8)))
      (then (local.set $ended_at_eof (i32.const 1)) (br $done)))  ;; 0 bytes read = EOF
    (br_if $done (i32.eq (i32.load8_u (i32.add (i32.const 64) (local.get $n))) (i32.const 10)))  ;; newline ends it and is dropped
    (local.set $n (i32.add (local.get $n) (i32.const 1)))
    (br $next)))
  ;; php answers `false` only when the line could not be read AT ALL. A line ended by EOF
  ;; instead of a newline is still that line, and an empty line is `""`.
  (if (i32.and (local.get $ended_at_eof) (i32.eqz (local.get $n)))
    (then (return (call $__rt_mixed_from_value (i64.const 3) (i64.const 0) (i64.const 0)))))
  (call $__rt_str_persist (i32.const 64) (i64.extend_i32_u (local.get $n)))
  (local.set $len)
  (local.set $ptr)
  (call $__rt_mixed_from_value (i64.const 1)
    (i64.extend_i32_u (local.get $ptr)) (local.get $len)))
"#;

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
        ERR_PRINT_R_UNRENDERABLE,
        ERR_LATE_STATIC_UNRESOLVED,
        ERR_STREAM_CHUNK_SIZE,
        ERR_BC_MALFORMED,
        ERR_Z_CORRUPT,
        ERR_REWIND_ADVANCED,
        ERR_FCLOSE_CLOSED,
        ERR_STREAM_CHUNK_TOO_LARGE,
        ERR_BCSCALE_RANGE,
        ERR_BC_SCALE_RANGE,
        ERR_GENERATOR_NO_RETURN,
        ERR_PREG_LIMIT,
        ERR_GENERATOR_CLOSED,
        ERR_OB_START_IN_HANDLER,
        ERR_OB_END_FLUSH_IN_HANDLER,
        ERR_OB_END_CLEAN_IN_HANDLER,
        ERR_OB_GET_CLEAN_IN_HANDLER,
        ERR_ARRAY_PAD_SIZE,
        ERR_UNION_NULL_LEFT,
        ERR_UNION_NULL_RIGHT,
        ERR_MERGE_NULL_FIRST,
        ERR_MERGE_NULL_SECOND,
        ERR_REPLACE_NULL_FIRST,
        ERR_REPLACE_NULL_SECOND,
        ERR_UNION_NULL_BOTH,
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
        // Appended LAST so every index the emitters above already use keeps its message.
        ERR_TOO_FEW_ARGS_PREFIX,
        ERR_TOO_FEW_ARGS_PASSED,
        ERR_TOO_FEW_ARGS_EXACTLY,
        ERR_TOO_FEW_ARGS_AT_LEAST,
        ERR_TOO_FEW_ARGS_SUFFIX,
        // Appended LAST so every index the emitters above already use keeps its message.
        ERR_UNDEFINED_FUNCTION_PREFIX,
        ERR_UNDEFINED_FUNCTION_SUFFIX,
        // Appended LAST for the same reason as every group above it.
        ERR_ERROR_VALUE_PREFIX,
        ERR_ERROR_VALUE_SUFFIX,
        // Appended LAST for the same reason as every group above it.
        ERR_ASSIGN_ON_NULL_PREFIX,
        ERR_ASSIGN_ON_NULL_SUFFIX,
        ERR_MODIFY_ON_NULL_PREFIX,
        DUMP_INT_PREFIX,
        DUMP_FLOAT_PREFIX,
        DUMP_STRING_PREFIX,
        DUMP_STRING_MIDDLE,
        DUMP_STRING_SUFFIX,
        DUMP_CLOSE,
        DUMP_TRUE,
        DUMP_FALSE,
        DUMP_NULL,
        DUMP_ARRAY_PREFIX,
        DUMP_ARRAY_OPEN,
        DUMP_ARRAY_CLOSE,
        DUMP_KEY_OPEN,
        DUMP_KEY_CLOSE,
        DUMP_QUOTE,
        DUMP_INDENT,
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
        // The declared-return coercion fragments come LAST so every index already in use
        // above keeps pointing at the same message.
        DEPRECATED_FLOAT_TO_INT_PREFIX,
        DEPRECATED_FLOAT_STR_TO_INT_PREFIX,
        DEPRECATED_TO_INT_SUFFIX,
        DEPRECATED_STR_TO_INT_SUFFIX,
        ERR_RETURN_TYPE_PREFIX,
        ERR_RETURN_TYPE_MIDDLE,
        ERR_RETURN_TYPE_SUFFIX,
        PHP_CLASS_CLOSURE,
        ERR_RETURN_TYPE_SEPARATOR,
        WARN_NAN_TO_STRING,
        WARN_NAN_TO_BOOL,
        PHP_VALUE_TRUE,
        PHP_VALUE_FALSE,
        ERR_COUNT_PREFIX,
        ERR_COUNT_SUFFIX,
        WARN_UNINIT_STRING_OFFSET,
        WARN_OFFSET_NEWLINE,
        // Appended LAST, like the return-coercion fragments above: every index already in use
        // keeps pointing at the same message, so the positional slices below do not move.
        WARN_PROPERTY_ON_NULL_PREFIX,
        WARN_PROPERTY_ON_NULL_SUFFIX,
        WARN_OFFSET_ON_TRUE,
        WARN_OFFSET_ON_FALSE,
        WARN_OFFSET_ON_INT,
        WARN_OFFSET_ON_FLOAT,
        WARN_OFFSET_ON_TYPE_BOOL,
        WARN_OFFSET_ON_TYPE_INT,
        WARN_OFFSET_ON_TYPE_FLOAT,
        WARN_FOPEN_FAILED,
        WARN_FILE_GET_CONTENTS_FAILED,
        // Appended LAST for the same reason as every group above it.
        ERR_ARGUMENT_TYPE_MIDDLE,
        ERR_ARGUMENT_NAME_PREFIX,
        ERR_ARGUMENT_TYPE_MUST_BE,
        ERR_ARGUMENT_TYPE_SUFFIX,
        DEPRECATED_ARGUMENT_NULL_PREFIX,
        DEPRECATED_ARGUMENT_NULL_MIDDLE,
        DEPRECATED_ARGUMENT_NULL_OF_TYPE,
        DEPRECATED_ARGUMENT_NULL_SUFFIX,
        WARN_FOREACH_NON_ITERABLE_PREFIX,
        WARN_FOREACH_NON_ITERABLE_SUFFIX,
        ERR_OPERAND_TYPES_PREFIX,
        ERR_OPERAND_TYPES_SPACE,
        // Appended LAST for the same reason as every group above it.
        WARN_CONSTANT_DEFINED_PREFIX,
        WARN_CONSTANT_DEFINED_SUFFIX,
        WARN_CONSTANT_DEFINED_SUFFIX_PHP9,
        // Appended LAST for the same reason as every group above it.
        WARN_INC_BOOL,
        WARN_DEC_BOOL,
        WARN_DEC_NULL,
        DEPRECATED_INC_STR,
        DEPRECATED_DEC_STR,
        DEPRECATED_DEC_EMPTY,
        // Appended LAST for the same reason as every group above it.
        DEPRECATED_FLOAT_KEY_PREFIX,
        DEPRECATED_FLOAT_KEY_SUFFIX,
        DEPRECATED_NULL_KEY,
        // Appended LAST for the same reason as every group above it.
        META_KEY_TIMED_OUT,
        META_KEY_BLOCKED,
        META_KEY_EOF,
        META_KEY_WRAPPER_TYPE,
        META_KEY_STREAM_TYPE,
        META_KEY_MODE,
        META_KEY_UNREAD_BYTES,
        META_KEY_SEEKABLE,
        META_KEY_URI,
        META_WRAPPER_PLAINFILE,
        META_STREAM_STDIO,
        META_WRAPPER_PHP,
        META_STREAM_MEMORY,
        META_STREAM_TEMP,
        // Appended LAST for the same reason as every group above it.
        PRINT_R_ARRAY_HEAD,
        PRINT_R_ARROW,
        WARN_UNDEFINED_PROPERTY_PREFIX,
        WARN_UNDEFINED_PROPERTY_SCOPE,
        // Appended LAST for the same reason as every group above it.
        WARN_PROPERTY_ON_MIDDLE,
        WARN_PROPERTY_ON_TRUE,
        WARN_PROPERTY_ON_FALSE,
        // Appended LAST for the same reason as every group above it.
        WARN_ARRAY_SUM_UNSUPPORTED,
        WARN_ARRAY_PRODUCT_UNSUPPORTED,
        WARN_ARRAY_FOLD_NEWLINE,
        // Appended LAST for the same reason as every group above it.
        META_STREAM_OUTPUT,
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
    emit_error_value_failure_runtime(wm, &method_offsets[21..23]);
    emit_assign_on_null_runtime(wm, &method_offsets[23..26]);
    super::inet::emit_var_dump_runtime(wm, &method_offsets[26..42]);
    emit_undefined_array_key_warning_runtime(wm, &warning_offsets[..17]);
    emit_return_coercion_runtime(wm, &warning_offsets[17..34], &method_offsets[2..11]);
    emit_property_on_null_warning_runtime(wm, &warning_offsets[34..36]);
    emit_offset_on_scalar_warning_runtime(wm, &warning_offsets[36..43]);
    emit_uninit_string_offset_warning_runtime(wm, &warning_offsets[32..34]);
    emit_open_failure_warning_runtime(wm, &warning_offsets[43..45]);
    emit_argument_coercion_runtime(
        wm,
        &warning_offsets[45..53],
        warning_offsets[21],
        warning_offsets[25],
        &method_offsets[2..11],
    );
    emit_foreach_warning_runtime(wm, &warning_offsets[53..55]);
    emit_constant_redefinition_warning(wm, &warning_offsets[57..60]);
    emit_inc_dec_diag_runtime(wm, &warning_offsets[60..66]);
    emit_mixed_key_diag_runtime(wm, &warning_offsets[66..69]);
    // The boxed-key getter calls those two deprecations, so it is emitted with them.
    wm.add_raw_func(super::arrays::RT_STR_CANONICAL_INT);
    wm.add_raw_func(&super::arrays::rt_array_get_mixed_index());
    // stream_get_meta_data builds its hash from the nine keys and four constant
    // values above, so it is emitted here where their offsets are known — like the
    // boxed-key getter, not in the file runtime a partial test module may emit alone.
    super::files::emit_stream_meta_runtime(wm, &[&warning_offsets[69..83], &warning_offsets[93..94]].concat());
    emit_print_r_runtime(wm, &warning_offsets[83..85]);
    emit_undefined_property_warning_runtime(wm, &warning_offsets[85..87], warning_offsets[2]);
    emit_property_on_non_object_warning_runtime(
        wm,
        warning_offsets[34],
        &warning_offsets[87..90],
        &method_offsets[2..11],
    );
    emit_arithmetic_coercion_runtime(
        wm,
        &warning_offsets[55..57],
        &warning_offsets[5..7],
        warning_offsets[33],
    );
    emit_boxed_array_fold_runtime(wm, &warning_offsets[90..93]);
}

/// Emits `array_sum`/`array_product` over a boxed array, with php's per-element rules.
///
/// Every element goes through `+`/`*` itself — `__rt_mixed_numeric_common`, which promotes an
/// overflow and warns for a leading-numeric string — except what php refuses to fold: a
/// non-numeric string or a resource warns and folds its integer value, an array, an object or
/// a closure warns and is skipped. Measured on php 8.5.10: `array_sum(["abc", 1])` is 1,
/// `array_product(["abc", 2])` is 0, `array_product([[1], 2])` is 2, each with its warning.
fn emit_boxed_array_fold_runtime(wm: &mut WatModule, offsets: &[(u32, u32)]) {
    debug_assert_eq!(offsets.len(), 3);
    let (sum_ptr, sum_len) = offsets[0];
    let (product_ptr, product_len) = offsets[1];
    let (newline_ptr, newline_len) = offsets[2];
    wm.add_raw_func(&format!(
        r#"(func $__rt_warn_array_fold_unsupported (param $mul i32) (param $tag i64) (param $lo i64)
  (local $word_ptr i32) (local $word_len i32)
  (if (global.get $__diag_suppress) (then (return)))
  (call $__rt_type_word_for_tag (local.get $tag) (local.get $lo))
  (local.set $word_len)
  (local.set $word_ptr)
  (if (local.get $mul)
    (then (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {product_ptr}) (i32.const {product_len})))
    (else (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {sum_ptr}) (i32.const {sum_len}))))
  (call $__rt_wasi_write_or_fail (i32.const 2) (local.get $word_ptr) (local.get $word_len))
  (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {newline_ptr}) (i32.const {newline_len})))"#
    ));
    wm.add_raw_func(
        r#"(func $__rt_boxed_array_fold (param $cell i32) (param $mul i32) (result i32)
  (local $src i32) (local $cursor i64) (local $kind i32) (local $more i64)
  (local $acc i32) (local $v i32) (local $num i32) (local $next i32)
  (local $tag i64) (local $lo i64) (local $hi i64)
  (local.set $acc (call $__rt_mixed_from_value (i64.const 0)
    (i64.extend_i32_u (local.get $mul)) (i64.const 0)))                 ;; 0 for a sum, 1 for a product
  (call $__rt_mixed_iter_start (local.get $cell))
  (local.set $kind)
  (local.set $cursor)
  (local.set $src)
  (block $done (loop $entry
    (call $__rt_mixed_iter_next (local.get $src) (local.get $cursor) (local.get $kind))
    (local.set $more)
    (local.set $cursor)
    (br_if $done (i64.eqz (local.get $more)))
    (local.set $v (call $__rt_mixed_iter_value (local.get $src) (local.get $cursor) (local.get $kind)))
    (call $__rt_mixed_unbox (local.get $v))
    (local.set $hi)
    (local.set $lo)
    (local.set $tag)
    (local.set $num (local.get $v))                                     ;; by default the element itself
    (block $operand
      ;; int, float, bool and null fold as they are
      (br_if $operand (i32.or (i32.or (i64.eqz (local.get $tag)) (i64.eq (local.get $tag) (i64.const 2)))
                              (i32.or (i64.eq (local.get $tag) (i64.const 3)) (i64.eq (local.get $tag) (i64.const 8)))))
      ;; a numeric or leading-numeric string folds through `+`/`*`, which warns for the latter
      (if (i64.eq (local.get $tag) (i64.const 1))
        (then (br_if $operand (call $__rt_str_numeric_class
          (i32.wrap_i64 (local.get $lo)) (i32.wrap_i64 (local.get $hi))))))
      (call $__rt_warn_array_fold_unsupported (local.get $mul) (local.get $tag) (local.get $lo))
      ;; a string or a resource folds its integer value; anything else is skipped
      (if (i32.or (i64.eq (local.get $tag) (i64.const 1)) (i64.eq (local.get $tag) (i64.const 9)))
        (then
          (local.set $num (call $__rt_mixed_from_value (i64.const 0)
            (call $__rt_mixed_cast_int (local.get $v)) (i64.const 0)))
          (br $operand)))
      (local.set $num (i32.const 0)))
    (if (local.get $num)
      (then
        (local.set $next (call $__rt_mixed_numeric_common (local.get $acc) (local.get $num)
          (select (i32.const 2) (i32.const 0) (local.get $mul))))      ;; op 2 = mul, op 0 = add
        (call $__rt_decref_any (local.get $acc))
        (local.set $acc (local.get $next))
        (if (i32.ne (local.get $num) (local.get $v))
          (then (call $__rt_decref_any (local.get $num))))))
    (call $__rt_decref_any (local.get $v))
    (br $entry)))
  (local.get $acc))"#,
    );
}

/// Emits the two array-key conversion deprecations. The float one renders the key
/// through `__rt_ftoa` between its fragments, exactly as php prints it.
fn emit_mixed_key_diag_runtime(wm: &mut WatModule, offsets: &[(u32, u32)]) {
    debug_assert_eq!(offsets.len(), 3);
    let (fp, fl) = offsets[0];
    let (sp, sl) = offsets[1];
    let (np, nl) = offsets[2];
    wm.add_raw_func(&format!(
        r#"(func $__rt_depr_float_key (param $bits i64)
  (local $ptr i32) (local $len i32)
  (if (global.get $__diag_suppress) (then (return)))
  (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {fp}) (i32.const {fl}))
  (call $__rt_ftoa (local.get $bits)
    (i32.add (global.get $__float_scratch) (i32.const 1024)) (i32.const 80)
    (i32.add (global.get $__float_scratch) (i32.const 2048)) (i32.const 792)
    (i32.add (global.get $__float_scratch) (i32.const 4096)))
  (local.set $len)
  (local.set $ptr)
  (call $__rt_wasi_write_or_fail (i32.const 2) (local.get $ptr) (local.get $len))
  (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {sp}) (i32.const {sl})))"#
    ));
    wm.add_raw_func(&format!(
        r#"(func $__rt_depr_null_key
  (if (global.get $__diag_suppress) (then (return)))
  (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {np}) (i32.const {nl})))"#
    ));
}

/// Emits the six `++`/`--` diagnostic helpers, one per measured PHP message.
///
/// Value semantics live in `__rt_mixed_inc_dec`; these only write the fixed text,
/// each behind the shared `@` suppression guard.
fn emit_inc_dec_diag_runtime(wm: &mut WatModule, offsets: &[(u32, u32)]) {
    debug_assert_eq!(offsets.len(), 6);
    for (name, (ptr, len)) in [
        "__rt_warn_inc_bool",
        "__rt_warn_dec_bool",
        "__rt_warn_dec_null",
        "__rt_depr_inc_str",
        "__rt_depr_dec_str",
        "__rt_depr_dec_empty",
    ]
    .into_iter()
    .zip(offsets)
    {
        wm.add_raw_func(&format!(
            r#"(func ${name}
  (if (global.get $__diag_suppress) (then (return)))
  (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {ptr}) (i32.const {len})))"#
        ));
    }
}

/// Emits the warning `foreach` produces for a value that is neither an array nor an object.
///
/// Measured on php-src 8.5.6: it WARNS, names the type that arrived, and runs the body zero
/// times rather than raising. The type word comes from the shared `__rt_type_word_for_tag`, so
/// an object would contribute its class — unreachable here, since an object IS iterable and
/// never reaches this arm.
fn emit_foreach_warning_runtime(wm: &mut WatModule, offsets: &[(u32, u32)]) {
    debug_assert_eq!(offsets.len(), 2);
    let (prefix_ptr, prefix_len) = offsets[0];
    let (suffix_ptr, suffix_len) = offsets[1];
    wm.add_raw_func(&format!(
        r#"(func $__rt_warn_foreach_non_iterable (param $tag i64) (param $lo i64)
  (local $word_ptr i32) (local $word_len i32)
  (if (global.get $__diag_suppress) (then (return)))
  (call $__rt_type_word_for_tag (local.get $tag) (local.get $lo))
  (local.set $word_len)
  (local.set $word_ptr)
  (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {prefix_ptr}) (i32.const {prefix_len}))
  (call $__rt_wasi_write_or_fail (i32.const 2) (local.get $word_ptr) (local.get $word_len))
  (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {suffix_ptr}) (i32.const {suffix_len})))"#
    ));
}

/// Emits the coercion php-src performs when a boxed value reaches an internal function's
/// declared `string` parameter.
///
/// Measured on php-src 8.5.6 for `strtoupper($mixed)`, one arm per runtime tag: a string, int,
/// float or bool converts EXACTLY as `(string)` does, which is why the scalar arms delegate to
/// `__rt_mixed_cast_string` rather than restating it; `null` converts to `""` but raises a
/// `Deprecated` first; and an array, object, resource or closure does not convert at all — it is
/// a `TypeError` naming what arrived, where `(string)` of the same array would have produced
/// `"Array"` with a warning. NaN warns on the way through, exactly as the declared-RETURN
/// coercion already does.
///
/// The function and parameter names travel from the call site as `(ptr, len)` pairs, the same
/// way `__rt_fail_return_type` carries its function name, so no per-call-site data layout is
/// needed beyond the two interned strings.
fn emit_argument_coercion_runtime(
    wm: &mut WatModule,
    offsets: &[(u32, u32)],
    error_prefix: (u32, u32),
    separator: (u32, u32),
    type_offsets: &[(u32, u32)],
) {
    debug_assert_eq!(offsets.len(), 8);
    debug_assert_eq!(type_offsets.len(), 9);
    let (argument_ptr, argument_len) = offsets[0];
    let (name_ptr, name_len) = offsets[1];
    let (must_be_ptr, must_be_len) = offsets[2];
    let (given_ptr, given_len) = offsets[3];
    let (deprecated_ptr, deprecated_len) = offsets[4];
    let (passing_ptr, passing_len) = offsets[5];
    let (of_type_ptr, of_type_len) = offsets[6];
    let (is_deprecated_ptr, is_deprecated_len) = offsets[7];
    // The word sits mid-sentence here, so its trailing newline is dropped from the length —
    // the same adjustment the declared-return fatal makes.
    let (string_word_ptr, string_word_len) = (type_offsets[1].0, type_offsets[1].1 - 1);
    let (error_prefix_ptr, error_prefix_len) = error_prefix;
    let (separator_ptr, separator_len) = separator;

    wm.add_raw_func(&format!(
        r#"(func $__rt_fail_argument_type (param $fn_ptr i32) (param $fn_len i32) (param $param_ptr i32) (param $param_len i32) (param $argno i64) (param $target_ptr i32) (param $target_len i32) (param $tag i64) (param $lo i64)
  (local $word_ptr i32) (local $word_len i32) (local $num_ptr i32) (local $num_len i32)
  (call $__rt_type_word_for_tag (local.get $tag) (local.get $lo))
  (local.set $word_len)
  (local.set $word_ptr)
  (drop (call $__rt_wasi_write_all (i32.const 2) (i32.const {error_prefix_ptr}) (i32.const {error_prefix_len})))
  (drop (call $__rt_wasi_write_all (i32.const 2) (local.get $fn_ptr) (local.get $fn_len)))
  (drop (call $__rt_wasi_write_all (i32.const 2) (i32.const {argument_ptr}) (i32.const {argument_len})))
  (call $__rt_itoa (local.get $argno) (global.get $__float_scratch))
  (local.set $num_len)
  (local.set $num_ptr)
  (drop (call $__rt_wasi_write_all (i32.const 2) (local.get $num_ptr) (local.get $num_len)))
  (drop (call $__rt_wasi_write_all (i32.const 2) (i32.const {name_ptr}) (i32.const {name_len})))
  (drop (call $__rt_wasi_write_all (i32.const 2) (local.get $param_ptr) (local.get $param_len)))
  (drop (call $__rt_wasi_write_all (i32.const 2) (i32.const {must_be_ptr}) (i32.const {must_be_len})))
  (drop (call $__rt_wasi_write_all (i32.const 2) (local.get $target_ptr) (local.get $target_len)))
  (drop (call $__rt_wasi_write_all (i32.const 2) (i32.const {separator_ptr}) (i32.const {separator_len})))
  (drop (call $__rt_wasi_write_all (i32.const 2) (local.get $word_ptr) (local.get $word_len)))
  (drop (call $__rt_wasi_write_all (i32.const 2) (i32.const {given_ptr}) (i32.const {given_len})))
  (call $wasi_proc_exit (i32.const 255))
  unreachable ;; elephc-trap:post-noreturn:argument-type-fatal-exit
)"#
    ));
    wm.add_raw_func(&format!(
        r#"(func $__rt_deprecate_argument_null (param $fn_ptr i32) (param $fn_len i32) (param $param_ptr i32) (param $param_len i32) (param $argno i64) (param $target_ptr i32) (param $target_len i32)
  (local $num_ptr i32) (local $num_len i32)
  (if (global.get $__diag_suppress) (then (return)))                ;; `@` silences deprecations too
  (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {deprecated_ptr}) (i32.const {deprecated_len}))
  (call $__rt_wasi_write_or_fail (i32.const 2) (local.get $fn_ptr) (local.get $fn_len))
  (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {passing_ptr}) (i32.const {passing_len}))
  (call $__rt_itoa (local.get $argno) (global.get $__float_scratch))
  (local.set $num_len)
  (local.set $num_ptr)
  (call $__rt_wasi_write_or_fail (i32.const 2) (local.get $num_ptr) (local.get $num_len))
  (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {name_ptr}) (i32.const {name_len}))
  (call $__rt_wasi_write_or_fail (i32.const 2) (local.get $param_ptr) (local.get $param_len))
  (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {of_type_ptr}) (i32.const {of_type_len}))
  (call $__rt_wasi_write_or_fail (i32.const 2) (local.get $target_ptr) (local.get $target_len))
  (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {is_deprecated_ptr}) (i32.const {is_deprecated_len})))"#
    ));
    wm.add_raw_func(&format!(
        r#"(func $__rt_mixed_arg_string (param $cell i32) (param $fn_ptr i32) (param $fn_len i32) (param $param_ptr i32) (param $param_len i32) (param $argno i64) (result i32) (result i32)
  (local $tag i64) (local $lo i64) (local $hi i64) (local $f f64) (local $ok i32) (local $sptr i32) (local $slen i32) (local $pptr i32) (local $plen i64)
  (call $__rt_mixed_unbox (local.get $cell))
  (local.set $hi)
  (local.set $lo)
  (local.set $tag)
  (if (i64.eq (local.get $tag) (i64.const 2))                     ;; NaN warns on the way through
    (then
      (local.set $f (f64.reinterpret_i64 (local.get $lo)))
      (if (f64.ne (local.get $f) (local.get $f))
        (then (call $__rt_deprecate_nan_to_string)))))
  (if (i64.le_u (local.get $tag) (i64.const 3))                   ;; int/string/float/bool convert as `(string)` does
    (then (return (call $__rt_mixed_cast_string (local.get $cell)))))
  (if (i64.eq (local.get $tag) (i64.const 8))                     ;; null still converts, after a deprecation
    (then
      (call $__rt_deprecate_argument_null (local.get $fn_ptr) (local.get $fn_len) (local.get $param_ptr) (local.get $param_len) (local.get $argno) (i32.const {string_word_ptr}) (i32.const {string_word_len}))
      (return (i32.const 0) (i32.const 0))))
  (if (i64.eq (local.get $tag) (i64.const 6))                     ;; an object with `__toString` CONVERTS
    (then
      (call $__rt_object_to_string (i32.wrap_i64 (local.get $lo)))
      (local.set $ok)
      (local.set $slen)
      (local.set $sptr)
      (if (local.get $ok)
        (then
          (call $__rt_str_persist (local.get $sptr) (i64.extend_i32_u (local.get $slen)))  ;; own an independent copy
          (local.set $plen)
          (local.set $pptr)
          (call $__rt_decref_any (local.get $sptr))               ;; a callee's Str return is OWNED
          (return (local.get $pptr) (i32.wrap_i64 (local.get $plen)))))))
  (call $__rt_fail_argument_type (local.get $fn_ptr) (local.get $fn_len) (local.get $param_ptr) (local.get $param_len) (local.get $argno) (i32.const {string_word_ptr}) (i32.const {string_word_len}) (local.get $tag) (local.get $lo))
  unreachable)                                                    ;; elephc-trap:post-noreturn:argument-coerce-tostring
"#
    ));
}

/// Emits the coercion PHP performs on a BOXED operand of an integer arithmetic operator.
///
/// This is a THIRD contract, distinct from both the declared-return and the declared-parameter
/// ones, and every difference was measured on php-src 8.5.6 with `$mixed % 3`:
///
/// - `null` is silently 0 — a parameter deprecates there and a return raises;
/// - a non-numeric string is `Unsupported operand types: string % int`, naming the operand types
///   rather than `must be of type int`;
/// - `INF`/`NAN` do NOT raise: they warn `The float INF is not representable as an int, cast
///   occurred` and yield 0, where a parameter raises a `TypeError`.
///
/// What IS shared is the numeric middle: a lost fraction deprecates identically, from a float
/// and from a float-shaped string alike, so those two notices come from the same helpers the
/// other boundaries call.
fn emit_arithmetic_coercion_runtime(
    wm: &mut WatModule,
    offsets: &[(u32, u32)],
    float_warning: &[(u32, u32)],
    newline: (u32, u32),
) {
    debug_assert_eq!(offsets.len(), 2);
    debug_assert_eq!(float_warning.len(), 2);
    let (prefix_ptr, prefix_len) = offsets[0];
    let (space_ptr, space_len) = offsets[1];
    let (not_repr_prefix_ptr, not_repr_prefix_len) = float_warning[0];
    let (not_repr_suffix_ptr, not_repr_suffix_len) = float_warning[1];
    let (newline_ptr, newline_len) = newline;
    let value_offset = super::mixed_numeric::CLASS_VALUE_OFFSET;

    wm.add_raw_func(&format!(
        r#"(func $__rt_fail_operand_types (param $tag i64) (param $lo i64) (param $op_ptr i32) (param $op_len i32) (param $right_ptr i32) (param $right_len i32)
  (local $word_ptr i32) (local $word_len i32)
  (call $__rt_type_word_for_tag (local.get $tag) (local.get $lo))
  (local.set $word_len)
  (local.set $word_ptr)
  (drop (call $__rt_wasi_write_all (i32.const 2) (i32.const {prefix_ptr}) (i32.const {prefix_len})))
  (drop (call $__rt_wasi_write_all (i32.const 2) (local.get $word_ptr) (local.get $word_len)))
  (drop (call $__rt_wasi_write_all (i32.const 2) (i32.const {space_ptr}) (i32.const {space_len})))
  (drop (call $__rt_wasi_write_all (i32.const 2) (local.get $op_ptr) (local.get $op_len)))
  (drop (call $__rt_wasi_write_all (i32.const 2) (i32.const {space_ptr}) (i32.const {space_len})))
  (drop (call $__rt_wasi_write_all (i32.const 2) (local.get $right_ptr) (local.get $right_len)))
  (drop (call $__rt_wasi_write_all (i32.const 2) (i32.const {newline_ptr}) (i32.const {newline_len})))
  (call $wasi_proc_exit (i32.const 255))
  unreachable ;; elephc-trap:post-noreturn:operand-types-fatal-exit
)"#
    ));

    wm.add_raw_func(&format!(
        r#"(func $__rt_mixed_arith_int (param $cell i32) (param $op_ptr i32) (param $op_len i32) (param $right_ptr i32) (param $right_len i32) (result i64)
  (local $tag i64) (local $lo i64) (local $hi i64) (local $f f64) (local $t f64) (local $cls i32) (local $fptr i32) (local $flen i32) (local $iv i64)
  (call $__rt_mixed_unbox (local.get $cell))
  (local.set $hi)
  (local.set $lo)
  (local.set $tag)
  (if (i64.eqz (local.get $tag))                                  ;; int
    (then (return (local.get $lo))))
  (if (i64.eq (local.get $tag) (i64.const 3))                     ;; bool: stored normalized 0/1
    (then (return (local.get $lo))))
  (if (i64.eq (local.get $tag) (i64.const 8))                     ;; null is SILENTLY zero here
    (then (return (i64.const 0))))
  (if (i64.eq (local.get $tag) (i64.const 2))                     ;; float
    (then
      (local.set $f (f64.reinterpret_i64 (local.get $lo)))
      (if (i32.and
            (f64.eq (local.get $f) (local.get $f))
            (i32.and
              (f64.ge (local.get $f) (f64.const -9223372036854775808))
              (f64.lt (local.get $f) (f64.const 9223372036854775808))))
        (then
          (local.set $t (f64.trunc (local.get $f)))
          (if (f64.ne (local.get $t) (local.get $f))
            (then (call $__rt_deprecate_return_float_to_int (local.get $lo))))
          (return (i64.trunc_f64_s (local.get $t)))))
      ;; Out of range or not a number: php-src WARNS and uses 0 rather than raising.
      (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {not_repr_prefix_ptr}) (i32.const {not_repr_prefix_len}))
      (call $__rt_ftoa (local.get $lo) (i32.add (global.get $__float_scratch) (i32.const 1024)) (i32.const 80) (i32.add (global.get $__float_scratch) (i32.const 2048)) (i32.const 792) (i32.add (global.get $__float_scratch) (i32.const 4096)))
      (local.set $flen)                                           ;; ftoa returns (ptr, len), len on top
      (local.set $fptr)
      (call $__rt_wasi_write_or_fail (i32.const 2) (local.get $fptr) (local.get $flen))
      (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {not_repr_suffix_ptr}) (i32.const {not_repr_suffix_len}))
      (return (i64.const 0))))
  (if (i64.eq (local.get $tag) (i64.const 1))                     ;; string
    (then
      (local.set $cls (call $__rt_str_numeric_class (i32.wrap_i64 (local.get $lo)) (i32.wrap_i64 (local.get $hi))))
      (if (i32.eqz (local.get $cls))                              ;; no numeric prefix: the operand TypeError below
        (then (call $__rt_fail_operand_types (local.get $tag) (local.get $lo) (local.get $op_ptr) (local.get $op_len) (local.get $right_ptr) (local.get $right_len))))
      (if (i32.gt_u (local.get $cls) (i32.const 2))               ;; leading-numeric ("5 apples"): the value PLUS a warning
        (then (call $__rt_warn_non_numeric_value)))
      (if (i32.and (local.get $cls) (i32.const 1))                ;; classes 1 and 3 are the integer forms
        (then (return (i64.load (i32.add (global.get $__float_scratch) (i32.const {value_offset}))))))
      (local.set $f (f64.load (i32.add (global.get $__float_scratch) (i32.const {value_offset}))))
      ;; A float-string saturates like php's zend_dval_to_lval_cap: out of range clamps to the
      ;; nearest limit, NaN and infinity answer 0. It never traps the way a raw truncation would.
      (if (f64.eq (f64.sub (local.get $f) (local.get $f)) (f64.const 0))   ;; finite
        (then
          (if (f64.ge (local.get $f) (f64.const 9223372036854775808))
            (then (local.set $iv (i64.const 9223372036854775807)))
            (else
              (if (f64.lt (local.get $f) (f64.const -9223372036854775808))
                (then (local.set $iv (i64.const -9223372036854775808)))
                (else (local.set $iv (i64.trunc_f64_s (local.get $f)))))))))
      (if (f64.ne (f64.convert_i64_s (local.get $iv)) (local.get $f))
        (then (call $__rt_deprecate_return_str_to_int (i32.wrap_i64 (local.get $lo)) (i32.wrap_i64 (local.get $hi)))))
      (return (local.get $iv))))
  (call $__rt_fail_operand_types (local.get $tag) (local.get $lo) (local.get $op_ptr) (local.get $op_len) (local.get $right_ptr) (local.get $right_len))
  unreachable)                                                    ;; elephc-trap:post-noreturn:arith-coerce-int
"#
    ));
}

/// Emits php-src's warning for a property read whose receiver is not an object at all.
///
/// The null form above is one message; every other tag names its own type — `on int`, `on string`
/// — so this one composes the line from the same `PHP_TYPE_*` words the method-call fatal reads,
/// with two of its own: php prints the VALUE of a bool receiver, `on true` and `on false`, not
/// `on bool`.
///
/// A tag no arm claims is reported as `unknown`, which is the table's own fallback rather than a
/// guess: the receiver is a runtime value, so there is no shape to refuse at compile time.
fn emit_property_on_non_object_warning_runtime(
    wm: &mut WatModule,
    prefix: (u32, u32),
    parts: &[(u32, u32)],
    type_words: &[(u32, u32)],
) {
    debug_assert_eq!(parts.len(), 3);
    debug_assert_eq!(type_words.len(), 9);
    let (prefix_ptr, prefix_len) = prefix;
    let (middle_ptr, middle_len) = parts[0];
    let (true_ptr, true_len) = parts[1];
    let (false_ptr, false_len) = parts[2];
    let mut wat = format!(
        r#"(func $__rt_warn_property_on_non_object (param $name_ptr i32) (param $name_len i32) (param $tag i64) (param $lo i64)
  (local $type_ptr i32) (local $type_len i32)
  (if (global.get $__diag_suppress) (then (return)))
  (local.set $type_ptr (i32.const {unknown_ptr}))
  (local.set $type_len (i32.const {unknown_len}))
"#,
        unknown_ptr = type_words[8].0,
        unknown_len = type_words[8].1,
    );
    // Tag to type word: 0 int, 1 string, 2 float, 4/5 array, 8 null, 9 resource. A bool is the
    // exception and reads its payload.
    for (tag, word) in [(0, 0), (1, 1), (2, 2), (4, 4), (5, 4), (8, 5), (9, 6)] {
        let (ptr, len) = type_words[word];
        wat.push_str(&format!(
            "  (if (i64.eq (local.get $tag) (i64.const {tag}))
    (then
      (local.set $type_ptr (i32.const {ptr}))
      (local.set $type_len (i32.const {len}))))
"
        ));
    }
    wat.push_str(&format!(
        r#"  (if (i64.eq (local.get $tag) (i64.const 3))
    (then
      (if (i64.eqz (local.get $lo))
        (then
          (local.set $type_ptr (i32.const {false_ptr}))
          (local.set $type_len (i32.const {false_len})))
        (else
          (local.set $type_ptr (i32.const {true_ptr}))
          (local.set $type_len (i32.const {true_len}))))))
  (drop (call $__rt_wasi_write_all (i32.const 2) (i32.const {prefix_ptr}) (i32.const {prefix_len})))
  (drop (call $__rt_wasi_write_all (i32.const 2) (local.get $name_ptr) (local.get $name_len)))
  (drop (call $__rt_wasi_write_all (i32.const 2) (i32.const {middle_ptr}) (i32.const {middle_len})))
  (drop (call $__rt_wasi_write_all (i32.const 2) (local.get $type_ptr) (local.get $type_len))))
"#
    ));
    wm.add_raw_func(&wat);
}

/// Emits php-src's warning for a property read whose receiver is null.
///
/// A raw object pointer can be 0 since a missed `array<Object>` element reads as null with the
/// element's own non-null type, and PHP names the property rather than just the receiver:
/// `Warning: Attempt to read property "age" on null`. The name arrives as a (pointer, length)
/// pair from the instruction's own interned string, so no per-property data segment is needed.
fn emit_property_on_null_warning_runtime(wm: &mut WatModule, offsets: &[(u32, u32)]) {
    debug_assert_eq!(offsets.len(), 2);
    let (prefix_ptr, prefix_len) = offsets[0];
    let (suffix_ptr, suffix_len) = offsets[1];
    wm.add_raw_func(&format!(
        r#"(func $__rt_warn_property_on_null (param $name_ptr i32) (param $name_len i32)
  (if (global.get $__diag_suppress) (then (return)))
  (drop (call $__rt_wasi_write_all (i32.const 2) (i32.const {prefix_ptr}) (i32.const {prefix_len})))
  (drop (call $__rt_wasi_write_all (i32.const 2) (local.get $name_ptr) (local.get $name_len)))
  (drop (call $__rt_wasi_write_all (i32.const 2) (i32.const {suffix_ptr}) (i32.const {suffix_len}))))"#
    ));
}

/// Emits `Warning: Undefined property: Bag::$k1` for a DYNAMIC property the object lacks.
///
/// Both names arrive at run time, which is what distinguishes this from every other warning
/// helper here: the property name is the value `$obj->$name` evaluated, and the CLASS comes from
/// the receiver's id through `__rt_class_name_by_cid` rather than from a per-class literal — so
/// one helper serves every class and a program with fifty of them lays out no extra bytes.
///
/// What it does NOT carry is php's ` in <file> on line N`, which is this target's standing
/// convention for every warning rather than a choice made here.
fn emit_undefined_property_warning_runtime(
    wm: &mut WatModule,
    offsets: &[(u32, u32)],
    suffix: (u32, u32),
) {
    debug_assert_eq!(offsets.len(), 2);
    let (prefix_ptr, prefix_len) = offsets[0];
    let (scope_ptr, scope_len) = offsets[1];
    let (suffix_ptr, suffix_len) = suffix;
    wm.add_raw_func(&format!(
        r#"(func $__rt_warn_undefined_property (param $cid i64) (param $name_ptr i32) (param $name_len i32)
  (local $class_ptr i32) (local $class_len i64)
  (if (global.get $__diag_suppress) (then (return)))
  (drop (call $__rt_wasi_write_all (i32.const 2) (i32.const {prefix_ptr}) (i32.const {prefix_len})))
  (call $__rt_class_name_by_cid (local.get $cid))                 ;; resolve the class name -> (ptr, len)
  (local.set $class_len)
  (local.set $class_ptr)
  (drop (call $__rt_wasi_write_all (i32.const 2) (local.get $class_ptr) (i32.wrap_i64 (local.get $class_len))))
  (drop (call $__rt_wasi_write_all (i32.const 2) (i32.const {scope_ptr}) (i32.const {scope_len})))
  (drop (call $__rt_wasi_write_all (i32.const 2) (local.get $name_ptr) (local.get $name_len)))
  (drop (call $__rt_wasi_write_all (i32.const 2) (i32.const {suffix_ptr}) (i32.const {suffix_len}))))"#
    ));
}

/// Emits the warning a read of `$scalar[$key]` produces, dispatched on the runtime tag.
///
/// PHP answers null for every non-container receiver, but it says so first, and WHAT it says
/// depends on the version profile the module was compiled for — the same split the null
/// receiver already carries. Before 8.3 the message names the TYPE for all three; from 8.3 it
/// names the type for int and float and the VALUE for a boolean, so `true` and `false` are two
/// distinct messages there and one shared message before.
///
/// The tag is the Mixed cell's own: 0 int, 2 float, 3 bool. Any other tag reaches this only if a
/// caller mis-dispatched, and writes nothing rather than a message for the wrong type.
fn emit_offset_on_scalar_warning_runtime(wm: &mut WatModule, offsets: &[(u32, u32)]) {
    debug_assert_eq!(offsets.len(), 7);
    let php83_or_later = crate::codegen_support::runtime::array_offset_on_null_warning()
        == crate::ir::ARRAY_OFFSET_ON_NULL_WARNING;
    let (true_ptr, true_len) = if php83_or_later { offsets[0] } else { offsets[4] };
    let (false_ptr, false_len) = if php83_or_later { offsets[1] } else { offsets[4] };
    let (int_ptr, int_len) = if php83_or_later { offsets[2] } else { offsets[5] };
    let (float_ptr, float_len) = if php83_or_later { offsets[3] } else { offsets[6] };
    wm.add_raw_func(&format!(
        r#"(func $__rt_warn_array_offset_on_scalar (param $tag i64) (param $lo i64)
  (if (global.get $__diag_suppress) (then (return)))
  (if (i64.eq (local.get $tag) (i64.const 0))                     ;; int receiver
    (then (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {int_ptr}) (i32.const {int_len})) (return)))
  (if (i64.eq (local.get $tag) (i64.const 2))                     ;; float receiver
    (then (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {float_ptr}) (i32.const {float_len})) (return)))
  (if (i64.eq (local.get $tag) (i64.const 3))                     ;; bool receiver: the VALUE names it from 8.3
    (then
      (if (i64.eqz (local.get $lo))
        (then (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {false_ptr}) (i32.const {false_len})))
        (else (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {true_ptr}) (i32.const {true_len})))))))"#
    ));
}

/// Emits the warnings `fopen` and `file_get_contents` produce for a path they cannot open.
///
/// Both answer `false` afterwards, so the value is right either way and this is purely the
/// diagnostic. The wording is the native backend's, which stops short of php-src's path and
/// errno detail; matching native keeps the two Elephc targets in agreement.
fn emit_open_failure_warning_runtime(wm: &mut WatModule, offsets: &[(u32, u32)]) {
    debug_assert_eq!(offsets.len(), 2);
    let (fopen_ptr, fopen_len) = offsets[0];
    let (get_ptr, get_len) = offsets[1];
    wm.add_raw_func(&format!(
        r#"(func $__rt_warn_fopen_failed
  (if (global.get $__diag_suppress) (then (return)))
  (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {fopen_ptr}) (i32.const {fopen_len})))"#
    ));
    wm.add_raw_func(&format!(
        r#"(func $__rt_warn_file_get_contents_failed
  (if (global.get $__diag_suppress) (then (return)))
  (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {get_ptr}) (i32.const {get_len})))"#
    ));
}

/// Emits the warning a string offset outside the string produces.
///
/// The index is written AS GIVEN — php-src reports a negative one negative, without resolving it
/// from the end first — and the read answers the empty string afterwards.
fn emit_uninit_string_offset_warning_runtime(wm: &mut WatModule, offsets: &[(u32, u32)]) {
    debug_assert_eq!(offsets.len(), 2);
    let (prefix_ptr, prefix_len) = offsets[0];
    let (newline_ptr, newline_len) = offsets[1];
    wm.add_raw_func(&format!(
        r#"(func $__rt_warn_uninit_string_offset (param $index i64)
  (local $ptr i32) (local $len i32)
  (if (global.get $__diag_suppress) (then (return)))
  (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {prefix_ptr}) (i32.const {prefix_len}))
  (call $__rt_itoa (local.get $index) (global.get $__float_scratch))
  (local.set $len)
  (local.set $ptr)
  (call $__rt_wasi_write_or_fail (i32.const 2) (local.get $ptr) (local.get $len))
  (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {newline_ptr}) (i32.const {newline_len})))"#
    ));
}

/// Emits the fatal path used when a `Mixed` receiver is not an object.
///
/// The helper composes the PHP-visible method name with the runtime Mixed tag,
/// writes the diagnostic to stderr, and terminates with PHP's fatal status 255.
fn emit_method_call_failure_runtime(wm: &mut WatModule, offsets: &[(u32, u32)]) {
    debug_assert_eq!(offsets.len(), 42);
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
    emit_too_few_arguments_failure_runtime(wm, &offsets[12..13], &offsets[14..19]);
    emit_undefined_function_failure_runtime(wm, &offsets[19..21]);
}

/// Emits the fatal a dynamic dispatch raises when php-src would not enter the selected class.
///
/// The runtime class-name table supplies the concrete class exactly as the undefined-method
/// fatal does — reusing its `::` separator — and the two counts are rendered through the shared
/// signed `__rt_itoa`. `exact` picks between php-src's two wordings; the caller knows which
/// applies because the signature is closed-world.
fn emit_too_few_arguments_failure_runtime(
    wm: &mut WatModule,
    separator: &[(u32, u32)],
    offsets: &[(u32, u32)],
) {
    debug_assert_eq!(separator.len(), 1);
    debug_assert_eq!(offsets.len(), 5);
    let (separator_ptr, separator_len) = separator[0];
    let (prefix_ptr, prefix_len) = offsets[0];
    let (passed_ptr, passed_len) = offsets[1];
    let (exactly_ptr, exactly_len) = offsets[2];
    let (at_least_ptr, at_least_len) = offsets[3];
    let (suffix_ptr, suffix_len) = offsets[4];
    wm.add_raw_func(&format!(
        r#"(func $__rt_fail_too_few_arguments (param $cid i64) (param $method_ptr i32) (param $method_len i32) (param $passed i64) (param $expected i64) (param $exact i32)
  (local $class_ptr i32) (local $class_len i64) (local $num_ptr i32) (local $num_len i32)
  (call $__rt_class_name_by_cid (local.get $cid))
  (local.set $class_len)
  (local.set $class_ptr)
  (drop (call $__rt_wasi_write_all (i32.const 2) (i32.const {prefix_ptr}) (i32.const {prefix_len})))
  (drop (call $__rt_wasi_write_all (i32.const 2) (local.get $class_ptr) (i32.wrap_i64 (local.get $class_len))))
  (drop (call $__rt_wasi_write_all (i32.const 2) (i32.const {separator_ptr}) (i32.const {separator_len})))
  (drop (call $__rt_wasi_write_all (i32.const 2) (local.get $method_ptr) (local.get $method_len)))
  (drop (call $__rt_wasi_write_all (i32.const 2) (i32.const {passed_ptr}) (i32.const {passed_len})))
  (call $__rt_itoa (local.get $passed) (global.get $__float_scratch))
  (local.set $num_len)
  (local.set $num_ptr)
  (drop (call $__rt_wasi_write_all (i32.const 2) (local.get $num_ptr) (local.get $num_len)))
  (if (local.get $exact)
    (then (drop (call $__rt_wasi_write_all (i32.const 2) (i32.const {exactly_ptr}) (i32.const {exactly_len}))))
    (else (drop (call $__rt_wasi_write_all (i32.const 2) (i32.const {at_least_ptr}) (i32.const {at_least_len})))))
  (call $__rt_itoa (local.get $expected) (global.get $__float_scratch))
  (local.set $num_len)
  (local.set $num_ptr)
  (drop (call $__rt_wasi_write_all (i32.const 2) (local.get $num_ptr) (local.get $num_len)))
  (drop (call $__rt_wasi_write_all (i32.const 2) (i32.const {suffix_ptr}) (i32.const {suffix_len})))
  (call $wasi_proc_exit (i32.const 255))
  unreachable ;; elephc-trap:post-noreturn:too-few-arguments-fatal-exit
)"#
    ));
}

/// Emits the runtime behind PHP's IMPLICIT coercion at a declared `int` return.
///
/// PHP does NOT convert there the way `(int)` does. Measured on php-src 8.5.6 and validated
/// against a 1200-value random sweep: an int or bool passes through; a float in i64's range
/// truncates, and a LOST FRACTION is a `Deprecated`, not a failure; a WHOLLY numeric string
/// converts by the same rules; and everything else — null, a leading-numeric or non-numeric
/// string, an out-of-range or non-finite float, a container, an object — is a `TypeError`
/// naming the function and the type that arrived.
///
/// The message needs no interned per-function text: the function name travels as `(ptr, len)`
/// from the call site, exactly as the non-object method-call fatal already does. An object
/// contributes its CLASS name rather than the word "object", which is why this helper reaches
/// for the class-name table like `__rt_fail_undefined_method` does.
///
/// The `TypeError` is raised as a deterministic fatal rather than a catchable throw: a raise
/// site composing its message at runtime cannot go through `emit_runtime_error_throw`, which
/// resolves a STATIC message from `default_strings`. The text and the 255 exit status are
/// PHP's; only a `catch (TypeError)` around the call would notice the difference.
fn emit_return_coercion_runtime(
    wm: &mut WatModule,
    offsets: &[(u32, u32)],
    type_offsets: &[(u32, u32)],
) {
    debug_assert_eq!(offsets.len(), 17);
    debug_assert_eq!(type_offsets.len(), 9);
    let (float_prefix_ptr, float_prefix_len) = offsets[0];
    let (str_prefix_ptr, str_prefix_len) = offsets[1];
    let (float_suffix_ptr, float_suffix_len) = offsets[2];
    let (str_suffix_ptr, str_suffix_len) = offsets[3];
    let (err_prefix_ptr, err_prefix_len) = offsets[4];
    let (err_middle_ptr, err_middle_len) = offsets[5];
    let (err_suffix_ptr, err_suffix_len) = offsets[6];
    let (closure_ptr, closure_len) = offsets[7];
    let (err_sep_ptr, err_sep_len) = offsets[8];
    let (nan_string_ptr, nan_string_len) = offsets[9];
    let (nan_bool_ptr, nan_bool_len) = offsets[10];
    let (true_ptr, true_len) = offsets[11];
    let (false_ptr, false_len) = offsets[12];
    let (count_prefix_ptr, count_prefix_len) = offsets[13];
    let (count_suffix_ptr, count_suffix_len) = offsets[14];
    let (uninit_offset_ptr, uninit_offset_len) = offsets[15];
    let (newline_ptr, newline_len) = offsets[16];
    // The shared `PHP_TYPE_*` words each end with the newline that terminates the method-call
    // fatal; here a word sits mid-sentence, so the newline is dropped from the length.
    let word = |index: usize| -> (u32, u32) {
        let (ptr, len) = type_offsets[index];
        debug_assert!(len > 1, "a type word is at least one byte plus its newline");
        (ptr, len - 1)
    };
    let (int_word_ptr, int_word_len) = word(0);
    let (string_word_ptr, string_word_len) = word(1);
    let (float_word_ptr, float_word_len) = word(2);
    let (bool_word_ptr, bool_word_len) = word(3);
    let (array_word_ptr, array_word_len) = word(4);
    let (null_word_ptr, null_word_len) = word(5);
    let (resource_word_ptr, resource_word_len) = word(6);
    let value_offset = super::mixed_numeric::CLASS_VALUE_OFFSET;

    // The word a runtime tag contributes to a diagnostic is the SAME wherever the value arrived
    // from — a declared return, a declared parameter — so it is resolved once here and every
    // fatal reads it. An object is why this cannot be a static table: it contributes its CLASS
    // name, which only the runtime class-name table knows.
    wm.add_raw_func(&format!(
        r#"(func $__rt_type_word_for_tag (param $tag i64) (param $lo i64) (result i32) (result i32)
  (local $word_ptr i32) (local $word_len i32) (local $cls_len i64)
  (local.set $word_ptr (i32.const {null_word_ptr}))               ;; tag 8 = null, the default
  (local.set $word_len (i32.const {null_word_len}))
  (if (i64.eq (local.get $tag) (i64.const 1))
    (then (local.set $word_ptr (i32.const {string_word_ptr})) (local.set $word_len (i32.const {string_word_len}))))
  (if (i64.eq (local.get $tag) (i64.const 2))
    (then (local.set $word_ptr (i32.const {float_word_ptr})) (local.set $word_len (i32.const {float_word_len}))))
  (if (i64.eqz (local.get $tag))
    (then (local.set $word_ptr (i32.const {int_word_ptr})) (local.set $word_len (i32.const {int_word_len}))))
  (if (i64.eq (local.get $tag) (i64.const 3))
    (then (local.set $word_ptr (i32.const {bool_word_ptr})) (local.set $word_len (i32.const {bool_word_len}))))
  (if (i32.or (i64.eq (local.get $tag) (i64.const 4)) (i64.eq (local.get $tag) (i64.const 5)))
    (then (local.set $word_ptr (i32.const {array_word_ptr})) (local.set $word_len (i32.const {array_word_len}))))
  (if (i64.eq (local.get $tag) (i64.const 9))
    (then (local.set $word_ptr (i32.const {resource_word_ptr})) (local.set $word_len (i32.const {resource_word_len}))))
  (if (i64.eq (local.get $tag) (i64.const 10))                    ;; PHP names a closure by its class
    (then (local.set $word_ptr (i32.const {closure_ptr})) (local.set $word_len (i32.const {closure_len}))))
  (if (i64.eq (local.get $tag) (i64.const 6))                     ;; an object contributes its CLASS name
    (then
      (call $__rt_class_name_by_cid (i64.load (i32.wrap_i64 (local.get $lo))))
      (local.set $cls_len)
      (local.set $word_ptr)
      (local.set $word_len (i32.wrap_i64 (local.get $cls_len)))))
  (local.get $word_ptr) (local.get $word_len))"#
    ));

    // One fatal serves all four targets. Measured: the word a tag contributes is the SAME
    // whatever the declared type — only the target word and the set of ACCEPTED tags differ.
    wm.add_raw_func(&format!(
        r#"(func $__rt_fail_return_type (param $fn_ptr i32) (param $fn_len i32) (param $target_ptr i32) (param $target_len i32) (param $tag i64) (param $lo i64)
  (local $word_ptr i32) (local $word_len i32)
  (call $__rt_type_word_for_tag (local.get $tag) (local.get $lo))
  (local.set $word_len)
  (local.set $word_ptr)
  (drop (call $__rt_wasi_write_all (i32.const 2) (i32.const {err_prefix_ptr}) (i32.const {err_prefix_len})))
  (drop (call $__rt_wasi_write_all (i32.const 2) (local.get $fn_ptr) (local.get $fn_len)))    ;; "f" or "C::m"
  (drop (call $__rt_wasi_write_all (i32.const 2) (i32.const {err_middle_ptr}) (i32.const {err_middle_len})))
  (drop (call $__rt_wasi_write_all (i32.const 2) (local.get $target_ptr) (local.get $target_len)))
  (drop (call $__rt_wasi_write_all (i32.const 2) (i32.const {err_sep_ptr}) (i32.const {err_sep_len})))
  (drop (call $__rt_wasi_write_all (i32.const 2) (local.get $word_ptr) (local.get $word_len)))
  (drop (call $__rt_wasi_write_all (i32.const 2) (i32.const {err_suffix_ptr}) (i32.const {err_suffix_len})))
  (call $wasi_proc_exit (i32.const 255))
  unreachable ;; elephc-trap:post-noreturn:return-type-fatal-exit
)"#
    ));
    wm.add_raw_func(&format!(
        r#"(func $__rt_deprecate_return_float_to_int (param $bits i64)
  (local $ptr i32) (local $len i32)
  (if (global.get $__diag_suppress) (then (return)))                ;; `@` silences deprecations too
  (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {float_prefix_ptr}) (i32.const {float_prefix_len}))  ;; "Deprecated: Implicit conversion from float "
  (call $__rt_ftoa (local.get $bits) (i32.add (global.get $__float_scratch) (i32.const 1024)) (i32.const 80) (i32.add (global.get $__float_scratch) (i32.const 2048)) (i32.const 792) (i32.add (global.get $__float_scratch) (i32.const 4096)))  ;; render the float the way PHP prints it
  (local.set $len)                                                ;; ftoa returns (ptr, len): pop the length first
  (local.set $ptr)                                                ;; then the pointer
  (call $__rt_wasi_write_or_fail (i32.const 2) (local.get $ptr) (local.get $len))
  (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {float_suffix_ptr}) (i32.const {float_suffix_len})))  ;; " to int loses precision\n""#
    ));
    wm.add_raw_func(&format!(
        r#"(func $__rt_deprecate_return_str_to_int (param $ptr i32) (param $len i32)
  (if (global.get $__diag_suppress) (then (return)))                ;; `@` silences deprecations too
  (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {str_prefix_ptr}) (i32.const {str_prefix_len}))  ;; ...from float-string "
  (call $__rt_wasi_write_or_fail (i32.const 2) (local.get $ptr) (local.get $len))  ;; the ORIGINAL bytes, padding included
  (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {str_suffix_ptr}) (i32.const {str_suffix_len})))  ;; " to int loses precision\n""#
    ));
    wm.add_raw_func(&format!(
        r#"(func $__rt_deprecate_nan_to_string
  (if (global.get $__diag_suppress) (then (return)))                ;; `@` silences deprecations too
  (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {nan_string_ptr}) (i32.const {nan_string_len})))"#
    ));
    wm.add_raw_func(&format!(
        r#"(func $__rt_deprecate_nan_to_bool
  (if (global.get $__diag_suppress) (then (return)))                ;; `@` silences deprecations too
  (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {nan_bool_ptr}) (i32.const {nan_bool_len})))"#
    ));

    wm.add_raw_func(&format!(
        r#"(func $__rt_mixed_to_int_core (param $cell i32) (result i32) (result i64)
  (local $tag i64) (local $lo i64) (local $hi i64) (local $f f64) (local $t f64) (local $cls i32)
  (call $__rt_mixed_unbox (local.get $cell))                      ;; unbox -> stack: tag, lo, hi
  (local.set $hi)
  (local.set $lo)
  (local.set $tag)
  (if (i64.eqz (local.get $tag))                                  ;; tag 0 = int: already PHP's answer
    (then (return (i32.const 1) (local.get $lo))))
  (if (i64.eq (local.get $tag) (i64.const 3))                     ;; tag 3 = bool: stored normalized 0/1
    (then (return (i32.const 1) (local.get $lo))))
  (if (i64.eq (local.get $tag) (i64.const 2))                     ;; tag 2 = float
    (then
      (local.set $f (f64.reinterpret_i64 (local.get $lo)))
      (if (i32.and
            (f64.eq (local.get $f) (local.get $f))                ;; NaN fits nothing
            (i32.and
              (f64.ge (local.get $f) (f64.const -9223372036854775808))   ;; -2^63 is IN range
              (f64.lt (local.get $f) (f64.const 9223372036854775808))))  ;; 2^63 is NOT
        (then
          (local.set $t (f64.trunc (local.get $f)))
          (if (f64.ne (local.get $t) (local.get $f))               ;; a lost fraction only DEPRECATES
            (then (call $__rt_deprecate_return_float_to_int (local.get $lo))))
          (return (i32.const 1) (i64.trunc_f64_s (local.get $t)))))))
  (if (i64.eq (local.get $tag) (i64.const 1))                     ;; tag 1 = string
    (then
      (local.set $cls (call $__rt_str_numeric_class (i32.wrap_i64 (local.get $lo)) (i32.wrap_i64 (local.get $hi))))
      (if (i32.eq (local.get $cls) (i32.const 1))                 ;; wholly integral, and it fits i64
        (then (return (i32.const 1) (i64.load (i32.add (global.get $__float_scratch) (i32.const {value_offset}))))))
      (if (i32.eq (local.get $cls) (i32.const 2))                 ;; wholly float-shaped (or an i64-overflowing integer text)
        (then
          (local.set $f (f64.load (i32.add (global.get $__float_scratch) (i32.const {value_offset}))))
          (if (i32.and
                (f64.eq (local.get $f) (local.get $f))
                (i32.and
                  (f64.ge (local.get $f) (f64.const -9223372036854775808))
                  (f64.lt (local.get $f) (f64.const 9223372036854775808))))
            (then
              (local.set $t (f64.trunc (local.get $f)))
              (if (f64.ne (local.get $t) (local.get $f))
                (then (call $__rt_deprecate_return_str_to_int (i32.wrap_i64 (local.get $lo)) (i32.wrap_i64 (local.get $hi)))))
              (return (i32.const 1) (i64.trunc_f64_s (local.get $t)))))))))
  (i32.const 0) (i64.const 0))                                    ;; no conversion exists
"#
    ));
    wm.add_raw_func(&format!(
        r#"(func $__rt_mixed_return_int (param $cell i32) (param $fn_ptr i32) (param $fn_len i32) (result i64)
  (local $ok i32) (local $value i64) (local $tag i64) (local $lo i64) (local $hi i64)
  (call $__rt_mixed_to_int_core (local.get $cell))
  (local.set $value)
  (local.set $ok)
  (if (local.get $ok)
    (then (return (local.get $value))))
  (call $__rt_mixed_unbox (local.get $cell))
  (local.set $hi)
  (local.set $lo)
  (local.set $tag)
  (call $__rt_fail_return_type (local.get $fn_ptr) (local.get $fn_len) (i32.const {int_word_ptr}) (i32.const {int_word_len}) (local.get $tag) (local.get $lo))
  unreachable)                                                    ;; elephc-trap:post-noreturn:return-coerce-int
"#
    ));
    // The same conversion at a declared PARAMETER, which differs from the return in exactly two
    // places. Measured on php-src 8.5.6 for `substr("abcdefgh", $mixed)`: `null` does NOT raise
    // there — it converts to 0 after a `Deprecated` naming the parameter — and the failure names
    // `Argument #N ($p)` rather than the return value. Everything numeric in between, the two
    // precision deprecations included, is byte-for-byte the same, which is why it comes from the
    // shared core rather than a second copy.
    wm.add_raw_func(&format!(
        r#"(func $__rt_mixed_arg_int (param $cell i32) (param $fn_ptr i32) (param $fn_len i32) (param $param_ptr i32) (param $param_len i32) (param $argno i64) (result i64)
  (local $ok i32) (local $value i64) (local $tag i64) (local $lo i64) (local $hi i64)
  (call $__rt_mixed_unbox (local.get $cell))
  (local.set $hi)
  (local.set $lo)
  (local.set $tag)
  (if (i64.eq (local.get $tag) (i64.const 8))                     ;; null still converts, after a deprecation
    (then
      (call $__rt_deprecate_argument_null (local.get $fn_ptr) (local.get $fn_len) (local.get $param_ptr) (local.get $param_len) (local.get $argno) (i32.const {int_word_ptr}) (i32.const {int_word_len}))
      (return (i64.const 0))))
  (call $__rt_mixed_to_int_core (local.get $cell))
  (local.set $value)
  (local.set $ok)
  (if (local.get $ok)
    (then (return (local.get $value))))
  (call $__rt_fail_argument_type (local.get $fn_ptr) (local.get $fn_len) (local.get $param_ptr) (local.get $param_len) (local.get $argno) (i32.const {int_word_ptr}) (i32.const {int_word_len}) (local.get $tag) (local.get $lo))
  unreachable)                                                    ;; elephc-trap:post-noreturn:argument-coerce-int
"#
    ));
    // A boxed value entering a `?int` destination: `(ok, payload, tag)`, where `tag` is 0 for an
    // int and 8 for a null. `null` and an int pass untouched; under `strict_types` nothing else
    // does. Otherwise every other scalar goes through the same core a declared `int` return uses,
    // so the bool, float and numeric-string answers (and the precision deprecations) are the
    // ones already measured against php-src. `ok` is 0 for a cell with no int value at all; the
    // caller raises, because only it knows how to build the `TypeError`.
    wm.add_raw_func(
        r#"(func $__rt_mixed_to_tagged_int (param $cell i32) (param $strict i32) (result i32) (result i64) (result i32)
  (local $ok i32) (local $value i64) (local $tag i64) (local $lo i64) (local $hi i64)
  (call $__rt_mixed_unbox (local.get $cell))
  (local.set $hi)
  (local.set $lo)
  (local.set $tag)
  (if (i64.eq (local.get $tag) (i64.const 8))                     ;; null stays null
    (then (return (i32.const 1) (i64.const 0) (i32.const 8))))
  (if (i64.eqz (local.get $tag))                                  ;; an int stays an int
    (then (return (i32.const 1) (local.get $lo) (i32.const 0))))
  (if (local.get $strict)                                         ;; strict_types performs no coercion
    (then (return (i32.const 0) (i64.const 0) (i32.const 0))))
  (call $__rt_mixed_to_int_core (local.get $cell))
  (local.set $value)
  (local.set $ok)
  (if (local.get $ok)
    (then (return (i32.const 1) (local.get $value) (i32.const 0))))
  (i32.const 0) (i64.const 0) (i32.const 0))                      ;; array, object, non-numeric string, ...
"#,
    );
    // The same three coercions at a typed PROPERTY or static-property destination of plain `int`,
    // `float` or `bool`: `(ok, value)`, where `ok` is 0 for a cell with no such form (the caller
    // raises). They differ from the `?int` helper only in that `null` is not accepted. `strict`
    // is `declare(strict_types=1)`: an int passes to `int`, an int or a float to `float`, a bool to
    // `bool`, and nothing else does.
    wm.add_raw_func(
        r#"(func $__rt_mixed_to_typed_int (param $cell i32) (param $strict i32) (result i32) (result i64)
  (local $ok i32) (local $value i64) (local $tag i32)
  (call $__rt_mixed_to_tagged_int (local.get $cell) (local.get $strict))
  (local.set $tag)
  (local.set $value)
  (local.set $ok)
  (if (i32.eq (local.get $tag) (i32.const 8))                     ;; null is not an int
    (then (return (i32.const 0) (i64.const 0))))
  (local.get $ok) (local.get $value))
"#,
    );
    wm.add_raw_func(
        r#"(func $__rt_mixed_to_typed_float (param $cell i32) (param $strict i32) (result i32) (result i64)
  (local $tag i64) (local $lo i64) (local $hi i64) (local $cls i32)
  (call $__rt_mixed_unbox (local.get $cell))
  (local.set $hi)
  (local.set $lo)
  (local.set $tag)
  (if (i64.eq (local.get $tag) (i64.const 2))                     ;; a float stays a float
    (then (return (i32.const 1) (local.get $lo))))
  (if (i64.eqz (local.get $tag))                                  ;; an int widens, even under strict_types
    (then (return (i32.const 1) (i64.reinterpret_f64 (f64.convert_i64_s (local.get $lo))))))
  (if (local.get $strict)
    (then (return (i32.const 0) (i64.const 0))))
  (if (i64.eq (local.get $tag) (i64.const 3))                     ;; a bool is 0.0 or 1.0
    (then (return (i32.const 1) (i64.reinterpret_f64 (f64.convert_i64_s (local.get $lo))))))
  (if (i64.eq (local.get $tag) (i64.const 1))                     ;; a WHOLLY numeric string only
    (then
      (local.set $cls (call $__rt_str_numeric_class (i32.wrap_i64 (local.get $lo)) (i32.wrap_i64 (local.get $hi))))
      (if (i32.or (i32.eq (local.get $cls) (i32.const 1)) (i32.eq (local.get $cls) (i32.const 2)))
        (then (return (i32.const 1) (call $__rt_mixed_cast_float (local.get $cell)))))))
  (i32.const 0) (i64.const 0))
"#,
    );
    wm.add_raw_func(
        r#"(func $__rt_mixed_to_typed_bool (param $cell i32) (param $strict i32) (result i32) (result i64)
  (local $tag i64) (local $lo i64) (local $hi i64) (local $f f64)
  (call $__rt_mixed_unbox (local.get $cell))
  (local.set $hi)
  (local.set $lo)
  (local.set $tag)
  (if (i64.eq (local.get $tag) (i64.const 3))                     ;; a bool stays a bool
    (then (return (i32.const 1) (local.get $lo))))
  (if (local.get $strict)
    (then (return (i32.const 0) (i64.const 0))))
  (if (i64.eq (local.get $tag) (i64.const 2))
    (then
      (local.set $f (f64.reinterpret_i64 (local.get $lo)))
      (if (f64.ne (local.get $f) (local.get $f))                  ;; NaN converts, after a notice
        (then (call $__rt_deprecate_nan_to_bool)))))
  (if (i32.or (i64.eqz (local.get $tag))
        (i32.or (i64.eq (local.get $tag) (i64.const 1)) (i64.eq (local.get $tag) (i64.const 2))))
    (then (return (i32.const 1) (call $__rt_mixed_cast_bool (local.get $cell)))))
  (i32.const 0) (i64.const 0))
"#,
    );
    // A float target takes every numeric tag EXACTLY as `(float)` does — NaN and the infinities
    // included, since none of them loses anything on the way to a float, so there is no notice
    // here at all. Only a string needs gating: a leading-numeric one converts under `(float)`
    // but is a `TypeError` at a return.
    wm.add_raw_func(&format!(
        r#"(func $__rt_mixed_return_float (param $cell i32) (param $fn_ptr i32) (param $fn_len i32) (result i64)
  (local $tag i64) (local $lo i64) (local $hi i64) (local $cls i32)
  (call $__rt_mixed_unbox (local.get $cell))
  (local.set $hi)
  (local.set $lo)
  (local.set $tag)
  (if (i32.or (i64.eqz (local.get $tag))
        (i32.or (i64.eq (local.get $tag) (i64.const 2)) (i64.eq (local.get $tag) (i64.const 3))))
    (then (return (call $__rt_mixed_cast_float (local.get $cell)))))  ;; int / float / bool
  (if (i64.eq (local.get $tag) (i64.const 1))
    (then
      (local.set $cls (call $__rt_str_numeric_class (i32.wrap_i64 (local.get $lo)) (i32.wrap_i64 (local.get $hi))))
      (if (i32.or (i32.eq (local.get $cls) (i32.const 1)) (i32.eq (local.get $cls) (i32.const 2)))  ;; WHOLLY numeric only
        (then (return (call $__rt_mixed_cast_float (local.get $cell)))))))
  (call $__rt_fail_return_type (local.get $fn_ptr) (local.get $fn_len) (i32.const {float_word_ptr}) (i32.const {float_word_len}) (local.get $tag) (local.get $lo))
  unreachable)                                                    ;; elephc-trap:post-noreturn:return-coerce-tofloat
"#
    ));
    // `string` and `bool` accept every scalar tag with the explicit cast's own value — the two
    // operations only part ways on the tags PHP refuses outright, plus the NaN notice.
    wm.add_raw_func(&format!(
        r#"(func $__rt_mixed_return_bool (param $cell i32) (param $fn_ptr i32) (param $fn_len i32) (result i64)
  (local $tag i64) (local $lo i64) (local $hi i64) (local $f f64)
  (call $__rt_mixed_unbox (local.get $cell))
  (local.set $hi)
  (local.set $lo)
  (local.set $tag)
  (if (i64.eq (local.get $tag) (i64.const 2))
    (then
      (local.set $f (f64.reinterpret_i64 (local.get $lo)))
      (if (f64.ne (local.get $f) (local.get $f))                  ;; NaN still converts, after a notice
        (then (call $__rt_deprecate_nan_to_bool)))))
  (if (i32.or (i64.eqz (local.get $tag))
        (i32.or (i64.eq (local.get $tag) (i64.const 1))
          (i32.or (i64.eq (local.get $tag) (i64.const 2)) (i64.eq (local.get $tag) (i64.const 3)))))
    (then (return (call $__rt_mixed_cast_bool (local.get $cell)))))
  (call $__rt_fail_return_type (local.get $fn_ptr) (local.get $fn_len) (i32.const {bool_word_ptr}) (i32.const {bool_word_len}) (local.get $tag) (local.get $lo))
  unreachable)                                                    ;; elephc-trap:post-noreturn:return-coerce-tobool
"#
    ));
    wm.add_raw_func(&format!(
        r#"(func $__rt_mixed_return_string (param $cell i32) (param $fn_ptr i32) (param $fn_len i32) (result i32) (result i32)
  (local $tag i64) (local $lo i64) (local $hi i64) (local $f f64)
  (local $sptr i32) (local $slen i32) (local $sflag i32) (local $pptr i32) (local $plen i64)
  (call $__rt_mixed_unbox (local.get $cell))
  (local.set $hi)
  (local.set $lo)
  (local.set $tag)
  (if (i64.eq (local.get $tag) (i64.const 2))
    (then
      (local.set $f (f64.reinterpret_i64 (local.get $lo)))
      (if (f64.ne (local.get $f) (local.get $f))
        (then (call $__rt_deprecate_nan_to_string)))))
  (if (i32.or (i64.eqz (local.get $tag))
        (i32.or (i64.eq (local.get $tag) (i64.const 1))
          (i32.or (i64.eq (local.get $tag) (i64.const 2)) (i64.eq (local.get $tag) (i64.const 3)))))
    (then (return (call $__rt_mixed_cast_string (local.get $cell)))))
  (if (i64.eq (local.get $tag) (i64.const 6))                     ;; object: __toString CONVERTS...
    (then
      (call $__rt_object_to_string (i32.wrap_i64 (local.get $lo)))
      (local.set $sflag)
      (local.set $slen)
      (local.set $sptr)
      (if (local.get $sflag)
        (then
          (call $__rt_str_persist (local.get $sptr) (i64.extend_i32_u (local.get $slen)))
          (local.set $plen)
          (local.set $pptr)
          (call $__rt_decref_any (local.get $sptr))
          (return (local.get $pptr) (i32.wrap_i64 (local.get $plen)))))))
  ;; ...and a class WITHOUT one is the RETURN TypeError, naming the class — measured,
  ;; NOT the cast's own could-not-be-converted Error.
  (call $__rt_fail_return_type (local.get $fn_ptr) (local.get $fn_len) (i32.const {string_word_ptr}) (i32.const {string_word_len}) (local.get $tag) (local.get $lo))
  unreachable)                                                    ;; elephc-trap:post-noreturn:return-coerce-tostring
"#
    ));
    // `count()` on a boxed value. Its own word table because a boolean is named by VALUE here,
    // and its own message because an internal function's `TypeError` carries no location.
    wm.add_raw_func(&format!(
        r#"(func $__rt_mixed_count (param $cell i32) (result i64)
  (local $tag i64) (local $lo i64) (local $hi i64) (local $word_ptr i32) (local $word_len i32) (local $cls_len i64)
  (call $__rt_mixed_unbox (local.get $cell))
  (local.set $hi)
  (local.set $lo)
  (local.set $tag)
  (if (i32.or (i64.eq (local.get $tag) (i64.const 4)) (i64.eq (local.get $tag) (i64.const 5)))
    (then (return (i64.load (i32.wrap_i64 (local.get $lo))))))    ;; element count @ +0
  (if (i64.eq (local.get $tag) (i64.const 6))
    (then
      (call $__rt_fail (i32.const 9))                             ;; a Countable object is not lowered on this target
      unreachable))                                               ;; elephc-trap:post-noreturn:count-object
  (local.set $word_ptr (i32.const {null_word_ptr}))               ;; tag 8 = null, the default
  (local.set $word_len (i32.const {null_word_len}))
  (if (i64.eqz (local.get $tag))
    (then (local.set $word_ptr (i32.const {int_word_ptr})) (local.set $word_len (i32.const {int_word_len}))))
  (if (i64.eq (local.get $tag) (i64.const 1))
    (then (local.set $word_ptr (i32.const {string_word_ptr})) (local.set $word_len (i32.const {string_word_len}))))
  (if (i64.eq (local.get $tag) (i64.const 2))
    (then (local.set $word_ptr (i32.const {float_word_ptr})) (local.set $word_len (i32.const {float_word_len}))))
  (if (i64.eq (local.get $tag) (i64.const 3))                     ;; a boolean is named by its VALUE
    (then
      (local.set $word_ptr (i32.const {false_ptr}))
      (local.set $word_len (i32.const {false_len}))
      (if (i64.ne (local.get $lo) (i64.const 0))
        (then (local.set $word_ptr (i32.const {true_ptr})) (local.set $word_len (i32.const {true_len}))))))
  (if (i64.eq (local.get $tag) (i64.const 9))
    (then (local.set $word_ptr (i32.const {resource_word_ptr})) (local.set $word_len (i32.const {resource_word_len}))))
  (if (i64.eq (local.get $tag) (i64.const 10))
    (then (local.set $word_ptr (i32.const {closure_ptr})) (local.set $word_len (i32.const {closure_len}))))
  (drop (call $__rt_wasi_write_all (i32.const 2) (i32.const {count_prefix_ptr}) (i32.const {count_prefix_len})))
  (drop (call $__rt_wasi_write_all (i32.const 2) (local.get $word_ptr) (local.get $word_len)))
  (drop (call $__rt_wasi_write_all (i32.const 2) (i32.const {count_suffix_ptr}) (i32.const {count_suffix_len})))
  (call $wasi_proc_exit (i32.const 255))
  unreachable)                                                    ;; elephc-trap:post-noreturn:count-type-fatal-exit
"#
    ));
    // `$s[$i]`: PHP counts a negative index from the END, and answers the EMPTY string with a
    // warning for anything still outside. The warning names the index AS WRITTEN.
    wm.add_raw_func(&format!(
        r#"(func $__rt_str_char_at (param $ptr i32) (param $len i64) (param $idx i64) (result i32) (result i64)
  (local $i i64) (local $tp i32) (local $tl i32)
  (local.set $i (local.get $idx))
  (if (i64.lt_s (local.get $i) (i64.const 0))                     ;; a negative index counts from the end
    (then (local.set $i (i64.add (local.get $len) (local.get $i)))))
  (if (i32.or (i64.lt_s (local.get $i) (i64.const 0)) (i64.ge_s (local.get $i) (local.get $len)))
    (then
      (if (i32.eqz (global.get $__diag_suppress))                  ;; `@$s[10]` silences the warning
        (then
          (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {uninit_offset_ptr}) (i32.const {uninit_offset_len}))
          (call $__rt_itoa (local.get $idx) (i32.add (global.get $__float_scratch) (i32.const 9344)))
          (local.set $tl)
          (local.set $tp)
          (call $__rt_wasi_write_or_fail (i32.const 2) (local.get $tp) (local.get $tl))
          (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {newline_ptr}) (i32.const {newline_len}))))
      (return (call $__rt_str_persist (local.get $ptr) (i64.const 0)))))  ;; PHP answers ""
  (call $__rt_str_persist                                         ;; one byte, owned by the caller
    (i32.add (local.get $ptr) (i32.wrap_i64 (local.get $i)))
    (i64.const 1)))
"#
    ));
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

/// Emits the fatal a call raises when no include has defined the function's body yet.
///
/// The name is a runtime argument rather than baked into the message because ONE dispatcher
/// serves a whole variant group, and which body is live is decided by whichever include ran.
fn emit_undefined_function_failure_runtime(wm: &mut WatModule, offsets: &[(u32, u32)]) {
    debug_assert_eq!(offsets.len(), 2);
    let (prefix_ptr, prefix_len) = offsets[0];
    let (suffix_ptr, suffix_len) = offsets[1];
    wm.add_raw_func(&format!(
        r#"(func $__rt_fail_undefined_function (param $name_ptr i32) (param $name_len i32)
  (drop (call $__rt_wasi_write_all (i32.const 2) (i32.const {prefix_ptr}) (i32.const {prefix_len})))
  (drop (call $__rt_wasi_write_all (i32.const 2) (local.get $name_ptr) (local.get $name_len)))
  (drop (call $__rt_wasi_write_all (i32.const 2) (i32.const {suffix_ptr}) (i32.const {suffix_len})))
  (call $wasi_proc_exit (i32.const 255))
  unreachable ;; elephc-trap:post-noreturn:undefined-function-fatal-exit
)"#
    ));
}

/// Emits the fatal an uncatchable `throw_error_value` raises, naming its runtime message.
///
/// KNOWN DIVERGENCE, shared with `ERR_UNCAUGHT_EXCEPTION`: reference PHP appends the file, line
/// and stack trace. The message text and the EXIT STATUS — 255 — are PHP's.
fn emit_error_value_failure_runtime(wm: &mut WatModule, offsets: &[(u32, u32)]) {
    debug_assert_eq!(offsets.len(), 2);
    let (prefix_ptr, prefix_len) = offsets[0];
    let (suffix_ptr, suffix_len) = offsets[1];
    wm.add_raw_func(&format!(
        r#"(func $__rt_fail_error_value (param $message_ptr i32) (param $message_len i32)
  (drop (call $__rt_wasi_write_all (i32.const 2) (i32.const {prefix_ptr}) (i32.const {prefix_len})))
  (drop (call $__rt_wasi_write_all (i32.const 2) (local.get $message_ptr) (local.get $message_len)))
  (drop (call $__rt_wasi_write_all (i32.const 2) (i32.const {suffix_ptr}) (i32.const {suffix_len})))
  (call $wasi_proc_exit (i32.const 255))
  unreachable ;; elephc-trap:post-noreturn:error-value-fatal-exit
)"#
    ));
}

/// Emits the `Error` php raises for a property written through a non-object.
///
/// KNOWN DIVERGENCE, shared with every other raise on this target: php's `Error` is CATCHABLE,
/// and this reports the uncaught form and exits. The capability audit refuses the write inside a
/// function that could catch it, so the two only ever disagree where php would also have exited.
fn emit_assign_on_null_runtime(wm: &mut WatModule, offsets: &[(u32, u32)]) {
    debug_assert_eq!(offsets.len(), 3);
    let (prefix_ptr, prefix_len) = offsets[0];
    let (suffix_ptr, suffix_len) = offsets[1];
    let (modify_ptr, modify_len) = offsets[2];
    wm.add_raw_func(&format!(
        r#"(func $__rt_fail_assign_on_null (param $name_ptr i32) (param $name_len i32)
  (drop (call $__rt_wasi_write_all (i32.const 2) (i32.const {prefix_ptr}) (i32.const {prefix_len})))
  (drop (call $__rt_wasi_write_all (i32.const 2) (local.get $name_ptr) (local.get $name_len)))
  (drop (call $__rt_wasi_write_all (i32.const 2) (i32.const {suffix_ptr}) (i32.const {suffix_len})))
  (call $wasi_proc_exit (i32.const 255))
  unreachable ;; elephc-trap:post-noreturn:assign-on-null-fatal-exit
)"#
    ));
    wm.add_raw_func(&format!(
        r#"(func $__rt_fail_modify_on_null (param $name_ptr i32) (param $name_len i32)
  (drop (call $__rt_wasi_write_all (i32.const 2) (i32.const {modify_ptr}) (i32.const {modify_len})))
  (drop (call $__rt_wasi_write_all (i32.const 2) (local.get $name_ptr) (local.get $name_len)))
  (drop (call $__rt_wasi_write_all (i32.const 2) (i32.const {suffix_ptr}) (i32.const {suffix_len})))
  (call $wasi_proc_exit (i32.const 255))
  unreachable ;; elephc-trap:post-noreturn:modify-on-null-fatal-exit
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
  (if (global.get $__diag_suppress) (then (return)))
  (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {prefix_ptr}) (i32.const {prefix_len}))
  (call $__rt_itoa (local.get $key) (global.get $__float_scratch))
  (local.set $key_len)
  (local.set $key_ptr)
  (call $__rt_wasi_write_or_fail (i32.const 2) (local.get $key_ptr) (local.get $key_len))
  (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {suffix_ptr}) (i32.const {suffix_len})))"#
    ));
    wm.add_raw_func(&format!(
        r#"(func $__rt_warn_undefined_array_key_str (param $key_ptr i32) (param $key_len i32)
  (if (global.get $__diag_suppress) (then (return)))
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
  (if (global.get $__diag_suppress) (then (return)))
  (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {array_to_string_ptr}) (i32.const {array_to_string_len})))"#
    ));
    wm.add_raw_func(&format!(
        r#"(func $__rt_warn_array_offset_on_null
  (if (global.get $__diag_suppress) (then (return)))
  (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {offset_on_null_ptr}) (i32.const {offset_on_null_len})))"#
    ));
    wm.add_raw_func(&format!(
        r#"(func $__rt_warn_float_not_representable (param $bits i64)
  (local $ptr i32) (local $len i32)
  (if (global.get $__diag_suppress) (then (return)))
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
  (if (global.get $__diag_suppress) (then (return)))
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
  (if (global.get $__diag_suppress) (then (return)))
  (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {object_prefix_ptr}) (i32.const {object_prefix_len}))  ;; "Warning: Object of class "
  (call $__rt_class_name_by_cid (local.get $cid))                 ;; resolve the class name -> (ptr, len)
  (local.set $len)                                                ;; pop the name length
  (local.set $ptr)                                                ;; pop the name pointer
  (call $__rt_wasi_write_or_fail (i32.const 2) (local.get $ptr) (i32.wrap_i64 (local.get $len)))  ;; the class name itself
  (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {object_float_ptr}) (i32.const {object_float_len})))  ;; " could not be converted to float\n""#
    ));
    wm.add_raw_func(&format!(
        r#"(func $__rt_warn_non_numeric_value
  (if (global.get $__diag_suppress) (then (return)))
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
  (if (global.get $__diag_suppress) (then (return)))                ;; `@` silences deprecations too
  (call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {chr_range_ptr}) (i32.const {chr_range_len})))"#
        ));
        wm.add_raw_func(&format!(
            r#"(func $__rt_deprecated_ord_length
  (if (global.get $__diag_suppress) (then (return)))                ;; `@` silences deprecations too
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
      (call $__rt_stdout_write (i32.const 16) (i32.const 1))))) ;; write "1""#;

/// `__rt_echo_str`: writes a string (a linear-memory pointer + byte length) to
/// stdout via `fd_write`. The length is an i64 (PHP int) wrapped to the i32 the
/// iovec field requires.
const RT_ECHO_STR: &str = r#"(func $__rt_echo_str (param $ptr i32) (param $len i64)
  (if (i64.gt_u (local.get $len) (i64.const 4294967295))
    (then
      (call $__rt_fail (i32.const 5))
      unreachable))                                          ;; elephc-trap:post-noreturn:echo-string-length-overflow wasm32 cannot address a larger byte range
  (call $__rt_stdout_write (local.get $ptr) (i32.wrap_i64 (local.get $len)))) ;; write to stdout"#;

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
  (call $__rt_stdout_write (local.get $ptr) (local.get $len))) ;; write to stdout"#;

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
  (call $__rt_stdout_write (local.get $ptr) (local.get $len))) ;; write to stdout"#;

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
        ERR_STREAM_CHUNK_SIZE, ERR_BC_MALFORMED, ERR_REWIND_ADVANCED, ERR_FCLOSE_CLOSED,
        ERR_STREAM_CHUNK_TOO_LARGE, ERR_BCSCALE_RANGE, ERR_BC_SCALE_RANGE,
        ERR_GENERATOR_NO_RETURN, ERR_GENERATOR_CLOSED, ERR_MIXED_HEAP_TYPE, ERR_ARRAY_PAD_SIZE,
        ERR_UNION_NULL_LEFT, ERR_UNION_NULL_RIGHT, ERR_MERGE_NULL_FIRST, ERR_MERGE_NULL_SECOND,
        ERR_REPLACE_NULL_FIRST, ERR_REPLACE_NULL_SECOND,
        RT_ARGV, RT_ECHO_BOOL, RT_ECHO_F64, RT_ECHO_I64, RT_ECHO_STR, RT_WASI_WRITE_OR_FAIL,
    };
    use super::super::heap::emit_heap_runtime;
    use super::super::objects::CATCHABLE_RUNTIME_ERRORS;
    use super::super::wat::{DataSegment, WatModule};
    use std::sync::atomic::{AtomicU32, Ordering};

    static TMP_SEQ: AtomicU32 = AtomicU32::new(0);

    /// Every fixed table carved from the float scratch region must be disjoint from the others.
    ///
    /// The output-buffer level table once overlapped the scanf scratch and the clock
    /// out-parameter, so `ob_start` followed by a clock read or a scan corrupted a level record.
    #[test]
    fn scratch_ranges_are_disjoint() {
        use super::super::{clock, dirs, files, output_buffer, scanf, streamtune, urls};
        let mut ranges: Vec<(&str, u32, u32)> = vec![
            ("strtod+ftoa", 0, 0x3000),
            // The file runtime's syscall scratch, one entry per use. They used to be a single lump
            // `IO_SCRATCH..FD_STREAM_META`, which could not see the `filestat` result overlapping
            // the first fds' eof flags.
            ("io iovec", files::IO_SCRATCH, files::IO_SCRATCH + 8),
            ("io nread/written", files::IO_SCRATCH + 8, files::IO_SCRATCH + 12),
            ("io opened/seek-probe", files::IO_SCRATCH + 0x10, files::IO_SCRATCH + 0x18),
            ("php://output position", files::OUTPUT_POS, files::OUTPUT_POS + 8),
            ("io prestat", files::IO_SCRATCH + 0x20, files::IO_SCRATCH + 0x28),
            ("fd extension pointer", files::FD_EXT_PTR, files::FD_EXT_PTR + 4),
            ("io fopen mode", files::IO_SCRATCH + 0x30, files::IO_SCRATCH + 0x40),
            ("io fd_seek result", files::SEEK_OUT, files::SEEK_OUT + 8),
            ("fd eof flags", files::FD_EOF_FLAGS, files::FD_EOF_FLAGS + files::FD_TABLE_INLINE),
            ("fd temp index", files::FD_TMP_INDEX, files::FD_TMP_INDEX + files::FD_TABLE_INLINE),
            ("temp name", files::TMP_NAME, files::TMP_NAME + files::TMP_NAME_LEN + 4),
            ("zero16", files::ZERO16, files::ZERO16 + 16),
            ("filestat", files::FILESTAT, files::FILESTAT + files::FILESTAT_BYTES),
            ("fd stream meta", files::FD_STREAM_META, files::FD_STREAM_META + 16 * 256),
            ("parse_url", urls::PARTS, 0x5100),
            ("scanf", scanf::SCAN_SCRATCH, scanf::SCAN_SCRATCH + scanf::SCAN_SCRATCH_BYTES),
            ("clock", clock::CLOCK_OUT, clock::CLOCK_OUT + 8),
            ("dirs", dirs::DIR_STATE, dirs::DIR_DOTS + 4),
            (
                "chunk sizes",
                streamtune::FD_CHUNK_SIZE,
                streamtune::FD_CHUNK_SIZE + streamtune::FD_CHUNK_SIZE_BYTES,
            ),
            (
                "ob levels",
                output_buffer::OB_LEVELS,
                output_buffer::OB_LEVELS + output_buffer::OB_LEVELS_BYTES,
            ),
        ];
        ranges.sort_by_key(|r| r.1);
        for pair in ranges.windows(2) {
            assert!(
                pair[0].2 <= pair[1].1,
                "scratch range {} ({:#x}..{:#x}) overlaps {} ({:#x}..{:#x})",
                pair[0].0, pair[0].1, pair[0].2, pair[1].0, pair[1].1, pair[1].2
            );
        }
        let top = ranges.last().unwrap();
        assert!(top.2 <= super::FLOAT_SCRATCH_SIZE, "{} overruns the scratch region", top.0);
    }

    /// Verifies a raised runtime error and its uncaught fatal name the same class and message.
    ///
    /// These two halves live apart on purpose: the raise site builds the object a `catch` will
    /// match, while `__rt_fail` owns the text `main` prints when no clause matched. Nothing in
    /// the emitter forces them to agree, so changing one and not the other would let a program
    /// catch a `DivisionByZeroError` yet report an `ArithmeticError` when it does not.
    #[test]
    fn raised_runtime_errors_agree_with_their_uncaught_diagnostics() {
        let fatals = [
            (9, ERR_MIXED_HEAP_TYPE),
            (1, ERR_DIV_ZERO),
            (2, ERR_MOD_ZERO),
            (3, ERR_NEG_SHIFT),
            (4, ERR_INTDIV_OVERFLOW),
            (11, ERR_STR_REPEAT_NEGATIVE),
            (12, ERR_STR_PAD_EMPTY),
            (13, ERR_EXPLODE_EMPTY_SEP),
            (14, ERR_STR_SPLIT_LENGTH),
            (17, ERR_STREAM_CHUNK_SIZE),
            (18, ERR_BC_MALFORMED),
            (20, ERR_REWIND_ADVANCED),
            (21, ERR_FCLOSE_CLOSED),
            (22, ERR_STREAM_CHUNK_TOO_LARGE),
            (23, ERR_BCSCALE_RANGE),
            (24, ERR_BC_SCALE_RANGE),
            (25, ERR_GENERATOR_NO_RETURN),
            (27, ERR_GENERATOR_CLOSED),
            (32, ERR_ARRAY_PAD_SIZE),
            (33, ERR_UNION_NULL_LEFT),
            (34, ERR_UNION_NULL_RIGHT),
            (35, ERR_MERGE_NULL_FIRST),
            (36, ERR_MERGE_NULL_SECOND),
            (37, ERR_REPLACE_NULL_FIRST),
            (38, ERR_REPLACE_NULL_SECOND),
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
        // The helpers reach the checked path THROUGH `__rt_stdout_write`, which is where an
        // output buffer intercepts them. Both of its variants must still end in the checked
        // write, so the errno guarantee survives the indirection rather than being weakened by
        // it.
        for variant in [
            crate::codegen_wasm::output_buffer::RT_STDOUT_WRITE,
            crate::codegen_wasm::output_buffer::RT_STDOUT_WRITE_PASSTHROUGH,
        ] {
            assert!(
                variant.contains("call $__rt_wasi_write_or_fail"),
                "the stdout interception point bypasses the checked WASI write path:\n{variant}"
            );
        }
        for echo in [RT_ECHO_BOOL, RT_ECHO_STR, RT_ECHO_I64, RT_ECHO_F64] {
            assert!(
                echo.contains("call $__rt_stdout_write"),
                "echo helper bypasses the stdout interception point:\n{echo}"
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
        crate::codegen_wasm::test_support::wasmer_available()
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
