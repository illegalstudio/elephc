//! Purpose:
//! Builds a non-owning spanning forest of the current application graph.
//!
//! Called from:
//! - Worker bootstrap, the first candidate collection, and request-boundary cache refresh.
//!
//! Key details:
//! - Breadth-first discovery reuses the reviewed one-hop GC visitor and temporary anchor bit.
//! - PHP callbacks do not run during discovery, sorting or marker retirement.
//! - Geometric libc storage is reused across refreshes and never becomes a PHP owner.

use super::{load, op, reg, restore, save};
use crate::codegen_support::{abi, emit::Emitter, platform::Arch, sentinels::{REFERENCE_CELL_HEAP_KIND, CALLABLE_HEAP_KIND}};

/// Emits append, refresh, comparison and request-boundary cache maintenance helpers.
pub(super) fn emit(e: &mut Emitter) {
    record(e);
    refresh(e);
    compare(e);
    pending(e);
}

/// Appends the first discovered owning edge for one graph identity.
fn record(e: &mut Emitter) {
    e.blank();
    e.comment("--- runtime: gc_proof_record ---");
    e.label_global("__rt_gc_proof_record");
    abi::emit_frame_prologue(e, 32);
    save(e, 0);
    // -- validate identity and graph representation before marking discovery membership --
    load(e, "x9", "r10", "_gc_proof_slot");
    op(e, "cmn x9, #1", "cmp r10, -1", "opaque callback edges have no cacheable native slot");
    abi::emit_branch_if_equal_wide(e, "__rt_gc_proof_record_done");
    abi::emit_call_label(e, "__rt_gc_candidate_live");
    abi::emit_branch_if_int_result_zero(e, "__rt_gc_proof_record_done");
    op(e, "ldrb w9, [x0, #-8]", "movzx r10d, BYTE PTR [rax - 8]", "exclude raw storage and strings from the spanning forest");
    op(e, &format!("cmp w9, #{CALLABLE_HEAP_KIND}"), &format!("cmp r10d, {CALLABLE_HEAP_KIND}"), "recognize a managed callable node");
    abi::emit_branch_if_equal_wide(e, "__rt_gc_proof_record_kind");
    op(e, &format!("cmp w9, #{REFERENCE_CELL_HEAP_KIND}"), &format!("cmp r10d, {REFERENCE_CELL_HEAP_KIND}"), "recognize a managed reference node");
    abi::emit_branch_if_equal_wide(e, "__rt_gc_proof_record_kind");
    op(e, "cmp w9, #2", "cmp r10d, 2", "bound the ordinary graph allocation kinds");
    op(e, "b.lo __rt_gc_proof_record_done", "jb __rt_gc_proof_record_done", "ignore scalar storage without outgoing graph ownership");
    op(e, "cmp w9, #5", "cmp r10d, 5", "exclude unrelated native resource representations");
    op(e, "b.hi __rt_gc_proof_record_done", "ja __rt_gc_proof_record_done", "keep unsupported shapes on ordinary collection paths");
    e.label("__rt_gc_proof_record_kind");
    op(e, "ldr x9, [x0, #-8]", "bts DWORD PTR [rax - 8], 20", "claim temporary discovery membership without owning the value");
    op(e, "tbnz x9, #20, __rt_gc_proof_record_done", "jc __rt_gc_proof_record_done", "the first incoming edge defines an acyclic spanning-tree parent");
    if e.target.arch == Arch::AArch64 {
        op(e, "orr x9, x9, #0x100000", "", "mark the identity before following any cyclic descendants");
        op(e, "str x9, [x0, #-8]", "", "publish discovery membership for subsequent parent visits");
    }
    // -- the rebuild budget includes every slot the recorded node's visitor will inspect --
    abi::emit_call_label(e, "__rt_gc_node_scan_cost");
    load(e, "x9", "r10", "_gc_proofs_cost");
    op(e, "adds x9, x9, x0", "add r10, rax", "accumulate node and slot work rather than only recorded graph identities");
    op(e, "b.cc __rt_gc_proof_cost_ready", "jnc __rt_gc_proof_cost_ready", "preserve an exact rebuild charge when it fits");
    op(e, "mov x9, #-1", "mov r10, -1", "saturate the structural work charge instead of undercounting it");
    e.label("__rt_gc_proof_cost_ready");
    abi::emit_store_reg_to_symbol(e, reg(e, "x9", "r10"), "_gc_proofs_cost", 0);
    restore(e, 0);
    // -- grow only raw metadata, preserving the candidate across libc --
    load(e, "x9", "r10", "_gc_proofs_count");
    load(e, "x10", "r11", "_gc_proofs_capacity");
    op(e, "cmp x9, x10", "cmp r10, r11", "test whether another three-word record fits the current buffer");
    op(e, "b.lo __rt_gc_proof_record_ready", "jb __rt_gc_proof_record_ready", "append without allocation when capacity remains");
    op(e, "lsl x10, x10, #1", "shl r11, 1", "amortize metadata growth across retained graph identities");
    op(e, "cmp x10, #256", "cmp r11, 256", "provide a useful initial record capacity");
    op(e, "b.hs __rt_gc_proof_record_capacity", "jae __rt_gc_proof_record_capacity", "keep an already sufficient doubled capacity");
    op(e, "mov x10, #256", "mov r11, 256", "initialize the first metadata allocation");
    e.label("__rt_gc_proof_record_capacity");
    abi::emit_store_reg_to_symbol(e, reg(e, "x10", "r11"), "_gc_proofs_capacity", 0);
    op(e, "add x1, x10, x10, lsl #1", "lea rsi, [r11 + r11 * 2]", "reserve three words for every proof record");
    op(e, "lsl x1, x1, #3", "shl rsi, 3", "convert record capacity into bytes for libc");
    load(e, "x0", "rdi", "_gc_proofs");
    abi::emit_call_label(e, "__rt_gc_candidate_realloc");
    abi::emit_branch_if_int_result_nonzero(e, "__rt_gc_proof_record_allocated");
    abi::emit_jump(e, "__rt_heap_allocation_failed");
    e.label("__rt_gc_proof_record_allocated");
    abi::emit_store_reg_to_symbol(e, abi::int_result_reg(e), "_gc_proofs", 0);
    e.label("__rt_gc_proof_record_ready");
    restore(e, 0);
    load(e, "x9", "r10", "_gc_proofs_count");
    op(e, "add x9, x9, x9, lsl #1", "lea r10, [r10 + r10 * 2]", "scale the append position by the record width");
    load(e, "x10", "r11", "_gc_proofs");
    op(e, "add x10, x10, x9, lsl #3", "lea r11, [r11 + r10 * 8]", "address the unused record slot");
    op(e, "str x0, [x10]", "mov QWORD PTR [r11], rax", "record the exact child base without retaining it");
    load(e, "x9", "r10", "_gc_proof_parent");
    op(e, "str x9, [x10, #8]", "mov QWORD PTR [r11 + 8], r10", "record the previously discovered non-owning parent");
    load(e, "x9", "r10", "_gc_proof_slot");
    op(e, "str x9, [x10, #16]", "mov QWORD PTR [r11 + 16], r10", "record the logical slot to validate against future layout changes");
    load(e, "x9", "r10", "_gc_proofs_count");
    op(e, "add x9, x9, #1", "add r10, 1", "publish the new spanning-forest length");
    abi::emit_store_reg_to_symbol(e, reg(e, "x9", "r10"), "_gc_proofs_count", 0);
    e.label("__rt_gc_proof_record_done");
    restore(e, 0);
    abi::emit_frame_restore(e, 32);
    abi::emit_return(e);
}

