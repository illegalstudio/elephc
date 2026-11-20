//! Purpose:
//! Joins small and staged string producers to checked concat result storage.
//!
//! Called from:
//! - Compiler-emitted string and platform query routines.
//!
//! Key details:
//! - The fallback owner is adoptable when the result escapes scratch lifetime.

use crate::{abi, Arch, Emitter};

use super::concat_scratch::emit_mark_reserved_result_temporary;

/// Reserves one output byte while keeping the incoming integer in x0 or rdi.
///
/// The returned destination is x1 on AArch64 or rax on x86-64. Heap fallback is marked
/// for the ordinary string persistence path. This runs at the producer's entry.
pub fn emit_reserve_byte_for_integer(emitter: &mut impl Emitter, ready: &str) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("stp x0, x30, [sp, #-16]!");                    // preserve the incoming integer and return link
            abi::emit_load_int_immediate(emitter, "x0", 1);
            abi::emit_call_label(emitter, "__rt_concat_reserve");
            emit_mark_reserved_result_temporary(emitter, ready, ["r8", "r9", "r10"]);
            abi::emit_reg_move(emitter, "x1", "x0");
            emitter.instruction("ldp x0, x30, [sp], #16");                      // restore the input and caller return link
        }
        Arch::X86_64 => {
            emitter.instruction("push rdi");                                    // preserve the incoming integer and align the call
            abi::emit_load_int_immediate(emitter, "rax", 1);
            abi::emit_call_label(emitter, "__rt_concat_reserve");
            emit_mark_reserved_result_temporary(emitter, ready, ["r8", "r9", "r10"]);
            emitter.instruction("pop rdi");                                     // restore the producer's integer and entry stack
        }
    }
}

/// Copies a fully staged string to a checked transient destination.
///
/// Input and output use the ordinary string result registers. The caller's temporary
/// source may be on the stack and must live through the copy.
pub fn emit_copy_staged_result(emitter: &mut impl Emitter, ready: &str) {
    abi::emit_call_label(emitter, "__rt_strcopy");
    if emitter.arch() == Arch::AArch64 {
        abi::emit_reg_move(emitter, "x0", "x1");
    }
    emit_mark_reserved_result_temporary(emitter, ready, ["r8", "r9", "r10"]);
}
