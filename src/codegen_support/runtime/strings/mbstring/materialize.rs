//! Purpose:
//! Transfers an already completed bridge result into the existing native materialization path.
//!
//! Called from:
//! - The protected shared-invocation runtime entry after Rust returns successfully.
//!
//! Key details:
//! - The caller transfers all result buffers; its source result is cleared before any helper call.
//! - Stack layout exactly matches __rt_mbstring_status for shared diagnostics and error construction.

use super::*;

/// Emits a consuming C-pointer entry reusing the existing result materializer on each target.
pub(super) fn emit(emitter: &mut Emitter) {
    emitter.label_global("__rt_mbstring_materialize");
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction("sub sp, sp, #128");                                // reserve the status materializer's exact frame layout
        emitter.instruction("stp x29, x30, [sp, #112]");                        // preserve caller linkage at the shared epilogue offsets
        emitter.instruction("add x29, sp, #112");                               // establish the common aligned materialization frame
        emitter.instruction("stp xzr, xzr, [sp, #64]");                         // initialize result length and kind before diagnostics
        for offset in [0, 16, 32] {
            emitter.instruction(&format!("ldp x9, x10, [x0, #{offset}]"));      // borrow the next pair of owned bridge-result words
            emitter.instruction(&format!("stp x9, x10, [sp, #{offset}]"));      // transfer the pair into shared materialization storage
            emitter.instruction(&format!("stp xzr, xzr, [x0, #{offset}]"));     // clear source ownership before any fallible native work
        }
        emitter.instruction("b __rt_mbstring_status_diagnostics");              // share diagnostics, result copying, error construction, and release
    } else {
        emitter.instruction("push rbp");                                        // preserve linkage and align all shared helper calls
        emitter.instruction("mov rbp, rsp");                                    // establish the status materializer's common frame pointer
        emitter.instruction("sub rsp, 112");                                    // reserve its exact result and exception spill layout
        emitter.instruction("mov QWORD PTR [rsp + 64], 0");                     // initialize scalar result length
        emitter.instruction("mov QWORD PTR [rsp + 72], 0");                     // initialize result kind before diagnostics
        for offset in [0, 8, 16, 24, 32, 40] {
            emitter.instruction(&format!("mov r10, QWORD PTR [rdi + {offset}]")); // borrow the next owned bridge-result word
            emitter.instruction(&format!("mov QWORD PTR [rsp + {offset}], r10")); // transfer it into shared materialization storage
            emitter.instruction(&format!("mov QWORD PTR [rdi + {offset}], 0")); // clear source ownership before any native helper runs
        }
        emitter.instruction("jmp __rt_mbstring_status_diagnostics");            // share the complete materialization and release path
    }
}
