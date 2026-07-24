//! Purpose:
//! Emits the shared runtime helper that turns an existing boxed `Mixed(null)`
//! or null-container cell into a fresh empty indexed array.
//!
//! Called from:
//! - Mixed array append, set, and fetch-for-write runtime helpers.
//!
//! Key details:
//! - The caller must only pass a non-null cell whose old payload owns no heap
//!   reference (tag 8, zero, or the in-band null-container sentinel).
//! - The fresh array's initial reference is transferred directly into the cell.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits `__rt_mixed_cell_autovivify_array` for the active target.
pub(crate) fn emit_mixed_cell_autovivify_array(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_cell_autovivify_array_x86_64(emitter);
        return;
    }
    emit_mixed_cell_autovivify_array_aarch64(emitter);
}

/// Emits the AArch64 helper that installs a fresh empty indexed array into a Mixed cell.
fn emit_mixed_cell_autovivify_array_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_cell_autovivify_array ---");
    emitter.label_global("__rt_mixed_cell_autovivify_array");

    emitter.instruction("sub sp, sp, #32");                                     // reserve the receiver slot and saved frame registers
    emitter.instruction("stp x29, x30, [sp, #16]");                             // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #16");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the existing Mixed cell across array allocation
    emitter.instruction("mov x0, #0");                                          // request a zero-capacity indexed array (grown on demand)
    emitter.instruction("mov x1, #8");                                          // autovivified arrays store boxed Mixed pointers
    emitter.instruction("bl __rt_array_new");                                   // allocate the fresh empty indexed array
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the existing Mixed cell
    emitter.instruction("mov x10, #4");                                         // runtime value tag 4 = indexed-array payload
    emitter.instruction("str x10, [x9]");                                       // retag the receiver cell as an indexed array
    emitter.instruction("str x0, [x9, #8]");                                    // transfer the fresh array reference into the cell
    emitter.instruction("str xzr, [x9, #16]");                                  // clear the unused high payload word
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore the caller frame and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the installed array pointer in x0
}

/// Emits the x86_64 helper that installs a fresh empty indexed array into a Mixed cell.
fn emit_mixed_cell_autovivify_array_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_cell_autovivify_array ---");
    emitter.label_global("__rt_mixed_cell_autovivify_array");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame
    emitter.instruction("sub rsp, 16");                                         // reserve the receiver slot while keeping call alignment
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the existing Mixed cell across array allocation
    emitter.instruction("xor edi, edi");                                        // request a zero-capacity indexed array (grown on demand)
    emitter.instruction("mov rsi, 8");                                          // autovivified arrays store boxed Mixed pointers
    emitter.instruction("call __rt_array_new");                                 // allocate the fresh empty indexed array
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the existing Mixed cell
    emitter.instruction("mov QWORD PTR [r10], 4");                              // retag the receiver cell as an indexed array
    emitter.instruction("mov QWORD PTR [r10 + 8], rax");                        // transfer the fresh array reference into the cell
    emitter.instruction("mov QWORD PTR [r10 + 16], 0");                         // clear the unused high payload word
    emitter.instruction("mov rsp, rbp");                                        // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the installed array pointer in rax
}
