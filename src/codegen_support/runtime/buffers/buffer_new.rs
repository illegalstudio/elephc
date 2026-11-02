//! Purpose:
//! Emits allocation for generation-safe Buffer handles and their separate heap payloads.
//! The static descriptor registry is the sole owner of buffer metadata.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `buffers`.
//!
//! Key details:
//! - Descriptor index zero and generation zero are invalid public-handle components.
//! - Only the exact requested payload bytes are cleared; allocator rounding is never exposed.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::runtime::data::BUFFER_ALLOC_SIZE_MSG;

use super::{BUFFER_DESCRIPTOR_SIZE, BUFFER_REGISTRY_CAPACITY};

/// Emits `__rt_buffer_new` for the active target.
///
/// Inputs are `x0`/`x1` on ARM64 and `rax`/`rdi` on x86_64 for length/stride.
/// Returns `(generation << 32) | index`; allocation exhaustion and size overflow
/// transfer to the non-returning buffer allocation diagnostic.
pub fn emit_buffer_new(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_buffer_new_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: buffer_new ---");
    emitter.label_global("__rt_buffer_new");

    // -- preserve allocation inputs and descriptor state --
    emitter.instruction("sub sp, sp, #64");                                     // reserve an aligned frame for arguments, descriptor state, and payload pointer
    emitter.instruction("stp x29, x30, [sp, #48]");                             // preserve the caller frame chain and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the temporary frame pointer
    emitter.instruction("str x0, [sp]");                                        // retain logical length across helper calls
    emitter.instruction("str x1, [sp, #8]");                                    // retain element stride across helper calls
    emitter.instruction("cmp x0, #0");                                          // reject a negative logical length before unsigned multiplication
    emitter.instruction("b.lt __rt_buffer_new_size_fail");                      // report invalid signed lengths through the strict buffer diagnostic
    emitter.instruction("umulh x3, x0, x1");                                    // detect a product that cannot fit in the payload-size word
    emitter.instruction("cbz x3, __rt_buffer_new_size_fits");                   // continue when len multiplied by stride fits exactly
    emitter.instruction("b __rt_buffer_new_size_fail");                         // reject a wrapped payload allocation size
    emitter.label("__rt_buffer_new_size_fits");
    emitter.instruction("mul x2, x0, x1");                                      // compute the exact requested payload byte count
    emitter.instruction("tbnz x2, #63, __rt_buffer_new_size_fail");             // reject a product outside the allocator's signed size domain
    emitter.instruction("lsr x3, x2, #32");                                     // check the allocator's 32-bit payload-size metadata boundary
    emitter.instruction("cbnz x3, __rt_buffer_new_size_fail");                  // reject payloads the heap header cannot represent exactly
    emitter.instruction("str x2, [sp, #16]");                                   // retain byte count for exact zero filling after allocation

    // -- take an eligible recycled descriptor or allocate the next slot --
    emitter.label("__rt_buffer_new_take_free");
    abi::emit_symbol_address(emitter, "x9", "_buffer_registry_free");
    emitter.instruction("ldr x10, [x9]");                                       // load the first recycled descriptor index, if any
    emitter.instruction("cbz x10, __rt_buffer_new_take_next");                  // allocate from the monotonic index when no reusable slot remains
    abi::emit_symbol_address(emitter, "x11", "_buffer_registry");
    emitter.instruction(&format!("mov x12, #{}", BUFFER_DESCRIPTOR_SIZE));      // materialize the fixed descriptor byte stride
    emitter.instruction("madd x11, x10, x12, x11");                             // resolve recycled index to its static descriptor
    emitter.instruction("ldr w12, [x11, #24]");                                 // inspect the prior u32 generation before incrementing it
    emitter.instruction("cmn w12, #1");                                         // detect u32::MAX without allowing generation wraparound
    emitter.instruction("b.eq __rt_buffer_new_retire_free");                    // permanently remove saturated generations from the free list
    emitter.instruction("ldr x13, [x11, #40]");                                 // load the successor before reusing the selected descriptor
    abi::emit_symbol_address(emitter, "x9", "_buffer_registry_free");
    emitter.instruction("str x13, [x9]");                                       // unlink the descriptor so one allocation owns this lifetime
    emitter.instruction("b __rt_buffer_new_slot_ready");                        // continue with the selected non-saturated descriptor
    emitter.label("__rt_buffer_new_retire_free");
    emitter.instruction("ldr x13, [x11, #40]");                                 // preserve the next candidate while retiring the saturated slot
    abi::emit_symbol_address(emitter, "x9", "_buffer_registry_free");
    emitter.instruction("str x13, [x9]");                                       // drop the saturated descriptor from future allocation attempts
    emitter.instruction("b __rt_buffer_new_take_free");                         // keep scanning until a non-saturated free slot is found
    emitter.label("__rt_buffer_new_take_next");
    abi::emit_symbol_address(emitter, "x9", "_buffer_registry_next");
    emitter.instruction("ldr x10, [x9]");                                       // load the first never-issued descriptor index
    emitter.instruction(&format!("mov x12, #{}", BUFFER_REGISTRY_CAPACITY));    // materialize the maximum usable descriptor index
    emitter.instruction("cmp x10, x12");                                        // ensure the static registry still has a never-used slot
    emitter.instruction("b.hi __rt_buffer_new_exhausted");                      // branch locally when no never-used descriptor remains
    emitter.instruction("add x12, x10, #1");                                    // advance the monotonic index before publishing this allocation
    emitter.instruction("str x12, [x9]");                                       // reserve this virgin slot against future allocations
    abi::emit_symbol_address(emitter, "x11", "_buffer_registry");
    emitter.instruction(&format!("mov x12, #{}", BUFFER_DESCRIPTOR_SIZE));      // materialize the fixed descriptor byte stride
    emitter.instruction("madd x11, x10, x12, x11");                             // resolve the fresh index to its static descriptor
    emitter.label("__rt_buffer_new_slot_ready");
    emitter.instruction("str x10, [sp, #24]");                                  // retain descriptor index for final handle packing
    emitter.instruction("str x11, [sp, #32]");                                  // retain descriptor address across heap allocation
    emitter.instruction("ldr w12, [x11, #24]");                                 // load the previous u32 generation from the selected descriptor
    emitter.instruction("add w12, w12, #1");                                    // assign the next non-zero generation for this slot lifetime
    emitter.instruction("str x12, [x11, #24]");                                 // install generation before the descriptor becomes active

    // -- allocate and exactly zero only the backing payload --
    emitter.instruction("ldr x0, [sp, #16]");                                   // pass exactly len multiplied by stride to the heap allocator
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate payload storage without embedding metadata in the heap block
    emitter.instruction("str x0, [sp, #40]");                                   // retain payload pointer across descriptor publication
    emitter.instruction("ldr x12, [sp, #16]");                                  // reload exact requested byte count for zero filling
    emitter.instruction("mov x11, x0");                                         // start zero-fill cursor at the payload base
    emitter.label("__rt_buffer_new_zero_words");
    emitter.instruction("cmp x12, #8");                                         // determine whether a full machine word remains inside the payload
    emitter.instruction("b.lo __rt_buffer_new_zero_tail");                      // finish remaining sub-word bytes without overrun
    emitter.instruction("str xzr, [x11], #8");                                  // clear one complete payload word and advance the cursor
    emitter.instruction("sub x12, x12, #8");                                    // account for the cleared word
    emitter.instruction("b __rt_buffer_new_zero_words");                        // continue while full words remain
    emitter.label("__rt_buffer_new_zero_tail");
    emitter.instruction("cbz x12, __rt_buffer_new_publish");                    // skip byte stores once the exact request has been cleared
    emitter.instruction("strb wzr, [x11], #1");                                 // clear one final payload byte without touching allocator padding
    emitter.instruction("sub x12, x12, #1");                                    // account for the final byte store
    emitter.instruction("b __rt_buffer_new_zero_tail");                         // clear every residual byte exactly once

    // -- publish descriptor fields and return the opaque scalar handle --
    emitter.label("__rt_buffer_new_publish");
    emitter.instruction("ldr x11, [sp, #32]");                                  // recover the selected static descriptor
    emitter.instruction("ldr x9, [sp, #40]");                                   // recover the allocated payload pointer
    emitter.instruction("str x9, [x11]");                                       // descriptor payload pointer at offset zero
    emitter.instruction("ldr x9, [sp]");                                        // reload logical length from the saved caller input
    emitter.instruction("str x9, [x11, #8]");                                   // descriptor logical length at offset eight
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload element stride from the saved caller input
    emitter.instruction("str x9, [x11, #16]");                                  // descriptor element stride at offset sixteen
    emitter.instruction("str xzr, [x11, #40]");                                 // clear stale free-list linkage before publishing the live descriptor
    emitter.instruction("mov x9, #1");                                          // materialize the active publication marker
    emitter.instruction("str x9, [x11, #32]");                                  // publish the fully initialized descriptor as live
    emitter.instruction("ldr x0, [x11, #24]");                                  // load this lifetime generation for public-handle packing
    emitter.instruction("lsl x0, x0, #32");                                     // position generation in the high u32 of the scalar handle
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the selected descriptor index
    emitter.instruction("orr x0, x0, x9");                                      // combine generation and index into the opaque buffer handle
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore caller frame state after successful allocation
    emitter.instruction("add sp, sp, #64");                                     // release the temporary allocation frame
    emitter.instruction("ret");                                                 // return the generation-safe scalar buffer handle
    emitter.label("__rt_buffer_new_exhausted");
    emitter.instruction("b __rt_buffer_registry_exhausted");                    // report that the finite descriptor registry has no issuable slot

    // -- fatal error: requested buffer length cannot be represented --
    emitter.label("__rt_buffer_new_size_fail");
    if emitter.phpext_output { abi::emit_cdylib_exit_escape(emitter); }
    emitter.instruction("mov x0, #2");                                          // fd = stderr
    abi::emit_symbol_address(emitter, "x1", "_buffer_alloc_size_msg");
    emitter.instruction(&format!("mov x2, #{}", BUFFER_ALLOC_SIZE_MSG.len()));  // pass the exact buffer-length diagnostic byte count
    emitter.syscall(4);
    abi::emit_cdylib_exit_escape(emitter);
    emitter.instruction("mov x0, #1");                                          // exit code 1
    emitter.syscall(1);
}

