//! Purpose:
//! Emits the `__rt_array_find_any_all` runtime helper for array_find / array_any / array_all.
//! Walks an indexed array, invoking the predicate callback on each element and returning per the mode.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - mode 0 = find (boxed first matching element or null), 1 = any (bool), 2 = all (bool); scalar elements only (8-byte).

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// array_find_any_all: predicate-driven search over an indexed array.
/// Input:  x0 = callback address, x1 = array pointer, x2 = optional environment, x3 = mode
/// Output: x0 = boxed Mixed (find: first match or null), or 1/0 (any/all)
///
/// mode 0 (find) returns the first element where the callback is truthy, boxed as a Mixed
/// value using the array value_type, or a boxed null when none match. mode 1 (any) returns 1
/// if any element is truthy. mode 2 (all) returns 1 only if every element is truthy. The
/// callback receives `(element [, env])`; element scalars are read as a single word.
pub fn emit_array_find_any_all(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_find_any_all_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_find_any_all ---");
    emitter.label_global("__rt_array_find_any_all");
    emitter.instruction("sub sp, sp, #80");                                     // allocate the find/any/all stack frame
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // set up the new frame pointer
    emitter.instruction("stp x19, x20, [sp, #48]");                             // save callee-saved callback address and environment
    emitter.instruction("str x21, [sp, #40]");                                  // save callee-saved element register
    emitter.instruction("mov x19, x0");                                         // x19 = callback address (callee-saved)
    emitter.instruction("mov x20, x2");                                         // x20 = optional environment (callee-saved)
    emitter.instruction("str x1, [sp, #0]");                                    // save the array pointer
    emitter.instruction("str x3, [sp, #8]");                                    // save the mode selector
    emitter.instruction("ldr x9, [x1]");                                        // load the array length
    emitter.instruction("str x9, [sp, #16]");                                   // save the array length
    emitter.instruction("ldr x9, [x1, #-8]");                                   // load the uniform heap-kind header word
    emitter.instruction("lsr x9, x9, #8");                                      // shift the packed value_type into the low bits
    emitter.instruction("and x9, x9, #0x7f");                                   // isolate the indexed-array value_type (also the Mixed tag)
    emitter.instruction("str x9, [sp, #24]");                                   // save the element value_type for find boxing
    emitter.instruction("str xzr, [sp, #32]");                                  // index i = 0
    emitter.label("__rt_array_find_any_all_loop");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the length
    emitter.instruction("ldr x10, [sp, #32]");                                  // reload the index
    emitter.instruction("cmp x10, x9");                                         // has the index reached the length?
    emitter.instruction("b.ge __rt_array_find_any_all_end");                    // no element matched / all visited
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the array pointer
    emitter.instruction("add x1, x1, #24");                                     // skip the 24-byte indexed-array header
    emitter.instruction("ldr x21, [x1, x10, lsl #3]");                          // load element[i] into the callee-saved element register
    emitter.instruction("mov x0, x21");                                         // pass the element as the first callback argument
    emitter.instruction("cbz x20, __rt_array_find_any_all_call");               // no environment keeps the one-argument callback ABI
    emitter.instruction("mov x1, x20");                                         // pass the environment as the second callback argument
    emitter.label("__rt_array_find_any_all_call");
    emitter.instruction("blr x19");                                             // call the predicate callback; truthy result in x0
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload the mode selector
    emitter.instruction("cbz x11, __rt_array_find_any_all_find");               // mode 0 is find
    emitter.instruction("cmp x11, #1");                                         // mode 1 is any?
    emitter.instruction("b.eq __rt_array_find_any_all_any");                    // handle the any mode
    emitter.instruction("cbz x0, __rt_array_find_any_all_zero");                // all mode: a falsy element returns 0
    emitter.instruction("b __rt_array_find_any_all_next");                      // all mode: keep checking
    emitter.label("__rt_array_find_any_all_any");
    emitter.instruction("cbnz x0, __rt_array_find_any_all_one");                // any mode: a truthy element returns 1
    emitter.instruction("b __rt_array_find_any_all_next");                      // any mode: keep checking
    emitter.label("__rt_array_find_any_all_find");
    emitter.instruction("cbz x0, __rt_array_find_any_all_next");                // find mode: skip falsy elements
    emitter.instruction("ldr x0, [sp, #24]");                                   // value_type tag for boxing the found element
    emitter.instruction("mov x1, x21");                                         // found element low word
    emitter.instruction("mov x2, #0");                                          // found element high word unused
    emitter.instruction("bl __rt_mixed_from_value");                            // box the found element as a Mixed value
    emitter.instruction("b __rt_array_find_any_all_ret");                       // return the boxed element
    emitter.label("__rt_array_find_any_all_next");
    emitter.instruction("ldr x10, [sp, #32]");                                  // reload the index
    emitter.instruction("add x10, x10, #1");                                    // advance to the next element
    emitter.instruction("str x10, [sp, #32]");                                  // save the advanced index
    emitter.instruction("b __rt_array_find_any_all_loop");                      // continue the predicate loop
    emitter.label("__rt_array_find_any_all_end");
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload the mode selector
    emitter.instruction("cbz x11, __rt_array_find_any_all_findnull");           // find found nothing: return boxed null
    emitter.instruction("cmp x11, #1");                                         // any mode?
    emitter.instruction("b.eq __rt_array_find_any_all_zero");                   // any matched nothing: return 0
    emitter.instruction("b __rt_array_find_any_all_one");                       // all elements passed: return 1
    emitter.label("__rt_array_find_any_all_findnull");
    emitter.instruction("mov x0, #8");                                          // value_tag 8 = null
    emitter.instruction("movz x1, #0xFFFE");                                    // null sentinel bits [15:0]
    emitter.instruction("movk x1, #0xFFFF, lsl #16");                           // null sentinel bits [31:16]
    emitter.instruction("movk x1, #0xFFFF, lsl #32");                           // null sentinel bits [47:32]
    emitter.instruction("movk x1, #0x7FFF, lsl #48");                           // null sentinel bits [63:48] = 0x7FFFFFFFFFFFFFFE
    emitter.instruction("mov x2, #0");                                          // value high word unused
    emitter.instruction("bl __rt_mixed_from_value");                            // box the null sentinel
    emitter.instruction("b __rt_array_find_any_all_ret");                       // return the boxed null
    emitter.label("__rt_array_find_any_all_one");
    emitter.instruction("mov x0, #1");                                          // boolean result: true
    emitter.instruction("b __rt_array_find_any_all_ret");                       // return the boolean result
    emitter.label("__rt_array_find_any_all_zero");
    emitter.instruction("mov x0, #0");                                          // boolean result: false
    emitter.label("__rt_array_find_any_all_ret");
    emitter.instruction("ldr x21, [sp, #40]");                                  // restore the callee-saved element register
    emitter.instruction("ldp x19, x20, [sp, #48]");                             // restore callee-saved callback address and environment
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate the stack frame
    emitter.instruction("ret");                                                 // return the result in x0
}

