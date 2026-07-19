//! Purpose:
//! Emits the resource-aware `__rt_mixed_clone` helper used by ordinary PHP
//! reads of boxed Mixed values.
//!
//! Called from:
//! - EIR `MixedClone` lowering and boxed array/hash read helpers.
//!
//! Key details:
//! - Ordinary values receive a detached Mixed cell through unbox/rebox.
//! - Resources retain and return the existing cell so all aliases share one
//!   resource lifetime and the destructor runs only after the final owner drops.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits `__rt_mixed_clone`, returning one owned PHP value read from a borrowed
/// Mixed cell in `x0`/`rax`.
pub fn emit_mixed_clone(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_clone_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: mixed_clone ---");
    emitter.label_global("__rt_mixed_clone");
    emitter.instruction("sub sp, sp, #32");                                     // reserve the source cell and saved frame registers
    emitter.instruction("stp x29, x30, [sp, #16]");                             // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #16");                                    // establish a stable helper frame
    emitter.instruction("str x0, [sp]");                                        // preserve the borrowed source cell across unboxing
    emitter.instruction("bl __rt_mixed_unbox");                                 // expose the concrete tag and payload for value cloning
    emitter.instruction("cmp x0, #9");                                          // does the value carry PHP resource identity?
    emitter.instruction("b.ne __rt_mixed_clone_value");                         // non-resources receive an independent zval cell
    emitter.instruction("ldr x0, [sp]");                                        // reload the shared resource cell
    emitter.instruction("bl __rt_incref");                                      // give the caller its own reference to the resource cell
    emitter.instruction("ldr x0, [sp]");                                        // return the retained resource cell itself
    emitter.instruction("b __rt_mixed_clone_done");                             // skip detached value boxing for resources
    emitter.label("__rt_mixed_clone_value");
    emitter.instruction("bl __rt_mixed_from_value");                            // detach the ordinary value into a fresh owned cell
    emitter.label("__rt_mixed_clone_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore the caller frame and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the owned Mixed value
}

/// Emits the Linux x86_64 implementation of `__rt_mixed_clone` using the
/// runtime's custom Mixed register convention.
fn emit_mixed_clone_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_clone ---");
    emitter.label_global("__rt_mixed_clone");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame
    emitter.instruction("sub rsp, 16");                                         // reserve one aligned source-cell spill slot
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the borrowed source cell across unboxing
    emitter.instruction("call __rt_mixed_unbox");                               // expose the concrete tag and payload for value cloning
    emitter.instruction("cmp rax, 9");                                          // does the value carry PHP resource identity?
    emitter.instruction("jne __rt_mixed_clone_value");                          // non-resources receive an independent zval cell
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the shared resource cell
    emitter.instruction("call __rt_incref");                                    // give the caller its own reference to the resource cell
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the retained resource cell itself
    emitter.instruction("jmp __rt_mixed_clone_done");                           // skip detached value boxing for resources
    emitter.label("__rt_mixed_clone_value");
    emitter.instruction("mov rsi, rdx");                                        // adapt the unboxed high word to mixed_from_value's ABI
    emitter.instruction("call __rt_mixed_from_value");                          // detach the ordinary value into a fresh owned cell
    emitter.label("__rt_mixed_clone_done");
    emitter.instruction("mov rsp, rbp");                                        // release the source-cell spill slot
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the owned Mixed value
}