/// Rebuilds the cache from actual roots and leaves fresh root anchors for candidate preparation.
fn refresh(e: &mut Emitter) {
    e.blank();
    e.comment("--- runtime: gc_proofs_refresh ---");
    e.label_global("__rt_gc_proofs_refresh");
    abi::emit_frame_prologue(e, 32);
    // -- reuse temporary anchors only while no PHP callback can mutate the graph --
    abi::emit_call_label(e, "__rt_gc_clear_candidate_anchors");
    abi::emit_store_zero_to_symbol(e, "_gc_proofs_count", 0);
    abi::emit_store_zero_to_symbol(e, "_gc_proofs_cost", 0);
    abi::emit_store_imm_to_symbol(e, "_gc_candidates_gathering", 0, 3);
    roots(e, "build");
    abi::emit_load_int_immediate(e, abi::int_result_reg(e), 0);
    save(e, 0);
    e.label("__rt_gc_proofs_build_loop");
    cursor(e);
    op(e, "b.hs __rt_gc_proofs_clear_init", "jae __rt_gc_proofs_clear_init", "finish discovery when the breadth-first frontier is exhausted");
    node(e);
    abi::emit_store_reg_to_symbol(e, abi::int_result_reg(e), "_gc_proof_parent", 0);
    abi::emit_call_label(e, "__rt_gc_visit_children");
    increment(e);
    abi::emit_jump(e, "__rt_gc_proofs_build_loop");
    // -- retire every discovery marker before incoming counts can reuse its bits --
    e.label("__rt_gc_proofs_clear_init");
    load(e, "x0", "rax", "_gc_proofs_cost");
    load(e, "x9", "r10", "_gc_proofs_visited");
    op(e, "add x0, x0, x9", "add rax, r10", "include complete cache rebuilds in the cumulative work diagnostic");
    abi::emit_store_reg_to_symbol(e, abi::int_result_reg(e), "_gc_proofs_visited", 0);
    abi::emit_load_int_immediate(e, abi::int_result_reg(e), 0);
    save(e, 0);
    e.label("__rt_gc_proofs_clear_loop");
    cursor(e);
    op(e, "b.hs __rt_gc_proofs_sort", "jae __rt_gc_proofs_sort", "sort only after every temporary discovery mark was retired");
    node(e);
    op(e, "ldr x9, [x0, #-8]", "and DWORD PTR [rax - 8], 0xffefffff", "retire the temporary discovery bit without changing PHP ownership");
    if e.target.arch == Arch::AArch64 {
        op(e, "bic x9, x9, #0x100000", "", "clear discovery membership before restoring actual root anchors");
        op(e, "str x9, [x0, #-8]", "", "publish the retired temporary mark");
    }
    increment(e);
    abi::emit_jump(e, "__rt_gc_proofs_clear_loop");
    // -- immutable address ordering makes later pure reads logarithmic in cache lookup --
    e.label("__rt_gc_proofs_sort");
    load(e, "x0", "rdi", "_gc_proofs");
    load(e, "x1", "rsi", "_gc_proofs_count");
    abi::emit_load_int_immediate(e, reg(e, "x2", "rdx"), 24);
    abi::emit_symbol_address(e, reg(e, "x3", "rcx"), "__rt_gc_proof_compare");
    e.bl_c("qsort"); // make exact payload lookup logarithmic without changing graph ownership
    abi::emit_call_label(e, "__rt_gc_proof_memo_reset");
    load(e, "x0", "rax", "_gc_proofs_count");
    load(e, "x9", "r10", "_gc_proofs_visited");
    op(e, "add x0, x0, x9", "add rax, r10", "include the metadata stamp reset in cumulative rebuild work");
    abi::emit_store_reg_to_symbol(e, abi::int_result_reg(e), "_gc_proofs_visited", 0);
    abi::emit_store_imm_to_symbol(e, "_gc_proofs_initialized", 0, 1);
    abi::emit_store_zero_to_symbol(e, "_gc_proofs_debt", 0);
    abi::emit_store_imm_to_symbol(e, "_gc_candidates_gathering", 0, 2);
    roots(e, "restore");
    abi::emit_frame_restore(e, 32);
    abi::emit_return(e);
}