/// x86_64 Linux implementation of `__rt_array_find_any_all`.
/// Input:  rdi = callback, rsi = array pointer, rdx = optional environment, rcx = mode
/// Output: rax = boxed Mixed (find) or 1/0 (any/all)
fn emit_array_find_any_all_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_find_any_all ---");
    emitter.label_global("__rt_array_find_any_all");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("push r12");                                            // preserve the callback address across the loop
    emitter.instruction("push r13");                                            // preserve the element register across the callback
    emitter.instruction("push r14");                                            // preserve the environment across the callback
    emitter.instruction("sub rsp, 40");                                         // reserve local slots for array/mode/length/value_type/index
    emitter.instruction("mov r12, rdi");                                        // r12 = callback address (callee-saved)
    emitter.instruction("mov r14, rdx");                                        // r14 = optional environment (callee-saved)
    emitter.instruction("mov QWORD PTR [rbp - 32], rsi");                       // save the array pointer
    emitter.instruction("mov QWORD PTR [rbp - 40], rcx");                       // save the mode selector
    emitter.instruction("mov rax, QWORD PTR [rsi]");                            // load the array length
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the array length
    emitter.instruction("mov rax, QWORD PTR [rsi - 8]");                        // load the uniform heap-kind header word
    emitter.instruction("shr rax, 8");                                          // shift the packed value_type into the low bits
    emitter.instruction("and rax, 127");                                        // isolate the indexed-array value_type (also the Mixed tag)
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // save the element value_type for find boxing
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // index i = 0
    emitter.label("__rt_array_find_any_all_loop");
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // reload the index
    emitter.instruction("cmp rax, QWORD PTR [rbp - 48]");                       // has the index reached the length?
    emitter.instruction("jge __rt_array_find_any_all_end");                     // no element matched / all visited
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the array pointer
    emitter.instruction("mov r13, QWORD PTR [r10 + rax * 8 + 24]");             // load element[i] into the callee-saved element register
    emitter.instruction("mov rdi, r13");                                        // pass the element as the first callback argument
    emitter.instruction("test r14, r14");                                       // is an environment present?
    emitter.instruction("jz __rt_array_find_any_all_call");                     // no environment keeps the one-argument callback ABI
    emitter.instruction("mov rsi, r14");                                        // pass the environment as the second callback argument
    emitter.label("__rt_array_find_any_all_call");
    emitter.instruction("call r12");                                            // call the predicate callback; truthy result in rax
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // reload the mode selector
    emitter.instruction("test r11, r11");                                       // mode 0 is find?
    emitter.instruction("jz __rt_array_find_any_all_find");                     // handle the find mode
    emitter.instruction("cmp r11, 1");                                          // mode 1 is any?
    emitter.instruction("je __rt_array_find_any_all_any");                      // handle the any mode
    emitter.instruction("test rax, rax");                                       // all mode: is this element falsy?
    emitter.instruction("jz __rt_array_find_any_all_zero");                     // all mode: a falsy element returns 0
    emitter.instruction("jmp __rt_array_find_any_all_next");                    // all mode: keep checking
    emitter.label("__rt_array_find_any_all_any");
    emitter.instruction("test rax, rax");                                       // any mode: is this element truthy?
    emitter.instruction("jnz __rt_array_find_any_all_one");                     // any mode: a truthy element returns 1
    emitter.instruction("jmp __rt_array_find_any_all_next");                    // any mode: keep checking
    emitter.label("__rt_array_find_any_all_find");
    emitter.instruction("test rax, rax");                                       // find mode: is this element truthy?
    emitter.instruction("jz __rt_array_find_any_all_next");                     // find mode: skip falsy elements
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // value_type tag for boxing the found element
    emitter.instruction("mov rdi, r13");                                        // found element low word
    emitter.instruction("xor esi, esi");                                        // found element high word unused
    emitter.instruction("call __rt_mixed_from_value");                          // box the found element as a Mixed value
    emitter.instruction("jmp __rt_array_find_any_all_ret");                     // return the boxed element
    emitter.label("__rt_array_find_any_all_next");
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // reload the index
    emitter.instruction("add rax, 1");                                          // advance to the next element
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // save the advanced index
    emitter.instruction("jmp __rt_array_find_any_all_loop");                    // continue the predicate loop
    emitter.label("__rt_array_find_any_all_end");
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // reload the mode selector
    emitter.instruction("test r11, r11");                                       // find mode found nothing?
    emitter.instruction("jz __rt_array_find_any_all_findnull");                 // find returns boxed null
    emitter.instruction("cmp r11, 1");                                          // any mode?
    emitter.instruction("je __rt_array_find_any_all_zero");                     // any matched nothing: return 0
    emitter.instruction("jmp __rt_array_find_any_all_one");                     // all elements passed: return 1
    emitter.label("__rt_array_find_any_all_findnull");
    emitter.instruction("mov rdi, 0x7ffffffffffffffe");                         // value low word = shared null sentinel
    emitter.instruction("xor esi, esi");                                        // value high word unused
    emitter.instruction("mov rax, 8");                                          // value_tag 8 = null
    emitter.instruction("call __rt_mixed_from_value");                          // box the null sentinel
    emitter.instruction("jmp __rt_array_find_any_all_ret");                     // return the boxed null
    emitter.label("__rt_array_find_any_all_one");
    emitter.instruction("mov rax, 1");                                          // boolean result: true
    emitter.instruction("jmp __rt_array_find_any_all_ret");                     // return the boolean result
    emitter.label("__rt_array_find_any_all_zero");
    emitter.instruction("xor eax, eax");                                        // boolean result: false
    emitter.label("__rt_array_find_any_all_ret");
    emitter.instruction("add rsp, 40");                                         // release the local slots
    emitter.instruction("pop r14");                                             // restore the environment register
    emitter.instruction("pop r13");                                             // restore the element register
    emitter.instruction("pop r12");                                             // restore the callback register
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the result in rax
}

