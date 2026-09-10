//! Purpose:
//! Runs cyclic object destructors before the collector releases any graph storage.
//!
//! Called from:
//! - Both target implementations of the shared cycle collector.
//!
//! Key details:
//! - Destructors can read every property in the doomed graph during this phase.
//! - Newly allocated objects are outside the current scan's candidate set.
//! - Pending exceptions remain in the enclosing resumable cleanup scope.

use crate::codegen_support::{emit::Emitter, platform::Arch};
use crate::codegen_support::runtime::exceptions::deep_cleanup::Scope;

#[cfg(test)]
mod tests;

/// Visits only unreachable objects captured before destructors started executing.
pub(super) fn emit(emitter: &mut Emitter, cleanup: Scope) {
    emitter.label("__rt_gc_collect_cycles_destruct_init");
    emitter.instruction(if emitter.target.arch == Arch::AArch64 { "ldr x9, [sp, #96]" } else { "cmp QWORD PTR [rbp - 72], 0" });// inspect whether destructor mutations have already been rescanned
    emitter.instruction(if emitter.target.arch == Arch::AArch64 { "cbnz x9, __rt_gc_collect_cycles_free_init" } else { "jne __rt_gc_collect_cycles_free_init" });// enter reclamation only after recomputing live roots
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction("ldr x9, [sp, #16]");                               // recover the heap base captured before graph traversal
        emitter.instruction("str x9, [sp]");                                    // start the destructor phase at the first captured header
        emitter.label("__rt_gc_collect_cycles_destruct_loop");
        emitter.instruction("ldr x9, [sp]");                                    // recover the next header before any nested PHP execution
        emitter.instruction("ldr x10, [sp, #8]");                               // limit destructors to the original heap window
        emitter.instruction("cmp x9, x10");                                     // determine whether every captured header has been visited
        emitter.instruction("b.hs __rt_gc_collect_cycles_rescan_init");         // release graph storage only after all original destructors run
        emitter.instruction("ldr w10, [x9]");                                   // read the current allocation capacity for the next header
        emitter.instruction("add x10, x9, x10");                                // advance beyond this allocation's payload
        emitter.instruction("add x10, x10, #16");                               // include the uniform allocation header
        emitter.instruction("str x10, [sp]");                                   // preserve traversal before a destructor changes allocator state
        emitter.instruction("ldr w10, [x9, #4]");                               // inspect whether this captured block is still live
        emitter.instruction("cbz w10, __rt_gc_collect_cycles_destruct_loop");   // skip storage already released by another destructor
        emitter.instruction("ldr x10, [x9, #8]");                               // read the current kind and original scan metadata
        emitter.instruction("and x11, x10, #0xff");                             // isolate the heap kind independently from GC marks
        emitter.instruction("cmp x11, #4");                                     // only object payloads carry PHP destructor methods
        emitter.instruction("b.ne __rt_gc_collect_cycles_destruct_loop");       // leave container storage intact until the free phase
        emitter.instruction("tbnz x10, #16, __rt_gc_collect_cycles_destruct_loop");// externally reachable objects must retain normal lifetimes
        emitter.instruction("tbz x10, #17, __rt_gc_collect_cycles_destruct_loop");// new destructor allocations wait for a future scan
        emitter.instruction("tbnz x10, #14, __rt_gc_collect_cycles_destruct_loop");// completed destructors cannot mutate the graph again
        emitter.instruction("mov x11, #2");                                     // record that callbacks may change the original root set
        emitter.instruction("str x11, [sp, #96]");                              // request a fresh root scan after all destructors finish
        emitter.instruction("add x0, x9, #16");                                 // pass the intact doomed object through the C argument convention
        cleanup.call(emitter, "__rt_call_object_destructor", true);
        emitter.instruction("b __rt_gc_collect_cycles_destruct_loop");          // continue even when this destructor raised an exception
    } else {
        emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                     // recover the original managed heap base
        emitter.instruction("mov QWORD PTR [rbp - 24], r8");                    // initialize the destructor scan before releasing any child
        emitter.label("__rt_gc_collect_cycles_destruct_loop");
        emitter.instruction("mov r8, QWORD PTR [rbp - 24]");                    // recover the next captured allocation header
        emitter.instruction("cmp r8, QWORD PTR [rbp - 16]");                    // stop at the heap end captured before destructor execution
        emitter.instruction("jae __rt_gc_collect_cycles_rescan_init");          // finish every original destructor before freeing graph nodes
        emitter.instruction("mov r9d, DWORD PTR [r8]");                         // read this allocation's full capacity
        emitter.instruction("lea r9, [r8 + r9 + 16]");                          // address the following heap header before invoking PHP
        emitter.instruction("mov QWORD PTR [rbp - 24], r9");                    // retain traversal across arbitrary destructor allocations
        emitter.instruction("cmp DWORD PTR [r8 + 4], 0");                       // determine whether this captured allocation is still live
        emitter.instruction("je __rt_gc_collect_cycles_destruct_loop");         // another destructor may already have released this storage
        emitter.instruction("mov r9, QWORD PTR [r8 + 8]");                      // inspect heap kind and the current scan's membership bits
        emitter.instruction("mov r10, r9");                                     // preserve metadata while isolating the object heap kind
        emitter.instruction("and r10, 0xff");                                   // ignore marks and the x86_64 heap magic word
        emitter.instruction("cmp r10, 4");                                      // only objects participate in this pre-release method pass
        emitter.instruction("jne __rt_gc_collect_cycles_destruct_loop");        // arrays, hashes, and boxes remain intact until later
        emitter.instruction("test r9, 0x10000");                                // preserve every object reachable from an external owner
        emitter.instruction("jnz __rt_gc_collect_cycles_destruct_loop");        // reachable objects do not run destructors during this scan
        emitter.instruction("test r9, 0x20000");                                // exclude allocations created or reused by earlier destructors
        emitter.instruction("jz __rt_gc_collect_cycles_destruct_loop");         // leave newly allocated objects for their own root lifetime
        emitter.instruction("test r9, 0x4000");                                 // inspect persistent destructor-called state
        emitter.instruction("jnz __rt_gc_collect_cycles_destruct_loop");        // skip receivers whose destructor already completed
        emitter.instruction("mov QWORD PTR [rbp - 72], 2");                     // rescan roots only when a callback may have mutated ownership
        emitter.instruction("lea rdi, [r8 + 16]");                              // pass the intact raw object to its native destructor dispatcher
        cleanup.call(emitter, "__rt_call_object_destructor", true);
        emitter.instruction("jmp __rt_gc_collect_cycles_destruct_loop");        // retain pending exceptions while visiting remaining objects
    }
    emit_rescan(emitter);
}

