//! Purpose:
//! Emits the `__rt_array_udiff_uintersect` runtime helper for array_udiff / array_uintersect.
//! Compares elements of two indexed arrays with a user comparator (equal when cmp returns 0).
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - O(n*m) nested scan; scalar (8-byte) elements; result preallocated to arr1 capacity so pushes never reallocate.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::try_handlers::{
    TRY_HANDLER_DIAG_DEPTH_OFFSET, TRY_HANDLER_JMP_BUF_OFFSET, TRY_HANDLER_SLOT_SIZE,
};
use crate::codegen_support::abi;

/// array_udiff_uintersect: filter arr1 by comparator-based membership in arr2.
/// Input:  x0 = comparator address, x1 = arr1, x2 = arr2, x3 = optional environment, x4 = mode
/// Output: x0 = new indexed array of kept elements (repacked at sequential indices)
///
/// For each arr1 element, scans arr2 calling `cmp(a, b [, env])`; a zero result means equal.
/// mode 0 (udiff) keeps elements absent from arr2; mode 1 (uintersect) keeps elements present.
/// x19 (comparator), x20 (env) and x21 (mode) are callee-saved across the comparator calls.
pub fn emit_array_udiff_uintersect(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_udiff_uintersect_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_udiff_uintersect ---");
    emitter.label_global("__rt_array_udiff_uintersect");
    let frame_bytes = 96 + TRY_HANDLER_SLOT_SIZE;
    let handler = 96;
    emitter.instruction(&format!("sub sp, sp, #{}", frame_bytes));              // allocate comparison locals plus an exception handler
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // set up the new frame pointer
    emitter.instruction("stp x19, x20, [sp, #64]");                             // save callee-saved comparator address and environment
    emitter.instruction("str x21, [sp, #56]");                                  // save callee-saved mode selector
    emitter.instruction("mov x19, x0");                                         // x19 = comparator address (callee-saved)
    emitter.instruction("str x1, [sp, #0]");                                    // save arr1 pointer
    emitter.instruction("str x2, [sp, #8]");                                    // save arr2 pointer
    emitter.instruction("mov x20, x3");                                         // x20 = optional environment (callee-saved)
    emitter.instruction("mov x21, x4");                                         // x21 = mode (0 = udiff, 1 = uintersect)
    emitter.instruction("ldr x0, [x1, #8]");                                    // x0 = arr1 capacity for the result allocation
    emitter.instruction("mov x1, #8");                                          // result element size = 8 bytes (scalar)
    emitter.instruction("bl __rt_array_new");                                   // allocate the result array, x0 = result
    emitter.instruction("str x0, [sp, #16]");                                   // save the result array pointer
    abi::emit_load_symbol_to_reg(emitter, "x10", "_exc_handler_top", 0);
    emitter.instruction(&format!("str x10, [sp, #{}]", handler));               // link the previous native exception handler
    abi::emit_load_symbol_to_reg(emitter, "x10", "_exc_call_frame_top", 0);
    emitter.instruction(&format!("str x10, [sp, #{}]", handler + 8));           // preserve the surviving activation frame
    abi::emit_load_symbol_to_reg(emitter, "x10", "_rt_diag_suppression", 0);
    emitter.instruction(&format!("str x10, [sp, #{}]", handler + TRY_HANDLER_DIAG_DEPTH_OFFSET)); // preserve diagnostic suppression across longjmp
    emitter.instruction(&format!("add x10, sp, #{}", handler));                 // materialize the comparison exception-handler record
    abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_handler_top", 0);
    emitter.instruction(&format!("add x0, sp, #{}", handler + TRY_HANDLER_JMP_BUF_OFFSET)); // pass the embedded jump buffer to setjmp
    emitter.bl_c("setjmp");                                                     // catch comparator exceptions while the result owner is live
    emitter.instruction("cbnz x0, __rt_array_udiff_uintersect_throw");          // release the result before propagating comparator failure
    emitter.instruction("str xzr, [sp, #24]");                                  // outer index i = 0
    emitter.label("__rt_array_udiff_uintersect_outer");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload arr1 pointer
    emitter.instruction("ldr x3, [x0]");                                        // x3 = arr1 length
    emitter.instruction("ldr x4, [sp, #24]");                                   // x4 = outer index i
    emitter.instruction("cmp x4, x3");                                          // has i reached arr1 length?
    emitter.instruction("b.ge __rt_array_udiff_uintersect_done");               // finish once every arr1 element is processed
    emitter.instruction("add x5, x0, #24");                                     // x5 = arr1 data base
    emitter.instruction("ldr x6, [x5, x4, lsl #3]");                            // x6 = arr1[i]
    emitter.instruction("str x6, [sp, #40]");                                   // save the current element across comparator calls
    emitter.instruction("str xzr, [sp, #32]");                                  // inner index j = 0
    emitter.label("__rt_array_udiff_uintersect_inner");
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload inner index j
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload arr2 pointer
    emitter.instruction("ldr x7, [x1]");                                        // x7 = arr2 length
    emitter.instruction("cmp x9, x7");                                          // has j reached arr2 length?
    emitter.instruction("b.ge __rt_array_udiff_uintersect_absent");             // no arr2 element compared equal
    emitter.instruction("add x8, x1, #24");                                     // x8 = arr2 data base
    emitter.instruction("ldr x10, [x8, x9, lsl #3]");                           // x10 = arr2[j]
    emitter.instruction("ldr x0, [sp, #40]");                                   // comparator argument a = current arr1 element
    emitter.instruction("mov x1, x10");                                         // comparator argument b = arr2[j]
    emitter.instruction("cbz x20, __rt_array_udiff_uintersect_cmp");            // no environment keeps the two-argument comparator ABI
    emitter.instruction("mov x2, x20");                                         // pass the environment as the third comparator argument
    emitter.label("__rt_array_udiff_uintersect_cmp");
    emitter.instruction("blr x19");                                             // call cmp(a, b [, env]); zero result means equal
    emitter.instruction("cbz x0, __rt_array_udiff_uintersect_present");         // a zero comparator result means the element is in arr2
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload inner index j
    emitter.instruction("add x9, x9, #1");                                      // advance to the next arr2 element
    emitter.instruction("str x9, [sp, #32]");                                   // save the advanced inner index
    emitter.instruction("b __rt_array_udiff_uintersect_inner");                 // continue scanning arr2
    emitter.label("__rt_array_udiff_uintersect_present");
    emitter.instruction("cbz x21, __rt_array_udiff_uintersect_advance");        // udiff drops elements present in arr2
    emitter.instruction("b __rt_array_udiff_uintersect_push");                  // uintersect keeps elements present in arr2
    emitter.label("__rt_array_udiff_uintersect_absent");
    emitter.instruction("cbz x21, __rt_array_udiff_uintersect_push");           // udiff keeps elements absent from arr2
    emitter.instruction("b __rt_array_udiff_uintersect_advance");               // uintersect drops elements absent from arr2
    emitter.label("__rt_array_udiff_uintersect_push");
    emitter.instruction("ldr x0, [sp, #16]");                                   // x0 = result array pointer
    emitter.instruction("ldr x1, [sp, #40]");                                   // x1 = element value to keep
    emitter.instruction("bl __rt_array_push_int");                              // append the kept element to the preallocated result
    emitter.label("__rt_array_udiff_uintersect_advance");
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload the outer index i
    emitter.instruction("add x4, x4, #1");                                      // advance to the next arr1 element
    emitter.instruction("str x4, [sp, #24]");                                   // save the advanced outer index
    emitter.instruction("b __rt_array_udiff_uintersect_outer");                 // continue the outer loop
    emitter.label("__rt_array_udiff_uintersect_done");
    emitter.instruction("ldr x0, [sp, #16]");                                   // x0 = result array pointer
    emitter.instruction(&format!("ldr x10, [sp, #{}]", handler));               // reload the preceding native exception handler
    abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_handler_top", 0);
    emitter.instruction(&format!("ldr x10, [sp, #{}]", handler + TRY_HANDLER_DIAG_DEPTH_OFFSET)); // restore diagnostic suppression after success
    abi::emit_store_reg_to_symbol(emitter, "x10", "_rt_diag_suppression", 0);
    emitter.instruction("ldr x21, [sp, #56]");                                  // restore the callee-saved mode selector
    emitter.instruction("ldp x19, x20, [sp, #64]");                             // restore callee-saved comparator address and environment
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction(&format!("add sp, sp, #{}", frame_bytes));              // deallocate comparison locals and exception handler
    emitter.instruction("ret");                                                 // return the result array in x0

    emitter.label("__rt_array_udiff_uintersect_throw");
    emitter.instruction(&format!("ldr x10, [sp, #{}]", handler));               // reload the preceding native exception handler
    abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_handler_top", 0);
    emitter.instruction(&format!("ldr x10, [sp, #{}]", handler + TRY_HANDLER_DIAG_DEPTH_OFFSET)); // restore diagnostic suppression skipped by longjmp
    abi::emit_store_reg_to_symbol(emitter, "x10", "_rt_diag_suppression", 0);
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the partially built comparison result array
    emitter.instruction("bl __rt_decref_array");                                // release the result container after comparator failure
    emitter.instruction("ldr x21, [sp, #56]");                                  // restore mode selector
    emitter.instruction("ldp x19, x20, [sp, #64]");                             // restore comparator and environment registers
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction(&format!("add sp, sp, #{}", frame_bytes));              // discard the protected comparison frame
    emitter.instruction("b __rt_throw_current");                                // resume exception propagation at the caller handler
}

