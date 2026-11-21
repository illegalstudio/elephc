//! Purpose:
//! Keeps native container destruction resumable when a child or PHP destructor throws.
//!
//! Called from:
//! - Indexed-array, hash, Mixed-cell, and object deep-free emitters.
//!
//! Key details:
//! - Each cleanup frame owns a pending flag and its incoming GC suppression state.
//! - Protected child calls suspend the prior exception so nested PHP catches stay independent.
//! - A completed container propagates the latest exception only after releasing its storage.

use crate::codegen_support::{emit::Emitter, platform::Arch};

pub(crate) use elephc_memoir::cleanup::Scope;

// The native coroutine unwind marker never becomes a PHP previous exception.
const FORCED_UNWIND_CLASS: i8 = -3;

/// Emits the shared callback boundary and exception-state restoration used by all cleanup scopes.
pub(super) fn emit(emitter: &mut Emitter) {
    super::emit_protected(emitter, "__rt_cleanup_call_protected", invoke_body);
    elephc_memoir::cleanup::emit_deep_cleanup_callback(emitter);
    elephc_memoir::cleanup::previous::emit(emitter, FORCED_UNWIND_CLASS);
    elephc_memoir::values::throwable_previous::emit_append_previous(emitter);
}

/// Calls the retained unary function with both supported native and C value conventions.
fn invoke_body(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x9, [sp, #224]");                          // recover the cleanup routine pointer after setjmp
            emitter.instruction("ldr x0, [sp, #232]");                          // provide the retained child value to its native release helper
            emitter.instruction("blr x9");                                      // contain every PHP exception raised by this child cleanup
        },
        Arch::X86_64 => {
            emitter.instruction("mov r11, QWORD PTR [rsp + 224]");              // recover the protected routine pointer
            emitter.instruction("mov rax, QWORD PTR [rsp + 232]");              // install the native unary value convention
            emitter.instruction("mov rdi, rax");                                // also provide the C convention used by object destructor calls
            emitter.instruction("call r11");                                    // catch an escaping destructor before its parent skips later children
        },
    }
}
