//! Purpose:
//! Emits `__rt_mixed_slot_publish`, which republishes a mutated refcounted
//! container (indexed array or hash) into a Mixed-widened static/global slot
//! after an in-place `$sym[] = ...` / `$sym[k] = ...` mutation.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//! - `crate::codegen::context::writeback_symbol_array_source` emits the call
//!   and stores the returned cell back into the slot symbol.
//!
//! Key details:
//! - When the owning Mixed cell is uniquely owned (rc == 1), the mutated
//!   container is swapped into the cell payload IN PLACE: `[cell] = tag`,
//!   `[cell + 8] = container`, `[cell + 16] = 0`. No new cell is allocated and no
//!   retain/decref churn happens, so a boot-once cache built with a loop of
//!   appends stays `O(1)` per push instead of re-boxing (and, after a grow
//!   relocation, leaking) on every mutation. Because the load path already
//!   dropped its spurious retain (`__rt_array_uncow_if_cell_unique`) the
//!   container refcount is exactly 1 here, so the swap keeps it unique.
//! - When the cell is shared (rc > 1, a live Mixed-typed alias such as
//!   `$x = $c`), it falls back to copy-on-write: box the (already cloned)
//!   container into a FRESH Mixed cell with a retain and decref the old cell, so
//!   the alias keeps observing the pre-mutation container. This mirrors the
//!   original re-box behaviour and preserves PHP value semantics.
//! - The container is passed post-mutation (post copy-on-write / post grow), so
//!   for the in-place path the caller guarantees it is the sole authoritative
//!   pointer; the payload swap simply overwrites any stale (grow-freed) pointer.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits `__rt_mixed_slot_publish(container, cell, tag) -> cell`.
///
/// Input:  x0 / rdi = mutated container pointer, x1 / rsi = current owning Mixed
/// cell pointer (the slot's stored value), x2 / rdx = runtime value tag for the
/// container (4 = indexed array, 5 = associative array).
/// Output: x0 / rax = the Mixed cell to store back into the slot symbol (the same
/// cell for the in-place path, a fresh cell for the copy-on-write path).
pub fn emit_mixed_slot_publish(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_slot_publish_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: mixed_slot_publish ---");
    emitter.label_global("__rt_mixed_slot_publish");

    emitter.instruction("cbz x1, __rt_mixed_slot_publish_rebox");               // a missing cell cannot be swapped in place; box a fresh one
    emitter.instruction("ldr w9, [x1, #-12]");                                  // load the owning Mixed cell refcount from its uniform header
    emitter.instruction("cmp w9, #1");                                          // is the cell uniquely owned (no live Mixed-typed alias)?
    emitter.instruction("b.ne __rt_mixed_slot_publish_rebox");                  // a shared cell must copy-on-write to preserve the alias

    // -- in-place payload swap: overwrite the uniquely owned cell's boxed value --
    emitter.instruction("str x2, [x1]");                                        // publish the container runtime tag into the cell tag word
    emitter.instruction("str x0, [x1, #8]");                                    // publish the (possibly relocated) container pointer into value_lo
    emitter.instruction("str xzr, [x1, #16]");                                  // clear value_hi; refcounted payloads only use value_lo
    emitter.instruction("mov x0, x1");                                          // return the same cell — the slot symbol keeps pointing at it
    emitter.instruction("ret");                                                 // in-place publish done

    // -- copy-on-write path: box the container into a fresh cell, drop the old one --
    emitter.label("__rt_mixed_slot_publish_rebox");
    emitter.instruction("sub sp, sp, #32");                                     // allocate a small frame for the shared-cell rebox path
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // set up a new frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // preserve the old cell pointer across the boxing call
    emitter.instruction("str x0, [sp, #8]");                                    // preserve the container pointer to undo the box retain afterward
    emitter.instruction("mov x9, x0");                                          // stage the container pointer while the tag moves into place
    emitter.instruction("mov x0, x2");                                          // pass the runtime value tag as the boxing helper tag argument
    emitter.instruction("mov x1, x9");                                          // pass the container pointer as the boxing helper low word
    emitter.instruction("mov x2, xzr");                                         // refcounted payloads leave the high word empty
    emitter.instruction("bl __rt_mixed_from_value");                            // box the container into a fresh Mixed cell, retaining the container
    // The container reaching this path is a freshly copy-on-write-cloned copy
    // (the shared cell forced ensure_unique to clone), so it must be TRANSFERRED
    // into the fresh cell, not retained. Undo the box retain: rc is >= 2 here, so
    // this never frees.
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the freshly cloned container pointer
    emitter.instruction("ldr w10, [x9, #-12]");                                 // load its refcount from the uniform header
    emitter.instruction("sub w10, w10, #1");                                    // undo the mixed_from_value retain so the new cell is the sole owner
    emitter.instruction("str w10, [x9, #-12]");                                 // publish the transferred container refcount
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the old cell pointer after boxing
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the fresh cell pointer across the old-cell release
    emitter.instruction("mov x0, x1");                                          // move the old cell into the decref argument register
    emitter.instruction("bl __rt_decref_mixed");                                // drop this owner's reference to the old shared cell (the alias keeps it)
    emitter.instruction("ldr x0, [sp, #0]");                                    // return the fresh cell pointer for the slot symbol
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the rebox frame
    emitter.instruction("ret");                                                 // copy-on-write publish done
}

