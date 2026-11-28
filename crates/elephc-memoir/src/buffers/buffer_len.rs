//! Purpose:
//! Emits the generation-safe `__rt_buffer_len` helper for Buffer length reads.
//! Public Buffer values are scalar handles and must be resolved before metadata access.
//!
//! Called from:
//! - `elephc::codegen_support::runtime::emitters::emit_runtime()` via `buffers`.
//!
//! Key details:
//! - `__rt_buffer_resolve` validates index, generation, and activity before offset-eight access.

use crate::abi;
use crate::Emitter;
use crate::Arch;

/// Emits `__rt_buffer_len` for the active target.
/// Accepts a scalar Buffer handle in the integer result register and returns the
/// logical element count from the validated descriptor's offset-eight field.
pub fn emit_buffer_len(emitter: &mut impl Emitter) {
    if emitter.arch() == Arch::X86_64 {
        emit_buffer_len_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: buffer_len ---");
    emitter.label_global("__rt_buffer_len");
    emitter.instruction("sub sp, sp, #16");                                     // reserve aligned storage for the caller return address across resolution
    emitter.instruction("str x30, [sp]");                                       // preserve caller return address before resolver branch-and-link overwrites it
    abi::emit_call_label(emitter, "__rt_buffer_resolve");
    emitter.instruction("ldr x0, [x0, #8]");                                    // return logical element count from the validated descriptor
    emitter.instruction("ldr x30, [sp]");                                       // restore original caller return address after descriptor lookup
    emitter.instruction("add sp, sp, #16");                                     // release aligned temporary storage before returning
    emitter.instruction("ret");                                                 // return length in x0
}

/// Emits the Linux x86_64 `__rt_buffer_len` variant.
/// Preserves `rbp` solely to satisfy the System V nested-call stack alignment.
fn emit_buffer_len_linux_x86_64(emitter: &mut impl Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: buffer_len ---");
    emitter.label_global("__rt_buffer_len");
    emitter.instruction("push rbp");                                            // preserve callee-saved frame pointer and align the nested resolver call
    abi::emit_call_label(emitter, "__rt_buffer_resolve");
    emitter.instruction("mov rax, QWORD PTR [rax + 8]");                        // return logical element count from the validated descriptor
    emitter.instruction("pop rbp");                                             // restore callee-saved frame pointer before returning to generated code
    emitter.instruction("ret");                                                 // return length in rax
}
