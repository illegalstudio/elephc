//! Purpose:
//! Emits the `__rt_mixed_from_array_kind` runtime helper that boxes an array-or-hash heap
//! pointer into a Mixed cell, choosing the runtime value tag (4 = indexed array, 5 =
//! associative hash) from the pointer's heap kind.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//! - Invoked by the EIR `preg_match()` `$matches` ref-cell writeback so a runtime-built hash
//!   (named capture groups) boxes as an associative Mixed while an indexed array boxes as an
//!   indexed Mixed, without the caller needing to know the runtime kind statically.
//!
//! Key details:
//! - Delegates the actual allocation and child retention to `__rt_mixed_from_value`, so the
//!   boxed cell owns a retained reference to the array/hash (the caller releases its own
//!   reference separately, e.g. through `__rt_decref_any`).
//! - A null pointer or any non-hash kind falls back to the indexed-array tag (4).

use crate::codegen_support::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits `__rt_mixed_from_array_kind` for the current target.
///
/// Input:  x0 / rax = array-or-hash heap pointer.
/// Output: x0 / rax = boxed Mixed cell whose tag is 5 when the pointer is an associative
/// hash (heap kind 3) and 4 otherwise. The boxed cell retains the array/hash child.
pub fn emit_mixed_from_array_kind(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_from_array_kind_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: mixed_from_array_kind ---");
    emitter.label_global("__rt_mixed_from_array_kind");

    emitter.instruction("sub sp, sp, #32");                                     // reserve frame for the saved pointer and fp/lr
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish the boxing helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the array/hash pointer across the kind probe
    emitter.instruction("bl __rt_heap_kind");                                   // read the heap kind byte for the pointer into x0
    emitter.instruction("cmp x0, #3");                                          // is this an associative hash (heap kind 3)?
    emitter.instruction("b.eq __rt_mixed_from_array_kind_hash");                // hashes box as associative Mixed values
    emitter.instruction("mov x0, #4");                                          // default to the indexed-array runtime value tag
    emitter.instruction("b __rt_mixed_from_array_kind_box");                    // continue with the chosen tag
    emitter.label("__rt_mixed_from_array_kind_hash");
    emitter.instruction("mov x0, #5");                                          // associative arrays use the hash runtime value tag
    emitter.label("__rt_mixed_from_array_kind_box");
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the array/hash pointer as the payload low word
    emitter.instruction("mov x2, #0");                                          // heap-backed payloads leave the high word empty
    emitter.instruction("bl __rt_mixed_from_value");                            // retain the child and box it into a Mixed cell
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the boxing helper frame
    emitter.instruction("ret");                                                 // return the boxed Mixed cell in x0
}

/// Emits the Linux x86_64 variant of `__rt_mixed_from_array_kind`.
///
/// Uses the System V AMD64 ABI: rax carries the array-or-hash pointer in and the boxed
/// Mixed cell out. Probes the heap kind via `__rt_heap_kind`, then boxes through
/// `__rt_mixed_from_value` with tag 5 for hashes (heap kind 3) and tag 4 otherwise.
fn emit_mixed_from_array_kind_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_from_array_kind ---");
    emitter.label_global("__rt_mixed_from_array_kind");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 16");                                         // reserve an aligned slot for the saved pointer
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the array/hash pointer across the kind probe
    emitter.instruction("call __rt_heap_kind");                                 // read the heap kind byte for the pointer into eax
    emitter.instruction("cmp eax, 3");                                          // is this an associative hash (heap kind 3)?
    emitter.instruction("je __rt_mixed_from_array_kind_hash_x");                // hashes box as associative Mixed values
    emitter.instruction("mov eax, 4");                                          // default to the indexed-array runtime value tag
    emitter.instruction("jmp __rt_mixed_from_array_kind_box_x");                // continue with the chosen tag
    emitter.label("__rt_mixed_from_array_kind_hash_x");
    emitter.instruction("mov eax, 5");                                          // associative arrays use the hash runtime value tag
    emitter.label("__rt_mixed_from_array_kind_box_x");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the array/hash pointer as the payload low word
    emitter.instruction("xor esi, esi");                                        // heap-backed payloads leave the high word empty
    emitter.instruction("call __rt_mixed_from_value");                          // retain the child and box it into a Mixed cell
    emitter.instruction("add rsp, 16");                                         // release the saved-pointer slot
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed Mixed cell in rax
}
