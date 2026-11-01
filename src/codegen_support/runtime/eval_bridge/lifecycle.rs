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
            emitter.instruction(&format!("ldr x0, [sp, #{}]", crate::codegen_support::runtime::exceptions::PROTECTED_INPUT_OFFSET));                          // recover the retained owned value from the C callback frame
            emitter.instruction("bl __rt_decref_mixed");                        // finish recursive cleanup before propagating a destructor exception
        },
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", crate::codegen_support::runtime::exceptions::PROTECTED_INPUT_OFFSET));              // adapt the retained C owner to the native Mixed release convention
            emitter.instruction("call __rt_decref_mixed");                      // contain destructor throws until the complete release has finished
        },
    }
}

/// Runs the shared collector after eval has removed a root from its live scope.
///
/// This is the UNCONDITIONAL collector, unlike the compiled `unset()` (`__rt_gc_safepoint`, which
/// only buffers the possible root like php does). An interpreted cycle is reachable at shutdown
/// only while the interpreter's context is alive -- the main epilogue's final pass does not
/// reach it -- so deferring here would never run its destructor at all. The price is that an
/// interpreted `unset($cycle)` destroys the cycle at once where php waits for its buffer.
fn collect_body(emitter: &mut Emitter) {
    abi::emit_call_label(emitter, "__rt_gc_collect_cycles");
}
