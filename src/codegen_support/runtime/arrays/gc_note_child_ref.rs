//! Purpose:
//! Emits the `__rt_gc_note_child_ref`, `__rt_gc_note_child_ref_done` runtime helper assembly for gc note child ref.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - GC helpers must honor cycle-collection suppression, mark bits, and parent/child references without double-releasing values.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::sentinels::REFERENCE_CELL_HEAP_KIND;

/// First bit available for transient incoming counts below the x86 heap marker.
pub(super) const X86_INCOMING_SHIFT: u32 = 19;
/// Saturation requests the exact fallback scan rather than an approximate root decision.
pub(super) const X86_INCOMING_MAX: u32 = (1 << (32 - X86_INCOMING_SHIFT)) - 1;
/// The counter excludes storage, reachability, snapshot pins, and the ELPH marker.
pub(super) const X86_INCOMING_MASK: u64 = (X86_INCOMING_MAX as u64) << X86_INCOMING_SHIFT;

/// Emits `__rt_gc_note_child_ref`, which records one heap-to-heap incoming edge for
/// cycle-aware GC. Skips null pointers, out-of-range pointers, freed blocks, and
/// non-refcounted kinds (strings/raw buffers). For valid refcounted array/hash/object/Mixed
/// children, bumps the transient incoming-edge counter stored in the high 32 bits of
/// the child block's kind word on ARM64. x86_64 caches a saturated count in
/// bits19..31 while preserving the high-word heap marker and the real refcount.
///
/// # Inputs
/// - `x0` (ARM64) or `rax` (x86_64): borrowed child payload pointer.
///
/// # Outputs
/// - No ownership or real refcount changes. ARM64 may clobber `x0` through `x14`.
/// - x86_64 preserves all general registers so the shared edge walker can resume.
///
/// # ABI
/// - Internal runtime pointer ABI; no allocation, PHP callbacks, or external calls.
pub fn emit_gc_note_child_ref(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_gc_note_child_ref_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: gc_note_child_ref ---");
    emitter.label_global("__rt_gc_note_child_ref");

    // -- null and heap-range checks --
    emitter.instruction("cbz x0, __rt_gc_note_child_ref_done");                 // ignore null child pointers
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_heap_buf");
    emitter.instruction("cmp x0, x9");                                          // is the child below the heap buffer?
    emitter.instruction("b.lo __rt_gc_note_child_ref_done");                    // only heap pointers participate in cycle accounting
    crate::codegen_support::abi::emit_symbol_address(emitter, "x10", "_heap_off");
    emitter.instruction("ldr x10, [x10]");                                      // load the current heap offset
    emitter.instruction("add x10, x9, x10");                                    // compute the current heap end
    emitter.instruction("cmp x0, x10");                                         // is the child at or beyond the current heap end?
    emitter.instruction("b.hs __rt_gc_note_child_ref_done");                    // invalid pointers contribute nothing

    // -- every live graph node, including boxed Mixed cells, receives incoming edges --
    emitter.instruction("ldr w11, [x0, #-12]");                                 // load the child refcount from the heap header
    emitter.instruction("cbz w11, __rt_gc_note_child_ref_done");                // freed blocks are not part of the graph
    emitter.instruction("ldr x12, [x0, #-8]");                                  // load the full child kind word
    emitter.instruction("and x13, x12, #0xff");                                 // isolate the low-byte heap kind tag
    emitter.instruction("cmp x13, #2");                                         // is this at least an indexed array?
    emitter.instruction("b.lo __rt_gc_note_child_ref_done");                    // strings/raw buffers do not participate in cycle accounting
    emitter.instruction(&format!("cmp x13, #{REFERENCE_CELL_HEAP_KIND}"));      // owned reference cells participate in incoming-edge accounting
    emitter.instruction("b.eq __rt_gc_note_child_ref_record");                  // count the cell owner rather than shortcutting its payload
    emitter.instruction("cmp x13, #5");                                         // include boxed Mixed nodes as well as arrays, hashes, and objects
    emitter.instruction("b.hi __rt_gc_note_child_ref_done");                    // ignore unknown/raw heap kinds

    // -- bump the transient incoming-edge counter stored in the high 32 bits of the kind word --
    emitter.label("__rt_gc_note_child_ref_record");
    emitter.instruction("mov x14, #1");                                         // prepare a single incoming-edge increment
    emitter.instruction("lsl x14, x14, #32");                                   // move the increment into the high 32 bits
    emitter.instruction("add x12, x12, x14");                                   // add one heap-incoming edge to this child block
    emitter.instruction("str x12, [x0, #-8]");                                  // persist the updated incoming-edge count

    emitter.label("__rt_gc_note_child_ref_done");
    emitter.instruction("ret");                                                 // return to the caller
}

