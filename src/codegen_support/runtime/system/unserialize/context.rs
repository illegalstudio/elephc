//! Purpose:
//! Emits per-call unserialize context lifecycle helpers for both supported architectures.
//!
//! Called from:
//! - `super::emit_unserialize()` between recursive decoding and object-storage helpers.
//!
//! Key details:
//! - Begin and end isolate nested calls by snapshotting policy, depth, and reference-registry state.

use crate::codegen_support::{abi, emit::Emitter};

/// Emits the AArch64 begin/end helpers that isolate one active unserialize call.
///
/// Nested calls snapshot the outer policy, parser depth, and populated reference
/// slots into a linked heap context. The end helper preserves the parsed result,
/// releases the current call's owned allow-list, and restores the outer context.
pub(super) fn emit_unserialize_context_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: unserialize context begin/end ---");
    emitter.label_global("__rt_unserialize_begin");
    emitter.instruction("sub sp, sp, #32");                                     // reserve spills plus an ABI-aligned helper frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // preserve the caller frame and return address across allocation
    emitter.instruction("add x29, sp, #16");                                    // establish a stable frame for snapshot sizing
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_unser_active");
    emitter.instruction("ldr x10, [x9]");                                       // load the active unserialize nesting count
    emitter.instruction("cbnz x10, __rt_unserialize_begin_nested");             // snapshot state only when a parser is already active
    emitter.instruction("mov x10, #1");                                         // mark the top-level parser active
    emitter.instruction("str x10, [x9]");                                       // publish the top-level nesting count
    emitter.instruction("b __rt_unserialize_begin_reset");                      // initialize the fresh per-call state

    emitter.label("__rt_unserialize_begin_nested");
    emitter.instruction("cmp x10, #256");                                       // has reentrant parser nesting reached its hard limit?
    emitter.instruction("b.lo __rt_unserialize_begin_nested_in_budget");        // keep the conditional branch inside the context helper atom
    emitter.instruction("b __rt_unser_depth_fatal");                            // reject another snapshot through the shared fatal path
    emitter.label("__rt_unserialize_begin_nested_in_budget");
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_unser_count");
    emitter.instruction("ldr x10, [x9]");                                       // load the outer registry's logical value count
    emitter.instruction("str x10, [sp]");                                       // preserve the logical count across context allocation
    emitter.instruction("mov x11, #65536");                                     // materialize the fixed registry capacity
    emitter.instruction("cmp x10, x11");                                        // does the logical count exceed the physical registry?
    emitter.instruction("csel x11, x10, x11, lo");                              // copy only the populated in-bounds registry prefix
    emitter.instruction("str x11, [sp, #8]");                                   // preserve the copy count across allocation
    emitter.instruction("lsl x0, x11, #3");                                     // convert the copied slot count to bytes
    emitter.instruction("add x0, x0, #56");                                     // include the seven-word context header
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the linked reentrant context snapshot
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_unser_context");
    emitter.instruction("ldr x12, [x9]");                                       // load the previous context link
    emitter.instruction("str x12, [x0]");                                       // context.prev = previous context
    crate::codegen_support::abi::emit_symbol_address(emitter, "x12", "_unser_allowed_mode");
    emitter.instruction("ldr x13, [x12]");                                      // load the outer allowed-class mode
    emitter.instruction("str x13, [x0, #8]");                                   // snapshot the outer allowed-class mode
    crate::codegen_support::abi::emit_symbol_address(emitter, "x12", "_unser_allowed_list");
    emitter.instruction("ldr x13, [x12]");                                      // load the outer context-owned allow-list
    emitter.instruction("str x13, [x0, #16]");                                  // move the outer allow-list reference into the snapshot
    crate::codegen_support::abi::emit_symbol_address(emitter, "x12", "_unser_allowed_list_mixed");
    emitter.instruction("ldr x13, [x12]");                                      // load the outer list representation flag
    emitter.instruction("str x13, [x0, #24]");                                  // snapshot the outer list representation flag
    emitter.instruction("ldr x13, [sp]");                                       // recover the outer logical registry count
    emitter.instruction("str x13, [x0, #32]");                                  // snapshot the logical registry count
    crate::codegen_support::abi::emit_symbol_address(emitter, "x12", "_unser_depth");
    emitter.instruction("ldr x13, [x12]");                                      // load the outer recursive parser depth
    emitter.instruction("str x13, [x0, #40]");                                  // snapshot the outer parser depth
    emitter.instruction("ldr x11, [sp, #8]");                                   // recover the bounded registry copy count
    emitter.instruction("str x11, [x0, #48]");                                  // record how many registry slots follow the header
    crate::codegen_support::abi::emit_symbol_address(emitter, "x12", "_unser_values");
    emitter.instruction("mov x13, #0");                                         // start copying the populated registry prefix
    emitter.label("__rt_unserialize_begin_copy");
    emitter.instruction("cmp x13, x11");                                        // copied every in-bounds outer registry slot?
    emitter.instruction("b.hs __rt_unserialize_begin_copy_done");               // finish once the used prefix is preserved
    emitter.instruction("ldr x14, [x12, x13, lsl #3]");                         // load one outer reference-registry entry
    emitter.instruction("add x15, x0, #56");                                    // derive the snapshot registry payload base
    emitter.instruction("str x14, [x15, x13, lsl #3]");                         // save the outer reference-registry entry
    emitter.instruction("add x13, x13, #1");                                    // advance to the next populated slot
    emitter.instruction("b __rt_unserialize_begin_copy");                       // continue copying the used registry prefix
    emitter.label("__rt_unserialize_begin_copy_done");
    emitter.instruction("str x0, [x9]");                                        // publish this snapshot as the current context link
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_unser_active");
    emitter.instruction("ldr x10, [x9]");                                       // reload the nesting count after allocation
    emitter.instruction("add x10, x10, #1");                                    // account for the nested parser
    emitter.instruction("str x10, [x9]");                                       // publish the incremented nesting count

    emitter.label("__rt_unserialize_begin_reset");
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_unser_allowed_mode");
    emitter.instruction("str xzr, [x9]");                                       // default this call to allow-all until options are installed
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_unser_allowed_list");
    emitter.instruction("str xzr, [x9]");                                       // this call starts without an owned allow-list
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_unser_allowed_list_mixed");
    emitter.instruction("str xzr, [x9]");                                       // default to direct-string list representation
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_unser_count");
    emitter.instruction("str xzr, [x9]");                                       // reset this call's reference-registry count
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_unser_depth");
    emitter.instruction("str xzr, [x9]");                                       // reset this call's recursive parser depth
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore the caller frame and return address
    emitter.instruction("add sp, sp, #32");                                     // release the begin helper frame
    emitter.instruction("ret");                                                 // enter the new isolated unserialize call

    emitter.label_global("__rt_unserialize_end");
    emitter.instruction("sub sp, sp, #48");                                     // reserve result/context spills plus an aligned helper frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // preserve the caller frame and return address across releases
    emitter.instruction("add x29, sp, #32");                                    // establish a stable frame for restoration
    emitter.instruction("str x0, [sp]");                                        // preserve the parsed Mixed result across cleanup
    crate::codegen_support::abi::emit_call_label(emitter, "__rt_unserialize_detach_data");
    crate::codegen_support::abi::emit_store_to_sp(emitter, "x0", 16);
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_unser_allowed_list");
    emitter.instruction("ldr x0, [x9]");                                        // load this call's owned allow-list reference
    emitter.instruction("str xzr, [x9]");                                       // unpublish the list before releasing its ownership
    emitter.instruction("cbz x0, __rt_unserialize_end_list_done");              // skip refcount traffic when no allow-list was installed
    emitter.instruction("bl __rt_decref_array");                                // release this call's allow-list ownership
    emitter.label("__rt_unserialize_end_list_done");
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_unser_context");
    emitter.instruction("ldr x10, [x9]");                                       // load the outer snapshot, if this call was reentrant
    emitter.instruction("cbz x10, __rt_unserialize_end_top");                   // top-level completion has no outer state to restore
    emitter.instruction("str x10, [sp, #8]");                                   // preserve the snapshot pointer across heap release
    emitter.instruction("ldr x11, [x10]");                                      // load the previous linked context
    emitter.instruction("str x11, [x9]");                                       // pop the current context snapshot
    crate::codegen_support::abi::emit_symbol_address(emitter, "x11", "_unser_allowed_mode");
    emitter.instruction("ldr x12, [x10, #8]");                                  // recover the outer allowed-class mode
    emitter.instruction("str x12, [x11]");                                      // restore the outer allowed-class mode
    crate::codegen_support::abi::emit_symbol_address(emitter, "x11", "_unser_allowed_list");
    emitter.instruction("ldr x12, [x10, #16]");                                 // recover the outer owned allow-list reference
    emitter.instruction("str x12, [x11]");                                      // republish the outer owned allow-list
    crate::codegen_support::abi::emit_symbol_address(emitter, "x11", "_unser_allowed_list_mixed");
    emitter.instruction("ldr x12, [x10, #24]");                                 // recover the outer list representation flag
    emitter.instruction("str x12, [x11]");                                      // restore the outer list representation flag
    crate::codegen_support::abi::emit_symbol_address(emitter, "x11", "_unser_count");
    emitter.instruction("ldr x12, [x10, #32]");                                 // recover the outer logical registry count
    emitter.instruction("str x12, [x11]");                                      // restore the outer logical registry count
    crate::codegen_support::abi::emit_symbol_address(emitter, "x11", "_unser_depth");
    emitter.instruction("ldr x12, [x10, #40]");                                 // recover the suspended outer parser depth
    emitter.instruction("str x12, [x11]");                                      // restore the suspended outer parser depth
    emitter.instruction("ldr x12, [x10, #48]");                                 // load the bounded registry snapshot length
    crate::codegen_support::abi::emit_symbol_address(emitter, "x11", "_unser_values");
    emitter.instruction("mov x13, #0");                                         // start restoring the outer registry prefix
    emitter.label("__rt_unserialize_end_copy");
    emitter.instruction("cmp x13, x12");                                        // restored every snapshotted registry slot?
    emitter.instruction("b.hs __rt_unserialize_end_copy_done");                 // finish when the full used prefix is live again
    emitter.instruction("add x14, x10, #56");                                   // derive the snapshot registry payload base
    emitter.instruction("ldr x15, [x14, x13, lsl #3]");                         // load one saved outer registry entry
    emitter.instruction("str x15, [x11, x13, lsl #3]");                         // restore the outer reference-registry entry
    emitter.instruction("add x13, x13, #1");                                    // advance to the next saved slot
    emitter.instruction("b __rt_unserialize_end_copy");                         // continue restoring the used registry prefix
    emitter.label("__rt_unserialize_end_copy_done");
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_unser_active");
    emitter.instruction("ldr x11, [x9]");                                       // load the active nesting count
    emitter.instruction("sub x11, x11, #1");                                    // account for the completed nested parser
    emitter.instruction("str x11, [x9]");                                       // publish the decremented nesting count
    emitter.instruction("ldr x0, [sp, #8]");                                    // pass the consumed snapshot to heap_free
    emitter.instruction("bl __rt_heap_free");                                   // release the temporary reentrancy snapshot
    emitter.instruction("b __rt_unserialize_end_return");                       // preserve the restored outer context

    emitter.label("__rt_unserialize_end_top");
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_unser_allowed_mode");
    emitter.instruction("str xzr, [x9]");                                       // clear the completed top-level policy mode
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_unser_allowed_list_mixed");
    emitter.instruction("str xzr, [x9]");                                       // clear the completed list representation flag
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_unser_count");
    emitter.instruction("str xzr, [x9]");                                       // retire the completed top-level registry
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_unser_depth");
    emitter.instruction("str xzr, [x9]");                                       // leave no parser depth behind after completion
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_unser_active");
    emitter.instruction("str xzr, [x9]");                                       // mark the unserialize runtime idle
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_unser_context");
    emitter.instruction("str xzr, [x9]");                                       // leave no linked snapshot after top-level completion
    emitter.label("__rt_unserialize_end_return");
    crate::codegen_support::abi::emit_load_temporary_stack_slot(emitter, "x0", 0);
    crate::codegen_support::abi::emit_load_temporary_stack_slot(emitter, "x1", 16);
    crate::codegen_support::abi::emit_call_label(emitter, "__rt_unserialize_finish_data");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore the caller frame and return address
    emitter.instruction("add sp, sp, #48");                                     // release the end helper frame
    emitter.instruction("ret");                                                 // return the unchanged parse result
}

