//! Purpose:
//! Emits `__rt_hash_bind_ref_element`: bind a hash element as a reference alias of an existing
//! kind-7 reference cell (`self::$a[$dir] = &self::$a[$k]`) and return the possibly-relocated hash.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - Backs the `HashBindRefElement` EIR op. The source element was already promoted to a shared
//!   kind-7 cell by `__rt_hash_ref_element`; this helper installs that same cell into `hash[key]`
//!   with value-tag 11 (Reference) so both elements alias one cell (Zend `&` semantics).
//! - The receiver is split first (`__rt_hash_ensure_unique`): `__rt_hash_ref_element` returns the
//!   table unchanged when the source element already is a reference, so the table may still be
//!   shared with a copy, and rebinding a key must never write through to that copy.
//! - Ordering is correctness-critical: retain (`__rt_ref_cell_incref`) FIRST, then rebind. Increfing
//!   first keeps a self-alias (`$dir == $k`, the key already holds this cell) and a fresh key
//!   balanced; releasing before increfing would free the cell and read freed memory (UAF).
//! - An existing key keeps its insertion-order position, as in php (which rebinds the bucket).
//!   `__rt_hash_set` already replaces an existing entry in place (and normalizes the table's value
//!   representation for the new tag), but it WRITES THROUGH an entry that already holds a reference
//!   cell, which would overwrite the old cell instead of aliasing the shared one. So an old
//!   reference is first detached from its entry (tag null, payload cleared, nothing released), the
//!   key is then set through `__rt_hash_set`, and only afterwards is the detached old cell's share
//!   released: a self-alias (`$dir == $k`) stays balanced by the retain done first.
//! - The table may be relocated by the split or the insert; the returned pointer is threaded forward
//!   and returned so the caller stores it back.

use crate::codegen_support::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits the `__rt_hash_bind_ref_element` runtime helper.
///
/// # ABI
/// - ARM64: in `x0` = hash pointer, `x1` = key_lo, `x2` = key_hi, `x3` = kind-7 cell pointer; out
///   `x0` = possibly-relocated hash pointer.
/// - x86_64: in `rdi` = hash pointer, `rsi` = key_lo, `rdx` = key_hi, `rcx` = kind-7 cell pointer;
///   out `rax` = possibly-relocated hash pointer.
pub fn emit_hash_bind_ref_element(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_hash_bind_ref_element_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: hash_bind_ref_element ---");
    emitter.label_global("__rt_hash_bind_ref_element");

    // -- frame: [0]=hash [8]=key_lo [16]=key_hi [24]=cell [32]=detached old cell --
    emitter.instruction("sub sp, sp, #64");                                     // allocate a frame for the hash, key, and cell spills
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set up the new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the hash pointer across helper calls
    emitter.instruction("str x1, [sp, #8]");                                    // save key_lo across helper calls
    emitter.instruction("str x2, [sp, #16]");                                   // save key_hi across helper calls
    emitter.instruction("str x3, [sp, #24]");                                   // save the reference cell pointer across helper calls

    // -- retain the shared cell FIRST so a self-alias and a fresh key stay refcount-balanced --
    emitter.instruction("mov x0, x3");                                          // pass the reference cell to the retain helper
    emitter.instruction("bl __rt_ref_cell_incref");                             // the target element takes a new owning share of the cell

    // -- split a shared table: rebinding a key must not write through to a copy --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the hash pointer
    emitter.instruction("bl __rt_hash_ensure_unique");                          // x0 = a table this owner holds alone
    emitter.instruction("str x0, [sp, #0]");                                    // keep the unique table

    // -- an old reference cell is detached from its entry; every other old value is replaced by hash_set --
    emitter.instruction("str xzr, [sp, #32]");                                  // no old reference cell to release yet
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload key_lo
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload key_hi
    emitter.instruction("bl __rt_hash_get");                                    // x0 = found, x4 = the matching entry address
    emitter.instruction("cbz x0, __rt_hash_bind_ref_element_insert");           // a missing key is appended below
    emitter.instruction("ldr x9, [x4, #40]");                                   // the entry's raw value tag
    emitter.instruction("cmp x9, #11");                                         // does the key already hold a reference cell?
    emitter.instruction("b.ne __rt_hash_bind_ref_element_insert");              // other values are replaced in place by hash_set
    emitter.instruction("ldr x9, [x4, #24]");                                   // the old reference cell
    emitter.instruction("str x9, [sp, #32]");                                   // keep it to release after the rebind
    emitter.instruction("str xzr, [x4, #24]");                                  // detach the cell from the entry
    emitter.instruction("str xzr, [x4, #32]");                                  // clear the payload high word
    emitter.instruction("mov x9, #8");                                          // runtime tag 8 = null
    emitter.instruction("str x9, [x4, #40]");                                   // the entry now owns nothing, so hash_set takes its replace path

    // -- bind the shared cell with value-tag 11 (Reference); an existing key keeps its position --
    emitter.label("__rt_hash_bind_ref_element_insert");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the unique table
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload key_lo for the store
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload key_hi for the store
    emitter.instruction("ldr x3, [sp, #24]");                                   // value = the shared reference cell pointer
    emitter.instruction("mov x4, xzr");                                         // value_hi = 0 for a single-word reference value
    emitter.instruction("mov x5, #11");                                         // value_tag = 11 (Reference)
    emitter.instruction("bl __rt_hash_set");                                    // x0 = final relocated hash
    emitter.instruction("str x0, [sp, #0]");                                    // keep the final table for the return
    emitter.instruction("ldr x0, [sp, #32]");                                   // the detached old reference cell, if any
    emitter.instruction("bl __rt_decref_any");                                  // drop the entry's old share (null is a no-op)

    emitter.label("__rt_hash_bind_ref_element_done");
    emitter.instruction("ldr x0, [sp, #0]");                                    // return the table pointer the caller stores back
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate the frame
    emitter.instruction("ret");                                                 // return (x0 = relocated hash)
}

