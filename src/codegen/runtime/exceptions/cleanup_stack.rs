//! Purpose:
//! Emits the `__rt_eh_push`, `__rt_eh_pop`, and `__rt_eh_drain` runtime helpers
//! for the exception cleanup stack.  These manage owning-temporary pointers
//! that must be released when a call throws and `longjmp` bypasses the
//! straight-line release code.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via
//!   `crate::codegen::runtime::exceptions`.
//!
//! Key details:
//! - The cleanup stack lives in the global `_eh_cleanup_stack` array (256
//!   pointers) with a word counter in `_eh_cleanup_top`.
//! - `__rt_eh_push` stores the pointer and increments the top.
//! - `__rt_eh_pop` decrements the top (the value was already released by the
//!   normal straight-line path).
//! - `__rt_eh_drain` pops all remaining entries, calling `__rt_decref_any`
//!   on each, and is invoked by `__rt_throw_current` before `longjmp`.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits `__rt_eh_push`, `__rt_eh_pop`, and `__rt_eh_drain` for the current
/// target.
///
/// # ABI
/// - `__rt_eh_push`: input pointer in the int result register (x0 / rdi).
///   Clobbers scratch only.
/// - `__rt_eh_pop`: no input.  Decrements `_eh_cleanup_top`.
/// - `__rt_eh_drain`: no input.  Loops over the stack, calling
///   `__rt_decref_any` on each entry, then zeroes the top.
pub fn emit_eh_cleanup_stack(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_eh_cleanup_stack_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: eh_cleanup_stack ---");

    // -- __rt_eh_push: store pointer and increment top --
    emitter.label_global("__rt_eh_push");
    emitter.instruction("sub sp, sp, #32");                                    // reserve a small frame for the push helper
    emitter.instruction("stp x29, x30, [sp, #16]");                            // save frame pointer and return address
    emitter.instruction("str x0, [sp]");                                       // save the input pointer before clobbering x0
    emitter.instruction("adrp x9, _eh_cleanup_top@PAGE");                      // x9 = page of the cleanup top counter
    emitter.instruction("add x9, x9, _eh_cleanup_top@PAGEOFF");                // x9 = address of the cleanup top counter
    emitter.instruction("ldr x9, [x9]");                                       // x9 = current cleanup stack top index
    emitter.instruction("adrp x10, _eh_cleanup_stack@PAGE");                   // x10 = page of the cleanup stack array
    emitter.instruction("add x10, x10, _eh_cleanup_stack@PAGEOFF");            // x10 = base of the cleanup stack array
    emitter.instruction("ldr x0, [sp]");                                       // reload the saved input pointer
    emitter.instruction("str x0, [x10, x9, lsl #3]");                          // store the owning temporary pointer at stack[top]
    emitter.instruction("add x9, x9, #1");                                     // increment the top index
    emitter.instruction("adrp x10, _eh_cleanup_top@PAGE");                     // x10 = page of the cleanup top counter
    emitter.instruction("add x10, x10, _eh_cleanup_top@PAGEOFF");              // x10 = address of the cleanup top counter
    emitter.instruction("str x9, [x10]");                                      // persist the updated top index
    emitter.instruction("ldp x29, x30, [sp, #16]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                    // release the helper frame
    emitter.instruction("ret");                                                // return to the caller

    // -- __rt_eh_pop: decrement top (normal path after call succeeds) --
    emitter.label_global("__rt_eh_pop");
    emitter.instruction("adrp x9, _eh_cleanup_top@PAGE");                      // x9 = page of the cleanup top counter
    emitter.instruction("add x9, x9, _eh_cleanup_top@PAGEOFF");                // x9 = address of the cleanup top counter
    emitter.instruction("ldr x9, [x9]");                                       // x9 = current cleanup stack top index
    emitter.instruction("sub x9, x9, #1");                                     // decrement the top index
    emitter.instruction("adrp x10, _eh_cleanup_top@PAGE");                     // x10 = page of the cleanup top counter
    emitter.instruction("add x10, x10, _eh_cleanup_top@PAGEOFF");              // x10 = address of the cleanup top counter
    emitter.instruction("str x9, [x10]");                                      // persist the updated top index
    emitter.instruction("ret");                                                // return to the caller

    // -- __rt_eh_drain: release all remaining entries and reset top --
    emitter.label_global("__rt_eh_drain");
    emitter.instruction("sub sp, sp, #32");                                    // reserve a small frame for the drain helper
    emitter.instruction("stp x29, x30, [sp, #16]");                            // save frame pointer and return address
    emitter.label("__rt_eh_drain_loop");
    emitter.instruction("adrp x9, _eh_cleanup_top@PAGE");                      // x9 = page of the cleanup top counter
    emitter.instruction("add x9, x9, _eh_cleanup_top@PAGEOFF");                // x9 = address of the cleanup top counter
    emitter.instruction("ldr x9, [x9]");                                       // x9 = current cleanup stack top index
    emitter.instruction("cbz x9, __rt_eh_drain_done");                         // stop when the stack is empty
    emitter.instruction("sub x9, x9, #1");                                     // decrement to the topmost entry index
    emitter.instruction("adrp x10, _eh_cleanup_top@PAGE");                     // x10 = page of the cleanup top counter
    emitter.instruction("add x10, x10, _eh_cleanup_top@PAGEOFF");              // x10 = address of the cleanup top counter
    emitter.instruction("str x9, [x10]");                                      // persist the decremented top
    emitter.instruction("adrp x10, _eh_cleanup_stack@PAGE");                   // x10 = page of the cleanup stack array
    emitter.instruction("add x10, x10, _eh_cleanup_stack@PAGEOFF");            // x10 = base of the cleanup stack array
    emitter.instruction("ldr x0, [x10, x9, lsl #3]");                          // load the owning temporary pointer from the stack
    emitter.instruction("bl __rt_decref_any");                                 // release the owning temporary (any type)
    emitter.instruction("b __rt_eh_drain_loop");                               // continue draining
    emitter.label("__rt_eh_drain_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                    // release the helper frame
    emitter.instruction("ret");                                                // return to the caller
}

/// Emits the x86_64 Linux variants of the cleanup-stack helpers.
fn emit_eh_cleanup_stack_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: eh_cleanup_stack ---");

    // -- __rt_eh_push: store pointer and increment top --
    emitter.label_global("__rt_eh_push");
    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame pointer
    emitter.instruction("mov r9, QWORD PTR [rip + _eh_cleanup_top]");           // r9 = current cleanup stack top index
    emitter.instruction("lea r10, [_eh_cleanup_stack]");                        // r10 = base of the cleanup stack array
    emitter.instruction("mov QWORD PTR [r10 + r9*8], rdi");                     // store the owning temporary pointer at stack[top]
    emitter.instruction("add r9, 1");                                           // increment the top index
    emitter.instruction("mov QWORD PTR [rip + _eh_cleanup_top], r9");           // persist the updated top index
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller

    // -- __rt_eh_pop: decrement top (normal path after call succeeds) --
    emitter.label_global("__rt_eh_pop");
    emitter.instruction("mov r9, QWORD PTR [rip + _eh_cleanup_top]");           // r9 = current cleanup stack top index
    emitter.instruction("sub r9, 1");                                           // decrement the top index
    emitter.instruction("mov QWORD PTR [rip + _eh_cleanup_top], r9");           // persist the updated top index
    emitter.instruction("ret");                                                 // return to the caller

    // -- __rt_eh_drain: release all remaining entries and reset top --
    emitter.label_global("__rt_eh_drain");
    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame pointer
    emitter.label("__rt_eh_drain_loop_x86_64");
    emitter.instruction("mov r9, QWORD PTR [rip + _eh_cleanup_top]");           // r9 = current cleanup stack top index
    emitter.instruction("test r9, r9");                                         // is the stack empty?
    emitter.instruction("jz __rt_eh_drain_done_x86_64");                        // stop when the stack is empty
    emitter.instruction("sub r9, 1");                                           // decrement to the topmost entry index
    emitter.instruction("mov QWORD PTR [rip + _eh_cleanup_top], r9");           // persist the decremented top
    emitter.instruction("lea r10, [_eh_cleanup_stack]");                        // r10 = base of the cleanup stack array
    emitter.instruction("mov rdi, QWORD PTR [r10 + r9*8]");                     // load the owning temporary pointer from the stack
    emitter.instruction("call __rt_decref_any");                                // release the owning temporary (any type)
    emitter.instruction("jmp __rt_eh_drain_loop_x86_64");                       // continue draining
    emitter.label("__rt_eh_drain_done_x86_64");
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller
}