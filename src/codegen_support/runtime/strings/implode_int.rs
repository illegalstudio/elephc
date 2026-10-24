//! Purpose:
//! Emits the `__rt_implode_int` runtime entry for `implode()` over an indexed array whose
//! elements are statically typed `int`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - The entry forces raw-int element layout and tail-jumps into `__rt_implode_typed`, the bounded,
//!   growable join `__rt_implode` uses. It used to be a separate walk that formatted every element
//!   with `__rt_itoa`, which reserves a fresh 21-byte window at `_concat_off` per call and never
//!   gave it back: past about 3,100 elements that cursor ran beyond the 64 KiB `_concat_buf` and
//!   wrote digits into the adjacent runtime globals (a corrupted result, or a crash at exit through
//!   a clobbered destructor pointer). The shared join formats each element with `__rt_itoa_into`
//!   straight into its own reserved destination and grows that destination onto the heap.

use crate::codegen_support::emit::Emitter;

/// Emits `__rt_implode_int`.
/// ABI (ARM64): x1/x2 = glue_ptr/glue_len, x3 = array_ptr → x1 = result_ptr, x2 = result_len.
/// ABI (x86_64): rdi/rsi = glue_ptr/glue_len, rdx = array_ptr → rax = result_ptr, rdx = result_len.
pub fn emit_implode_int(emitter: &mut Emitter) {
    super::implode::emit_implode_typed_entry(emitter, "__rt_implode_int", 0);
}
