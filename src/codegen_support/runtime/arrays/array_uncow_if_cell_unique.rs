//! Purpose:
//! Emits the `__rt_array_uncow_if_cell_unique` runtime helper: when a
//! refcounted container (indexed array or hash) is boxed inside a uniquely
//! owned Mixed cell, drop the one extra reference the boxed-load added so an
//! in-place mutation can proceed without a spurious copy-on-write clone.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//! - Emitted before an in-place `$sym[] = ...` / `$sym[k] = ...` mutation whose
//!   container was loaded (and retained) from a Mixed-widened static/global slot
//!   (`crate::codegen::lower_inst::arrays`).
//!
//! Key details:
//! - A container boxed in a Mixed cell has a true share count of rc(container)
//!   AND rc(cell). Unboxing a `Mixed -> Array/Hash` load always retains the
//!   container to conservatively force copy-on-write, protecting Mixed-typed
//!   aliases (`$x = $c`) that share the *cell*. For an in-place read-modify-write
//!   back to the same slot that retain is spurious when the cell is uniquely
//!   owned (cell rc == 1): nobody else observes the container through the cell,
//!   so the append can mutate in place.
//! - This helper decrements the container refcount by exactly one, and ONLY when
//!   the cell refcount is 1. The load-retain guarantees the container refcount is
//!   at least 2 in that case (cell owner + load retain), so the decrement can
//!   never reach zero and never frees — it is a plain refcount adjustment, not a
//!   release. When the cell is shared (rc > 1) it does nothing, leaving the
//!   retain in place so `__rt_array_ensure_unique` still clones for the alias.
//! - The uniform heap header stores the 32-bit refcount at `[ptr - 12]`; the
//!   Mixed cell stores its boxed payload pointer at `[cell + 8]`. This helper
//!   only touches refcount words, so it is identical for arrays and hashes.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits `__rt_array_uncow_if_cell_unique(container, cell)`.
///
/// Input:  x0 / rdi = boxed container pointer, x1 / rsi = owning Mixed cell pointer.
/// Output: none (the container refcount is decremented in place when the cell is
/// uniquely owned). Both inputs may be null and are guarded.
pub fn emit_array_uncow_if_cell_unique(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_uncow_if_cell_unique_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_uncow_if_cell_unique ---");
    emitter.label_global("__rt_array_uncow_if_cell_unique");

    emitter.instruction("cbz x1, __rt_array_uncow_if_cell_unique_done");        // a missing owning cell means there is no spurious retain to drop
    emitter.instruction("cbz x0, __rt_array_uncow_if_cell_unique_done");        // a null container carries no refcount to adjust
    emitter.instruction("ldr w9, [x1, #-12]");                                  // load the owning Mixed cell refcount from its uniform header
    emitter.instruction("cmp w9, #1");                                          // is the cell uniquely owned (no Mixed-typed alias shares it)?
    emitter.instruction("b.ne __rt_array_uncow_if_cell_unique_done");           // a shared cell keeps the retain so copy-on-write still clones for the alias
    emitter.instruction("ldr w10, [x0, #-12]");                                 // load the boxed container refcount from its uniform header
    emitter.instruction("sub w10, w10, #1");                                    // drop the spurious boxed-load retain (rc >= 2 here, so this never reaches zero)
    emitter.instruction("str w10, [x0, #-12]");                                 // publish the adjusted container refcount so ensure_unique sees a unique owner
    emitter.label("__rt_array_uncow_if_cell_unique_done");
    emitter.instruction("ret");                                                 // return without freeing anything
}

/// Emits the x86_64 Linux variant of `__rt_array_uncow_if_cell_unique`.
///
/// Mirrors the ARM64 logic using System V AMD64 registers: rdi = container,
/// rsi = owning Mixed cell. Guards both null inputs, checks the cell refcount,
/// and decrements the container refcount by one only when the cell rc is 1.
fn emit_array_uncow_if_cell_unique_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_uncow_if_cell_unique ---");
    emitter.label_global("__rt_array_uncow_if_cell_unique");

    emitter.instruction("test rsi, rsi");                                       // a missing owning cell means there is no spurious retain to drop
    emitter.instruction("je __rt_array_uncow_if_cell_unique_done");             // return immediately when no owning cell was provided
    emitter.instruction("test rdi, rdi");                                       // a null container carries no refcount to adjust
    emitter.instruction("je __rt_array_uncow_if_cell_unique_done");             // return immediately for a null container
    emitter.instruction("mov r10d, DWORD PTR [rsi - 12]");                      // load the owning Mixed cell refcount from its uniform header
    emitter.instruction("cmp r10d, 1");                                         // is the cell uniquely owned (no Mixed-typed alias shares it)?
    emitter.instruction("jne __rt_array_uncow_if_cell_unique_done");            // a shared cell keeps the retain so copy-on-write still clones for the alias
    emitter.instruction("mov r11d, DWORD PTR [rdi - 12]");                      // load the boxed container refcount from its uniform header
    emitter.instruction("sub r11d, 1");                                         // drop the spurious boxed-load retain (rc >= 2 here, so this never reaches zero)
    emitter.instruction("mov DWORD PTR [rdi - 12], r11d");                      // publish the adjusted container refcount so ensure_unique sees a unique owner
    emitter.label("__rt_array_uncow_if_cell_unique_done");
    emitter.instruction("ret");                                                 // return without freeing anything
}