/// Emits the x86_64 Linux variant of `__rt_hash_bind_ref_element`.
fn emit_hash_bind_ref_element_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_bind_ref_element ---");
    emitter.label_global("__rt_hash_bind_ref_element");

    // -- frame: [-8]=hash [-16]=key_lo [-24]=key_hi [-32]=cell [-40]=detached old cell --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before spilling helper state
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 48");                                         // reserve slots for the hash, key, cell and detached-cell spills
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the hash pointer across helper calls
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save key_lo across helper calls
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save key_hi across helper calls
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the reference cell pointer across helper calls

    // -- retain the shared cell FIRST so a self-alias and a fresh key stay refcount-balanced --
    emitter.instruction("mov rax, rcx");                                        // pass the reference cell to the retain helper (rax ABI)
    emitter.instruction("call __rt_ref_cell_incref");                           // the target element takes a new owning share of the cell

    // -- split a shared table: rebinding a key must not write through to a copy --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the hash pointer
    emitter.instruction("call __rt_hash_ensure_unique");                        // rax = a table this owner holds alone
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // keep the unique table

    // -- an old reference cell is detached from its entry; every other old value is replaced by hash_set --
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // no old reference cell to release yet
    emitter.instruction("mov rdi, rax");                                        // the unique table is the lookup receiver
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload key_lo
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // reload key_hi
    emitter.instruction("call __rt_hash_get");                                  // rax = found, r8 = the matching entry address
    emitter.instruction("test rax, rax");                                       // did the lookup find the key?
    emitter.instruction("jz __rt_hash_bind_ref_element_insert");                // a missing key is appended below
    emitter.instruction("cmp QWORD PTR [r8 + 40], 11");                         // does the key already hold a reference cell?
    emitter.instruction("jne __rt_hash_bind_ref_element_insert");               // other values are replaced in place by hash_set
    emitter.instruction("mov r9, QWORD PTR [r8 + 24]");                         // the old reference cell
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // keep it to release after the rebind
    emitter.instruction("mov QWORD PTR [r8 + 24], 0");                          // detach the cell from the entry
    emitter.instruction("mov QWORD PTR [r8 + 32], 0");                          // clear the payload high word
    emitter.instruction("mov QWORD PTR [r8 + 40], 8");                          // runtime tag 8 = null: the entry owns nothing, so hash_set takes its replace path

    // -- bind the shared cell with value-tag 11 (Reference); an existing key keeps its position --
    emitter.label("__rt_hash_bind_ref_element_insert");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the unique table
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload key_lo for the store
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // reload key_hi for the store
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // value = the shared reference cell pointer
    emitter.instruction("xor r8d, r8d");                                        // value_hi = 0 for a single-word reference value
    emitter.instruction("mov r9d, 11");                                         // value_tag = 11 (Reference)
    emitter.instruction("call __rt_hash_set");                                  // rax = final relocated hash
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // keep the final table for the return
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // the detached old reference cell, if any
    emitter.instruction("call __rt_decref_any");                                // drop the entry's old share (null is a no-op)

    emitter.label("__rt_hash_bind_ref_element_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the table pointer the caller stores back
    emitter.instruction("add rsp, 48");                                         // release the spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return (rax = relocated hash)
}
