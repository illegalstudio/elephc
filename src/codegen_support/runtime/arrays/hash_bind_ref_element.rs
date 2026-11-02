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
//! - Ordering is correctness-critical: retain (`__rt_ref_cell_incref`) FIRST, then release the key's
//!   prior value (`__rt_hash_unset`, which decrefs an existing tag-11 cell through its own share),
//!   then insert (`__rt_hash_set` with tag 11). Increfing first keeps a self-alias (`$dir == $k`,
//!   the key already holds this cell) and a fresh key balanced; releasing before increfing would
//!   free the cell and read freed memory (UAF).
//! - Unsetting the key before the insert guarantees `__rt_hash_set` takes the normal insert path,
//!   never the tag-11 write-through path (which would overwrite the old cell's inner value instead
//!   of aliasing the shared cell).
//! - Both `__rt_hash_unset` and `__rt_hash_set` may relocate the table (copy-on-write / grow); the
//!   returned pointer is threaded forward and returned so the caller stores it back.

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

    // -- frame: [0]=hash [8]=key_lo [16]=key_hi [24]=cell --
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

    // -- release any prior value at the key (an existing tag-11 cell decrefs its own share) --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the hash pointer
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload key_lo
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload key_hi
    emitter.instruction("bl __rt_hash_unset");                                  // remove the key's prior value; x0 = possibly-relocated hash

    // -- insert the shared cell at the now-absent key with value-tag 11 (Reference) --
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload key_lo for the insert
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload key_hi for the insert
    emitter.instruction("ldr x3, [sp, #24]");                                   // value = the shared reference cell pointer
    emitter.instruction("mov x4, xzr");                                         // value_hi = 0 for a single-word reference value
    emitter.instruction("mov x5, #11");                                         // value_tag = 11 (Reference)
    emitter.instruction("bl __rt_hash_set");                                    // insert path (key absent): x0 = final relocated hash

    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate the frame
    emitter.instruction("ret");                                                 // return (x0 = relocated hash)
}

/// Emits the x86_64 Linux variant of `__rt_hash_bind_ref_element`.
fn emit_hash_bind_ref_element_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_bind_ref_element ---");
    emitter.label_global("__rt_hash_bind_ref_element");

    // -- frame: [-8]=hash [-16]=key_lo [-24]=key_hi [-32]=cell --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before spilling helper state
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 32");                                         // reserve slots for the hash, key, and cell spills
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the hash pointer across helper calls
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save key_lo across helper calls
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save key_hi across helper calls
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the reference cell pointer across helper calls

    // -- retain the shared cell FIRST so a self-alias and a fresh key stay refcount-balanced --
    emitter.instruction("mov rax, rcx");                                        // pass the reference cell to the retain helper (rax ABI)
    emitter.instruction("call __rt_ref_cell_incref");                           // the target element takes a new owning share of the cell

    // -- release any prior value at the key (an existing tag-11 cell decrefs its own share) --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload key_lo
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // reload key_hi
    emitter.instruction("call __rt_hash_unset");                                // remove the key's prior value; rax = possibly-relocated hash

    // -- insert the shared cell at the now-absent key with value-tag 11 (Reference) --
    emitter.instruction("mov rdi, rax");                                        // the relocated hash becomes the insert's first argument
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload key_lo for the insert
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // reload key_hi for the insert
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // value = the shared reference cell pointer
    emitter.instruction("xor r8d, r8d");                                        // value_hi = 0 for a single-word reference value
    emitter.instruction("mov r9d, 11");                                         // value_tag = 11 (Reference)
    emitter.instruction("call __rt_hash_set");                                  // insert path (key absent): rax = final relocated hash

    emitter.instruction("add rsp, 32");                                         // release the spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return (rax = relocated hash)
}
