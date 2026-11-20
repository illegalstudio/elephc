//! Purpose:
//! Formats the display identity of a resource as PHP byte text.
//!
//! Called from:
//! - Resource casts, string arguments, and the eval bridge.
//!
//! Key details:
//! - The frame stages the text before Memoir publishes a checked scratch copy.
//! - The result is borrowed because several runtime callers forward it without claiming it.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use elephc_memoir::strings::staged_concat::emit_copy_staged_to_scratch;

/// Formats the display identity as `Resource id #N` on the active target.
pub fn emit_resource_to_string(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: resource_to_string ---");
    emitter.label_global("__rt_resource_to_string");
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// Stages the signed decimal suffix and prepends the resource label on AArch64.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.instruction("sub sp, sp, #64");                                     // reserve the byte staging area and caller linkage
    emitter.instruction("stp x29, x30, [sp, #48]");                             // preserve the frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the formatter frame
    abi::emit_call_label(emitter, "__rt_resource_id_of");
    emitter.instruction("add x1, sp, #16");                                     // leave at least thirteen bytes before the digit window
    abi::emit_call_label(emitter, "__rt_itoa_into");
    emitter.instruction("add x2, x1, #13");                                     // include the resource label in the staged length
    emitter.instruction("sub x1, x0, #13");                                     // prepend the label immediately before the first digit
    abi::emit_symbol_address(emitter, "x9", "_resource_id_prefix");
    emitter.instruction("mov x10, #0");                                         // start at the first label byte
    emitter.label("__rt_resource_to_string_prefix");
    emitter.instruction("ldrb w11, [x9, x10]");                                 // load a byte of the resource label
    emitter.instruction("strb w11, [x1, x10]");                                 // write it immediately before the decimal suffix
    emitter.instruction("add x10, x10, #1");                                    // advance the label cursor
    emitter.instruction("cmp x10, #13");                                        // did all label bytes fit?
    emitter.instruction("b.lo __rt_resource_to_string_prefix");                 // continue through the end of the label
    emit_copy_staged_to_scratch(emitter);
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore the caller linkage
    emitter.instruction("add sp, sp, #64");                                     // release staging storage
    emitter.instruction("ret");                                                 // return the borrowed string pair
}

/// Stages the signed decimal suffix and prepends the resource label on x86-64.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.instruction("push rbp");                                            // preserve the caller's frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the formatter frame
    emitter.instruction("sub rsp, 48");                                         // reserve the complete staged byte sequence
    abi::emit_call_label(emitter, "__rt_resource_id_of");
    emitter.instruction("mov rdi, rax");                                        // pass the display identity to the integer formatter
    emitter.instruction("lea rsi, [rbp - 32]");                                 // leave sixteen bytes before the digit window
    abi::emit_call_label(emitter, "__rt_itoa_into");
    emitter.instruction("add rdx, 13");                                         // include the label in the staged length
    emitter.instruction("sub rax, 13");                                         // prepend the label just before the first digit
    abi::emit_symbol_address(emitter, "r9", "_resource_id_prefix");
    emitter.instruction("xor ecx, ecx");                                        // start at the first label byte
    emitter.label("__rt_resource_to_string_prefix_x86");
    emitter.instruction("mov r10b, BYTE PTR [r9 + rcx]");                       // load a byte of the resource label
    emitter.instruction("mov BYTE PTR [rax + rcx], r10b");                      // write it immediately before the decimal suffix
    emitter.instruction("add rcx, 1");                                          // advance the label cursor
    emitter.instruction("cmp rcx, 13");                                         // did all label bytes fit?
    emitter.instruction("jb __rt_resource_to_string_prefix_x86");               // continue through the end of the label
    emit_copy_staged_to_scratch(emitter);
    emitter.instruction("leave");                                               // release the staging area and restore the frame
    emitter.instruction("ret");                                                 // return the borrowed string pair
}
