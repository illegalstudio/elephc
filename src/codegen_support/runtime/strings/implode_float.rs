//! Purpose:
//! Emits the `__rt_implode_float` runtime entry for `implode()` over float arrays.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - A homogeneous float array stores raw 8-byte doubles after the 24-byte array header, so
//!   neither the string-slot walk nor the integer walk can read it (#640).
//! - The entry forces raw-float element layout and tail-jumps into `__rt_implode_typed`, the
//!   bounded, growable join, which formats each element with `__rt_ftoa_into` inside its own
//!   reserved destination. The former dedicated walk appended glue and `__rt_ftoa` output at
//!   `_concat_off` with no bound and ran past the 64 KiB `_concat_buf` on a long join.

use crate::codegen_support::emit::Emitter;

/// Emits `__rt_implode_float`.
/// ABI (ARM64): x1/x2 = glue_ptr/glue_len, x3 = array_ptr → x1 = result_ptr, x2 = result_len.
/// ABI (x86_64): rdi/rsi = glue_ptr/glue_len, rdx = array_ptr → rax = result_ptr, rdx = result_len.
pub fn emit_implode_float(emitter: &mut Emitter) {
    super::implode::emit_implode_typed_entry(emitter, "__rt_implode_float", 2);
}