/// x86_64 Linux implementation of `__rt_array_udiff_uintersect`.
/// Input:  rdi = comparator, rsi = arr1, rdx = arr2, rcx = optional environment, r8 = mode
/// Output: rax = new indexed array of kept elements
fn emit_array_udiff_uintersect_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_udiff_uintersect ---");
    emitter.label_global("__rt_array_udiff_uintersect");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("push r12");                                            // preserve the comparator address across the loops
    emitter.instruction("push r13");                                            // preserve the environment across the comparator calls
    emitter.instruction("push r14");                                            // preserve the mode selector across the comparator calls
    let frame_bytes = 80 + TRY_HANDLER_SLOT_SIZE;
    let local_bytes = frame_bytes - 24;
    emitter.instruction(&format!("sub rsp, {}", local_bytes));                  // reserve comparison locals plus an exception handler
    emitter.instruction("mov r12, rdi");                                        // r12 = comparator address (callee-saved)
    emitter.instruction("mov r13, rcx");                                        // r13 = optional environment (callee-saved)
    emitter.instruction("mov r14, r8");                                         // r14 = mode (0 = udiff, 1 = uintersect)
    emitter.instruction("mov QWORD PTR [rbp - 32], rsi");                       // save arr1 pointer
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // save arr2 pointer
    emitter.instruction("mov rdi, QWORD PTR [rsi + 8]");                        // rdi = arr1 capacity for the result allocation
    emitter.instruction("mov rsi, 8");                                          // result element size = 8 bytes (scalar)
    emitter.instruction("call __rt_array_new");                                 // allocate the result array, rax = result
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the result array pointer
    abi::emit_load_symbol_to_reg(emitter, "r10", "_exc_handler_top", 0);
    emitter.instruction(&format!("mov QWORD PTR [rbp - {}], r10", frame_bytes)); // link the previous native exception handler
    abi::emit_load_symbol_to_reg(emitter, "r10", "_exc_call_frame_top", 0);
    emitter.instruction(&format!("mov QWORD PTR [rbp - {}], r10", frame_bytes - 8)); // preserve the surviving activation frame
    abi::emit_load_symbol_to_reg(emitter, "r10", "_rt_diag_suppression", 0);
    emitter.instruction(&format!("mov QWORD PTR [rbp - {}], r10", frame_bytes - TRY_HANDLER_DIAG_DEPTH_OFFSET)); // preserve diagnostic suppression across longjmp
    emitter.instruction(&format!("lea r10, [rbp - {}]", frame_bytes));          // materialize the comparison exception-handler record
    abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_handler_top", 0);
    emitter.instruction(&format!("lea rdi, [rbp - {}]", frame_bytes - TRY_HANDLER_JMP_BUF_OFFSET)); // pass the embedded jump buffer to setjmp
    emitter.bl_c("setjmp");                                                     // catch comparator exceptions while the result owner is live
    emitter.instruction("test eax, eax");                                       // did control return through longjmp?
    emitter.instruction("jnz __rt_array_udiff_uintersect_throw_x86");           // release the result before propagating comparator failure
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // outer index i = 0
    emitter.label("__rt_array_udiff_uintersect_outer");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload arr1 pointer
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // reload the outer index i
    emitter.instruction("cmp rax, QWORD PTR [r10]");                            // has i reached arr1 length?
    emitter.instruction("jge __rt_array_udiff_uintersect_done");                // finish once every arr1 element is processed
    emitter.instruction("mov r11, QWORD PTR [r10 + rax * 8 + 24]");             // r11 = arr1[i]
    emitter.instruction("mov QWORD PTR [rbp - 64], r11");                       // save the current element across comparator calls
    emitter.instruction("mov QWORD PTR [rbp - 72], 0");                         // inner index j = 0
    emitter.label("__rt_array_udiff_uintersect_inner");
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // reload inner index j
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload arr2 pointer
    emitter.instruction("cmp rax, QWORD PTR [r10]");                            // has j reached arr2 length?
    emitter.instruction("jge __rt_array_udiff_uintersect_absent");              // no arr2 element compared equal
    emitter.instruction("mov rsi, QWORD PTR [r10 + rax * 8 + 24]");             // comparator argument b = arr2[j]
    emitter.instruction("mov rdi, QWORD PTR [rbp - 64]");                       // comparator argument a = current arr1 element
    emitter.instruction("test r13, r13");                                       // is an environment present?
    emitter.instruction("jz __rt_array_udiff_uintersect_cmp");                  // no environment keeps the two-argument comparator ABI
    emitter.instruction("mov rdx, r13");                                        // pass the environment as the third comparator argument
    emitter.label("__rt_array_udiff_uintersect_cmp");
    emitter.instruction("call r12");                                            // call cmp(a, b [, env]); zero result means equal
    emitter.instruction("test rax, rax");                                       // did the comparator report equality?
    emitter.instruction("jz __rt_array_udiff_uintersect_present");              // a zero comparator result means the element is in arr2
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // reload inner index j
    emitter.instruction("add rax, 1");                                          // advance to the next arr2 element
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // save the advanced inner index
    emitter.instruction("jmp __rt_array_udiff_uintersect_inner");               // continue scanning arr2
    emitter.label("__rt_array_udiff_uintersect_present");
    emitter.instruction("test r14, r14");                                       // is the mode udiff (0)?
    emitter.instruction("jz __rt_array_udiff_uintersect_advance");              // udiff drops elements present in arr2
    emitter.instruction("jmp __rt_array_udiff_uintersect_push");                // uintersect keeps elements present in arr2
    emitter.label("__rt_array_udiff_uintersect_absent");
    emitter.instruction("test r14, r14");                                       // is the mode udiff (0)?
    emitter.instruction("jz __rt_array_udiff_uintersect_push");                 // udiff keeps elements absent from arr2
    emitter.instruction("jmp __rt_array_udiff_uintersect_advance");             // uintersect drops elements absent from arr2
    emitter.label("__rt_array_udiff_uintersect_push");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // rdi = result array pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 64]");                       // rsi = element value to keep
    emitter.instruction("call __rt_array_push_int");                            // append the kept element to the preallocated result
    emitter.label("__rt_array_udiff_uintersect_advance");
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // reload the outer index i
    emitter.instruction("add rax, 1");                                          // advance to the next arr1 element
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // save the advanced outer index
    emitter.instruction("jmp __rt_array_udiff_uintersect_outer");               // continue the outer loop
    emitter.label("__rt_array_udiff_uintersect_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // rax = result array pointer
    emitter.instruction(&format!("mov r10, QWORD PTR [rbp - {}]", frame_bytes)); // reload the preceding native exception handler
    abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_handler_top", 0);
    emitter.instruction(&format!("mov r10, QWORD PTR [rbp - {}]", frame_bytes - TRY_HANDLER_DIAG_DEPTH_OFFSET)); // restore diagnostic suppression after success
    abi::emit_store_reg_to_symbol(emitter, "r10", "_rt_diag_suppression", 0);
    emitter.instruction(&format!("add rsp, {}", local_bytes));                  // release comparison locals and exception handler
    emitter.instruction("pop r14");                                             // restore the mode register
    emitter.instruction("pop r13");                                             // restore the environment register
    emitter.instruction("pop r12");                                             // restore the comparator register
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the result array in rax

    emitter.label("__rt_array_udiff_uintersect_throw_x86");
    emitter.instruction(&format!("mov r10, QWORD PTR [rbp - {}]", frame_bytes)); // reload the preceding native exception handler
    abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_handler_top", 0);
    emitter.instruction(&format!("mov r10, QWORD PTR [rbp - {}]", frame_bytes - TRY_HANDLER_DIAG_DEPTH_OFFSET)); // restore diagnostic suppression skipped by longjmp
    abi::emit_store_reg_to_symbol(emitter, "r10", "_rt_diag_suppression", 0);
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // reload the partially built comparison result array
    emitter.instruction("call __rt_decref_array");                              // release the result container after comparator failure
    emitter.instruction(&format!("add rsp, {}", local_bytes));                  // discard comparison locals and exception handler
    emitter.instruction("pop r14");                                             // restore mode selector
    emitter.instruction("pop r13");                                             // restore environment register
    emitter.instruction("pop r12");                                             // restore comparator register
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("jmp __rt_throw_current");                              // resume exception propagation at the caller handler
}
