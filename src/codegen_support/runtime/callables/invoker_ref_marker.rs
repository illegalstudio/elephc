//! Purpose:
//! Emits `__rt_invoker_ref_cell_to_mixed`, which turns the value an invoker reference marker
//! points at into one owned boxed Mixed cell.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters` (always emitted with the callable runtime).
//! - Descriptor invokers in `crate::codegen::runtime_callable_invoker`, once per by-value
//!   argument that may arrive as a reference marker.
//!
//! Key details:
//! - Inputs: the reference-cell pointer and the source runtime tag (AArch64 `x0`/`x1`, x86_64
//!   `rdi`/`rsi`). Output: the owned Mixed cell in `x0`/`rax`.
//! - A cell that already holds a boxed Mixed (tag 7) is retained and returned as is; any other
//!   payload is boxed through `__rt_mixed_from_value`, reading the high word only for strings.
//! - This used to be inlined at every argument of every invoker (about twenty instructions each,
//!   over thousands of invokers); the helper is the same sequence, called.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits `__rt_invoker_ref_cell_to_mixed` for the current target.
pub fn emit_invoker_ref_cell_to_mixed(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: invoker_ref_cell_to_mixed ---");
    emitter.label_global("__rt_invoker_ref_cell_to_mixed");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #16");                             // reserve the helper frame
            emitter.instruction("stp x29, x30, [sp]");                          // save frame pointer and return address
            emitter.instruction("mov x29, sp");                                 // establish the helper frame
            emitter.instruction("mov x9, x0");                                  // keep the reference-cell pointer
            emitter.instruction("ldr x10, [x9]");                               // load the referenced low payload word
            emitter.instruction("cmp x1, #7");                                  // does the cell already hold a boxed Mixed?
            emitter.instruction("b.eq __rt_invoker_ref_cell_to_mixed_retain");  // reuse it instead of nesting a box
            emitter.instruction("mov x2, #0");                                  // non-string payloads carry no high word
            emitter.instruction("cmp x1, #1");                                  // is the referenced value a string?
            emitter.instruction("b.ne __rt_invoker_ref_cell_to_mixed_box");     // only strings need their length word
            emitter.instruction("ldr x2, [x9, #8]");                            // load the string length word
            emitter.label("__rt_invoker_ref_cell_to_mixed_box");
            emitter.instruction("mov x0, x1");                                  // x0 = runtime tag for the boxing helper
            emitter.instruction("mov x1, x10");                                 // x1 = low payload word
            emitter.instruction("bl __rt_mixed_from_value");                    // box (retaining or persisting) the payload
            emitter.instruction("b __rt_invoker_ref_cell_to_mixed_done");       // return the owned cell
            emitter.label("__rt_invoker_ref_cell_to_mixed_retain");
            emitter.instruction("mov x0, x10");                                 // the referenced Mixed cell is the result
            emitter.instruction("str x0, [sp, #-16]!");                         // keep it across the retain
            emitter.instruction("bl __rt_incref");                              // the caller receives its own reference
            emitter.instruction("ldr x0, [sp], #16");                           // restore the retained cell
            emitter.label("__rt_invoker_ref_cell_to_mixed_done");
            emitter.instruction("ldp x29, x30, [sp]");                          // restore frame pointer and return address
            emitter.instruction("add sp, sp, #16");                             // release the helper frame
            emitter.instruction("ret");                                         // return the owned Mixed cell in x0
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // save the caller frame pointer
            emitter.instruction("mov rbp, rsp");                                // establish the helper frame
            emitter.instruction("sub rsp, 16");                                 // keep the stack 16-byte aligned for nested calls
            emitter.instruction("mov r10, QWORD PTR [rdi]");                    // load the referenced low payload word
            emitter.instruction("cmp rsi, 7");                                  // does the cell already hold a boxed Mixed?
            emitter.instruction("je __rt_invoker_ref_cell_to_mixed_retain");    // reuse it instead of nesting a box
            emitter.instruction("xor edx, edx");                                // non-string payloads carry no high word
            emitter.instruction("cmp rsi, 1");                                  // is the referenced value a string?
            emitter.instruction("jne __rt_invoker_ref_cell_to_mixed_box");      // only strings need their length word
            emitter.instruction("mov rdx, QWORD PTR [rdi + 8]");                // load the string length word
            emitter.label("__rt_invoker_ref_cell_to_mixed_box");
            emitter.instruction("mov rax, rsi");                                // rax = runtime tag for the boxing helper
            emitter.instruction("mov rdi, r10");                                // rdi = low payload word
            emitter.instruction("mov rsi, rdx");                                // rsi = high payload word
            emitter.instruction("call __rt_mixed_from_value");                  // box (retaining or persisting) the payload
            emitter.instruction("jmp __rt_invoker_ref_cell_to_mixed_done");     // return the owned cell
            emitter.label("__rt_invoker_ref_cell_to_mixed_retain");
            emitter.instruction("mov rax, r10");                                // the referenced Mixed cell is the result
            emitter.instruction("mov QWORD PTR [rsp], rax");                    // keep it across the retain
            emitter.instruction("call __rt_incref");                            // the caller receives its own reference
            emitter.instruction("mov rax, QWORD PTR [rsp]");                    // restore the retained cell
            emitter.label("__rt_invoker_ref_cell_to_mixed_done");
            emitter.instruction("mov rsp, rbp");                                // release the helper frame
            emitter.instruction("pop rbp");                                     // restore the caller frame pointer
            emitter.instruction("ret");                                         // return the owned Mixed cell in rax
        }
    }
}
