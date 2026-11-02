//! Purpose:
//! Emits runtime-dispatched `array_filter()` support for boxed gradual arrays.
//! Routes indexed and associative payloads to target-specific filter loops.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via the arrays runtime module.
//!
//! Key details:
//! - Callback values and keys use boxed `Mixed` ABI values because the source shape is dynamic.
//! - Filtered results use hash storage so original integer/string keys and PHP insertion order
//!   survive gaps while list-shape consumers can still recognize contiguous numeric keys.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

mod aarch64;
mod x86_64;

pub(super) const ARRAY_FILTER_MODE_MSG_LEN: usize = "array_filter(): Argument #3 ($mode) must be one of ARRAY_FILTER_USE_VALUE, ARRAY_FILTER_USE_KEY, or ARRAY_FILTER_USE_BOTH.".len();

/// Emits `__rt_array_filter_mixed` for the selected target.
pub fn emit_array_filter_mixed(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => aarch64::emit(emitter),
        Arch::X86_64 => x86_64::emit(emitter),
    }
}
