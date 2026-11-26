//! Purpose:
//! Keeps a published persistent root and its complete managed graph alive while shutdown runs destructors.
//!
//! Called from:
//! - Persistent symbol and reference-cell drains before their final owner is detached.
//!
//! Key details:
//! - Reachable nodes are snapshotted and pinned before the first user callback.
//! - Candidate collection stays disabled and nested collection stays suppressed until every pin is retired.
//! - The result reports newly dispatched object destructors so callers can repeat after graph mutation.

use crate::layout::{CALLABLE_HEAP_KIND, REFERENCE_CELL_HEAP_KIND};
use crate::{abi, Arch, Emitter};

const FRAME: usize = 80;
const ROOT: usize = 8;
const SNAPSHOT: usize = 16;
const COUNT: usize = 24;
const INDEX: usize = 32;
const OLD_CANDIDATES: usize = 40;
const OLD_SUPPRESSION: usize = 48;
const DESTRUCTED: usize = 56;

/// Selects one allocation-free physical heap pass.
#[derive(Clone, Copy)]
enum ScanPhase {
    ClearStart,
    Count,
    Fill,
    ClearFinish,
}

impl ScanPhase {
    /// Returns the stable label component for this heap pass.
    fn label(self) -> &'static str {
        match self {
            Self::ClearStart => "clear_start",
            Self::Count => "count",
            Self::Fill => "fill",
            Self::ClearFinish => "clear_finish",
        }
    }

    /// Reports whether this pass removes transient reachability marks.
    fn clears_marks(self) -> bool {
        matches!(self, Self::ClearStart | Self::ClearFinish)
    }
}

