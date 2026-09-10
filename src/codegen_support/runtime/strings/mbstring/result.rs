//! Purpose:
//! Transfers successful mbstring scalar and array results into owned runtime Mixed cells.
//!
//! Called from:
//! - Native mixed-return lowering and every boxed eval mbstring operation.
//!
//! Key details:
//! - The input is the status adapter's value/status/length/kind register tuple.
//! - Boxing copies strings or retains arrays, then consumes the temporary native owner.
//! - The returned cell uses the same boxed-value ABI as Magician runtime hooks.

use super::*;

/// Emits the common result boxer for the current supported target architecture.
pub(super) fn emit(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => aarch64(emitter),
        Arch::X86_64 => x86_64(emitter),
    }
}

/// Boxes the AArch64 result tuple and consumes any owned input string or array.
fn aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_mbstring_box_result");
    emitter.instruction("stp x29, x30, [sp, #-32]!");                           // preserve linkage and reserve two ownership slots
    emitter.instruction("mov x29, sp");                                         // establish the result boxer frame
    emitter.instruction(&format!("cmp x3, #{}", RESULT_STRING));                // distinguish owned strings from immediate scalar payloads
    emitter.instruction("csel x9, x0, xzr, eq");                                // retain only an owned string for eventual release
    emitter.instruction(&format!("cmp x3, #{}", RESULT_STRING_ARRAY));          // recognize a temporary owned array
    emitter.instruction("csel x9, x0, x9, eq");                                 // consume either heap-result owner after boxing
    emitter.instruction(&format!("cmp x3, #{}", RESULT_ARRAY));                 // recognize an owned associative-array graph result
    emitter.instruction("csel x9, x0, x9, eq");                                 // consume the graph root after boxed ownership is acquired
    emitter.instruction("str x9, [sp, #16]");                                   // save the native heap pointer across allocation
    emitter.instruction("mov x1, x0");                                          // pass the scalar or string pointer as the low boxed payload
    emitter.instruction(&format!("cmp x3, #{}", RESULT_STRING));                // select the string tag independently of optional ownership
    emitter.instruction("cset x0, eq");                                         // map integer and string outcomes to runtime tags zero and one
    emitter.instruction("mov x9, #8");                                          // prepare the runtime null tag
    emitter.instruction(&format!("cmp x3, #{}", RESULT_NULL));                  // distinguish null from false and integer zero
    emitter.instruction("csel x0, x9, x0, eq");                                 // retain the PHP null identity in the boxed result
    emitter.instruction("mov x9, #3");                                          // prepare the runtime boolean tag
    emitter.instruction(&format!("cmp x3, #{}", RESULT_BOOL));                  // recognize PHP boolean outcomes independently of their values
    emitter.instruction("csel x0, x9, x0, eq");                                 // select the boolean tag while preserving other scalar kinds
    emitter.instruction("mov x9, #4");                                          // prepare the runtime indexed-array tag
    emitter.instruction(&format!("cmp x3, #{}", RESULT_STRING_ARRAY));          // recognize a successful array result
    emitter.instruction("csel x0, x9, x0, eq");                                 // select array ownership while preserving scalar tags
    emitter.instruction("mov x9, #5");                                          // select the concrete associative-array tag
    emitter.instruction(&format!("cmp x3, #{}", RESULT_ARRAY));                 // recognize the restored graph storage representation
    emitter.instruction("csel x0, x9, x0, eq");                                 // preserve associative metadata in the boxed result
    emitter.instruction("bl __rt_mixed_from_value");                            // allocate one owned cell and copy strings or retain the array payload
    emitter.instruction("str x0, [sp, #24]");                                   // retain the fresh cell while consuming its source heap value
    emitter.instruction("ldr x0, [sp, #16]");                                   // load optional native heap ownership
    emitter.instruction("bl __rt_decref_any");                                  // release the source owner after the cell owns its required storage
    emitter.instruction("ldr x0, [sp, #24]");                                   // return the fresh cell to AOT or eval
    emitter.instruction("ldp x29, x30, [sp], #32");                             // release ownership slots and restore caller linkage
    emitter.instruction("ret");                                                 // transfer exactly one owned Mixed cell
}

/// Boxes the x86_64 result tuple and consumes any owned input string or array.
fn x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_mbstring_box_result");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish an aligned result boxer frame
    emitter.instruction("sub rsp, 16");                                         // reserve the native heap and boxed-result owners
    emitter.instruction("mov rdi, rax");                                        // pass the scalar or string pointer as the low boxed payload
    emitter.instruction("mov rsi, rcx");                                        // pass the string length or unused zero high payload
    emitter.instruction("xor r10d, r10d");                                      // prepare an empty optional string owner
    emitter.instruction(&format!("cmp r8, {}", RESULT_STRING));                 // distinguish owned strings from immediate scalar payloads
    emitter.instruction("cmove r10, rax");                                      // retain only an owned string for eventual release
    emitter.instruction(&format!("cmp r8, {}", RESULT_ARRAY));                  // recognize an owned associative-array graph result
    emitter.instruction("cmove r10, rax");                                      // consume the graph root after boxed ownership is acquired
    emitter.instruction(&format!("cmp r8, {}", RESULT_STRING_ARRAY));           // recognize a temporary owned array
    emitter.instruction("cmove r10, rax");                                      // consume either heap-result owner after boxing
    emitter.instruction("mov QWORD PTR [rsp], r10");                            // save the source heap value across cell allocation
    emitter.instruction(&format!("cmp r8, {}", RESULT_STRING));                 // select the string tag independently of optional ownership
    emitter.instruction("sete al");                                             // map integer and string outcomes to runtime tags zero and one
    emitter.instruction("movzx eax, al");                                       // clear previous payload bits from the runtime tag
    emitter.instruction("mov r10d, 8");                                         // prepare the runtime null tag
    emitter.instruction(&format!("cmp r8, {}", RESULT_NULL));                   // distinguish null from false and integer zero
    emitter.instruction("cmove rax, r10");                                      // retain the PHP null identity in the boxed result
    emitter.instruction("mov r10d, 3");                                         // prepare the runtime boolean tag
    emitter.instruction(&format!("cmp r8, {}", RESULT_BOOL));                   // recognize PHP boolean outcomes independently of their values
    emitter.instruction("cmove rax, r10");                                      // select the boolean tag while preserving other scalar kinds
    emitter.instruction("mov r10d, 4");                                         // prepare the runtime indexed-array tag
    emitter.instruction(&format!("cmp r8, {}", RESULT_STRING_ARRAY));           // recognize a successful array result
    emitter.instruction("cmove rax, r10");                                      // select array ownership while preserving scalar tags
    emitter.instruction("mov r10d, 5");                                         // select the concrete associative-array tag
    emitter.instruction(&format!("cmp r8, {}", RESULT_ARRAY));                  // recognize the restored graph storage representation
    emitter.instruction("cmove rax, r10");                                      // preserve associative metadata in the boxed result
    emitter.instruction("call __rt_mixed_from_value");                          // allocate one owned cell and copy strings or retain the array payload
    emitter.instruction("mov QWORD PTR [rsp + 8], rax");                        // retain the fresh cell while consuming its source heap value
    emitter.instruction("mov rax, QWORD PTR [rsp]");                            // load optional native heap ownership
    emitter.instruction("call __rt_decref_any");                                // release the source owner after the cell owns its required storage
    emitter.instruction("mov rax, QWORD PTR [rsp + 8]");                        // return the fresh cell to AOT or eval
    emitter.instruction("mov rsp, rbp");                                        // release both ownership slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // transfer exactly one owned Mixed cell
}
