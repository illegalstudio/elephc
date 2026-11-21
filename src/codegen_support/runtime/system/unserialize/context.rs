//! Purpose:
//! Emits per-call unserialize context lifecycle helpers for both supported architectures.
//!
//! Called from:
//! - `super::emit_unserialize()` between recursive decoding and object-storage helpers.
//!
//! Key details:
//! - Begin and end snapshot policy and depth; modern calls also snapshot the reference registry.
//! - A negative saved count marks legacy sharing, whose owners survive until the outer decode ends.

use crate::codegen_support::emit::Emitter;
use elephc_memoir::values::{reference_registry::UNSERIALIZE_CAPACITY, reference_snapshot};

pub(super) const SNAPSHOT: reference_snapshot::Storage<'static> = reference_snapshot::Storage {
    label_prefix: "__rt_unserialize",
    head: "_unser_context",
    mode: "_unser_allowed_mode",
    owner: "_unser_allowed_list",
    owner_repr: "_unser_allowed_list_mixed",
    count: "_unser_count",
    depth: "_unser_depth",
    slots: "_unser_values",
};

/// Emits the AArch64 begin/end helpers that isolate one active unserialize call.
///
/// Nested calls snapshot the outer policy and parser depth into a linked heap
/// context. Modern hooks also copy populated reference slots; legacy hooks share them. The end helper preserves the parsed result,
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
    SNAPSHOT.emit_prepare_prefix(emitter, UNSERIALIZE_CAPACITY, "_unser_legacy_active", "__rt_unserialize_begin_copy_count");
    SNAPSHOT.emit_push(emitter);
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
    crate::codegen_support::abi::emit_load_symbol_to_reg(emitter, "x10", "_unser_legacy_active", 0);
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_unser_count");
    emitter.instruction("cbnz x10, __rt_unserialize_begin_keep_count");         // nested legacy payloads consume the enclosing reference indices
    emitter.instruction("str xzr, [x9]");                                       // reset this call's reference-registry count
    emitter.label("__rt_unserialize_begin_keep_count");
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
    crate::codegen_support::abi::emit_call_label(emitter, "__rt_unserialize_promote_legacy_data");
    crate::codegen_support::abi::emit_call_label(emitter, "__rt_unserialize_detach_data");
    crate::codegen_support::abi::emit_store_to_sp(emitter, "x0", 16);
    SNAPSHOT.emit_retire_current_owner(emitter, "__rt_unserialize_end_list_done");
    SNAPSHOT.emit_restore_or_jump(emitter);
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_unser_active");
    emitter.instruction("ldr x11, [x9]");                                       // load the active nesting count
    emitter.instruction("sub x11, x11, #1");                                    // account for the completed nested parser
    emitter.instruction("str x11, [x9]");                                       // publish the decremented nesting count
    reference_snapshot::emit_release(emitter);
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
    emitter.instruction("mov r10, QWORD PTR [rip + _unser_active]");            // load the active unserialize nesting count
    emitter.instruction("test r10, r10");                                       // is another parser already active?
    emitter.instruction("jnz __rt_unserialize_begin_nested_x");                 // snapshot the outer call before resetting globals
    emitter.instruction("mov QWORD PTR [rip + _unser_active], 1");              // mark the top-level parser active
    emitter.instruction("jmp __rt_unserialize_begin_reset_x");                  // initialize the fresh per-call state

    emitter.label("__rt_unserialize_begin_nested_x");
    emitter.instruction("cmp r10, 256");                                        // has reentrant parser nesting reached its hard limit?
    emitter.instruction("jae __rt_unser_depth_fatal_x");                        // reject another snapshot before allocating bounded heap state
    SNAPSHOT.emit_prepare_prefix(emitter, UNSERIALIZE_CAPACITY, "_unser_legacy_active", "__rt_unserialize_begin_copy_count_x");
    SNAPSHOT.emit_push(emitter);
    emitter.instruction("add QWORD PTR [rip + _unser_active], 1");              // account for the nested parser

    emitter.label("__rt_unserialize_begin_reset_x");
    emitter.instruction("mov QWORD PTR [rip + _unser_allowed_mode], 0");        // default this call to allow-all until options are installed
    emitter.instruction("mov QWORD PTR [rip + _unser_allowed_list], 0");        // this call starts without an owned allow-list
    emitter.instruction("mov QWORD PTR [rip + _unser_allowed_list_mixed], 0");  // default to direct-string list representation
    emitter.instruction("cmp QWORD PTR [rip + _unser_legacy_active], 0");       // nested legacy payloads share reference numbering
    emitter.instruction("jne __rt_unserialize_begin_keep_count_x");             // preserve the enclosing live reference count
    emitter.instruction("mov QWORD PTR [rip + _unser_count], 0");               // reset this call's reference-registry count
    emitter.label("__rt_unserialize_begin_keep_count_x");
    emitter.instruction("mov QWORD PTR [rip + _unser_depth], 0");               // reset this call's recursive parser depth
    emitter.instruction("leave");                                               // restore the caller frame after begin setup
    emitter.instruction("ret");                                                 // enter the new isolated unserialize call

    emitter.label_global("__rt_unserialize_end");
    emitter.instruction("push rbp");                                            // preserve the caller frame across cleanup calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame for result/context spills
    emitter.instruction("sub rsp, 32");                                         // reserve aligned spills for the result and snapshot pointer
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the parsed Mixed result across cleanup
    crate::codegen_support::abi::emit_call_label(emitter, "__rt_unserialize_promote_legacy_data");
    crate::codegen_support::abi::emit_call_label(emitter, "__rt_unserialize_detach_data");
    crate::codegen_support::abi::store_at_offset(emitter, "rax", 24);
    SNAPSHOT.emit_retire_current_owner(emitter, "__rt_unserialize_end_list_done_x");
    SNAPSHOT.emit_restore_or_jump(emitter);
    emitter.instruction("sub QWORD PTR [rip + _unser_active], 1");              // account for the completed nested parser
    reference_snapshot::emit_release(emitter);
    emitter.instruction("jmp __rt_unserialize_end_return_x");                   // preserve the restored outer context

    emitter.label("__rt_unserialize_end_top_x");
    emitter.instruction("mov QWORD PTR [rip + _unser_allowed_mode], 0");        // clear the completed top-level policy mode
    emitter.instruction("mov QWORD PTR [rip + _unser_allowed_list_mixed], 0");  // clear the completed list representation flag
    emitter.instruction("mov QWORD PTR [rip + _unser_count], 0");               // retire the completed top-level registry
    emitter.instruction("mov QWORD PTR [rip + _unser_depth], 0");               // leave no parser depth behind after completion
    emitter.instruction("mov QWORD PTR [rip + _unser_active], 0");              // mark the unserialize runtime idle
    emitter.instruction("mov QWORD PTR [rip + _unser_context], 0");             // leave no linked snapshot after top-level completion
    emitter.label("__rt_unserialize_end_return_x");
    crate::codegen_support::abi::load_at_offset(emitter, "rdi", 8);
    crate::codegen_support::abi::load_at_offset(emitter, "rsi", 24);
    crate::codegen_support::abi::emit_call_label(emitter, "__rt_unserialize_finish_data");
    emitter.instruction("leave");                                               // restore the caller frame and stack
    emitter.instruction("ret");                                                 // return the unchanged parse result
}
