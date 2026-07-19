//! Purpose:
//! Emits the `__rt_zval_type` runtime helper that returns the PHP `IS_*` type
//! byte stored in a `zval`'s `u1.type` field.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::zval`.
//!
//! Key details:
//! - Input: `x0` / `rax` = zval pointer.
//! - Output: `x0` / `eax` = type byte (low byte of `u1.type_info` at offset 8).
//! - This is a leaf helper with no calls, so it needs no stack frame.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// zval_type: read the PHP IS_* type byte from a zval.
/// Input:  x0 = zval pointer
/// Output: x0 = type byte (IS_UNDEF=0, IS_NULL=1, IS_FALSE=2, IS_TRUE=3,
///         IS_LONG=4, IS_DOUBLE=5, IS_STRING=6, IS_ARRAY=7, ...)
pub fn emit_zval_type(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_zval_type_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: zval_type ---");
    emitter.label_global("__rt_zval_type");
    emitter.instruction("ldrb w0, [x0, #8]");                                   // load the low type byte from zval.u1.type_info
    emitter.instruction("ret");                                                 // return the type byte in x0
}

/// x86_64 Linux implementation of `__rt_zval_type`.
/// Input:  rax = zval pointer
/// Output: eax = type byte
fn emit_zval_type_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: zval_type ---");
    emitter.label_global("__rt_zval_type");
    emitter.instruction("movzx eax, BYTE PTR [rax + 8]");                       // load the low type byte from zval.u1.type_info
    emitter.instruction("ret");                                                 // return the type byte in eax
}