/// Emits the Linux x86_64 variant of `__rt_buffer_new`.
/// Uses `rax` for length/result and `rdi` for stride as required by buffer lowering.
fn emit_buffer_new_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: buffer_new ---");
    emitter.label_global("__rt_buffer_new");

    // -- preserve allocation inputs and descriptor state --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving an aligned local frame
    emitter.instruction("mov rbp, rsp");                                        // establish a stable base for allocation state spills
    emitter.instruction("sub rsp, 64");                                         // reserve aligned slots for arguments, descriptor state, and payload pointer
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // retain logical length across the nested heap allocation
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // retain element stride across the nested heap allocation
    emitter.instruction("test rax, rax");                                       // reject a negative logical length before unsigned multiplication
    emitter.instruction("js __rt_buffer_new_size_fail");                        // report invalid signed lengths through the strict buffer diagnostic
    emitter.instruction("mul rdi");                                             // calculate unsigned rdx:rax = length multiplied by stride
    emitter.instruction("test rdx, rdx");                                       // detect a payload byte count that overflowed the machine word
    emitter.instruction("jnz __rt_buffer_new_size_fail");                       // reject a wrapped payload allocation size
    emitter.instruction("test rax, rax");                                       // inspect the product's sign bit before heap-size accounting
    emitter.instruction("js __rt_buffer_new_size_fail");                        // reject a product outside the allocator's signed size domain
    emitter.instruction("mov r10d, 0xffffffff");                                // materialize u32::MAX without sign-extending the comparison operand
    emitter.instruction("cmp rax, r10");                                        // check the allocator's 32-bit payload-size metadata boundary
    emitter.instruction("ja __rt_buffer_new_size_fail");                        // reject payloads the heap header cannot represent exactly
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // retain exact payload bytes for allocation and zero filling

    // -- take an eligible recycled descriptor or allocate the next slot --
    abi::emit_symbol_address(emitter, "r11", "_buffer_registry_free");
    emitter.instruction("mov QWORD PTR [rbp - 56], r11");                       // retain the free-list head address while scratch registers are reused
    emitter.label("__rt_buffer_new_take_free_x");
    emitter.instruction("mov r10, QWORD PTR [r11]");                            // load the first recycled descriptor index, if any
    emitter.instruction("test r10, r10");                                       // distinguish an empty recycled list from a candidate index
    emitter.instruction("jz __rt_buffer_new_take_next_x");                      // allocate from the monotonic index when no reusable slot remains
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // retain recycled descriptor index before scaling it to a byte offset
    abi::emit_symbol_address(emitter, "r11", "_buffer_registry");
    emitter.instruction(&format!("imul r10, {}", BUFFER_DESCRIPTOR_SIZE));      // scale the recycled index by the descriptor byte stride
    emitter.instruction("add r11, r10");                                        // resolve recycled index to its static descriptor
    emitter.instruction("mov r10d, DWORD PTR [r11 + 24]");                      // inspect the prior u32 generation before incrementing it
    emitter.instruction("cmp r10d, -1");                                        // detect u32::MAX without permitting generation wraparound
    emitter.instruction("je __rt_buffer_new_retire_free_x");                    // permanently remove saturated generations from the free list
    emitter.instruction("mov r10, QWORD PTR [r11 + 40]");                       // load the successor before reusing the selected descriptor
    emitter.instruction("mov rcx, QWORD PTR [rbp - 56]");                       // recover the free-list head address after scratch use
    emitter.instruction("mov QWORD PTR [rcx], r10");                            // unlink the descriptor so one allocation owns this lifetime
    emitter.instruction("jmp __rt_buffer_new_slot_ready_x");                    // continue with the selected non-saturated descriptor
    emitter.label("__rt_buffer_new_retire_free_x");
    emitter.instruction("mov r10, QWORD PTR [r11 + 40]");                       // preserve the successor while retiring the saturated descriptor
    emitter.instruction("mov rcx, QWORD PTR [rbp - 56]");                       // recover the free-list head address for unlinking
    emitter.instruction("mov QWORD PTR [rcx], r10");                            // drop the saturated descriptor from future allocation attempts
    emitter.instruction("mov r11, rcx");                                        // restore the list-head register for the next candidate load
    emitter.instruction("jmp __rt_buffer_new_take_free_x");                     // keep scanning until a non-saturated free slot is found
    emitter.label("__rt_buffer_new_take_next_x");
    abi::emit_symbol_address(emitter, "r11", "_buffer_registry_next");
    emitter.instruction("mov r10, QWORD PTR [r11]");                            // load the first never-issued descriptor index
    emitter.instruction(&format!("cmp r10, {}", BUFFER_REGISTRY_CAPACITY));     // ensure the static registry still has a virgin slot
    emitter.instruction("ja __rt_buffer_registry_exhausted");                   // report that the finite descriptor registry has no issuable slot
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // retain fresh descriptor index before scaling it to a byte offset
    emitter.instruction("lea rcx, [r10 + 1]");                                  // compute the next never-issued index before publishing this allocation
    emitter.instruction("mov QWORD PTR [r11], rcx");                            // reserve this virgin slot against future allocations
    abi::emit_symbol_address(emitter, "r11", "_buffer_registry");
    emitter.instruction(&format!("imul r10, {}", BUFFER_DESCRIPTOR_SIZE));      // scale the fresh index by the descriptor byte stride
    emitter.instruction("add r11, r10");                                        // resolve fresh index to its static descriptor
    emitter.label("__rt_buffer_new_slot_ready_x");
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // retain descriptor address across payload allocation
    emitter.instruction("mov ecx, DWORD PTR [r11 + 24]");                       // load the previous u32 generation from the selected descriptor
    emitter.instruction("add ecx, 1");                                          // assign the next non-zero generation for this slot lifetime
    emitter.instruction("mov QWORD PTR [r11 + 24], rcx");                       // install generation before the descriptor becomes active

    // -- allocate and exactly zero only the backing payload --
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // pass exactly len multiplied by stride to the heap allocator
    emitter.instruction("call __rt_heap_alloc");                                // allocate payload storage without embedding metadata in the heap block
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // retain payload pointer across descriptor publication
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // reload exact requested byte count for zero filling
    emitter.instruction("mov r11, rax");                                        // start zero-fill cursor at the payload base
    emitter.label("__rt_buffer_new_zero_words_x");
    emitter.instruction("cmp rcx, 8");                                          // determine whether a full machine word remains inside the payload
    emitter.instruction("jb __rt_buffer_new_zero_tail_x");                      // finish remaining sub-word bytes without overrun
    emitter.instruction("mov QWORD PTR [r11], 0");                              // clear one complete payload word
    emitter.instruction("add r11, 8");                                          // advance the cursor after the complete word store
    emitter.instruction("sub rcx, 8");                                          // account for the cleared word
    emitter.instruction("jmp __rt_buffer_new_zero_words_x");                    // continue while full words remain
    emitter.label("__rt_buffer_new_zero_tail_x");
    emitter.instruction("test rcx, rcx");                                       // determine whether exact residual bytes remain to be cleared
    emitter.instruction("jz __rt_buffer_new_publish_x");                        // skip byte stores once the exact request has been cleared
    emitter.instruction("mov BYTE PTR [r11], 0");                               // clear one final payload byte without touching allocator padding
    emitter.instruction("inc r11");                                             // advance the cursor by the single cleared byte
    emitter.instruction("dec rcx");                                             // account for the final byte store
    emitter.instruction("jmp __rt_buffer_new_zero_tail_x");                     // clear every residual byte exactly once

    // -- publish descriptor fields and return the opaque scalar handle --
    emitter.label("__rt_buffer_new_publish_x");
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // recover the selected static descriptor
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // recover the allocated payload pointer
    emitter.instruction("mov QWORD PTR [r11], r10");                            // descriptor payload pointer at offset zero
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload logical length from the saved caller input
    emitter.instruction("mov QWORD PTR [r11 + 8], r10");                        // descriptor logical length at offset eight
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload element stride from the saved caller input
    emitter.instruction("mov QWORD PTR [r11 + 16], r10");                       // descriptor element stride at offset sixteen
    emitter.instruction("mov QWORD PTR [r11 + 40], 0");                         // clear stale free-list linkage before publishing the live descriptor
    emitter.instruction("mov QWORD PTR [r11 + 32], 1");                         // publish the fully initialized descriptor as live
    emitter.instruction("mov rax, QWORD PTR [r11 + 24]");                       // load this lifetime generation for public-handle packing
    emitter.instruction("shl rax, 32");                                         // position generation in the high u32 of the scalar handle
    emitter.instruction("or rax, QWORD PTR [rbp - 32]");                        // combine generation and index into the opaque buffer handle
    emitter.instruction("add rsp, 64");                                         // release the temporary allocation frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.instruction("ret");                                                 // return the generation-safe scalar buffer handle

    // -- fatal error: requested buffer length cannot be represented --
    emitter.label("__rt_buffer_new_size_fail");
    if emitter.phpext_output { abi::emit_cdylib_exit_escape(emitter); }
    emitter.instruction("mov edi, 2");                                          // fd = stderr for the buffer-length fatal error message
    abi::emit_symbol_address(emitter, "rsi", "_buffer_alloc_size_msg");
    emitter.instruction(&format!("mov edx, {}", BUFFER_ALLOC_SIZE_MSG.len()));  // pass the exact buffer-length diagnostic byte count
    emitter.instruction("mov eax, 1");                                          // Linux x86_64 syscall 1 = write
    emitter.instruction("syscall");                                             // print the fatal buffer-length message to stderr
    abi::emit_cdylib_exit_escape(emitter);
    emitter.instruction("mov edi, 1");                                          // exit code 1 for an unrepresentable buffer length
    emitter.instruction("mov eax, 60");                                         // Linux x86_64 syscall 60 = exit
    emitter.instruction("syscall");                                             // terminate the process after reporting the buffer-length failure
}
