//! Purpose:
//! Emits target dispatch for JSON string escaping runtime helpers.
//! Provides the runtime assembly used by JSON builtins on the selected target.
//!
//! Called from:
//! - `crate::codegen::runtime::system` during runtime emission.
//!
//! Key details:
//! - The exported helper name must stay stable for all JSON encoder call sites.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

mod aarch64;
mod x86_64;

/// __rt_json_encode_str: JSON-encode a string (add quotes, escape special chars).
/// Input:  x1 = string ptr, x2 = string len  (ARM64)
///         rax = string ptr, rdx = string len (x86_64)
/// Output: x1/rax = result ptr (in concat_buf), x2/rdx = result len
pub(crate) fn emit_json_encode_str(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        x86_64::emit(emitter);
        return;
    }

    aarch64::emit(emitter);
}
