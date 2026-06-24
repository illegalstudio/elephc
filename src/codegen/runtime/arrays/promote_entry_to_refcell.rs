//! Purpose:
//! Emits the `__rt_promote_entry_to_refcell` runtime helper assembly.
//! Promotes a hash entry's value slot (or a foreach-by-reference interior pointer into one) into a
//! shared heap-kind-6 reference cell, so the original container and a reference-assignment target
//! can alias and mutate the same value.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//! - `lower_ref_assign_element` when the reference source is a foreach-by-reference interior pointer.
//!
//! Key details:
//! - The interior pointer addresses a hash entry value triple laid out as `[lo@0][hi@8][tag@16]`
//!   (the tag is last), which differs from a reference cell's `[tag@0][lo@8][hi@16]`; the move maps
//!   the words across explicitly.
//! - The entry's value is MOVED into the new cell (no retain/persist): ownership transfers, and the
//!   entry slot is overwritten with `(value_lo=cell, value_hi=0, value_tag=11)`.
//! - When the entry is already a reference (tag 11) the existing cell is returned unchanged, so a
//!   value aliased into several targets shares one cell. The caller increfs the returned cell for
//!   each new owner.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// High 32 bits of the x86_64 heap-block kind word, stamped so the runtime can tell managed
/// allocations from foreign pointers. Value: `"ELPH"` in ASCII.
const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

/// promote_entry_to_refcell: turn a hash entry value slot into a shared reference cell.
/// Input:  x0 = interior pointer to the entry value triple `[lo@0][hi@8][tag@16]`
/// Output: x0 = reference cell pointer (existing cell when already a reference, else a fresh one)
pub fn emit_promote_entry_to_refcell(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_promote_entry_to_refcell_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: promote_entry_to_refcell ---");
    emitter.label_global("__rt_promote_entry_to_refcell");

    emitter.instruction("ldr x9, [x0, #16]");                                   // x9 = the entry value_tag (tag is the last word of the triple)
    emitter.instruction("cmp x9, #11");                                         // is this entry already a reference cell?
    emitter.instruction("b.eq __rt_promote_entry_to_refcell_reuse");            // yes — return the existing shared cell

    // -- fresh promotion: allocate a reference cell and move the entry value triple into it --
    emitter.instruction("str x30, [sp, #-16]!");                                // preserve the return address across the allocation call
    emitter.instruction("str x0, [sp, #-16]!");                                 // save the interior pointer across the allocation call
    emitter.instruction("mov x0, #24");                                         // reference cells store a tag plus two payload words
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the reference cell storage (refcount initialized to 1)
    emitter.instruction("mov x9, #6");                                          // low byte 6 = reference cell heap kind
    emitter.instruction("str x9, [x0, #-8]");                                   // install the reference-cell heap kind in the uniform header
    emitter.instruction("ldr x10, [sp], #16");                                  // reload the interior pointer to the entry value triple
    emitter.instruction("ldr x11, [x10, #16]");                                 // x11 = entry value_tag (from the triple's last word)
    emitter.instruction("str x11, [x0]");                                       // store the value tag at cell[0]
    emitter.instruction("ldr x11, [x10]");                                      // x11 = entry value_lo (from the triple's first word)
    emitter.instruction("str x11, [x0, #8]");                                   // store the low payload word at cell[8]
    emitter.instruction("ldr x11, [x10, #8]");                                  // x11 = entry value_hi (from the triple's middle word)
    emitter.instruction("str x11, [x0, #16]");                                  // store the high payload word at cell[16]
    emitter.instruction("str x0, [x10]");                                       // entry value_lo = the new reference cell pointer
    emitter.instruction("str xzr, [x10, #8]");                                  // entry value_hi = 0 for a reference entry
    emitter.instruction("mov x11, #11");                                        // per-entry value_tag 11 = reference
    emitter.instruction("str x11, [x10, #16]");                                 // entry value_tag = 11 so reads dereference the cell
    emitter.instruction("ldr x30, [sp], #16");                                  // restore the saved return address
    emitter.instruction("ret");                                                 // return the new reference cell pointer in x0

    emitter.label("__rt_promote_entry_to_refcell_reuse");
    emitter.instruction("ldr x0, [x0]");                                        // x0 = the existing reference cell pointer (entry value_lo)
    emitter.instruction("ret");                                                 // return the shared reference cell pointer in x0
}

/// x86_64 Linux implementation of `__rt_promote_entry_to_refcell`.
/// Input:  rax = interior pointer to the entry value triple `[lo@0][hi@8][tag@16]`
/// Output: rax = reference cell pointer (existing cell when already a reference, else a fresh one)
/// Clobbers: r10, r11 as scratch during allocation, header stamping, and the value move.
fn emit_promote_entry_to_refcell_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: promote_entry_to_refcell ---");
    emitter.label_global("__rt_promote_entry_to_refcell");

    emitter.instruction("mov r10, QWORD PTR [rax + 16]");                       // r10 = the entry value_tag (tag is the last word of the triple)
    emitter.instruction("cmp r10, 11");                                         // is this entry already a reference cell?
    emitter.instruction("je __rt_promote_entry_to_refcell_reuse");              // yes — return the existing shared cell

    // -- fresh promotion: allocate a reference cell and move the entry value triple into it --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before spilling the interior pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved interior pointer
    emitter.instruction("sub rsp, 16");                                         // reserve a spill slot for the interior pointer across the allocation call
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the interior pointer across the allocation call
    emitter.instruction("mov rax, 24");                                         // reference cells store a tag plus two payload words
    emitter.instruction("call __rt_heap_alloc");                                // allocate the reference cell storage (refcount initialized to 1)
    emitter.instruction(&format!("mov r10, 0x{:x}", (X86_64_HEAP_MAGIC_HI32 << 32) | 6)); // materialize the reference-cell heap kind word with the x86_64 marker
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the allocated payload as a reference cell in the uniform header
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload the interior pointer to the entry value triple
    emitter.instruction("mov r10, QWORD PTR [r11 + 16]");                       // r10 = entry value_tag (from the triple's last word)
    emitter.instruction("mov QWORD PTR [rax], r10");                            // store the value tag at cell[0]
    emitter.instruction("mov r10, QWORD PTR [r11]");                            // r10 = entry value_lo (from the triple's first word)
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // store the low payload word at cell[8]
    emitter.instruction("mov r10, QWORD PTR [r11 + 8]");                        // r10 = entry value_hi (from the triple's middle word)
    emitter.instruction("mov QWORD PTR [rax + 16], r10");                       // store the high payload word at cell[16]
    emitter.instruction("mov QWORD PTR [r11], rax");                            // entry value_lo = the new reference cell pointer
    emitter.instruction("mov QWORD PTR [r11 + 8], 0");                          // entry value_hi = 0 for a reference entry
    emitter.instruction("mov QWORD PTR [r11 + 16], 11");                        // entry value_tag = 11 so reads dereference the cell
    emitter.instruction("add rsp, 16");                                         // release the interior-pointer spill slot
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.instruction("ret");                                                 // return the new reference cell pointer in rax

    emitter.label("__rt_promote_entry_to_refcell_reuse");
    emitter.instruction("mov rax, QWORD PTR [rax]");                            // rax = the existing reference cell pointer (entry value_lo)
    emitter.instruction("ret");                                                 // return the shared reference cell pointer in rax
}