/// Recounts live graph owners after callbacks while preserving the original candidate set.
fn emit_rescan(emitter: &mut Emitter) {
    let arm = emitter.target.arch == Arch::AArch64;
    emitter.label("__rt_gc_collect_cycles_rescan_init");
    emitter.instruction(if arm { "ldr x9, [sp, #96]" } else { "cmp QWORD PTR [rbp - 72], 0" });// inspect whether this scan invoked any original destructor
    emitter.instruction(if arm { "cbz x9, __rt_gc_collect_cycles_free_init" } else { "je __rt_gc_collect_cycles_free_init" });// unchanged graphs need only their original root scan
    if arm {
        emitter.instruction("mov x9, #1");                                      // record that the destructor phase has finished
        emitter.instruction("str x9, [sp, #96]");                               // skip destructor dispatch after recounting roots
        emitter.instruction("ldr x9, [sp, #16]");                               // revisit the original managed heap range
    } else {
        emitter.instruction("mov QWORD PTR [rbp - 72], 1");                     // complete the destructor phase before recomputing liveness
        emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                     // restart at the captured heap base
    }
    emitter.label("__rt_gc_collect_cycles_rescan_loop");
    emitter.instruction(if arm { "ldr x10, [sp, #8]" } else { "cmp r8, QWORD PTR [rbp - 16]" });// retain the original scan boundary
    if arm { emitter.instruction("cmp x9, x10"); }                              // stop before allocations beyond the original heap extent
    emitter.instruction(if arm { "b.hs __rt_gc_collect_cycles_count_init" } else { "jae __rt_gc_collect_cycles_root_init" });// recount incoming edges and external roots using current owners
    emitter.instruction(if arm { "ldr w10, [x9]" } else { "mov r9d, DWORD PTR [r8]" });// preserve the next block position before clearing metadata
    emitter.instruction(if arm { "ldr w11, [x9, #4]" } else { "cmp DWORD PTR [r8 + 4], 0" });// distinguish live objects and containers from freed allocations
    emitter.instruction(if arm { "cbz w11, __rt_gc_collect_cycles_rescan_next" } else { "je __rt_gc_collect_cycles_rescan_next" });// keep free-list links untouched
    if arm {
        emitter.instruction("ldr x11, [x9, #8]");                               // read kind, candidate, and stale incoming-edge metadata
        emitter.instruction("and x12, x11, #0xff");                             // distinguish object destructor state from ordinary container counts
        emitter.instruction("cmp x12, #4");                                     // only object allocations use the temporary high refcount bit
        emitter.instruction("b.ne __rt_gc_collect_cycles_rescan_kind_ready");   // retain other heap kinds' counts verbatim
        emitter.instruction("ldr w12, [x9, #4]");                               // inspect real owners after the completed destructor callback
        emitter.instruction("and w12, w12, #0x7fffffff");                       // remove temporary destruction state for a retained object
        emitter.instruction("cbz w12, __rt_gc_collect_cycles_rescan_kind_ready");// keep zero-owner pending releases distinguishable from free blocks
        emitter.instruction("str w12, [x9, #4]");                               // restore ordinary refcounts before the fresh external-root scan
        emitter.label("__rt_gc_collect_cycles_rescan_kind_ready");
        emitter.instruction("mov x12, #0xffff");                                // preserve kind and persistent object flags
        emitter.instruction("movk x12, #2, lsl #16");                           // preserve only the original candidate flag above the low word
        emitter.instruction("and x11, x11, x12");                               // clear reachable and incoming-edge counts before recounting
        emitter.instruction("str x11, [x9, #8]");                               // retain new allocation exclusion and destructor-called state
    } else {
        emitter.instruction("mov r10, QWORD PTR [r8 + 8]");                     // read the packed kind and GC metadata
        emitter.instruction("mov r11, r10");                                    // preserve packed metadata while inspecting object kind
        emitter.instruction("and r11d, 0xff");                                  // isolate the uniform heap allocation kind
        emitter.instruction("cmp r11d, 4");                                     // only objects carry temporary destructor refcount state
        emitter.instruction("jne __rt_gc_collect_cycles_rescan_kind_ready");    // leave ordinary array and cell counts unchanged
        emitter.instruction("mov r11d, DWORD PTR [r8 + 4]");                    // inspect owners acquired or removed during destruction
        emitter.instruction("and r11d, 0x7fffffff");                            // exclude the destructor guard from retained ownership
        emitter.instruction("jz __rt_gc_collect_cycles_rescan_kind_ready");     // keep zero-owner pending releases visible to the free pass
        emitter.instruction("mov DWORD PTR [r8 + 4], r11d");                    // restore ordinary counts before checking external roots
        emitter.label("__rt_gc_collect_cycles_rescan_kind_ready");
        emitter.instruction("mov r11, 0xffffffff0002ffff");                     // preserve heap magic, object flags, and original candidacy
        emitter.instruction("and r10, r11");                                    // invalidate stale reachability after destructor mutations
        emitter.instruction("mov QWORD PTR [r8 + 8], r10");                     // publish metadata for a fresh external-root scan
    }
    emitter.label("__rt_gc_collect_cycles_rescan_next");
    emitter.instruction(if arm { "add x9, x9, x10" } else { "add r8, r9" });    // skip the current payload allocation
    emitter.instruction(if arm { "add x9, x9, #16" } else { "add r8, 16" });    // skip its uniform heap header
    emitter.instruction(if arm { "b __rt_gc_collect_cycles_rescan_loop" } else { "jmp __rt_gc_collect_cycles_rescan_loop" });// refresh the next block before reclaiming any graph storage
}
