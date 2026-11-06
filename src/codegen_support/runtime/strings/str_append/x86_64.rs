//! Purpose:
//! Emits the x86_64 consuming string append implementation.
//!
//! Called from:
//! - `super::emit_str_append` for matching supported targets.
//!
//! Key details:
//! - Header size is capacity, logical length stays in the returned string pair.
//! - The old owner survives all allocation and copying until the replacement is ready.

use crate::codegen_support::emit::Emitter;

/// Emits the target ABI helper while preserving frame alignment across nested calls.
pub(super) fn emit(emitter: &mut Emitter, mbstring: bool) {
    emitter.blank();
    emitter.comment("--- runtime: consuming string append ---");
    // -- set up the frame and preserve the input owner --
    emitter.label_global("__rt_str_append");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish stable spill addresses
    emitter.instruction("sub rsp, 64");                                         // keep nested calls aligned and reserve eight spill words
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the owner being consumed
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save its logical byte length
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // save the borrowed RHS pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rsi");                       // save the RHS byte length
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // initially return the unchanged destination
    // -- validate the new length before reading payload bytes --
    emitter.instruction("add rdx, rsi");                                        // compute the replacement length before reading any payload
    emitter.instruction("jc __rt_str_append_overflow");                         // reject unsigned length wrap
    emitter.instruction("mov r8, 0xfffffff8");                                  // largest aligned payload representable by the heap header
    emitter.instruction("cmp rdx, r8");                                         // check the requested length against the allocator's size contract
    emitter.instruction("ja __rt_str_append_overflow");                         // reject unrepresentable lengths before header or payload access
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // preserve the replacement logical length
    emitter.instruction("test rsi, rsi");                                       // appending zero bytes changes neither ownership nor metadata
    emitter.instruction("jz __rt_str_append_done");                             // return the original string without copying
    // -- check ownership, sharing and RHS overlap --
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // nonowned inputs have no reusable capacity
    emitter.instruction("call __rt_heap_kind");                                 // classify the original pointer without dereferencing foreign memory
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // remember whether an old string owner must be retired
    emitter.instruction("cmp eax, 1");                                          // only ordinary owned strings permit destructive append
    emitter.instruction("jne __rt_str_append_allocate");                        // copy literals, scratch, views and other nonowned inputs
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // reload the old payload pointer
    emitter.instruction("mov ecx, DWORD PTR [r8 - 16]");                        // read the allocator's payload capacity
    emitter.instruction("mov QWORD PTR [rbp - 64], rcx");                       // keep capacity available for geometric growth
    emitter.instruction("cmp DWORD PTR [r8 - 12], 1");                          // an in-place write requires the sole runtime reference
    emitter.instruction("jne __rt_str_append_copy");                            // shared copies must base growth on logical length
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // examine the borrowed RHS interval
    emitter.instruction("mov r10, r9");                                         // start its end-address calculation
    emitter.instruction("add r10, QWORD PTR [rbp - 32]");                       // compute the exclusive RHS end
    emitter.instruction("jc __rt_str_append_overflow");                         // reject a wrapped input interval before copying
    emitter.instruction("lea r11, [r8 + rcx]");                                 // compute the old allocation's exclusive end
    emitter.instruction("cmp r9, r11");                                         // does RHS start beyond the allocation?
    emitter.instruction("jae __rt_str_append_capacity");                        // nonoverlapping RHS can use spare capacity
    emitter.instruction("cmp r10, r8");                                         // does RHS end before the allocation starts?
    emitter.instruction("ja __rt_str_append_copy");                             // overlapping RHS must survive independent copying
    // -- reuse spare capacity only for a unique disjoint owner --
    emitter.label("__rt_str_append_capacity");
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // reload the replacement logical length
    emitter.instruction("cmp rdx, rcx");                                        // does the unique allocation already have enough capacity?
    emitter.instruction("ja __rt_str_append_allocate");                         // otherwise reserve geometrically larger storage
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // the existing owner is reused rather than retired
    emitter.instruction("jmp __rt_str_append_prepare");                         // detach metadata before changing any existing bytes
    // -- choose geometric capacity and allocate before retiring the owner --
    emitter.label("__rt_str_append_copy");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // bound copied storage by used bytes rather than old spare capacity
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // avoid exponential reservation under repeated sharing
    emitter.label("__rt_str_append_allocate");
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // seed requested capacity from the prior allocation
    emitter.instruction("add rax, rax");                                        // double capacity to amortize repeated appends
    emitter.instruction("mov r8, 0xfffffff8");                                  // materialize the maximum aligned heap payload
    emitter.instruction("cmp rax, r8");                                         // could doubling exceed the header's representable range?
    emitter.instruction("jbe __rt_str_append_choose_capacity");                 // keep representable geometric growth
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // near the size limit, reserve only the checked requirement
    emitter.label("__rt_str_append_choose_capacity");
    emitter.instruction("cmp rax, QWORD PTR [rbp - 40]");                       // compare growth against the actual required length
    emitter.instruction("jae __rt_str_append_min_capacity");                    // preserve capacity when growth is sufficient
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // otherwise reserve the required length
    emitter.label("__rt_str_append_min_capacity");
    emitter.instruction("cmp rax, 16");                                         // avoid a series of tiny initial allocations
    emitter.instruction("jae __rt_str_append_reserve");                         // capacities of at least sixteen bytes are ready
    emitter.instruction("mov rax, 16");                                         // start small strings with sixteen bytes of capacity
    emitter.label("__rt_str_append_reserve");
    emitter.instruction("call __rt_heap_alloc");                                // allocate before consuming any old ownership
    emitter.instruction(&format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(1))); // stamp ordinary string ownership with the x86 heap marker
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // publish the new allocation's string kind
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // preserve the replacement payload across further calls
    // -- detach lazy metadata before mutating the result bytes --
    emitter.label("__rt_str_append_prepare");
    if mbstring {
        emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                   // select the changed string identity before any byte store
        emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                   // provide its complete new logical length without reading bytes
        emitter.instruction("call __rt_mbstring_ini_fresh");                    // detach the old lazy origin before payload mutation
    }
    emitter.instruction("mov r8, QWORD PTR [rbp - 48]");                        // recover the result pointer after optional metadata calls
    emitter.instruction("cmp r8, QWORD PTR [rbp - 8]");                         // distinguish unique reuse from independent replacement
    emitter.instruction("jne __rt_str_append_prefix_setup");                    // a replacement needs the old prefix copied first
    emitter.instruction("add r8, QWORD PTR [rbp - 16]");                        // reuse starts writing after the preserved old prefix
    emitter.instruction("jmp __rt_str_append_right_setup");                     // only RHS bytes need writing in the reused buffer
    // -- copy the prefix when a replacement allocation is required --
    emitter.label("__rt_str_append_prefix_setup");
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // load the old prefix source
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // load the prefix byte count
    emitter.label("__rt_str_append_left");
    emitter.instruction("test rcx, rcx");                                       // check whether the entire old prefix has been copied
    emitter.instruction("jz __rt_str_append_right_setup");                      // proceed to RHS after finishing the prefix
    emitter.instruction("mov r10b, BYTE PTR [r9]");                             // read the next old byte
    emitter.instruction("mov BYTE PTR [r8], r10b");                             // initialize its replacement byte
    emitter.instruction("inc r9");                                              // advance the prefix source
    emitter.instruction("inc r8");                                              // advance the destination
    emitter.instruction("dec rcx");                                             // consume one prefix byte
    emitter.instruction("jmp __rt_str_append_left");                            // continue copying the prefix
    // -- append the complete borrowed RHS including embedded NUL bytes --
    emitter.label("__rt_str_append_right_setup");
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // load the borrowed RHS source
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // load the new-byte count
    emitter.label("__rt_str_append_right");
    emitter.instruction("test rcx, rcx");                                       // check whether all RHS bytes have been copied
    emitter.instruction("jz __rt_str_append_publish");                          // publish only after the complete result is initialized
    emitter.instruction("mov r10b, BYTE PTR [r9]");                             // read the next RHS byte, including embedded NULs
    emitter.instruction("mov BYTE PTR [r8], r10b");                             // append that byte without interpreting its value
    emitter.instruction("inc r9");                                              // advance the RHS source
    emitter.instruction("inc r8");                                              // advance the destination
    emitter.instruction("dec rcx");                                             // consume one RHS byte
    emitter.instruction("jmp __rt_str_append_right");                           // continue copying the RHS
    // -- retire the consumed owner only after the replacement is complete --
    emitter.label("__rt_str_append_publish");
    emitter.instruction("cmp QWORD PTR [rbp - 56], 1");                         // does replacement consume an old managed string reference?
    emitter.instruction("jne __rt_str_append_done");                            // literals and reused unique owners require no release
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the original managed string owner
    emitter.instruction("call __rt_decref_any");                                // retire exactly that reference after copying and metadata publication
    // -- return the replacement pair and restore the caller frame --
    emitter.label("__rt_str_append_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // return the reused or replacement payload
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // return the checked logical byte length
    emitter.instruction("add rsp, 64");                                         // release helper spill storage
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the replacement owner for the adjacent local store
    // -- forward invalid lengths to the allocation failure boundary --
    emitter.label("__rt_str_append_overflow");
    emitter.instruction("jmp __rt_alloc_overflow");                             // enter the existing fatal or recoverable native allocation boundary
}
