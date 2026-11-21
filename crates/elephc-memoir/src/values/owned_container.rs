//! Purpose:
//! Boxes a fresh container and retires the caller's original container owner.
//!
//! Called from:
//! - Compiler adapters for parsers that transfer newly constructed arrays, hashes or objects.
//!
//! Key details:
//! - Mixed acquires its payload owner before the original owner is released.
//! - The compiler supplies the helper label and heading; parser policy stays outside Memoir.

use crate::{Arch, AssemblyEmitter, TargetAbi};

/// Emits a native helper consuming one fresh container owner and returning its owned Mixed box.
pub fn emit<E: AssemblyEmitter + TargetAbi>(emitter: &mut E, entry: &str, heading: &str) {
    emitter.blank();
    emitter.comment(heading);
    emitter.label_global(entry);
    match emitter.arch() {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// Emits the consuming container-owner transfer for aarch64.
fn emit_aarch64<E: AssemblyEmitter + TargetAbi>(emitter: &mut E) {
    // -- tags 4/5/6 keep the borrowed boxing ABI while consuming the fresh payload --
    emitter.instruction("sub sp, sp, #32");                                     // reserve payload, result, and saved linkage
    emitter.instruction("stp x29, x30, [sp, #16]");                             // preserve the container builder frame
    emitter.instruction("add x29, sp, #16");                                    // establish a frame across retain and release
    emitter.instruction("str x1, [sp]");                                        // save the fresh array, hash, or object owner
    emitter.instruction("bl __rt_mixed_from_value");                            // let the new Mixed cell retain the container
    emitter.instruction("str x0, [sp, #8]");                                    // preserve the new boxed owner across release
    emitter.instruction("ldr x0, [sp]");                                        // recover the builder's original container reference
    emitter.instruction("bl __rt_decref_any");                                  // retire that reference after Mixed has acquired its own
    emitter.instruction("ldr x0, [sp, #8]");                                    // return the independently owned Mixed cell
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore the container builder frame
    emitter.instruction("add sp, sp, #32");                                     // release boxing scratch storage
    emitter.instruction("ret");                                                 // transfer the boxed graph to the parser caller
}

/// Emits the consuming container-owner transfer for x86_64.
fn emit_x86_64<E: AssemblyEmitter + TargetAbi>(emitter: &mut E) {
    // -- tags 4/5/6 consume the fresh payload after the ordinary boxer retains it --
    emitter.instruction("push rbp");                                            // preserve the container builder frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish stable payload and result spill slots
    emitter.instruction("sub rsp, 16");                                         // keep calls aligned while saving both owners
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the fresh array, hash, or object reference
    emitter.instruction("call __rt_mixed_from_value");                          // acquire the container reference owned by the Mixed cell
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the boxed result across uniform release
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // recover the original builder reference
    emitter.instruction("call __rt_decref_any");                                // retire the fresh owner now retained by Mixed
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // return the independently owned Mixed cell
    emitter.instruction("mov rsp, rbp");                                        // release the boxing scratch frame
    emitter.instruction("pop rbp");                                             // restore the container builder frame pointer
    emitter.instruction("ret");                                                 // transfer the boxed graph to the parser caller
}
