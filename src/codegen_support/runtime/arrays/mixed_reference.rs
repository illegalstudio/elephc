//! Purpose:
//! Emits persistent PHP reference cells using GC-traced nested Mixed storage.
//!
//! Called from:
//! - Managed runtime emission and eval reference creation, reads, and writes.
//!
//! Key details:
//! - Tag seven with high word one identifies a reference owning one boxed PHP value.
//! - Replacement clones the incoming value and returns the previous owner for explicit cleanup.
//! - Ordinary copies dereference before retaining resource identity or cloning value storage.
//! - Array copies clone zero-owner borrowed cells whose protected child release is still active.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits persistent references and the shared nested-cell dereference helper for both architectures.
pub fn emit_mixed_reference(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::AArch64 { aarch64(emitter); } else { x86_64(emitter); }
    emit_array_reference_copy(emitter);
}

/// Implements reference storage and borrowed dereferencing with the AArch64 value convention.
fn aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_mixed_deref");
    emitter.label("__rt_mixed_deref_loop");
    emitter.instruction("cbz x0, __rt_mixed_deref_done");                       // null has no nested cell
    emitter.instruction("ldr x9, [x0]");                                        // inspect the current boxed tag
    emitter.instruction("cmp x9, #7");                                          // nested Mixed and reference cells share a traced child pointer
    emitter.instruction("b.ne __rt_mixed_deref_done");                          // return the concrete boxed value without changing ownership
    emitter.instruction("ldr x0, [x0, #8]");                                    // follow the owned child cell
    emitter.instruction("b __rt_mixed_deref_loop");                             // peel another nested wrapper
    emitter.label("__rt_mixed_deref_done");
    emitter.instruction("ret");                                                 // return the borrowed concrete box
    emitter.label_global("__rt_reference_is");
    emitter.instruction("cbz x0, __rt_reference_is_false");                     // null is not a persistent reference
    emitter.instruction("ldr x9, [x0]");                                        // inspect the raw wrapper tag
    emitter.instruction("cmp x9, #7");                                          // only nested Mixed wrappers can carry reference identity
    emitter.instruction("b.ne __rt_reference_is_false");                        // ordinary values have no shared writable reference cell
    emitter.instruction("ldr x9, [x0, #16]");                                   // read the reserved reference marker
    emitter.instruction("cmp x9, #1");                                          // recognize the persistent reference ABI marker
    emitter.instruction("cset x0, eq");                                         // return a normalized reference predicate
    emitter.instruction("ret");                                                 // leave the input ownership unchanged
    emitter.label("__rt_reference_is_false");
    emitter.instruction("mov x0, #0");                                          // report an ordinary PHP value
    emitter.instruction("ret");                                                 // return the predicate
    emitter.label_global("__rt_reference_new");
    emitter.instruction("sub sp, sp, #32");                                     // reserve current value, wrapper, and caller linkage
    emitter.instruction("stp x29, x30, [sp, #16]");                             // preserve the caller across allocation and cleanup
    emitter.instruction("add x29, sp, #16");                                    // establish an aligned runtime frame
    emitter.instruction("bl __rt_mixed_clone");                                 // take an independent PHP value before publishing a reference
    emitter.instruction("str x0, [sp]");                                        // retain the owned current value across wrapper allocation
    emitter.instruction("mov x1, x0");                                          // provide the owned child cell to nested boxing
    emitter.instruction("mov x0, #7");                                          // select the GC-traced nested Mixed tag
    emitter.instruction("mov x2, #1");                                          // mark the wrapper as a persistent PHP reference
    emitter.instruction("bl __rt_mixed_from_value");                            // allocate the wrapper and retain its child
    emitter.instruction("str x0, [sp, #8]");                                    // preserve the completed reference across temporary cleanup
    emitter.instruction("ldr x0, [sp]");                                        // recover the temporary current-value owner
    emitter.instruction("bl __rt_decref_any");                                  // leave the child owned only through the new reference
    emitter.instruction("ldr x0, [sp, #8]");                                    // return the completed owned reference cell
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore caller linkage
    emitter.instruction("add sp, sp, #32");                                     // release helper storage
    emitter.instruction("ret");                                                 // transfer one reference owner
    emitter.label_global("__rt_reference_replace");
    emitter.instruction("sub sp, sp, #32");                                     // reserve reference identity and caller linkage
    emitter.instruction("stp x29, x30, [sp, #16]");                             // preserve the caller across value cloning
    emitter.instruction("add x29, sp, #16");                                    // establish the replacement frame
    emitter.instruction("str x0, [sp]");                                        // keep the shared reference identity stable
    emitter.instruction("mov x0, x1");                                          // copy the borrowed incoming PHP value
    emitter.instruction("bl __rt_mixed_clone");                                 // detach another reference and preserve resource value identity
    emitter.instruction("ldr x9, [sp]");                                        // recover the shared reference wrapper
    emitter.instruction("ldr x10, [x9, #8]");                                   // capture its previous owned value for caller cleanup
    emitter.instruction("str x0, [x9, #8]");                                    // publish the new value before any destructor can execute
    emitter.instruction("mov x0, x10");                                         // transfer the replaced value owner to the caller
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore caller linkage
    emitter.instruction("add sp, sp, #32");                                     // release replacement storage
    emitter.instruction("ret");                                                 // return the old owner without running a destructor
}

