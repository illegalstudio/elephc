//! Purpose:
//! Emits `__rt_hash_ref_element`: promote a hash element to a kind-7 reference cell and return
//! both the (possibly relocated) hash pointer and the shared cell pointer.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - Backs `$x = &$arr[$k]` (hash + indexed→hash) by making the element bucket hold a reference
//!   (value-tag 11) pointing at a shared refcounted cell. Composes the existing runtime primitives
//!   `__rt_hash_get` (read element + tag), `__rt_incref` (balance the moved value), `__rt_ref_cell_alloc`
//!   (create the cell owning the value) and `__rt_hash_set` (write the cell + tag 11 back, growing if
//!   needed) rather than re-walking the bucket layout.
//! - The moved value is retained before the overwriting `__rt_hash_set` so the cell and the released
//!   old bucket value stay balanced (a no-op for scalar elements, which is the common case).
//! - If the element is already a reference, its existing cell is returned and the hash is unchanged.
//! - Reference cells hold a single inner value word; multi-word (string) elements are rejected
//!   downstream by the ref-cell local machinery, so only `value_lo` is moved into the cell.

use crate::codegen_support::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits the `__rt_hash_ref_element` runtime helper.
///
/// # ABI
/// - ARM64: in `x0` = hash pointer, `x1` = key_lo, `x2` = key_hi; out `x0` = possibly-relocated hash
///   pointer, `x1` = kind-7 reference cell pointer.
/// - x86_64: in `rdi` = hash pointer, `rsi` = key_lo, `rdx` = key_hi; out `rax` = possibly-relocated
///   hash pointer, `rdx` = kind-7 reference cell pointer.
pub fn emit_hash_ref_element(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_hash_ref_element_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: hash_ref_element ---");
    emitter.label_global("__rt_hash_ref_element");

    // -- frame: [0]=hash [8]=key_lo [16]=key_hi [24]=value_lo [32]=tag [40]=cell --
    emitter.instruction("sub sp, sp, #64");                                     // allocate a frame for the hash, key, value, tag, and cell spills
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set up the new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the hash pointer across helper calls
    emitter.instruction("str x1, [sp, #8]");                                    // save key_lo across helper calls
    emitter.instruction("str x2, [sp, #16]");                                   // save key_hi across helper calls

    emitter.instruction("bl __rt_hash_get");                                    // x0=found, x1=value_lo, x2=value_hi, x3=value_tag
    emitter.instruction("cbz x0, __rt_hash_ref_element_vivify");                // missing keys auto-vivify a null reference element
    emitter.instruction("cmp x3, #11");                                         // is the element already a reference cell?
    emitter.instruction("b.eq __rt_hash_ref_element_existing");                 // reuse the existing cell without mutating the bucket

    // -- promote an ordinary element: retain the value, box it in a cell, write it back --
    emitter.instruction("str x1, [sp, #24]");                                   // save the current element value word
    emitter.instruction("str x3, [sp, #32]");                                   // save the current element value-tag
    emitter.instruction("mov x0, x1");                                          // move the element value into the retain argument register
    emitter.instruction("bl __rt_incref");                                      // retain the moved value so the cell and the overwritten bucket stay balanced
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the element value word
    emitter.instruction("ldr x1, [sp, #32]");                                   // reload the element value-tag
    emitter.instruction("bl __rt_ref_cell_alloc");                              // x0 = new kind-7 cell owning the element value
    emitter.instruction("str x0, [sp, #40]");                                   // save the new cell pointer
    emitter.instruction("b __rt_hash_ref_element_store");                       // write the cell back into the bucket

    emitter.label("__rt_hash_ref_element_vivify");
    emitter.instruction("mov x0, xzr");                                         // vivify a null inner value
    emitter.instruction("mov x1, xzr");                                         // null uses integer value-tag 0
    emitter.instruction("bl __rt_ref_cell_alloc");                              // x0 = new kind-7 cell owning null
    emitter.instruction("str x0, [sp, #40]");                                   // save the new cell pointer

    emitter.label("__rt_hash_ref_element_store");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the hash pointer
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload key_lo
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload key_hi
    emitter.instruction("ldr x3, [sp, #40]");                                   // value = the reference cell pointer
    emitter.instruction("mov x4, xzr");                                         // value_hi = 0 for a single-word reference value
    emitter.instruction("mov x5, #11");                                         // value_tag = 11 (Reference)
    emitter.instruction("bl __rt_hash_set");                                    // x0 = possibly-relocated hash after storing the reference
    emitter.instruction("ldr x1, [sp, #40]");                                   // return the shared cell pointer in x1
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate the frame
    emitter.instruction("ret");                                                 // return (x0 = hash, x1 = cell)

    emitter.label("__rt_hash_ref_element_existing");
    emitter.instruction("ldr x0, [sp, #0]");                                    // the hash is unchanged for an already-referenced element
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate the frame
    emitter.instruction("ret");                                                 // return (x0 = hash, x1 = existing cell)
}

