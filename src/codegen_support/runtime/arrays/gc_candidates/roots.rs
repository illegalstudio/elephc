//! Purpose:
//! Prunes known strong worker roots before expanding suspect graphs.
//!
//! Called from:
//! - The per-program global/static root visitor during candidate preparation.
//!
//! Key details:
//! - Root discovery visits direct object children, never large array/hash contents.
//! - Box/reference chains stop at an already anchored identity, including cycles.
//! - Anchor bit 20 exists only during discovery and is cleared before edge counting.

use super::{load, op, restore, save};
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use crate::codegen_support::sentinels::REFERENCE_CELL_HEAP_KIND;

/// Emits bounded root normalization and exact retirement of temporary anchor metadata.
pub(super) fn emit(e: &mut Emitter) {
    e.blank();
    e.comment("--- runtime: gc_candidate_root ---");
    e.label_global("__rt_gc_candidate_root");
    abi::emit_frame_prologue(e, 32);
    // -- normalize owning root prefixes without following large container contents --
    // -- full cache construction records each actual root before its breadth-first traversal --
    load(e, "x9", "r10", "_gc_candidates_gathering");
    op(e, "cmp x9, #3", "cmp r10, 3", "distinguish complete proof construction from shallow current-root anchoring");
    op(e, "b.ne __rt_gc_candidate_root_loop", "jne __rt_gc_candidate_root_loop", "ordinary preparation normalizes only the bounded root prefix");
    abi::emit_store_zero_to_symbol(e, "_gc_proof_parent", 0);
    abi::emit_store_zero_to_symbol(e, "_gc_proof_slot", 0);
    abi::emit_call_label(e, "__rt_gc_proof_record");
    abi::emit_jump(e, "__rt_gc_candidate_root_done");
    e.label("__rt_gc_candidate_root_loop");
    abi::emit_call_label(e, "__rt_gc_candidate_live");
    abi::emit_branch_if_int_result_zero(e, "__rt_gc_candidate_root_done");
    op(e, "ldr x9, [x0, #-8]", "mov r10, QWORD PTR [rax - 8]", "inspect whether this root prefix was already normalized");
    op(e, "tbnz x9, #20, __rt_gc_candidate_root_done", "test r10, 0x100000", "test the temporary root anchor to stop alias cycles");
    if e.target.arch == Arch::X86_64 { op(e, "", "jnz __rt_gc_candidate_root_done", "return when this identity is already an established anchor"); }
    save(e, 0);
    abi::emit_call_label(e, "__rt_gc_candidate_anchor");
    restore(e, 0);
    op(e, "ldrb w9, [x0, #-8]", "movzx r10d, BYTE PTR [rax - 8]", "classify the validated root before normalizing its prefix");
    op(e, "cmp w9, #4", "cmp r10d, 4", "identify an object whose direct properties can be anchored");
    op(e, "b.eq __rt_gc_candidate_root_object", "je __rt_gc_candidate_root_object", "visit this rooted object's immediate owned children");
    op(e, "cmp w9, #5", "cmp r10d, 5", "recognize a boxed Mixed root");
    op(e, "b.eq __rt_gc_candidate_root_box", "je __rt_gc_candidate_root_box", "normalize the box only when it can hold a graph child");
    op(e, &format!("cmp w9, #{REFERENCE_CELL_HEAP_KIND}"), &format!("cmp r10d, {REFERENCE_CELL_HEAP_KIND}"), "recognize a managed reference prefix");
    op(e, "b.ne __rt_gc_candidate_root_done", "jne __rt_gc_candidate_root_done", "stop normalization at ordinary non-wrapper roots");
    op(e, "ldr x9, [x0, #-8]", "mov r10, QWORD PTR [rax - 8]", "recover the reference cell's current payload descriptor");
    op(e, "ubfx x9, x9, #8, #7", "shr r10, 8", "isolate its runtime payload shape");
    if e.target.arch == Arch::X86_64 { op(e, "", "and r10d, 0x7f", "remove flags before deciding whether the cell owns a graph"); }
    op(e, "cmp x9, #4", "cmp r10d, 4", "exclude scalar and string reference payloads");
    op(e, "b.lo __rt_gc_candidate_root_done", "jb __rt_gc_candidate_root_done", "stop before treating a scalar value as a heap pointer");
    op(e, "cmp x9, #10", "cmp r10, 10", "callable wrappers own managed descriptor nodes");
    op(e, "b.eq __rt_gc_candidate_callable_root_51", "je __rt_gc_candidate_callable_root_51", "normalize a callable payload without admitting scalar representations");
    op(e, "cmp x9, #7", "cmp r10d, 7", "bound supported graph-bearing reference payloads");
    op(e, "b.hi __rt_gc_candidate_root_done", "ja __rt_gc_candidate_root_done", "reject unknown payload shapes");
    e.label("__rt_gc_candidate_callable_root_51");
    op(e, "ldr x0, [x0]", "mov rax, QWORD PTR [rax]", "follow the independently owned cell child");
    abi::emit_jump(e, "__rt_gc_candidate_root_loop");
    // -- follow only a graph-bearing boxed payload --
    e.label("__rt_gc_candidate_root_box");
    op(e, "ldr x9, [x0]", "mov r10, QWORD PTR [rax]", "inspect the Mixed box's actual runtime tag");
    op(e, "cmp x9, #4", "cmp r10, 4", "exclude scalar, string and null boxed values");
    op(e, "b.lo __rt_gc_candidate_root_done", "jb __rt_gc_candidate_root_done", "stop before reading a non-graph boxed payload");
    op(e, "cmp x9, #10", "cmp r10, 10", "callable wrappers own managed descriptor nodes");
    op(e, "b.eq __rt_gc_candidate_callable_root_60", "je __rt_gc_candidate_callable_root_60", "normalize a callable payload without admitting scalar representations");
    op(e, "cmp x9, #7", "cmp r10, 7", "bound supported boxed graph tags");
    op(e, "b.hi __rt_gc_candidate_root_done", "ja __rt_gc_candidate_root_done", "ignore unknown runtime representations");
    e.label("__rt_gc_candidate_callable_root_60");
    op(e, "ldr x0, [x0, #8]", "mov rax, QWORD PTR [rax + 8]", "follow the box's actual owned graph child");
    abi::emit_jump(e, "__rt_gc_candidate_root_loop");
    e.label("__rt_gc_candidate_root_object");
    abi::emit_call_label(e, "__rt_gc_visit_children");
    e.label("__rt_gc_candidate_root_done");
    abi::emit_frame_restore(e, 32);
    abi::emit_return(e);

    e.blank();
    e.comment("--- runtime: gc_clear_candidate_anchors ---");
    e.label_global("__rt_gc_clear_candidate_anchors");
    abi::emit_frame_prologue(e, 32);
    abi::emit_load_int_immediate(e, abi::int_result_reg(e), 0);
    save(e, 0);
    // -- clear temporary anchors before incoming-edge counting --
    e.label("__rt_gc_clear_anchor_loop");
    restore(e, 0);
    load(e, "x9", "r10", "_gc_candidates_anchors_count");
    op(e, "cmp x0, x9", "cmp rax, r10", "test whether every temporary anchor has been inspected");
    op(e, "b.hs __rt_gc_clear_anchors_done", "jae __rt_gc_clear_anchors_done", "finish retirement after the complete anchor vector");
    load(e, "x9", "r10", "_gc_candidates_anchors");
    op(e, "ldr x0, [x9, x0, lsl #3]", "mov rax, QWORD PTR [r10 + rax * 8]", "borrow the next anchored header without creating ownership");
    op(e, "add x0, x0, #16", "add rax, 16", "validate the payload before touching its transient marker");
    abi::emit_call_label(e, "__rt_gc_candidate_live");
    abi::emit_branch_if_int_result_zero(e, "__rt_gc_clear_anchor_next");
    op(e, "ldr x9, [x0, #-8]", "and DWORD PTR [rax - 8], 0xffefffff", "clear only the temporary anchor metadata");
    if e.target.arch == Arch::AArch64 {
        op(e, "bic x9, x9, #0x100000", "", "remove the anchor bit before incoming counts reuse that range");
        op(e, "str x9, [x0, #-8]", "", "publish the cleared marker while preserving other flags");
    }
    e.label("__rt_gc_clear_anchor_next");
    restore(e, 0);
    op(e, "add x0, x0, #1", "add rax, 1", "advance to the next anchor identity");
    save(e, 0);
    abi::emit_jump(e, "__rt_gc_clear_anchor_loop");
    e.label("__rt_gc_clear_anchors_done");
    abi::emit_store_zero_to_symbol(e, "_gc_candidates_anchors_count", 0);
    abi::emit_frame_restore(e, 32);
    abi::emit_return(e);
}