/// Implements the same reference protocol using the runtime SysV register convention.
fn x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_mixed_deref");
    emitter.label("__rt_mixed_deref_loop");
    emitter.instruction("test rax, rax");                                       // recognize a null boxed value
    emitter.instruction("jz __rt_mixed_deref_done");                            // null contains no nested value
    emitter.instruction("cmp QWORD PTR [rax], 7");                              // recognize a nested Mixed or persistent reference wrapper
    emitter.instruction("jne __rt_mixed_deref_done");                           // return the concrete borrowed cell
    emitter.instruction("mov rax, QWORD PTR [rax + 8]");                        // follow the owned boxed child
    emitter.instruction("jmp __rt_mixed_deref_loop");                           // peel the remaining nested wrappers
    emitter.label("__rt_mixed_deref_done");
    emitter.instruction("ret");                                                 // return the borrowed concrete value
    emitter.label_global("__rt_reference_is");
    emitter.instruction("test rax, rax");                                       // recognize a null input
    emitter.instruction("jz __rt_reference_is_false");                          // null is not a reference
    emitter.instruction("cmp QWORD PTR [rax], 7");                              // only nested Mixed wrappers can be reference cells
    emitter.instruction("jne __rt_reference_is_false");                         // ordinary boxed values carry no reference marker
    emitter.instruction("cmp QWORD PTR [rax + 16], 1");                         // inspect the persistent reference ABI marker
    emitter.instruction("sete al");                                             // normalize the reference predicate
    emitter.instruction("movzx eax, al");                                       // clear unused predicate bits
    emitter.instruction("ret");                                                 // return without changing input ownership
    emitter.label("__rt_reference_is_false");
    emitter.instruction("xor eax, eax");                                        // report an ordinary boxed value
    emitter.instruction("ret");                                                 // return the reference predicate
    emitter.label_global("__rt_reference_new");
    emitter.instruction("push rbp");                                            // preserve linkage and align runtime calls
    emitter.instruction("mov rbp, rsp");                                        // establish the reference allocation frame
    emitter.instruction("sub rsp, 16");                                         // reserve owned value and wrapper spills
    emitter.instruction("call __rt_mixed_clone");                               // detach the borrowed PHP value before creating reference storage
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // retain the initial owned value across allocation
    emitter.instruction("mov rdi, rax");                                        // pass the child cell to nested value boxing
    emitter.instruction("mov rax, 7");                                          // select GC-traced nested Mixed storage
    emitter.instruction("mov rsi, 1");                                          // mark the wrapper as a persistent PHP reference
    emitter.instruction("call __rt_mixed_from_value");                          // allocate a wrapper that retains its child
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the completed reference during temporary cleanup
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // recover the temporary child owner
    emitter.instruction("call __rt_decref_any");                                // leave the child owned by its wrapper
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // return the completed reference owner
    emitter.instruction("leave");                                               // release spills and restore caller linkage
    emitter.instruction("ret");                                                 // transfer the owned reference cell
    emitter.label_global("__rt_reference_replace");
    emitter.instruction("push rbp");                                            // preserve linkage and align cloning calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable replacement frame
    emitter.instruction("sub rsp, 16");                                         // reserve the borrowed reference identity
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the target wrapper across cloning
    emitter.instruction("mov rax, rdi");                                        // pass the borrowed incoming PHP value
    emitter.instruction("call __rt_mixed_clone");                               // copy a value independently from any reference wrapper
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // recover the shared reference identity
    emitter.instruction("mov r11, QWORD PTR [r10 + 8]");                        // capture the old owner before replacing it
    emitter.instruction("mov QWORD PTR [r10 + 8], rax");                        // publish the new current value before cleanup
    emitter.instruction("mov rax, r11");                                        // transfer the replaced owner to the caller
    emitter.instruction("leave");                                               // release spills and restore caller linkage
    emitter.instruction("ret");                                                 // return without executing a PHP destructor
}

