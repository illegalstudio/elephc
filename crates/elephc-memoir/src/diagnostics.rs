//! Purpose:
//! Owns allocation failure diagnostics shared by memory code and data emission.
//!
//! Called from:
//! - Compiler memory emission and its integration tests.
//!
//! Key details:
//! - Runs only in the compiler and preserves the generated target assembly.

/// Fatal error message when an array allocation request cannot be sized safely,
/// i.e. `capacity * elem_size` does not fit in the machine word. PHP reports the
/// same class of failure as a `ValueError` naming the offending argument
/// (`array_fill(): Argument #2 ($count) is too large`); elephc's runtime has no
/// per-call-site context inside `__rt_array_new`, so it reports the shared cause.
pub const ARRAY_ALLOC_SIZE_MSG: &str =
    "Fatal error: requested array size exceeds the maximum allowed array size\n";

/// Fatal error message when `buffer_new<T>()` receives a negative length or a
/// length whose `len * stride` payload size does not fit in the machine word.
/// `buffer_new` is an elephc extension with no PHP equivalent, so the wording is
/// elephc's own rather than a PHP parity string.
pub const BUFFER_ALLOC_SIZE_MSG: &str =
    "Fatal error: buffer_new() length is negative or exceeds the maximum buffer size\n";

/// Fatal error message when a runtime string producer is asked for a result whose byte
/// count cannot be allocated: either the size computation itself wrapped (`str_repeat()`'s
/// `len * times`, an encoder's `2 * len` / `3 * len` expansion) or the requested size
/// exceeds the configured heap capacity. PHP reports the same class of failure as
/// `Fatal error: Possible integer overflow in memory allocation (...)`; elephc's runtime
/// has no per-call-site operand context, so it reports the shared cause.
pub const ALLOC_OVERFLOW_MSG: &str =
    "Fatal error: Possible integer overflow in memory allocation\n";

/// Fatal error message when a runtime writer would append past the shared `_concat_buf`
/// arena. A writer that calls `__rt_concat_headroom` before an append fails closed here rather
/// than overwriting `_concat_off` and the adjacent heap. `json_encode()` and `serialize()` do
/// not stop here: they write through the growable `__rt_strbuf_*` buffer, which moves to the
/// heap past the arena, as PHP grows its result buffer.
pub const CONCAT_OVERFLOW_MSG: &str =
    "Fatal error: string result exceeds the configured string buffer\n";

/// Shared web stream close slot.
pub const WEB_STREAM_CLOSE_SLOT: &str = "_web_application_stream_close";
