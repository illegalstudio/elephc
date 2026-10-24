//! Purpose:
//! Emits the `__rt_implode_bool` runtime entry for `implode()` over an indexed array whose
//! elements are statically typed `bool` (or the `false` literal subtype).
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - PHP stringifies `true` as `"1"` and `false` as the EMPTY string, not as `"0"`, and the glue is
//!   still emitted around an empty rendering (`implode(",", [true, false])` is `"1,"`).
//! - The entry forces raw-bool element layout and tail-jumps into `__rt_implode_typed`, whose raw
//!   bool arm renders exactly that, inside a bounded destination that grows onto the heap. The
//!   former dedicated walk appended into `_concat_buf` with no bound.

use crate::codegen_support::emit::Emitter;

/// Emits `__rt_implode_bool`.
/// ABI (ARM64): x1/x2 = glue_ptr/glue_len, x3 = array_ptr → x1 = result_ptr, x2 = result_len.
/// ABI (x86_64): rdi/rsi = glue_ptr/glue_len, rdx = array_ptr → rax = result_ptr, rdx = result_len.
pub fn emit_implode_bool(emitter: &mut Emitter) {
    super::implode::emit_implode_typed_entry(emitter, "__rt_implode_bool", 3);
}