/// Copies an array slot, cloning active-release borrows and detaching orphan references.
fn emit_array_reference_copy(emitter: &mut Emitter) {
    emitter.label_global("__rt_reference_array_copy");
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction("cbz x0, __rt_reference_array_copy_retain");        // preserve empty indexed slots
        emitter.instruction("ldr w9, [x0, #-12]");                              // recognize a borrowed cell still visible during protected child release
        emitter.instruction("cbz w9, __rt_reference_array_copy_clone");         // copy the PHP value without reviving its dying Mixed allocation
        emitter.instruction("ldr x9, [x0]");                                    // inspect the boxed slot tag
        emitter.instruction("cmp x9, #7");                                      // only nested wrappers may carry reference identity
        emitter.instruction("b.ne __rt_reference_array_copy_retain");           // retain ordinary boxed slots
        emitter.instruction("ldr x9, [x0, #16]");                               // inspect the persistent reference marker
        emitter.instruction("cmp x9, #1");                                      // recognize a PHP reference wrapper
        emitter.instruction("b.ne __rt_reference_array_copy_retain");           // retain ordinary nested Mixed values
        emitter.instruction("ldr w9, [x0, #-12]");                              // count the original reference owners before copying the array
        emitter.instruction("cmp w9, #1");                                      // does only the original array slot own this reference?
        emitter.instruction("b.ne __rt_reference_array_copy_retain");           // keep references shared with another live owner
        emitter.label("__rt_reference_array_copy_clone");
        emitter.instruction("b __rt_mixed_clone");                              // detach an orphan reference into an ordinary PHP value
        emitter.label("__rt_reference_array_copy_retain");
        emitter.instruction("b __rt_incref");                                   // retain the original boxed slot for the cloned array
    } else {
        emitter.instruction("test rax, rax");                                   // recognize an empty indexed slot
        emitter.instruction("jz __rt_incref");                                  // preserve absent slot pointers
        emitter.instruction("cmp DWORD PTR [rax - 12], 0");                     // recognize a borrowed cell still visible during protected child release
        emitter.instruction("je __rt_mixed_clone");                             // copy the PHP value without reviving its dying Mixed allocation
        emitter.instruction("cmp QWORD PTR [rax], 7");                          // only nested wrappers may carry reference identity
        emitter.instruction("jne __rt_incref");                                 // retain ordinary boxed slots
        emitter.instruction("cmp QWORD PTR [rax + 16], 1");                     // recognize the persistent reference marker
        emitter.instruction("jne __rt_incref");                                 // retain ordinary nested Mixed values
        emitter.instruction("cmp DWORD PTR [rax - 12], 1");                     // does only the original array slot own this reference?
        emitter.instruction("je __rt_mixed_clone");                             // detach an orphan reference into an ordinary PHP value
        emitter.instruction("jmp __rt_incref");                                 // preserve references shared with another live owner
    }
}
