//! Purpose:
//! Emits the `__rt_array_filter_int_direct` runtime helper for pure int predicates.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::managed::emit_runtime()`.
//!
//! Key details:
//! - The source is a packed `array<int>`; the callback is a capture-free closure with an
//!   `int` parameter and a `bool` return, called through its raw entry point.
//! - Every kept entry is inserted into a hash under its original integer key, preserving
//!   PHP's key-keeping `array_filter()` semantics without boxing the int values.
//! - The caller boxes the returned hash once; exceptional cleanup is unnecessary because the
//!   lowering admits only predicates whose EIR effects cannot throw, warn, or mutate state.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the direct int filter helper for the active target.
///
/// AArch64: x0 = callback entry, x1 = source array, x2 = ignored environment; x0 = hash.
/// x86_64: rdi = callback entry, rsi = source array, rdx = ignored environment; rax = hash.
pub fn emit_array_filter_int_direct(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_filter_int_direct_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_filter_int_direct ---");
    emitter.label_global("__rt_array_filter_int_direct");
    emitter.instruction("sub sp, sp, #64");                                     // reserve callback, cursor, hash, source, and length slots
    emitter.instruction("stp x29, x30, [sp, #48]");                             // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #48");                                    // establish a stable frame base
    emitter.instruction("stp x19, x20, [sp, #0]");                              // save the callback and loop cursor
    emitter.instruction("stp x21, x22, [sp, #16]");                             // save the hash and source array
    emitter.instruction("str x23, [sp, #32]");                                  // save the source length
    emitter.instruction("mov x19, x0");                                         // keep the predicate entry across hash calls
    emitter.instruction("mov x22, x1");                                         // keep the packed source array
    emitter.instruction("ldr x23, [x1]");                                       // read the source length
    emitter.instruction("mov x0, #4");                                          // start with a small hash capacity
    emitter.instruction("bl __rt_hash_new");                                    // allocate the key-preserving result hash
    emitter.instruction("mov x21, x0");                                         // keep the hash across predicate calls
    emitter.instruction("mov x20, #0");                                         // start at the first source element

    emitter.label("__rt_array_filter_int_direct_loop");
    emitter.instruction("cmp x20, x23");                                        // has every source element been visited?
    emitter.instruction("b.ge __rt_array_filter_int_direct_done");              // finish once the source is exhausted
    emitter.instruction("add x9, x22, #24");                                    // address the packed source payload
    emitter.instruction("ldr x0, [x9, x20, lsl #3]");                           // load the current int element
    emitter.instruction("blr x19");                                             // call the raw bool predicate
    emitter.instruction("cbz x0, __rt_array_filter_int_direct_next");           // skip elements the predicate rejects
    emitter.instruction("mov x0, x21");                                         // pass the result hash
    emitter.instruction("mov x1, x20");                                         // preserve the original integer key
    emitter.instruction("mov x2, #-1");                                         // mark the key as an integer key
    emitter.instruction("add x9, x22, #24");                                    // address the packed source payload again
    emitter.instruction("ldr x3, [x9, x20, lsl #3]");                           // copy the kept int value
    emitter.instruction("mov x4, #0");                                          // int values use no high payload word
    emitter.instruction("mov x5, #0");                                          // runtime tag 0 = int
    emitter.instruction("bl __rt_hash_set");                                    // insert the kept entry without boxing its value
    emitter.instruction("mov x21, x0");                                         // a growing hash may have moved

    emitter.label("__rt_array_filter_int_direct_next");
    emitter.instruction("add x20, x20, #1");                                    // advance to the next source element
    emitter.instruction("b __rt_array_filter_int_direct_loop");                 // continue filtering

    emitter.label("__rt_array_filter_int_direct_done");
    emitter.instruction("mov x0, x21");                                         // return the owning result hash
    emitter.instruction("ldp x19, x20, [sp, #0]");                              // restore the callback and loop cursor
    emitter.instruction("ldp x21, x22, [sp, #16]");                             // restore the hash and source array
    emitter.instruction("ldr x23, [sp, #32]");                                  // restore the source length
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore the caller frame and return address
    emitter.instruction("add sp, sp, #64");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the packed result hash
}

/// Emits the direct int filter helper for Linux x86_64.
fn emit_array_filter_int_direct_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_filter_int_direct ---");
    emitter.label_global("__rt_array_filter_int_direct");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("push rbx");                                            // save the loop cursor
    emitter.instruction("push r12");                                            // save the predicate entry
    emitter.instruction("push r13");                                            // save the packed source array
    emitter.instruction("push r14");                                            // save the result hash
    emitter.instruction("push r15");                                            // save the source length
    emitter.instruction("sub rsp, 8");                                          // align the stack before helper calls
    emitter.instruction("mov r12, rdi");                                        // keep the predicate entry across hash calls
    emitter.instruction("mov r13, rsi");                                        // keep the packed source array
    emitter.instruction("mov r15, QWORD PTR [rsi]");                            // read the source length
    emitter.instruction("mov rdi, 4");                                          // start with a small hash capacity
    emitter.instruction("call __rt_hash_new");                                  // allocate the key-preserving result hash
    emitter.instruction("mov r14, rax");                                        // keep the hash across predicate calls
    emitter.instruction("xor ebx, ebx");                                        // start at the first source element

    emitter.label("__rt_array_filter_int_direct_loop_x86_64");
    emitter.instruction("cmp rbx, r15");                                        // has every source element been visited?
    emitter.instruction("jge __rt_array_filter_int_direct_done_x86_64");        // finish once the source is exhausted
    emitter.instruction("mov rax, QWORD PTR [r13 + rbx * 8 + 24]");             // load the current int element
    emitter.instruction("mov rdi, rax");                                        // pass the element to the raw predicate
    emitter.instruction("call r12");                                            // call the raw bool predicate
    emitter.instruction("test eax, eax");                                       // did the predicate keep this element?
    emitter.instruction("jz __rt_array_filter_int_direct_next_x86_64");         // skip rejected elements
    emitter.instruction("mov rdi, r14");                                        // pass the result hash
    emitter.instruction("mov rsi, rbx");                                        // preserve the original integer key
    emitter.instruction("mov rdx, -1");                                         // mark the key as an integer key
    emitter.instruction("mov rcx, QWORD PTR [r13 + rbx * 8 + 24]");             // copy the kept int value
    emitter.instruction("xor r8d, r8d");                                        // int values use no high payload word
    emitter.instruction("xor r9d, r9d");                                        // runtime tag 0 = int
    emitter.instruction("call __rt_hash_set");                                  // insert the kept entry without boxing its value
    emitter.instruction("mov r14, rax");                                        // a growing hash may have moved

    emitter.label("__rt_array_filter_int_direct_next_x86_64");
    emitter.instruction("inc rbx");                                             // advance to the next source element
    emitter.instruction("jmp __rt_array_filter_int_direct_loop_x86_64");        // continue filtering

    emitter.label("__rt_array_filter_int_direct_done_x86_64");
    emitter.instruction("mov rax, r14");                                        // return the owning result hash
    emitter.instruction("add rsp, 8");                                          // release the alignment slot
    emitter.instruction("pop r15");                                             // restore the source length
    emitter.instruction("pop r14");                                             // restore the result hash
    emitter.instruction("pop r13");                                             // restore the packed source array
    emitter.instruction("pop r12");                                             // restore the predicate entry
    emitter.instruction("pop rbx");                                             // restore the loop cursor
    emitter.instruction("leave");                                               // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the packed result hash
}
