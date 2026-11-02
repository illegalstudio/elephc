//! Purpose:
//! Memoizes proved and unproved forest prefixes within one stable preparation pass.
//!
//! Called from:
//! - Candidate preparation, proof refresh and path validation.
//!
//! Key details:
//! - Epoch words live beside the sorted forest and never own PHP values.
//! - A second iterative parent walk stamps the prefix without recursion or a path stack.
//! - Recounts advance the epoch before callbacks' changed graph is reconsidered.

use super::{load, op, reg, restore, save};
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits reusable memo storage and wrap-safe preparation epochs.
pub(super) fn emit(e: &mut Emitter) {
    reset(e);
    epoch(e);
}

/// Reuses one native word per sorted forest record, clearing stamps after a rebuild.
fn reset(e: &mut Emitter) {
    e.blank();
    e.comment("--- runtime: gc_proof_memo_reset ---");
    e.label_global("__rt_gc_proof_memo_reset");
    abi::emit_frame_prologue(e, 32);
    // -- reserve metadata without retaining any PHP node --
    load(e, "x0", "rax", "_gc_proofs_count");
    load(e, "x9", "r10", "_gc_proof_memos_capacity");
    op(e, "cmp x0, x9", "cmp rax, r10", "reuse the memo buffer when it covers every sorted record");
    op(e, "b.ls __rt_gc_proof_memo_clear", "jbe __rt_gc_proof_memo_clear", "avoid native allocation during ordinary validation");
    load(e, "x0", "rax", "_gc_proofs_capacity");
    save(e, 0);
    op(e, "lsl x1, x0, #3", "lea rsi, [rax * 8]", "allocate one epoch word for each reserved forest slot");
    load(e, "x0", "rdi", "_gc_proof_memos");
    abi::emit_call_label(e, "__rt_gc_candidate_realloc");
    abi::emit_branch_if_int_result_nonzero(e, "__rt_gc_proof_memo_allocated");
    abi::emit_jump(e, "__rt_heap_allocation_failed");
    e.label("__rt_gc_proof_memo_allocated");
    abi::emit_store_reg_to_symbol(e, abi::int_result_reg(e), "_gc_proof_memos", 0);
    restore(e, 0);
    abi::emit_store_reg_to_symbol(e, abi::int_result_reg(e), "_gc_proof_memos_capacity", 0);
    // -- stamps belong to the new sorted order, never the preceding forest --
    e.label("__rt_gc_proof_memo_clear");
    load(e, "x9", "r10", "_gc_proof_memos");
    load(e, "x10", "r11", "_gc_proofs_count");
    e.label("__rt_gc_proof_memo_clear_loop");
    op(e, "cbz x10, __rt_gc_proof_memo_clear_done", "test r11, r11", "stop before dereferencing an empty memo buffer");
    if e.target.arch == Arch::X86_64 { op(e, "", "jz __rt_gc_proof_memo_clear_done", "finish after every current record stamp was retired"); }
    op(e, "str xzr, [x9], #8", "mov QWORD PTR [r10], 0", "retire the preceding record's validation outcome");
    if e.target.arch == Arch::X86_64 { op(e, "", "add r10, 8", "advance one native memo word"); }
    op(e, "sub x10, x10, #1", "sub r11, 1", "consume one bounded memo slot");
    abi::emit_jump(e, "__rt_gc_proof_memo_clear_loop");
    e.label("__rt_gc_proof_memo_clear_done");
    abi::emit_frame_restore(e, 32);
    abi::emit_return(e);
}

