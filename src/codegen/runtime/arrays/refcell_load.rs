//! Purpose:
//! Emits the `__rt_refcell_load` runtime helper assembly for dereferencing a reference cell.
//! Reads the single value triple stored inside a heap-kind-6 reference cell.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - References never nest, so this peels exactly one level (unlike `__rt_mixed_unbox`).
//! - A null cell pointer dereferences to the null runtime triple (tag 8), matching mixed unbox.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// refcell_load: read the value triple stored inside a reference cell.
/// Input:  x0 = reference cell pointer (may be null)
/// Output: x0 = value tag, x1 = value_lo, x2 = value_hi
pub fn emit_refcell_load(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_refcell_load_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: refcell_load ---");
    emitter.label_global("__rt_refcell_load");

    emitter.instruction("cbz x0, __rt_refcell_load_null");                      // null cells dereference to the null runtime triple
    emitter.instruction("ldr x9, [x0]");                                        // x9 = the referenced value tag at cell[0]
    emitter.instruction("ldr x1, [x0, #8]");                                    // return the referenced low payload word in x1
    emitter.instruction("ldr x2, [x0, #16]");                                   // return the referenced high payload word in x2
    emitter.instruction("mov x0, x9");                                          // return the referenced value tag in x0
    emitter.instruction("ret");                                                 // return the dereferenced value triple

    emitter.label("__rt_refcell_load_null");
    emitter.instruction("mov x0, #8");                                          // runtime tag 8 = null
    emitter.instruction("mov x1, #0");                                          // null has no low payload word
    emitter.instruction("mov x2, #0");                                          // null has no high payload word
    emitter.instruction("ret");                                                 // return the normalized null payload triple
}

/// x86_64 Linux variant of `__rt_refcell_load` using System V ABI register conventions.
/// Input:  rax = reference cell pointer (may be null)
/// Output: rax = value tag, rdi = value_lo, rdx = value_hi
fn emit_refcell_load_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: refcell_load ---");
    emitter.label_global("__rt_refcell_load");

    emitter.instruction("test rax, rax");                                       // null cells dereference to the null runtime triple
    emitter.instruction("je __rt_refcell_load_null");                           // null cell pointers unwrap to the null runtime tag
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // r10 = the referenced value tag at cell[0]
    emitter.instruction("mov rdi, QWORD PTR [rax + 8]");                        // return the referenced low payload word in rdi
    emitter.instruction("mov rdx, QWORD PTR [rax + 16]");                       // return the referenced high payload word in rdx
    emitter.instruction("mov rax, r10");                                        // return the referenced value tag in rax
    emitter.instruction("ret");                                                 // return the dereferenced value triple

    emitter.label("__rt_refcell_load_null");
    emitter.instruction("mov rax, 8");                                          // runtime tag 8 = null
    emitter.instruction("xor rdi, rdi");                                        // null has no low payload word
    emitter.instruction("xor rdx, rdx");                                        // null has no high payload word
    emitter.instruction("ret");                                                 // return the normalized null payload triple
}