/// Records one x86 incoming edge in bits19..31 after validating snapshot membership.
///
/// Saturation preserves the heap marker and requests an exact root-classification
/// fallback. The borrowed child and every live source-walker register are preserved.
fn emit_gc_note_child_ref_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: gc_note_child_ref ---");
    emitter.label_global("__rt_gc_note_child_ref");

    // -- preserve the edge walker's registers while validating the child --
    emitter.instruction("push r8");                                             // preserve the current source edge address
    emitter.instruction("push r9");                                             // preserve the source container pointer
    emitter.instruction("push r10");                                            // preserve the source descriptor or child count
    emitter.instruction("push r11");                                            // preserve the source iteration index
    emitter.instruction("test rax, rax");                                       // null edges cannot name a heap candidate
    emitter.instruction("jz __rt_gc_note_child_ref_done");                      // skip an absent child before touching a header
    crate::codegen_support::abi::emit_symbol_address(emitter, "r8", "_heap_buf");
    emitter.instruction("lea r9, [r8 + 16]");                                   // the first payload follows the uniform header
    emitter.instruction("cmp rax, r9");                                         // reject static, sentinel, and header addresses
    emitter.instruction("jb __rt_gc_note_child_ref_done");                      // only managed payloads can receive incoming edges
    crate::codegen_support::abi::emit_load_symbol_to_reg(emitter, "r9", "_heap_off", 0);
    emitter.instruction("add r9, r8");                                          // derive the current managed heap end
    emitter.instruction("cmp rax, r9");                                         // reject addresses beyond allocated heap storage
    emitter.instruction("jae __rt_gc_note_child_ref_done");                     // out-of-range children contribute no edge
    emitter.instruction("cmp DWORD PTR [rax - 12], 0");                         // inspect the real child owner count
    emitter.instruction("je __rt_gc_note_child_ref_done");                      // freed blocks are not graph candidates
    emitter.instruction("mov r10, QWORD PTR [rax - 8]");                        // load the marker, snapshot flags, and kind
    emitter.instruction("mov r11, r10");                                        // preserve the complete word while checking its marker
    emitter.instruction("shr r11, 32");                                         // isolate the x86 managed-heap marker
    emitter.instruction(&format!("cmp r11, {}", crate::codegen_support::sentinels::X86_64_HEAP_MAGIC_HI32)); // reject foreign storage even inside the managed address range
    emitter.instruction("jne __rt_gc_note_child_ref_done");                     // a foreign header is not a valid graph node
    emitter.instruction("test r10, 0x20000");                                   // require membership in the current collection snapshot
    emitter.instruction("jz __rt_gc_note_child_ref_done");                      // new allocations are outside this edge-counting pass
    emitter.instruction("mov r11, r10");                                        // isolate the kind without disturbing the packed count
    emitter.instruction("and r11d, 0xff");                                      // discard storage and collector metadata
    emitter.instruction("cmp r11d, 2");                                         // strings and raw blocks own no cyclic graph identity
    emitter.instruction("jb __rt_gc_note_child_ref_done");                      // do not count non-graph payloads
    emitter.instruction(&format!("cmp r11d, {REFERENCE_CELL_HEAP_KIND}"));      // independently owned reference cells are graph nodes
    emitter.instruction("je __rt_gc_note_child_ref_record");                    // record the edge to the cell rather than its payload
    emitter.instruction("cmp r11d, 5");                                         // arrays, hashes, objects, and Mixed cells are supported
    emitter.instruction("ja __rt_gc_note_child_ref_done");                      // ignore resources and other non-graph kinds

    // -- saturate instead of allowing a count to carry into the heap marker --
    emitter.label("__rt_gc_note_child_ref_record");
    emitter.instruction(&format!("shr r10, {X86_INCOMING_SHIFT}"));             // position the transient count in the low bits
    emitter.instruction(&format!("and r10d, {X86_INCOMING_MAX}"));              // remove the shifted marker and unrelated flags
    emitter.instruction(&format!("cmp r10d, {X86_INCOMING_MAX}"));              // a full field must retain the exact-fallback sentinel
    emitter.instruction("je __rt_gc_note_child_ref_done");                      // avoid wrapping a saturated incoming count
    emitter.instruction(&format!("add DWORD PTR [rax - 8], {}", 1 << X86_INCOMING_SHIFT)); // add one edge without changing owner counts or marker bits

    // -- restore every register borrowed from the shared outgoing-edge walker --
    emitter.label("__rt_gc_note_child_ref_done");
    emitter.instruction("pop r11");                                             // restore the source iteration index
    emitter.instruction("pop r10");                                             // restore the source descriptor or child count
    emitter.instruction("pop r9");                                              // restore the source container pointer
    emitter.instruction("pop r8");                                              // restore the source edge address
    emitter.instruction("ret");                                                 // resume enumeration with the child pointer unchanged
}