/// Emits the x86_64 Linux variant of `__rt_mixed_slot_publish`.
///
/// Mirrors the ARM64 logic using System V AMD64 registers: rdi = container,
/// rsi = old cell, rdx = tag, rax = returned cell. In-place swaps a uniquely
/// owned cell's payload; otherwise boxes into a fresh cell and decrefs the old.
fn emit_mixed_slot_publish_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_slot_publish ---");
    emitter.label_global("__rt_mixed_slot_publish");

    emitter.instruction("test rsi, rsi");                                       // a missing cell cannot be swapped in place; box a fresh one
    emitter.instruction("je __rt_mixed_slot_publish_rebox");                    // fall through to the copy-on-write path when no cell was provided
    emitter.instruction("mov r10d, DWORD PTR [rsi - 12]");                      // load the owning Mixed cell refcount from its uniform header
    emitter.instruction("cmp r10d, 1");                                         // is the cell uniquely owned (no live Mixed-typed alias)?
    emitter.instruction("jne __rt_mixed_slot_publish_rebox");                   // a shared cell must copy-on-write to preserve the alias

    // -- in-place payload swap: overwrite the uniquely owned cell's boxed value --
    emitter.instruction("mov QWORD PTR [rsi], rdx");                            // publish the container runtime tag into the cell tag word
    emitter.instruction("mov QWORD PTR [rsi + 8], rdi");                        // publish the (possibly relocated) container pointer into value_lo
    emitter.instruction("mov QWORD PTR [rsi + 16], 0");                         // clear value_hi; refcounted payloads only use value_lo
    emitter.instruction("mov rax, rsi");                                        // return the same cell — the slot symbol keeps pointing at it
    emitter.instruction("ret");                                                 // in-place publish done

    // -- copy-on-write path: box the container into a fresh cell, drop the old one --
    emitter.label("__rt_mixed_slot_publish_rebox");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before the boxing call
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 16");                                         // reserve one aligned spill slot for the old/new cell pointer
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // preserve the old cell pointer across the boxing call
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // preserve the container pointer to undo the box retain afterward
    emitter.instruction("mov rax, rdx");                                        // pass the runtime value tag as the boxing helper tag argument
    emitter.instruction("mov rsi, 0");                                          // refcounted payloads leave the high word empty
    emitter.instruction("call __rt_mixed_from_value");                          // box the container into a fresh Mixed cell, retaining the container
    // The container reaching this path is a freshly copy-on-write-cloned copy, so
    // transfer it into the fresh cell instead of retaining: undo the box retain
    // (rc is >= 2 here, so this never frees).
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the freshly cloned container pointer
    emitter.instruction("mov r11d, DWORD PTR [r10 - 12]");                      // load its refcount from the uniform header
    emitter.instruction("sub r11d, 1");                                         // undo the mixed_from_value retain so the new cell is the sole owner
    emitter.instruction("mov DWORD PTR [r10 - 12], r11d");                      // publish the transferred container refcount
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the old cell pointer after boxing
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the fresh cell pointer across the old-cell release
    emitter.instruction("mov rax, r10");                                        // move the old cell into the decref pointer register (rax)
    emitter.instruction("call __rt_decref_mixed");                              // drop this owner's reference to the old shared cell (the alias keeps it)
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the fresh cell pointer for the slot symbol
    emitter.instruction("add rsp, 16");                                         // release the aligned spill slot
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // copy-on-write publish done
}
