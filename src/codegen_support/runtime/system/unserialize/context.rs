//! Purpose:
//! Emits per-call unserialize context lifecycle helpers for both supported architectures.
//!
//! Called from:
//! - `super::emit_unserialize()` between recursive decoding and object-storage helpers.
//!
//! Key details:
//! - Begin and end isolate nested calls by snapshotting policy, depth, and reference-registry state.

use crate::codegen_support::emit::Emitter;

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
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_unser_warning_emitted");
    emitter.instruction("str xzr, [x9]");                                       // no warning emitted for this call
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_unser_force_failure");
    emitter.instruction("str xzr, [x9]");                                       // no post-hook forced failure
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_unser_failure_offset");
    emitter.instruction("mov x10, #-1");                                       // use an out-of-band sentinel so offset zero remains reportable
    emitter.instruction("str x10, [x9]");                                      // no validator failure offset recorded for this call
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
    emitter.instruction("ldr x0, [sp]");                                        // restore the parsed Mixed result for the lowering
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
    emitter.instruction("mov r10, QWORD PTR [rip + _unser_active]");            // load the active unserialize nesting count
    emitter.instruction("test r10, r10");                                       // is another parser already active?
    emitter.instruction("jnz __rt_unserialize_begin_nested_x");                 // snapshot the outer call before resetting globals
    emitter.instruction("mov QWORD PTR [rip + _unser_active], 1");              // mark the top-level parser active
    emitter.instruction("jmp __rt_unserialize_begin_reset_x");                  // initialize the fresh per-call state

    emitter.label("__rt_unserialize_begin_nested_x");
    emitter.instruction("cmp r10, 256");                                        // has reentrant parser nesting reached its hard limit?
    emitter.instruction("jae __rt_unser_depth_fatal_x");                        // reject another snapshot before allocating bounded heap state
    emitter.instruction("mov r10, QWORD PTR [rip + _unser_count]");             // load the outer registry's logical value count
    emitter.instruction("mov QWORD PTR [rbp - 8], r10");                        // preserve the logical count across context allocation
    emitter.instruction("mov r11, 65536");                                      // materialize the fixed registry capacity
    emitter.instruction("cmp r10, r11");                                        // does the logical count exceed the physical registry?
    emitter.instruction("cmovb r11, r10");                                      // copy only the populated in-bounds registry prefix
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // preserve the copy count across allocation
    emitter.instruction("mov rax, r11");                                        // start computing the snapshot allocation size
    emitter.instruction("shl rax, 3");                                          // convert copied slots to bytes
    emitter.instruction("add rax, 56");                                         // include the seven-word context header
    emitter.instruction("call __rt_heap_alloc");                                // allocate the linked reentrant context snapshot
    emitter.instruction("mov r10, QWORD PTR [rip + _unser_context]");           // load the previous context link
    emitter.instruction("mov QWORD PTR [rax], r10");                            // context.prev = previous context
    emitter.instruction("mov r10, QWORD PTR [rip + _unser_allowed_mode]");      // load the outer allowed-class mode
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // snapshot the outer allowed-class mode
    emitter.instruction("mov r10, QWORD PTR [rip + _unser_allowed_list]");      // load the outer context-owned allow-list
    emitter.instruction("mov QWORD PTR [rax + 16], r10");                       // move the outer allow-list reference into the snapshot
    emitter.instruction("mov r10, QWORD PTR [rip + _unser_allowed_list_mixed]"); // load the outer list representation flag
    emitter.instruction("mov QWORD PTR [rax + 24], r10");                       // snapshot the outer list representation flag
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // recover the outer logical registry count
    emitter.instruction("mov QWORD PTR [rax + 32], r10");                       // snapshot the logical registry count
    emitter.instruction("mov r10, QWORD PTR [rip + _unser_depth]");             // load the outer recursive parser depth
    emitter.instruction("mov QWORD PTR [rax + 40], r10");                       // snapshot the outer parser depth
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // recover the bounded registry copy count
    emitter.instruction("mov QWORD PTR [rax + 48], r11");                       // record how many registry slots follow the header
    emitter.instruction("lea rdx, [rip + _unser_values]");                      // load the outer reference-registry base
    emitter.instruction("xor r10d, r10d");                                      // start copying the populated registry prefix
    emitter.label("__rt_unserialize_begin_copy_x");
    emitter.instruction("cmp r10, r11");                                        // copied every in-bounds outer registry slot?
    emitter.instruction("jae __rt_unserialize_begin_copy_done_x");              // finish once the used prefix is preserved
    emitter.instruction("mov rcx, QWORD PTR [rdx + r10 * 8]");                  // load one outer reference-registry entry
    emitter.instruction("mov QWORD PTR [rax + r10 * 8 + 56], rcx");             // save the outer reference-registry entry
    emitter.instruction("add r10, 1");                                          // advance to the next populated slot
    emitter.instruction("jmp __rt_unserialize_begin_copy_x");                   // continue copying the used registry prefix
    emitter.label("__rt_unserialize_begin_copy_done_x");
    emitter.instruction("mov QWORD PTR [rip + _unser_context], rax");           // publish this snapshot as the current context link
    emitter.instruction("add QWORD PTR [rip + _unser_active], 1");              // account for the nested parser

    emitter.label("__rt_unserialize_begin_reset_x");
    emitter.instruction("mov QWORD PTR [rip + _unser_allowed_mode], 0");        // default this call to allow-all until options are installed
    emitter.instruction("mov QWORD PTR [rip + _unser_allowed_list], 0");        // this call starts without an owned allow-list
    emitter.instruction("mov QWORD PTR [rip + _unser_allowed_list_mixed], 0");  // default to direct-string list representation
    emitter.instruction("mov QWORD PTR [rip + _unser_count], 0");               // reset this call's reference-registry count
    emitter.instruction("mov QWORD PTR [rip + _unser_warning_emitted], 0");      // no warning emitted for this call
    emitter.instruction("mov QWORD PTR [rip + _unser_force_failure], 0");        // no post-hook forced failure
    emitter.instruction("mov QWORD PTR [rip + _unser_failure_offset], -1");    // no validator failure offset recorded for this call
    emitter.instruction("mov QWORD PTR [rip + _unser_depth], 0");               // reset this call's recursive parser depth
    emitter.instruction("leave");                                               // restore the caller frame after begin setup
    emitter.instruction("ret");                                                 // enter the new isolated unserialize call

    emitter.label_global("__rt_unserialize_end");
    emitter.instruction("push rbp");                                            // preserve the caller frame across cleanup calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame for result/context spills
    emitter.instruction("sub rsp, 32");                                         // reserve aligned spills for the result and snapshot pointer
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the parsed Mixed result across cleanup
    emitter.instruction("mov rax, QWORD PTR [rip + _unser_allowed_list]");      // load this call's owned allow-list reference
    emitter.instruction("mov QWORD PTR [rip + _unser_allowed_list], 0");        // unpublish the list before releasing its ownership
    emitter.instruction("test rax, rax");                                       // was an allow-list installed for this call?
    emitter.instruction("jz __rt_unserialize_end_list_done_x");                 // skip refcount traffic when no list was installed
    emitter.instruction("call __rt_decref_array");                              // release this call's allow-list ownership
    emitter.label("__rt_unserialize_end_list_done_x");
    emitter.instruction("mov r10, QWORD PTR [rip + _unser_context]");           // load the outer snapshot, if this call was reentrant
    emitter.instruction("test r10, r10");                                       // does an outer parser need restoration?
    emitter.instruction("jz __rt_unserialize_end_top_x");                       // top-level completion has no outer state
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // preserve the snapshot pointer across heap release
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load the previous linked context
    emitter.instruction("mov QWORD PTR [rip + _unser_context], r11");           // pop the current context snapshot
    emitter.instruction("mov r11, QWORD PTR [r10 + 8]");                        // recover the outer allowed-class mode
    emitter.instruction("mov QWORD PTR [rip + _unser_allowed_mode], r11");      // restore the outer allowed-class mode
    emitter.instruction("mov r11, QWORD PTR [r10 + 16]");                       // recover the outer owned allow-list reference
    emitter.instruction("mov QWORD PTR [rip + _unser_allowed_list], r11");      // republish the outer owned allow-list
    emitter.instruction("mov r11, QWORD PTR [r10 + 24]");                       // recover the outer list representation flag
    emitter.instruction("mov QWORD PTR [rip + _unser_allowed_list_mixed], r11"); // restore the outer list representation flag
    emitter.instruction("mov r11, QWORD PTR [r10 + 32]");                       // recover the outer logical registry count
    emitter.instruction("mov QWORD PTR [rip + _unser_count], r11");             // restore the outer logical registry count
    emitter.instruction("mov r11, QWORD PTR [r10 + 40]");                       // recover the suspended outer parser depth
    emitter.instruction("mov QWORD PTR [rip + _unser_depth], r11");             // restore the suspended outer parser depth
    emitter.instruction("mov r11, QWORD PTR [r10 + 48]");                       // load the bounded registry snapshot length
    emitter.instruction("lea rdx, [rip + _unser_values]");                      // load the active reference-registry base
    emitter.instruction("xor ecx, ecx");                                        // start restoring the outer registry prefix
    emitter.label("__rt_unserialize_end_copy_x");
    emitter.instruction("cmp rcx, r11");                                        // restored every snapshotted registry slot?
    emitter.instruction("jae __rt_unserialize_end_copy_done_x");                // finish when the full used prefix is live again
    emitter.instruction("mov r8, QWORD PTR [r10 + rcx * 8 + 56]");              // load one saved outer registry entry
    emitter.instruction("mov QWORD PTR [rdx + rcx * 8], r8");                   // restore the outer reference-registry entry
    emitter.instruction("add rcx, 1");                                          // advance to the next saved slot
    emitter.instruction("jmp __rt_unserialize_end_copy_x");                     // continue restoring the used registry prefix
    emitter.label("__rt_unserialize_end_copy_done_x");
    emitter.instruction("sub QWORD PTR [rip + _unser_active], 1");              // account for the completed nested parser
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // pass the consumed snapshot to heap_free
    emitter.instruction("call __rt_heap_free");                                 // release the temporary reentrancy snapshot
    emitter.instruction("jmp __rt_unserialize_end_return_x");                   // preserve the restored outer context

    emitter.label("__rt_unserialize_end_top_x");
    emitter.instruction("mov QWORD PTR [rip + _unser_allowed_mode], 0");        // clear the completed top-level policy mode
    emitter.instruction("mov QWORD PTR [rip + _unser_allowed_list_mixed], 0");  // clear the completed list representation flag
    emitter.instruction("mov QWORD PTR [rip + _unser_count], 0");               // retire the completed top-level registry
    emitter.instruction("mov QWORD PTR [rip + _unser_depth], 0");               // leave no parser depth behind after completion
    emitter.instruction("mov QWORD PTR [rip + _unser_active], 0");              // mark the unserialize runtime idle
    emitter.instruction("mov QWORD PTR [rip + _unser_context], 0");             // leave no linked snapshot after top-level completion
    emitter.label("__rt_unserialize_end_return_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // restore the parsed Mixed result for the lowering
    emitter.instruction("leave");                                               // restore the caller frame and stack
    emitter.instruction("ret");                                                 // return the unchanged parse result
}