/// Emits the graph snapshot helper for every supported target ABI.
pub fn emit_published_graph_destructors(emitter: &mut impl Emitter) {
    match emitter.arch() {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// Emits the AArch64 graph mark, snapshot, destructor, and unpin sequence.
fn emit_aarch64(emitter: &mut impl Emitter) {
    emitter.label_global("__rt_shutdown_destruct_graph");
    abi::emit_frame_prologue(emitter, FRAME);
    abi::store_at_offset(emitter, "x0", ROOT);
    for offset in [SNAPSHOT, COUNT, INDEX, DESTRUCTED] {
        abi::emit_store_zero_to_local_slot(emitter, offset);
    }
    abi::emit_load_symbol_to_reg(emitter, "x0", "_gc_candidates_enabled", 0);
    abi::store_at_offset(emitter, "x0", OLD_CANDIDATES);
    abi::emit_store_zero_to_symbol(emitter, "_gc_candidates_enabled", 0);
    abi::emit_load_symbol_to_reg(emitter, "x0", "_gc_release_suppressed", 0);
    abi::store_at_offset(emitter, "x0", OLD_SUPPRESSION);
    abi::emit_load_int_immediate(emitter, "x0", 1);
    abi::emit_store_reg_to_symbol(emitter, "x0", "_gc_release_suppressed", 0);

    emit_scan_aarch64(emitter, ScanPhase::ClearStart);
    abi::load_at_offset(emitter, "x0", ROOT);
    abi::emit_call_label(emitter, "__rt_gc_mark_reachable");
    emit_scan_aarch64(emitter, ScanPhase::Count);
    abi::load_at_offset(emitter, "x0", INDEX);
    emitter.instruction("cbz x0, __rt_shutdown_destruct_graph_finish");         // avoid a snapshot when the graph has no unfinished object
    abi::load_at_offset(emitter, "x0", COUNT);
    emitter.instruction("lsl x0, x0, #3");                                      // reserve one native pointer per marked graph node
    emitter.bl_c("malloc");
    emitter.instruction("cbnz x0, __rt_shutdown_destruct_graph_allocated");     // require complete protection before invoking PHP
    emitter.instruction("b __rt_heap_allocation_failed");                       // use the shared fatal allocation path
    emitter.label("__rt_shutdown_destruct_graph_allocated");
    abi::store_at_offset(emitter, "x0", SNAPSHOT);
    abi::emit_store_zero_to_local_slot(emitter, INDEX);
    emit_scan_aarch64(emitter, ScanPhase::Fill);

    abi::emit_store_zero_to_local_slot(emitter, INDEX);
    emitter.label("__rt_shutdown_destruct_graph_objects");
    abi::load_at_offset(emitter, "x9", INDEX);
    abi::load_at_offset(emitter, "x10", COUNT);
    emitter.instruction("cmp x9, x10");                                         // have all stable graph identities been inspected?
    emitter.instruction("b.hs __rt_shutdown_destruct_graph_unpin_init");        // retire pins only after every callback has completed
    abi::load_at_offset(emitter, "x10", SNAPSHOT);
    emitter.instruction("ldr x0, [x10, x9, lsl #3]");                           // borrow the next snapshot-owned payload
    emitter.instruction("ldr x11, [x0, #-8]");                                  // inspect physical kind and persistent destructor state
    emitter.instruction("and x12, x11, #0xff");                                 // isolate the managed heap kind
    emitter.instruction("cmp x12, #4");                                         // only object nodes dispatch PHP destructors
    emitter.instruction("b.ne __rt_shutdown_destruct_graph_object_next");       // containers remain pinned without a callback
    emitter.instruction(&format!(
        "tbnz x11, #14, __rt_shutdown_destruct_graph_object_next"
    ));                                                                         // never repeat a completed object destructor
    emitter.instruction("ldr w12, [x0, #-12]");                                 // inspect reentrant destructor state
    emitter.instruction("tbnz w12, #31, __rt_shutdown_destruct_graph_object_next"); // leave an active destructor to its current owner
    abi::load_at_offset(emitter, "x12", DESTRUCTED);
    emitter.instruction("add x12, x12, #1");                                    // report one newly dispatched object
    abi::store_at_offset(emitter, "x12", DESTRUCTED);
    crate::cleanup_preserve::emit_unary_cleanup_preserving_exception(
        emitter,
        "__rt_call_object_destructor",
        "x0",
    );
    abi::load_at_offset(emitter, "x9", INDEX);
    abi::load_at_offset(emitter, "x10", SNAPSHOT);
    emitter.instruction("ldr x0, [x10, x9, lsl #3]");                           // recover the receiver after the protected callback
    crate::values::destructor_header::emit_clear_running(emitter);
    emitter.label("__rt_shutdown_destruct_graph_object_next");
    abi::load_at_offset(emitter, "x9", INDEX);
    emitter.instruction("add x9, x9, #1");                                      // advance through the stable snapshot
    abi::store_at_offset(emitter, "x9", INDEX);
    emitter.instruction("b __rt_shutdown_destruct_graph_objects");              // continue without walking callback-mutated heap headers

    emitter.label("__rt_shutdown_destruct_graph_unpin_init");
    abi::emit_store_zero_to_local_slot(emitter, INDEX);
    emitter.label("__rt_shutdown_destruct_graph_unpin");
    abi::load_at_offset(emitter, "x9", INDEX);
    abi::load_at_offset(emitter, "x10", COUNT);
    emitter.instruction("cmp x9, x10");                                         // have all artificial graph owners been retired?
    emitter.instruction("b.hs __rt_shutdown_destruct_graph_free");              // dispose native storage after the final protected release
    abi::load_at_offset(emitter, "x10", SNAPSHOT);
    emitter.instruction("ldr x0, [x10, x9, lsl #3]");                           // transfer one artificial graph owner to cleanup
    crate::cleanup_preserve::emit_unary_cleanup_preserving_exception(
        emitter,
        "__rt_decref_any",
        "x0",
    );
    abi::load_at_offset(emitter, "x9", INDEX);
    emitter.instruction("add x9, x9, #1");                                      // advance only after cleanup has balanced the owner
    abi::store_at_offset(emitter, "x9", INDEX);
    emitter.instruction("b __rt_shutdown_destruct_graph_unpin");                // keep the published root visible through every nested release
    emitter.label("__rt_shutdown_destruct_graph_free");
    abi::load_at_offset(emitter, "x0", SNAPSHOT);
    emitter.bl_c("free");

    emitter.label("__rt_shutdown_destruct_graph_finish");
    emit_scan_aarch64(emitter, ScanPhase::ClearFinish);
    abi::load_at_offset(emitter, "x9", OLD_CANDIDATES);
    abi::emit_store_reg_to_symbol(emitter, "x9", "_gc_candidates_enabled", 0);
    abi::load_at_offset(emitter, "x9", OLD_SUPPRESSION);
    abi::emit_store_reg_to_symbol(emitter, "x9", "_gc_release_suppressed", 0);
    abi::load_at_offset(emitter, "x0", DESTRUCTED);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);
}

/// Emits one AArch64 heap pass without invoking user code or moving the heap.
fn emit_scan_aarch64(emitter: &mut impl Emitter, phase: ScanPhase) {
    let label = phase.label();
    let loop_label = format!("__rt_shutdown_destruct_graph_{label}_loop");
    let known = format!("__rt_shutdown_destruct_graph_{label}_known");
    let ready = format!("__rt_shutdown_destruct_graph_{label}_ready");
    let next = format!("__rt_shutdown_destruct_graph_{label}_next");
    let done = format!("__rt_shutdown_destruct_graph_{label}_done");
    abi::emit_symbol_address(emitter, "x9", "_heap_buf");
    abi::emit_load_symbol_to_reg(emitter, "x10", "_heap_off", 0);
    abi::emit_symbol_address(emitter, "x9", "_heap_buf");
    emitter.instruction("add x10, x9, x10");                                    // bound this allocation-free pass by the current heap extent
    emitter.label(&loop_label);
    emitter.instruction("cmp x9, x10");                                         // has the physical heap cursor reached its bound?
    emitter.instruction(&format!("b.hs {done}"));                               // finish before reading outside managed storage
    emitter.instruction("ldr w11, [x9]");                                       // preserve payload size for the next physical block
    emitter.instruction("ldr w12, [x9, #4]");                                   // inspect whether this block is currently live
    emitter.instruction(&format!("cbz w12, {next}"));                           // free-list blocks carry no managed graph owner
    emitter.instruction("ldr x13, [x9, #8]");                                   // load kind plus persistent and transient metadata
    emitter.instruction("and x14, x13, #0xff");                                 // isolate the physical heap kind
    emitter.instruction("cmp x14, #2");                                         // graph containers start at indexed arrays
    emitter.instruction(&format!("b.lo {next}"));                               // raw blocks and strings are not traversed
    emitter.instruction("cmp x14, #5");                                         // ordinary graph containers end at boxed Mixed
    emitter.instruction(&format!("b.ls {known}"));                              // admit array, hash, object, and Mixed nodes
    emitter.instruction(&format!("cmp x14, #{CALLABLE_HEAP_KIND}"));            // callable descriptors own capture graphs
    emitter.instruction(&format!("b.eq {ready}"));                              // admit callable graph nodes
    emitter.instruction(&format!("cmp x14, #{REFERENCE_CELL_HEAP_KIND}"));      // reference cells own typed graph children
    emitter.instruction(&format!("b.eq {ready}"));                              // admit reference-cell graph nodes
    emitter.instruction(&format!("b {next}"));                                  // ignore unrelated managed block kinds
    emitter.label(&known);
    emitter.label(&ready);
    if !phase.clears_marks() {
        emitter.instruction(&format!("tbz x13, #16, {next}"));                  // select only nodes marked from the published root
    }
    match phase {
        ScanPhase::ClearStart | ScanPhase::ClearFinish => {
            emitter.instruction("bic x13, x13, #0x10000");                      // clear only the transient reachability bit
            emitter.instruction("str x13, [x9, #8]");                           // preserve kind and all persistent metadata
        }
        ScanPhase::Count => {
            abi::load_at_offset(emitter, "x15", COUNT);
            emitter.instruction("add x15, x15, #1");                            // reserve one snapshot slot for this marked node
            abi::store_at_offset(emitter, "x15", COUNT);
            emitter.instruction("cmp x14, #4");                                 // only objects can require a destructor callback
            emitter.instruction(&format!("b.ne {next}"));                       // other graph nodes need pins but no callback
            emitter.instruction(&format!("tbnz x13, #14, {next}"));             // completed destructors need no new pass
            emitter.instruction(&format!("tbnz w12, #31, {next}"));             // do not reenter an active destructor
            abi::emit_load_int_immediate(emitter, "x15", 1);
            abi::store_at_offset(emitter, "x15", INDEX);
        }
        ScanPhase::Fill => {
            emitter.instruction("add w12, w12, #1");                            // acquire one artificial owner before any callback
            emitter.instruction("str w12, [x9, #4]");                           // publish the pin in the uniform header
            abi::load_at_offset(emitter, "x15", SNAPSHOT);
            abi::load_at_offset(emitter, "x16", INDEX);
            emitter.instruction("add x17, x9, #16");                            // snapshot the stable payload identity
            emitter.instruction("str x17, [x15, x16, lsl #3]");                 // retain the identity outside the PHP heap
            emitter.instruction("add x16, x16, #1");                            // advance the fill position
            abi::store_at_offset(emitter, "x16", INDEX);
        }
    }
    emitter.label(&next);
    emitter.instruction("add x9, x9, x11");                                     // skip the current payload
    emitter.instruction("add x9, x9, #16");                                     // skip the uniform heap header
    emitter.instruction(&format!("b {loop_label}"));                            // continue the physical heap pass
    emitter.label(&done);
}

/// Emits the System V x86_64 graph mark, snapshot, destructor, and unpin sequence.
fn emit_x86_64(emitter: &mut impl Emitter) {
    emitter.label_global("__rt_shutdown_destruct_graph");
    abi::emit_frame_prologue(emitter, FRAME);
    abi::store_at_offset(emitter, "rdi", ROOT);
    for offset in [SNAPSHOT, COUNT, INDEX, DESTRUCTED] {
        abi::emit_store_zero_to_local_slot(emitter, offset);
    }
    abi::emit_load_symbol_to_reg(emitter, "rax", "_gc_candidates_enabled", 0);
    abi::store_at_offset(emitter, "rax", OLD_CANDIDATES);
    abi::emit_store_zero_to_symbol(emitter, "_gc_candidates_enabled", 0);
    abi::emit_load_symbol_to_reg(emitter, "rax", "_gc_release_suppressed", 0);
    abi::store_at_offset(emitter, "rax", OLD_SUPPRESSION);
    abi::emit_load_int_immediate(emitter, "rax", 1);
    abi::emit_store_reg_to_symbol(emitter, "rax", "_gc_release_suppressed", 0);

    emit_scan_x86_64(emitter, ScanPhase::ClearStart);
    abi::load_at_offset(emitter, "rax", ROOT);
    abi::emit_call_label(emitter, "__rt_gc_mark_reachable");
    emit_scan_x86_64(emitter, ScanPhase::Count);
    abi::load_at_offset(emitter, "rax", INDEX);
    emitter.instruction("test rax, rax");                                       // inspect whether the graph owns an unfinished object
    emitter.instruction("jz __rt_shutdown_destruct_graph_finish");              // avoid a native snapshot when no callback is needed
    abi::load_at_offset(emitter, "rdi", COUNT);
    emitter.instruction("shl rdi, 3");                                          // reserve one native pointer per marked graph node
    emitter.bl_c("malloc");
    emitter.instruction("test rax, rax");                                       // require complete protection before invoking PHP
    emitter.instruction("jnz __rt_shutdown_destruct_graph_allocated");          // continue only with a valid snapshot
    emitter.instruction("jmp __rt_heap_allocation_failed");                     // use the shared fatal allocation path
    emitter.label("__rt_shutdown_destruct_graph_allocated");
    abi::store_at_offset(emitter, "rax", SNAPSHOT);
    abi::emit_store_zero_to_local_slot(emitter, INDEX);
    emit_scan_x86_64(emitter, ScanPhase::Fill);

    abi::emit_store_zero_to_local_slot(emitter, INDEX);
    emitter.label("__rt_shutdown_destruct_graph_objects");
    abi::load_at_offset(emitter, "rax", INDEX);
    abi::load_at_offset(emitter, "r10", COUNT);
    emitter.instruction("cmp rax, r10");                                        // have all stable graph identities been inspected?
    emitter.instruction("jae __rt_shutdown_destruct_graph_unpin_init");         // retire pins only after every callback has completed
    abi::load_at_offset(emitter, "rcx", SNAPSHOT);
    emitter.instruction("mov rdi, QWORD PTR [rcx + rax * 8]");                  // borrow the next snapshot-owned payload
    emitter.instruction("mov r11, QWORD PTR [rdi - 8]");                        // inspect physical kind and persistent destructor state
    emitter.instruction("mov r10, r11");                                        // preserve the full metadata word
    emitter.instruction("and r10d, 0xff");                                      // isolate the managed heap kind
    emitter.instruction("cmp r10d, 4");                                         // only object nodes dispatch PHP destructors
    emitter.instruction("jne __rt_shutdown_destruct_graph_object_next");        // containers remain pinned without a callback
    emitter.instruction(&format!(
        "test r11, {:#x}",
        crate::values::destructor_header::FINISHED_MASK
    ));                                                                         // inspect persistent destructor completion
    emitter.instruction("jnz __rt_shutdown_destruct_graph_object_next");        // never repeat a completed object destructor
    emitter.instruction(&format!(
        "test DWORD PTR [rdi - 12], {:#x}",
        crate::values::destructor_header::RUNNING_MASK
    ));                                                                         // inspect the temporary destructor reentrancy guard
    emitter.instruction("jnz __rt_shutdown_destruct_graph_object_next");        // leave an active destructor to its current owner
    emitter.instruction(&format!("add QWORD PTR [rbp - {DESTRUCTED}], 1"));     // report one newly dispatched object
    crate::cleanup_preserve::emit_unary_cleanup_preserving_exception(
        emitter,
        "__rt_call_object_destructor",
        "rdi",
    );
    abi::load_at_offset(emitter, "rax", INDEX);
    abi::load_at_offset(emitter, "rcx", SNAPSHOT);
    emitter.instruction("mov rdi, QWORD PTR [rcx + rax * 8]");                  // recover the receiver after the protected callback
    crate::values::destructor_header::emit_clear_running(emitter);
    emitter.label("__rt_shutdown_destruct_graph_object_next");
    emitter.instruction(&format!("add QWORD PTR [rbp - {INDEX}], 1"));          // advance through the stable snapshot
    emitter.instruction("jmp __rt_shutdown_destruct_graph_objects");            // continue without walking callback-mutated heap headers

    emitter.label("__rt_shutdown_destruct_graph_unpin_init");
    abi::emit_store_zero_to_local_slot(emitter, INDEX);
    emitter.label("__rt_shutdown_destruct_graph_unpin");
    abi::load_at_offset(emitter, "rax", INDEX);
    abi::load_at_offset(emitter, "r10", COUNT);
    emitter.instruction("cmp rax, r10");                                        // have all artificial graph owners been retired?
    emitter.instruction("jae __rt_shutdown_destruct_graph_free");               // dispose native storage after the final protected release
    abi::load_at_offset(emitter, "rcx", SNAPSHOT);
    emitter.instruction("mov rax, QWORD PTR [rcx + rax * 8]");                  // transfer one artificial graph owner to cleanup
    crate::cleanup_preserve::emit_unary_cleanup_preserving_exception(
        emitter,
        "__rt_decref_any",
        "rax",
    );
    emitter.instruction(&format!("add QWORD PTR [rbp - {INDEX}], 1"));          // advance only after cleanup has balanced the owner
    emitter.instruction("jmp __rt_shutdown_destruct_graph_unpin");              // keep the published root visible through every nested release
    emitter.label("__rt_shutdown_destruct_graph_free");
    abi::load_at_offset(emitter, "rdi", SNAPSHOT);
    emitter.bl_c("free");

    emitter.label("__rt_shutdown_destruct_graph_finish");
    emit_scan_x86_64(emitter, ScanPhase::ClearFinish);
    abi::load_at_offset(emitter, "r10", OLD_CANDIDATES);
    abi::emit_store_reg_to_symbol(emitter, "r10", "_gc_candidates_enabled", 0);
    abi::load_at_offset(emitter, "r10", OLD_SUPPRESSION);
    abi::emit_store_reg_to_symbol(emitter, "r10", "_gc_release_suppressed", 0);
    abi::load_at_offset(emitter, "rax", DESTRUCTED);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);
}