/// Emits the x86_64 begin/end helpers that isolate one active unserialize call.
///
/// This is the SysV counterpart of [`emit_unserialize_context_aarch64`], with the
/// same linked snapshot and allow-list ownership contract.
pub(super) fn emit_unserialize_context_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: unserialize context begin/end ---");
    emitter.label_global("__rt_unserialize_begin");
    emitter.instruction("push rbp");                                            // preserve the caller frame across context allocation
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame for snapshot sizing
    emitter.instruction("sub rsp, 32");                                         // reserve logical-count and copy-count spills
    abi::emit_load_symbol_to_reg(emitter, "r10", "_unser_active", 0);         // load this context's active unserialize nesting count
    emitter.instruction("test r10, r10");                                       // is another parser already active?
    emitter.instruction("jnz __rt_unserialize_begin_nested_x");                 // snapshot the outer call before resetting globals
    abi::emit_store_imm_to_symbol(emitter, "_unser_active", 0, 1);             // mark this context's top-level parser active
    emitter.instruction("jmp __rt_unserialize_begin_reset_x");                  // initialize the fresh per-call state

    emitter.label("__rt_unserialize_begin_nested_x");
    emitter.instruction("cmp r10, 256");                                        // has reentrant parser nesting reached its hard limit?
    emitter.instruction("jae __rt_unser_depth_fatal_x");                        // reject another snapshot before allocating bounded heap state
    abi::emit_load_symbol_to_reg(emitter, "r10", "_unser_count", 0);          // load this context's outer registry value count
    emitter.instruction("mov QWORD PTR [rbp - 8], r10");                        // preserve the logical count across context allocation
    emitter.instruction("mov r11, 65536");                                      // materialize the fixed registry capacity
    emitter.instruction("cmp r10, r11");                                        // does the logical count exceed the physical registry?
    emitter.instruction("cmovb r11, r10");                                      // copy only the populated in-bounds registry prefix
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // preserve the copy count across allocation
    emitter.instruction("mov rax, r11");                                        // start computing the snapshot allocation size
    emitter.instruction("shl rax, 3");                                          // convert copied slots to bytes
    emitter.instruction("add rax, 56");                                         // include the seven-word context header
    emitter.instruction("call __rt_heap_alloc");                                // allocate the linked reentrant context snapshot
    abi::emit_load_symbol_to_reg(emitter, "r10", "_unser_context", 0);        // load this runtime context's previous parser link
    emitter.instruction("mov QWORD PTR [rax], r10");                            // context.prev = previous context
    abi::emit_load_symbol_to_reg(emitter, "r10", "_unser_allowed_mode", 0);   // load this context's outer allowed-class mode
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // snapshot the outer allowed-class mode
    abi::emit_load_symbol_to_reg(emitter, "r10", "_unser_allowed_list", 0);   // load this context's outer owned allow-list
    emitter.instruction("mov QWORD PTR [rax + 16], r10");                       // move the outer allow-list reference into the snapshot
    abi::emit_load_symbol_to_reg(emitter, "r10", "_unser_allowed_list_mixed", 0); // load this context's outer list representation flag
    emitter.instruction("mov QWORD PTR [rax + 24], r10");                       // snapshot the outer list representation flag
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // recover the outer logical registry count
    emitter.instruction("mov QWORD PTR [rax + 32], r10");                       // snapshot the logical registry count
    abi::emit_load_symbol_to_reg(emitter, "r10", "_unser_depth", 0);          // load this context's outer parser depth
    emitter.instruction("mov QWORD PTR [rax + 40], r10");                       // snapshot the outer parser depth
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // recover the bounded registry copy count
    emitter.instruction("mov QWORD PTR [rax + 48], r11");                       // record how many registry slots follow the header
    abi::emit_symbol_address(emitter, "rdx", "_unser_values");                // load this context's reference-registry base
    emitter.instruction("xor r10d, r10d");                                      // start copying the populated registry prefix
    emitter.label("__rt_unserialize_begin_copy_x");
    emitter.instruction("cmp r10, r11");                                        // copied every in-bounds outer registry slot?
    emitter.instruction("jae __rt_unserialize_begin_copy_done_x");              // finish once the used prefix is preserved
    emitter.instruction("mov rcx, QWORD PTR [rdx + r10 * 8]");                  // load one outer reference-registry entry
    emitter.instruction("mov QWORD PTR [rax + r10 * 8 + 56], rcx");             // save the outer reference-registry entry
    emitter.instruction("add r10, 1");                                          // advance to the next populated slot
    emitter.instruction("jmp __rt_unserialize_begin_copy_x");                   // continue copying the used registry prefix
    emitter.label("__rt_unserialize_begin_copy_done_x");
    abi::emit_store_reg_to_symbol(emitter, "rax", "_unser_context", 0);       // publish this context's current parser snapshot
    abi::emit_load_symbol_to_reg(emitter, "r10", "_unser_active", 0);         // load this context's nesting count
    emitter.instruction("add r10, 1");                                          // account for the nested parser
    abi::emit_store_reg_to_symbol(emitter, "r10", "_unser_active", 0);        // publish this context's incremented nesting count

    emitter.label("__rt_unserialize_begin_reset_x");
    abi::emit_store_zero_to_symbol(emitter, "_unser_allowed_mode", 0);         // default this context's call to allow-all
    abi::emit_store_zero_to_symbol(emitter, "_unser_allowed_list", 0);         // start without a context-owned allow-list
    abi::emit_store_zero_to_symbol(emitter, "_unser_allowed_list_mixed", 0);   // default to direct-string list representation
    abi::emit_store_zero_to_symbol(emitter, "_unser_count", 0);                // reset this context's reference-registry count
    abi::emit_store_zero_to_symbol(emitter, "_unser_depth", 0);                // reset this context's recursive parser depth
    emitter.instruction("leave");                                               // restore the caller frame after begin setup
    emitter.instruction("ret");                                                 // enter the new isolated unserialize call

    emitter.label_global("__rt_unserialize_end");
    emitter.instruction("push rbp");                                            // preserve the caller frame across cleanup calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame for result/context spills
    emitter.instruction("sub rsp, 32");                                         // reserve aligned spills for the result and snapshot pointer
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the parsed Mixed result across cleanup
    crate::codegen_support::abi::emit_call_label(emitter, "__rt_unserialize_detach_data");
    crate::codegen_support::abi::store_at_offset(emitter, "rax", 24);
    abi::emit_load_symbol_to_reg(emitter, "rax", "_unser_allowed_list", 0);   // load this context's owned allow-list reference
    abi::emit_store_zero_to_symbol(emitter, "_unser_allowed_list", 0);         // unpublish the list before releasing ownership
    emitter.instruction("test rax, rax");                                       // was an allow-list installed for this call?
    emitter.instruction("jz __rt_unserialize_end_list_done_x");                 // skip refcount traffic when no list was installed
    emitter.instruction("call __rt_decref_array");                              // release this call's allow-list ownership
    emitter.label("__rt_unserialize_end_list_done_x");
    abi::emit_load_symbol_to_reg(emitter, "r10", "_unser_context", 0);        // load this context's outer parser snapshot
    emitter.instruction("test r10, r10");                                       // does an outer parser need restoration?
    emitter.instruction("jz __rt_unserialize_end_top_x");                       // top-level completion has no outer state
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // preserve the snapshot pointer across heap release
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load the previous linked context
    abi::emit_store_reg_to_symbol(emitter, "r11", "_unser_context", 0);       // pop this context's current parser snapshot
    emitter.instruction("mov r11, QWORD PTR [r10 + 8]");                        // recover the outer allowed-class mode
    abi::emit_store_reg_to_symbol(emitter, "r11", "_unser_allowed_mode", 0);  // restore this context's outer allowed-class mode
    emitter.instruction("mov r11, QWORD PTR [r10 + 16]");                       // recover the outer owned allow-list reference
    abi::emit_store_reg_to_symbol(emitter, "r11", "_unser_allowed_list", 0);  // republish this context's outer owned allow-list
    emitter.instruction("mov r11, QWORD PTR [r10 + 24]");                       // recover the outer list representation flag
    abi::emit_store_reg_to_symbol(emitter, "r11", "_unser_allowed_list_mixed", 0); // restore this context's outer list representation
    emitter.instruction("mov r11, QWORD PTR [r10 + 32]");                       // recover the outer logical registry count
    abi::emit_store_reg_to_symbol(emitter, "r11", "_unser_count", 0);         // restore this context's outer registry count
    emitter.instruction("mov r11, QWORD PTR [r10 + 40]");                       // recover the suspended outer parser depth
    abi::emit_store_reg_to_symbol(emitter, "r11", "_unser_depth", 0);         // restore this context's outer parser depth
    emitter.instruction("mov r11, QWORD PTR [r10 + 48]");                       // load the bounded registry snapshot length
    abi::emit_symbol_address(emitter, "rdx", "_unser_values");                // load this context's active reference registry
    emitter.instruction("xor ecx, ecx");                                        // start restoring the outer registry prefix
    emitter.label("__rt_unserialize_end_copy_x");
    emitter.instruction("cmp rcx, r11");                                        // restored every snapshotted registry slot?
    emitter.instruction("jae __rt_unserialize_end_copy_done_x");                // finish when the full used prefix is live again
    emitter.instruction("mov r8, QWORD PTR [r10 + rcx * 8 + 56]");              // load one saved outer registry entry
    emitter.instruction("mov QWORD PTR [rdx + rcx * 8], r8");                   // restore the outer reference-registry entry
    emitter.instruction("add rcx, 1");                                          // advance to the next saved slot
    emitter.instruction("jmp __rt_unserialize_end_copy_x");                     // continue restoring the used registry prefix
    emitter.label("__rt_unserialize_end_copy_done_x");
    abi::emit_dec_symbol(emitter, "_unser_active");                            // account for this context's completed nested parser
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // pass the consumed snapshot to heap_free
    emitter.instruction("call __rt_heap_free");                                 // release the temporary reentrancy snapshot
    emitter.instruction("jmp __rt_unserialize_end_return_x");                   // preserve the restored outer context

    emitter.label("__rt_unserialize_end_top_x");
    abi::emit_store_zero_to_symbol(emitter, "_unser_allowed_mode", 0);         // clear this context's completed policy mode
    abi::emit_store_zero_to_symbol(emitter, "_unser_allowed_list_mixed", 0);   // clear this context's list representation flag
    abi::emit_store_zero_to_symbol(emitter, "_unser_count", 0);                // retire this context's completed registry
    abi::emit_store_zero_to_symbol(emitter, "_unser_depth", 0);                // leave no parser depth behind
    abi::emit_store_zero_to_symbol(emitter, "_unser_active", 0);               // mark this context's unserialize runtime idle
    abi::emit_store_zero_to_symbol(emitter, "_unser_context", 0);              // leave no linked snapshot behind
    emitter.label("__rt_unserialize_end_return_x");
    crate::codegen_support::abi::load_at_offset(emitter, "rdi", 8);
    crate::codegen_support::abi::load_at_offset(emitter, "rsi", 24);
    crate::codegen_support::abi::emit_call_label(emitter, "__rt_unserialize_finish_data");
    emitter.instruction("leave");                                               // restore the caller frame and stack
    emitter.instruction("ret");                                                 // return the unchanged parse result
}
