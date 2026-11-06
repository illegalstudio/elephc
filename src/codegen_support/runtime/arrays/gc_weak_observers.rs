//! Purpose:
//! Retires weak observers of final unreachable objects before the collector starts sweeping.
//!
//! Called from:
//! - The collector after destructor resurrection has been resolved by root recounting.
//!
//! Key details:
//! - Existing native snapshots pin the entire candidate graph throughout PHP callbacks.
//! - Every notified key requests another mark pass, protecting new allocations and graph changes.
//! - Callback exceptions join the collector's pending owner after all observers are processed.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits the bounded callback phase shared by all supported collectors.
pub(super) fn emit(emitter: &mut Emitter) {
    let arm = emitter.target.arch == Arch::AArch64;
    let result = abi::int_result_reg(emitter);
    let arg0 = abi::int_arg_reg_name(emitter.target, 0);
    emitter.blank();
    emitter.label_global("__rt_gc_weak_observers");
    abi::emit_frame_prologue(emitter, 64);
    abi::emit_store_zero_to_local_slot(emitter, 24);
    abi::emit_store_zero_to_local_slot(emitter, 32);
    abi::emit_load_symbol_to_reg(emitter, result, "_weak_map_key_retire_fn", 0);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_gc_weak_finish");
    abi::emit_load_symbol_to_reg(emitter, result, "_gc_pin_head", 0);
    abi::store_at_offset(emitter, result, 8);
    abi::emit_store_zero_to_local_slot(emitter, 16);
    emitter.label("__rt_gc_weak_node");
    abi::load_at_offset(emitter, result, 8);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_gc_weak_finish");
    if arm {
        emitter.instruction("mov x10, x0");                                     // retain the current native snapshot chunk
        emitter.instruction("ldr x11, [x10, #8]");                              // read the number of initialized candidate pointers
        abi::load_at_offset(emitter, result, 16);
        emitter.instruction("cmp x0, x11");                                     // stop at the end of this snapshot chunk
        emitter.instruction("b.hs __rt_gc_weak_next_chunk");                    // continue through older snapshot chunks
        emitter.instruction("add x10, x10, #16");                               // skip native snapshot linkage and count
        emitter.instruction("ldr x0, [x10, x0, lsl #3]");                       // borrow a pinned candidate identity
        emitter.instruction("ldr x10, [x0, #-8]");                              // inspect final reachability and heap kind
        emitter.instruction("tbnz x10, #16, __rt_gc_weak_next");                // resurrected objects keep their weak observers
        emitter.instruction("and x10, x10, #0xff");                             // isolate the graph node's heap kind
        emitter.instruction("cmp x10, #4");                                     // only ordinary objects can be weak map keys
        emitter.instruction("b.ne __rt_gc_weak_next");                          // container pins only protect callback data
        abi::emit_symbol_address(emitter, "x9", "_heap_buf");
        emitter.instruction("sub x10, x0, x9");                                 // derive the candidate's weak-cache granule
        emitter.instruction("lsr x10, x10, #4");                                // convert the byte offset to its direct cache index
        abi::emit_symbol_address(emitter, "x9", "_obj_weak_reference_index");
        emitter.instruction("ldr x10, [x9, x10, lsl #3]");                      // only observed keys can invoke PHP cleanup
        emitter.instruction("cbz x10, __rt_gc_weak_next");                      // unobserved candidates need no recount
    } else {
        emitter.instruction("mov r10, rax");                                    // retain the current native snapshot chunk
        emitter.instruction("mov r11, QWORD PTR [r10 + 8]");                    // read the number of initialized candidate pointers
        abi::load_at_offset(emitter, result, 16);
        emitter.instruction("cmp rax, r11");                                    // stop at the end of this snapshot chunk
        emitter.instruction("jae __rt_gc_weak_next_chunk");                     // continue through older snapshot chunks
        emitter.instruction("mov rax, QWORD PTR [r10 + rax * 8 + 16]");         // borrow a pinned candidate identity
        emitter.instruction("mov r10, QWORD PTR [rax - 8]");                    // inspect final reachability and heap kind
        emitter.instruction("test r10, 0x10000");                               // inspect the final external-root reachability mark
        emitter.instruction("jnz __rt_gc_weak_next");                           // resurrected objects keep their observers
        emitter.instruction("and r10d, 0xff");                                  // isolate the graph-node heap kind
        emitter.instruction("cmp r10d, 4");                                     // only ordinary objects can be weak map keys
        emitter.instruction("jne __rt_gc_weak_next");                           // container pins only protect callback data
        abi::emit_symbol_address(emitter, "r11", "_heap_buf");
        emitter.instruction("mov r10, rax");                                    // preserve the receiver while deriving its direct cache index
        emitter.instruction("sub r10, r11");                                    // derive its heap-relative allocation offset
        emitter.instruction("shr r10, 4");                                      // convert the byte offset to a weak-cache granule
        abi::emit_symbol_address(emitter, "r11", "_obj_weak_reference_index");
        emitter.instruction("cmp QWORD PTR [r11 + r10 * 8], 0");                // only observed keys can invoke PHP cleanup
        emitter.instruction("je __rt_gc_weak_next");                            // unobserved candidates need no recount
    }
    abi::store_at_offset(emitter, result, 40);
    abi::emit_call_label(emitter, "__rt_gc_destructor_begin");
    abi::load_at_offset(emitter, arg0, 40);
    super::super::exceptions::emit_guarded_cleanup_call(emitter, "__rt_weak_map_notify", arg0, 32);
    abi::emit_call_label(emitter, "__rt_gc_destructor_end");
    abi::emit_load_int_immediate(emitter, result, 1);
    abi::store_at_offset(emitter, result, 24);
    emitter.label("__rt_gc_weak_next");
    abi::load_at_offset(emitter, result, 16);
    if arm { emitter.instruction("add x0, x0, #1"); }                           // advance after each stable snapshot identity
    else { emitter.instruction("add rax, 1"); }                                 // advance after each stable snapshot identity
    abi::store_at_offset(emitter, result, 16);
    abi::emit_jump(emitter, "__rt_gc_weak_node");
    emitter.label("__rt_gc_weak_next_chunk");
    abi::load_at_offset(emitter, result, 8);
    abi::emit_load_from_address(emitter, result, result, 0);
    abi::store_at_offset(emitter, result, 8);
    abi::emit_store_zero_to_local_slot(emitter, 16);
    abi::emit_jump(emitter, "__rt_gc_weak_node");
    emitter.label("__rt_gc_weak_finish");
    abi::load_at_offset(emitter, result, 32);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_gc_weak_return");
    let previous = if arm { "x1" } else { "rdi" };
    abi::emit_load_symbol_to_reg(emitter, previous, "_gc_pending_throw", 0);
    abi::emit_store_reg_to_symbol(emitter, result, "_gc_pending_throw", 0);
    abi::emit_call_label(emitter, "__rt_exception_chain");
    emitter.label("__rt_gc_weak_return");
    abi::load_at_offset(emitter, result, 24);
    abi::emit_frame_restore(emitter, 64);
    abi::emit_return(emitter);
}