/// Emits one x86_64 heap pass without invoking user code or moving the heap.
fn emit_scan_x86_64(emitter: &mut impl Emitter, phase: ScanPhase) {
    let label = phase.label();
    let loop_label = format!("__rt_shutdown_destruct_graph_{label}_loop");
    let known = format!("__rt_shutdown_destruct_graph_{label}_known");
    let ready = format!("__rt_shutdown_destruct_graph_{label}_ready");
    let next = format!("__rt_shutdown_destruct_graph_{label}_next");
    let done = format!("__rt_shutdown_destruct_graph_{label}_done");
    abi::emit_symbol_address(emitter, "r8", "_heap_buf");
    abi::emit_load_symbol_to_reg(emitter, "r9", "_heap_off", 0);
    emitter.instruction("add r9, r8");                                          // bound this allocation-free pass by the current heap extent
    emitter.label(&loop_label);
    emitter.instruction("cmp r8, r9");                                          // has the physical heap cursor reached its bound?
    emitter.instruction(&format!("jae {done}"));                                // finish before reading outside managed storage
    emitter.instruction("mov r10d, DWORD PTR [r8]");                            // preserve payload size for the next physical block
    emitter.instruction("mov ecx, DWORD PTR [r8 + 4]");                         // inspect whether this block is currently live
    emitter.instruction("test ecx, ecx");                                       // free-list blocks carry no managed graph owner
    emitter.instruction(&format!("jz {next}"));                                 // skip dead physical blocks
    emitter.instruction("mov r11, QWORD PTR [r8 + 8]");                         // load kind plus persistent and transient metadata
    emitter.instruction("mov edx, r11d");                                       // retain the full word while isolating its kind
    emitter.instruction("and edx, 0xff");                                       // isolate the physical heap kind
    emitter.instruction("cmp edx, 2");                                          // graph containers start at indexed arrays
    emitter.instruction(&format!("jb {next}"));                                 // raw blocks and strings are not traversed
    emitter.instruction("cmp edx, 5");                                          // ordinary graph containers end at boxed Mixed
    emitter.instruction(&format!("jbe {known}"));                               // admit array, hash, object, and Mixed nodes
    emitter.instruction(&format!("cmp edx, {CALLABLE_HEAP_KIND}"));             // callable descriptors own capture graphs
    emitter.instruction(&format!("je {ready}"));                                // admit callable graph nodes
    emitter.instruction(&format!("cmp edx, {REFERENCE_CELL_HEAP_KIND}"));       // reference cells own typed graph children
    emitter.instruction(&format!("je {ready}"));                                // admit reference-cell graph nodes
    emitter.instruction(&format!("jmp {next}"));                                // ignore unrelated managed block kinds
    emitter.label(&known);
    emitter.label(&ready);
    if !phase.clears_marks() {
        emitter.instruction("test r11, 0x10000");                               // inspect reachability from the published root
        emitter.instruction(&format!("jz {next}"));                             // skip every unmarked graph node
    }
    match phase {
        ScanPhase::ClearStart | ScanPhase::ClearFinish => {
            emitter.instruction("and QWORD PTR [r8 + 8], -65537");              // clear only the transient reachability bit
        }
        ScanPhase::Count => {
            emitter.instruction(&format!("add QWORD PTR [rbp - {COUNT}], 1"));  // reserve one snapshot slot for this marked node
            emitter.instruction("cmp edx, 4");                                  // only objects can require a destructor callback
            emitter.instruction(&format!("jne {next}"));                        // other graph nodes need pins but no callback
            emitter.instruction(&format!(
                "test r11, {:#x}",
                crate::values::destructor_header::FINISHED_MASK
            ));                                                                 // inspect persistent destructor completion
            emitter.instruction(&format!("jnz {next}"));                        // completed destructors need no new pass
            emitter.instruction(&format!(
                "test ecx, {:#x}",
                crate::values::destructor_header::RUNNING_MASK
            ));                                                                 // inspect the temporary destructor reentrancy guard
            emitter.instruction(&format!("jnz {next}"));                        // do not reenter an active destructor
            emitter.instruction(&format!("mov QWORD PTR [rbp - {INDEX}], 1"));  // record that a callback pass is required
        }
        ScanPhase::Fill => {
            emitter.instruction("add DWORD PTR [r8 + 4], 1");                   // acquire one artificial owner before any callback
            abi::load_at_offset(emitter, "rcx", SNAPSHOT);
            abi::load_at_offset(emitter, "rax", INDEX);
            emitter.instruction("lea rdx, [r8 + 16]");                          // snapshot the stable payload identity
            emitter.instruction("mov QWORD PTR [rcx + rax * 8], rdx");          // retain the identity outside the PHP heap
            emitter.instruction(&format!("add QWORD PTR [rbp - {INDEX}], 1"));  // advance the fill position
        }
    }
    emitter.label(&next);
    emitter.instruction("lea r8, [r8 + r10 + 16]");                             // advance past payload and uniform header
    emitter.instruction(&format!("jmp {loop_label}"));                          // continue the physical heap pass
    emitter.label(&done);
}