/// Advances the pass generation, clearing ancient stamps before an epoch wraps.
fn epoch(e: &mut Emitter) {
    e.blank();
    e.comment("--- runtime: gc_proof_epoch_begin ---");
    e.label_global("__rt_gc_proof_epoch_begin");
    abi::emit_frame_prologue(e, 32);
    // -- even epochs distinguish unproved stamps from their proved low-bit variant --
    load(e, "x0", "rax", "_gc_proof_epoch");
    op(e, "add x0, x0, #2", "add rax, 2", "invalidate every previous pass without scanning the forest");
    abi::emit_branch_if_int_result_nonzero(e, "__rt_gc_proof_epoch_ready");
    abi::emit_call_label(e, "__rt_gc_proof_memo_reset");
    load(e, "x0", "rax", "_gc_proofs_count");
    load(e, "x9", "r10", "_gc_proofs_visited");
    op(e, "add x0, x0, x9", "add rax, r10", "report the rare wrap-time metadata scan as real work");
    abi::emit_store_reg_to_symbol(e, abi::int_result_reg(e), "_gc_proofs_visited", 0);
    abi::emit_load_int_immediate(e, abi::int_result_reg(e), 2);
    e.label("__rt_gc_proof_epoch_ready");
    abi::emit_store_reg_to_symbol(e, abi::int_result_reg(e), "_gc_proof_epoch", 0);
    abi::emit_frame_restore(e, 32);
    abi::emit_return(e);
}

/// Emits result propagation and iterative prefix stamping inside the validator's 64-byte frame.
pub(super) fn finish(e: &mut Emitter) {
    // -- both outcomes mean only whether this cached path proves current liveness --
    e.label("__rt_gc_proof_valid_missing");
    abi::emit_load_int_immediate(e, abi::int_result_reg(e), 0);
    save(e, 40);
    load(e, "x0", "rax", "_gc_proof_epoch");
    abi::emit_jump(e, "__rt_gc_proof_memo_init");
    e.label("__rt_gc_proof_valid_found");
    restore(e, 0);
    save(e, 40);
    load(e, "x0", "rax", "_gc_proof_epoch");
    op(e, "orr x0, x0, #1", "or rax, 1", "distinguish a proved path from an unproved path in this generation");
    e.label("__rt_gc_proof_memo_init");
    save(e, 32);
    restore(e, 0);
    save(e, 24);
    load(e, "x0", "rax", "_gc_proofs_count");
    op(e, "add x0, x0, #1", "add rax, 1", "bound the second walk by the immutable forest size");
    save(e, 16);
    // -- stamp every shared prefix node, without assuming a failed path is unreachable --
    e.label("__rt_gc_proof_memo_path_loop");
    restore(e, 16);
    op(e, "sub x0, x0, #1", "sub rax, 1", "consume one finite memo propagation step");
    save(e, 16);
    abi::emit_branch_if_int_result_zero(e, "__rt_gc_proof_valid_done");
    restore(e, 24);
    abi::emit_call_label(e, "__rt_gc_proof_lookup");
    abi::emit_branch_if_int_result_zero(e, "__rt_gc_proof_valid_done");
    load(e, "x9", "r10", "_gc_proof_memos");
    op(e, "ldr x10, [sp, #32]", "mov r11, QWORD PTR [rsp + 32]", "recover the outcome of the fully checked prefix");
    op(e, "str x10, [x9, x1, lsl #3]", "mov QWORD PTR [r10 + rdi * 8], r11", "memoize this forest record for this preparation pass only");
    load(e, "x9", "r10", "_gc_proofs_visited");
    op(e, "add x9, x9, #1", "add r10, 1", "count memo propagation rather than hiding its extra path walk");
    abi::emit_store_reg_to_symbol(e, reg(e, "x9", "r10"), "_gc_proofs_visited", 0);
    op(e, "ldr x10, [x0]", "mov r11, QWORD PTR [rax]", "recover the current immutable record key");
    op(e, "ldr x11, [sp, #8]", "mov rcx, QWORD PTR [rsp + 8]", "stop at the terminal node reached by the validation walk");
    op(e, "cmp x10, x11", "cmp r11, rcx", "include the terminal record without walking beyond the checked prefix");
    abi::emit_branch_if_equal_wide(e, "__rt_gc_proof_valid_done");
    op(e, "ldr x0, [x0, #8]", "mov rax, QWORD PTR [rax + 8]", "follow only immutable metadata during outcome propagation");
    save(e, 24);
    abi::emit_jump(e, "__rt_gc_proof_memo_path_loop");
    e.label("__rt_gc_proof_valid_done");
    restore(e, 40);
}