/// Emits the x86_64 Linux variant of `__rt_hash_ref_element`.
fn emit_hash_ref_element_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_ref_element ---");
    emitter.label_global("__rt_hash_ref_element");

    // -- frame: [-8]=hash [-16]=key_lo [-24]=key_hi [-32]=value_lo [-40]=tag [-48]=cell --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before spilling helper state
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 48");                                         // reserve slots for the hash, key, value, tag, and cell spills
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the hash pointer across helper calls
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save key_lo across helper calls
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save key_hi across helper calls

    emitter.instruction("call __rt_hash_get");                                  // rax=found, rdi=value_lo, rsi=value_hi, rcx=value_tag
    emitter.instruction("test rax, rax");                                       // did the associative lookup find the key?
    emitter.instruction("jz __rt_hash_ref_element_vivify");                     // missing keys auto-vivify a null reference element
    emitter.instruction("cmp rcx, 11");                                         // is the element already a reference cell?
    emitter.instruction("je __rt_hash_ref_element_existing");                   // reuse the existing cell without mutating the bucket

    emitter.instruction("mov QWORD PTR [rbp - 32], rdi");                       // save the current element value word
    emitter.instruction("mov QWORD PTR [rbp - 40], rcx");                       // save the current element value-tag
    emitter.instruction("mov rax, rdi");                                        // move the element value into the retain argument register
    emitter.instruction("call __rt_incref");                                    // retain the moved value so the cell and the overwritten bucket stay balanced
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // reload the element value word into the alloc value register
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // reload the element value-tag into the alloc tag register
    emitter.instruction("call __rt_ref_cell_alloc");                            // rax = new kind-7 cell owning the element value
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the new cell pointer
    emitter.instruction("jmp __rt_hash_ref_element_store");                     // write the cell back into the bucket

    emitter.label("__rt_hash_ref_element_vivify");
    emitter.instruction("xor edi, edi");                                        // vivify a null inner value
    emitter.instruction("xor esi, esi");                                        // null uses integer value-tag 0
    emitter.instruction("call __rt_ref_cell_alloc");                            // rax = new kind-7 cell owning null
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the new cell pointer

    emitter.label("__rt_hash_ref_element_store");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload key_lo
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // reload key_hi
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // value = the reference cell pointer
    emitter.instruction("xor r8d, r8d");                                        // value_hi = 0 for a single-word reference value
    emitter.instruction("mov r9d, 11");                                         // value_tag = 11 (Reference)
    emitter.instruction("call __rt_hash_set");                                  // rax = possibly-relocated hash after storing the reference
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // return the shared cell pointer in rdx
    emitter.instruction("add rsp, 48");                                         // release the spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return (rax = hash, rdx = cell)

    emitter.label("__rt_hash_ref_element_existing");
    emitter.instruction("mov rdx, rdi");                                        // the existing cell pointer is the fetched value word
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // the hash is unchanged for an already-referenced element
    emitter.instruction("add rsp, 48");                                         // release the spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return (rax = hash, rdx = existing cell)
}
