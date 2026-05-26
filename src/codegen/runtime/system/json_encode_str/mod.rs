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

/// Dispatches to the target-specific `__rt_json_encode_str` emitter.
///
/// Writes the `__rt_json_encode_str` global label and all associated helpers
/// to `emitter`.  Input/outputABI:
/// - ARM64:  x1=src ptr,  x2=src len  →  x1=result ptr (in concat_buf), x2=result len
/// - x86_64: rax=src ptr, rdx=src len → rax=result ptr (in concat_buf), rdx=result len
///
/// The emitted helper is callable from generated code after `emit_json_encode_str`
/// has been output.  The symbol name `__rt_json_encode_str` is stable for all call sites.
pub(crate) fn emit_json_encode_str(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        x86_64::emit(emitter);
        return;
    }

    aarch64::emit(emitter);
}