/// Invokes the compiler-published current-root visitor without assuming it exists.
fn roots(e: &mut Emitter, phase: &str) {
    let done = format!("__rt_gc_proofs_{phase}_roots_done");
    load(e, "x9", "r10", "_gc_candidate_root_scan_fn");
    op(e, &format!("cbz x9, {done}"), "test r10, r10", "skip unavailable per-program root metadata");
    if e.target.arch == Arch::X86_64 { op(e, "", &format!("jz {done}"), "leave an empty forest when no root visitor was installed"); }
    op(e, "blr x9", "call r10", "borrow actual initialized global and static owners");
    e.label(&done);
}

/// Compares two borrowed records using the libc qsort callback ABI.
fn compare(e: &mut Emitter) {
    e.blank();
    e.comment("--- runtime: gc_proof_compare ---");
    e.label_global("__rt_gc_proof_compare");
    // -- sort by unsigned payload identity without subtracting potentially distant addresses --
    op(e, "ldr x9, [x0]", "mov r8, QWORD PTR [rdi]", "read the first record's non-owning payload key");
    op(e, "ldr x10, [x1]", "mov r9, QWORD PTR [rsi]", "read the second record's non-owning payload key");
    op(e, "cmp x9, x10", "cmp r8, r9", "order exact bases using unsigned pointer comparison");
    op(e, "cset w0, hi", "seta al", "produce the positive ordering component");
    op(e, "cset w9, lo", "setb dl", "produce the negative ordering component");
    if e.target.arch == Arch::X86_64 {
        op(e, "", "movzx eax, al", "widen the positive comparison result to C int");
        op(e, "", "movzx edx, dl", "widen the negative comparison result to C int");
    }
    op(e, "sub w0, w0, w9", "sub eax, edx", "return exactly minus one, zero or one without pointer overflow");
    abi::emit_return(e);
}

