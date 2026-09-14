//! Purpose:
//! Contains eval-owned releases and cycle collection behind native exception handlers.
//!
//! Called from:
//! - The generated eval bridge and Magician's runtime value adapter.
//!
//! Key details:
//! - Destructors finish native cleanup before returning a pending Throwable status to Rust.

use super::*;
use crate::codegen_support::runtime::exceptions::emit_protected;

/// Emits C entry points whose native callbacks cannot unwind through a Rust caller.
pub(super) fn emit(emitter: &mut Emitter) {
    let release = emitter.target.extern_symbol("__elephc_eval_value_release_protected");
    emit_protected(emitter, &release, release_body);
    let collect = emitter.target.extern_symbol("__elephc_eval_collect_cycles");
    emit_protected(emitter, &collect, collect_body);
}

/// Consumes the first C argument as one owned Mixed box inside the protected frame.
fn release_body(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x0, [sp, #224]");                          // recover the retained owned value from the C callback frame
            emitter.instruction("bl __rt_decref_mixed");                        // finish recursive cleanup before propagating a destructor exception
        },
        Arch::X86_64 => {
            emitter.instruction("mov rax, QWORD PTR [rsp + 224]");              // adapt the retained C owner to the native Mixed release convention
            emitter.instruction("call __rt_decref_mixed");                      // contain destructor throws until the complete release has finished
        },
    }
}

/// Runs the shared collector after eval has removed a root from its live scope.
fn collect_body(emitter: &mut Emitter) {
    abi::emit_call_label(emitter, "__rt_gc_collect_cycles");
}
