//! Purpose:
//! Integrates raw integer token probing into the existing recursive JSON array parser.
//!
//! Called from:
//! - `super::arrays::emit_aarch64()` and `super::arrays::emit_x86_64()`.
//!
//! Key details:
//! - The fresh array remains unique and unpublished; existing widening transfers prefix ownership.
//! - AArch64 slots48/56 precede saved linkage64/72 in the80-byte array frame.
//! - x86_64 slots-56/-64 fit the64-byte array frame and never overlap existing parser state.

use crate::codegen_support::emit::Emitter;

/// Emits the AArch64 prefix path after the array parser saves the current token start.
pub(super) fn emit_aarch64(emitter: &mut Emitter) {
    // -- keep a fresh integer prefix packed until the first unsupported token --
    emitter.instruction("ldr x10, [sp, #24]");                                  // load the array under construction
    emitter.instruction("ldr x10, [x10, #-8]");                                 // inspect its runtime element representation
    emitter.instruction("and x10, x10, #0x7f00");                               // exclude the heap kind and COW bit from the type check
    emitter.instruction("cbnz x10, __rt_json_array_prefix_generic");            // widened arrays keep the ordinary recursive path
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the borrowed JSON slice
    emitter.instruction("add x1, x1, x9");                                      // address the first byte of this token
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the bounded slice length
    emitter.instruction("sub x2, x2, x9");                                      // expose only the unconsumed suffix to the probe
    emitter.instruction("bl __rt_json_integer_token");                          // classify an integer without boxing or recording an error
    emitter.instruction("cbz x0, __rt_json_array_prefix_widen");                // convert existing raw slots before decoding any other value
    emitter.instruction("str x1, [sp, #48]");                                   // preserve the accepted raw integer across runtime calls
    emitter.instruction("str x2, [sp, #56]");                                   // preserve the byte count before the separator
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the source pointer for the diagnostic candidate
    emitter.instruction("ldr x9, [sp, #32]");                                   // recover the original token start
    emitter.instruction("add x0, x0, x9");                                      // record the same token start as recursive decoding
    emitter.instruction("bl __rt_json_note_error_candidate");                   // publish the candidate in constant time after successful classification
    emitter.instruction("ldr x0, [sp, #24]");                                   // pass the unique array constructor owner
    emitter.instruction("ldr x1, [sp, #48]");                                   // pass the raw integer with no temporary Mixed cell
    emitter.instruction("bl __rt_array_push_int");                              // append or grow using the existing scalar ownership path
    emitter.instruction("str x0, [sp, #24]");                                   // preserve the possibly relocated array owner
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the original token start
    emitter.instruction("ldr x10, [sp, #56]");                                  // reload the consumed token and whitespace length
    emitter.instruction("add x9, x9, x10");                                     // advance directly to the unconsumed separator
    emitter.instruction("str x9, [sp, #16]");                                   // preserve the cursor for ordinary separator validation
    emitter.instruction("b __rt_json_array_prefix_separator");                  // bypass the generic boundary scanner and boxed decoder
    emitter.label("__rt_json_array_prefix_widen");
    emitter.instruction("ldr x0, [sp, #24]");                                   // recover the unexposed array owner and all accepted raw slots
    emitter.instruction("mov x1, #0");                                          // the source slots contain signed integers
    emitter.instruction("bl __rt_array_to_mixed");                              // box the existing prefix once using normal COW-aware conversion
    emitter.instruction("str x0, [sp, #24]");                                   // preserve the converted array owner
    emitter.label("__rt_json_array_prefix_generic");
    emitter.instruction("ldr x9, [sp, #16]");                                   // restore the token start clobbered by the probe or conversion
}

/// Emits the x86_64 prefix path after the array parser saves the current token start.
pub(super) fn emit_x86_64(emitter: &mut Emitter) {
    // -- keep a fresh integer prefix packed until the first unsupported token --
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // load the array under construction
    emitter.instruction("mov r8, QWORD PTR [r8 - 8]");                          // inspect its runtime element representation
    emitter.instruction("and r8, 0x7f00");                                      // exclude the heap kind and COW bit from the type check
    emitter.instruction("jnz __rt_json_array_prefix_generic_x");                // widened arrays keep the ordinary recursive path
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the borrowed JSON slice
    emitter.instruction("add rax, rcx");                                        // address the first byte of this token
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the bounded slice length
    emitter.instruction("sub rdx, rcx");                                        // expose only the unconsumed suffix to the probe
    emitter.instruction("call __rt_json_integer_token");                        // classify an integer without boxing or recording an error
    emitter.instruction("test rax, rax");                                       // inspect the classification result
    emitter.instruction("jz __rt_json_array_prefix_widen_x");                   // convert existing raw slots before decoding any other value
    emitter.instruction("mov QWORD PTR [rbp - 56], rdi");                       // preserve the accepted raw integer across runtime calls
    emitter.instruction("mov QWORD PTR [rbp - 64], rdx");                       // preserve the byte count before the separator
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the source pointer for the diagnostic candidate
    emitter.instruction("add rax, QWORD PTR [rbp - 40]");                       // record the same token start as recursive decoding
    emitter.instruction("call __rt_json_note_error_candidate");                 // publish the candidate in constant time after successful classification
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // pass the unique array constructor owner
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // pass the raw integer with no temporary Mixed cell
    emitter.instruction("call __rt_array_push_int");                            // append or grow using the existing scalar ownership path
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // preserve the possibly relocated array owner
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // reload the original token start
    emitter.instruction("add rcx, QWORD PTR [rbp - 64]");                       // advance directly to the unconsumed separator
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                       // preserve the cursor for ordinary separator validation
    emitter.instruction("jmp __rt_json_array_prefix_separator_x");              // bypass the generic boundary scanner and boxed decoder
    emitter.label("__rt_json_array_prefix_widen_x");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // recover the unexposed array owner and all accepted raw slots
    emitter.instruction("xor esi, esi");                                        // the source slots contain signed integers
    emitter.instruction("call __rt_array_to_mixed");                            // box the existing prefix once using normal COW-aware conversion
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // preserve the converted array owner
    emitter.label("__rt_json_array_prefix_generic_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // restore the source pointer clobbered by the probe or conversion
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // restore the original token start before generic scanning
}