/// Amortizes cache reconstruction against already traced surviving candidate identities.
fn pending(e: &mut Emitter) {
    e.blank();
    e.comment("--- runtime: gc_proofs_refresh_pending ---");
    e.label_global("__rt_gc_proofs_refresh_pending");
    load(e, "x9", "r10", "_gc_proofs_debt");
    op(e, "cbz x9, __rt_gc_proofs_pending_done", "test r10, r10", "skip cache construction when no surviving candidate work has accrued");
    if e.target.arch == Arch::X86_64 { op(e, "", "jz __rt_gc_proofs_pending_done", "keep cache maintenance outside a graph with no unproved surviving candidates"); }
    // -- a small retained write cannot independently force a walk of unrelated application state --
    load(e, "x10", "r11", "_gc_proofs_cost");
    op(e, "cmp x10, #256", "cmp r11, 256", "require a useful minimum amount of earned cache work");
    op(e, "b.hs __rt_gc_proofs_debt_threshold_ready", "jae __rt_gc_proofs_debt_threshold_ready", "use the previous structural scan cost as the rebuild charge for large graphs");
    op(e, "mov x10, #256", "mov r11, 256", "avoid frequent rebuilds for a tiny retained graph");
    e.label("__rt_gc_proofs_debt_threshold_ready");
    op(e, "cmp x9, x10", "cmp r10, r11", "compare actual surviving-candidate work with the complete forest rebuild cost");
    op(e, "b.lo __rt_gc_proofs_pending_done", "jb __rt_gc_proofs_pending_done", "continue immediate candidate collection while retaining the earned cache budget");
    abi::emit_frame_prologue(e, 16);
    abi::emit_call_label(e, "__rt_gc_proofs_refresh");
    abi::emit_call_label(e, "__rt_gc_clear_candidate_anchors");
    abi::emit_store_zero_to_symbol(e, "_gc_candidates_gathering", 0);
    abi::emit_frame_restore(e, 16);
    e.label("__rt_gc_proofs_pending_done");
    abi::emit_return(e);
}

/// Compares the private traversal index with the current growing record count.
fn cursor(e: &mut Emitter) {
    restore(e, 0);
    load(e, "x9", "r10", "_gc_proofs_count");
    op(e, "cmp x0, x9", "cmp rax, r10", "bound this breadth-first cursor by the current record count");
}

/// Resolves the node belonging to the current breadth-first record index.
fn node(e: &mut Emitter) {
    op(e, "add x9, x0, x0, lsl #1", "lea r10, [rax + rax * 2]", "scale the traversal index by three record words");
    load(e, "x10", "r11", "_gc_proofs");
    op(e, "ldr x0, [x10, x9, lsl #3]", "mov rax, QWORD PTR [r11 + r10 * 8]", "borrow the next discovered graph identity");
}

/// Advances the private cursor without assuming nested discovery preserved registers.
fn increment(e: &mut Emitter) {
    restore(e, 0);
    op(e, "add x0, x0, #1", "add rax, 1", "advance to the next discovered graph record");
    save(e, 0);
}
