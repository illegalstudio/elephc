//! Purpose:
//! Allocates zero-initialized storage for native compression filter state.
//!
//! Called from:
//! - Compiler deflate and bzip2 write-filter initialization.
//!
//! Key details:
//! - The caller supplies the foreign payload size and selects the native codec.
//! - Preserve the existing owned-heap tag and target-specific pointer spill.

use crate::{abi, Arch, Emitter};

/// Allocates and zeros the filter state in the current caller frame.
///
/// The pointer is saved at sp + 8 on AArch64 and rbp - 16 on x86_64.
pub fn emit_zeroed(emitter: &mut impl Emitter, bytes: i64, prefix: &str) {
    let result = emitter.int_result_reg();
    abi::emit_load_int_immediate(emitter, result, bytes);
    crate::strings::str_new_owned::emit_allocate_owned_string(emitter);
    let zero = format!("{prefix}_zero");
    let done = format!("{prefix}_zeroed");

    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("str x0, [sp, #8]");                            // park the allocated state before the codec call
            emitter.instruction("mov x9, #0");                                  // start at the first state byte
            emitter.label(&zero);
            emitter.instruction(&format!("cmp x9, #{bytes}"));                  // test the number of bytes initialized
            emitter.instruction(&format!("b.ge {done}"));                       // the complete state is ready for native initialization
            emitter.instruction("strb wzr, [x0, x9]");                          // clear one byte before exposing the state to the codec
            emitter.instruction("add x9, x9, #1");                              // advance to the next state byte
            emitter.instruction(&format!("b {zero}"));                          // continue clearing the payload
        }
        Arch::X86_64 => {
            emitter.instruction("mov QWORD PTR [rbp - 16], rax");               // park the allocated state before the codec call
            emitter.instruction("xor r9, r9");                                  // start at the first state byte
            emitter.label(&zero);
            emitter.instruction(&format!("cmp r9, {bytes}"));                   // test the number of bytes initialized
            emitter.instruction(&format!("jge {done}"));                        // the complete state is ready for native initialization
            emitter.instruction("mov BYTE PTR [rax + r9], 0");                  // clear one byte before exposing the state to the codec
            emitter.instruction("inc r9");                                      // advance to the next state byte
            emitter.instruction(&format!("jmp {zero}"));                        // continue clearing the payload
        }
    }
    emitter.label(&done);
}
