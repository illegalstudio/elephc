//! Purpose:
//! Emits the `__rt_refcell_store` runtime helper assembly for writing through a reference cell.
//! Replaces the value held inside a heap-kind-6 reference cell in place, so every owner that
//! shares the cell observes the new value (PHP reference write-through semantics).
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//! - `__rt_hash_set` when overwriting a reference slot with a plain value, and from EIR lowering
//!   when a reference-aliased local is assigned a new value.
//!
//! Key details:
//! - Releases the previously referenced value by its tag, retains/persists the new value the same
//!   way `__rt_refcell_alloc` does, then overwrites the cell's `[tag][lo][hi]` triple in place.
//! - The cell pointer and refcount are untouched; only the boxed inner value changes.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// refcell_store: release the cell's current inner value and write a new value triple in place.
/// Input: x0 = reference cell pointer, x1 = new value_tag, x2 = new value_lo, x3 = new value_hi
/// Output: none
pub fn emit_refcell_store(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_refcell_store_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: refcell_store ---");
    emitter.label_global("__rt_refcell_store");

    emitter.instruction("sub sp, sp, #48");                                     // allocate a frame for the cell pointer and the new triple
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up the new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the reference cell pointer across helper calls
    emitter.instruction("str x1, [sp, #8]");                                    // save the new value tag across helper calls
    emitter.instruction("str x2, [sp, #16]");                                   // save the new value low word across helper calls
    emitter.instruction("str x3, [sp, #24]");                                   // save the new value high word across helper calls

    // -- release the value currently referenced by the cell --
    emitter.instruction("ldr x9, [x0]");                                        // load the cell's current inner value tag
    emitter.instruction("cmp x9, #1");                                          // is the current inner value a string?
    emitter.instruction("b.eq __rt_refcell_store_rel_string");                  // strings release through heap_free_safe
    emitter.instruction("cmp x9, #4");                                          // does the current inner value hold a heap-backed child?
    emitter.instruction("b.lo __rt_refcell_store_retain");                      // scalars/bools/floats/null need no release
    emitter.instruction("cmp x9, #7");                                          // do heap-backed inner tags stay within the supported range?
    emitter.instruction("b.ls __rt_refcell_store_rel_any");                     // tags 4-7 release through the uniform dispatcher
    emitter.instruction("cmp x9, #10");                                         // is the current inner value a callable descriptor?
    emitter.instruction("b.eq __rt_refcell_store_rel_callable");                // callable descriptors release through the helper
    emitter.instruction("b __rt_refcell_store_retain");                         // unknown tags need no release

    emitter.label("__rt_refcell_store_rel_string");
    emitter.instruction("ldr x0, [x0, #8]");                                    // load the current inner string pointer
    emitter.instruction("bl __rt_heap_free_safe");                              // release the previously referenced string payload
    emitter.instruction("b __rt_refcell_store_retain");                         // continue with retaining the new value

    emitter.label("__rt_refcell_store_rel_any");
    emitter.instruction("ldr x0, [x0, #8]");                                    // load the current inner heap child pointer
    emitter.instruction("bl __rt_decref_any");                                  // release the previously referenced heap child
    emitter.instruction("b __rt_refcell_store_retain");                         // continue with retaining the new value

    emitter.label("__rt_refcell_store_rel_callable");
    emitter.instruction("ldr x0, [x0, #8]");                                    // load the current inner callable descriptor pointer
    emitter.instruction("bl __rt_callable_descriptor_release");                 // release the previously referenced descriptor

    // -- retain/persist the new value before storing it --
    emitter.label("__rt_refcell_store_retain");
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the new value tag
    emitter.instruction("cmp x9, #1");                                          // does the new value hold a string?
    emitter.instruction("b.eq __rt_refcell_store_new_string");                  // strings must be persisted for the cell owner
    emitter.instruction("cmp x9, #4");                                          // does the new value hold a heap-backed child?
    emitter.instruction("b.lo __rt_refcell_store_write");                       // scalars/bools/floats/null need no retention
    emitter.instruction("cmp x9, #7");                                          // do heap-backed new tags stay within the supported range?
    emitter.instruction("b.ls __rt_refcell_store_new_retain");                  // tags 4-7 must be retained for the cell owner
    emitter.instruction("cmp x9, #10");                                         // does the new value hold a callable descriptor?
    emitter.instruction("b.eq __rt_refcell_store_new_retain");                  // callable descriptors are retained for the cell owner
    emitter.instruction("b __rt_refcell_store_write");                          // scalars can be written without retention

    emitter.label("__rt_refcell_store_new_string");
    emitter.instruction("ldr x1, [sp, #16]");                                   // load the new string pointer for persistence
    emitter.instruction("ldr x2, [sp, #24]");                                   // load the new string length for persistence
    emitter.instruction("bl __rt_str_persist");                                 // duplicate the new string payload for the cell owner
    emitter.instruction("str x1, [sp, #16]");                                   // replace the saved low word with the owned string pointer
    emitter.instruction("str x2, [sp, #24]");                                   // replace the saved high word with the owned string length
    emitter.instruction("b __rt_refcell_store_write");                          // continue once the string payload is safely owned

    emitter.label("__rt_refcell_store_new_retain");
    emitter.instruction("ldr x0, [sp, #16]");                                   // load the new heap child pointer
    emitter.instruction("bl __rt_incref");                                      // retain the shared new child pointer for the cell owner

    // -- overwrite the cell's inner triple in place --
    emitter.label("__rt_refcell_store_write");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the reference cell pointer
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the new value tag
    emitter.instruction("str x9, [x0]");                                        // store the new value tag at cell[0]
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the normalized new low word
    emitter.instruction("str x9, [x0, #8]");                                    // store the new low word at cell[8]
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the normalized new high word
    emitter.instruction("str x9, [x0, #16]");                                   // store the new high word at cell[16]
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate the stack frame
    emitter.instruction("ret");                                                 // return after writing through the reference cell
}

/// x86_64 Linux variant of `__rt_refcell_store`.
/// Releases the cell's current inner value, retains/persists the new value, and overwrites the
/// cell's triple in place. The refcount and cell pointer are unchanged.
/// Input: rdi = reference cell pointer, rsi = new value_tag, rdx = new value_lo, rcx = new value_hi
/// Output: none
/// Clobbers: r10 as scratch during the final triple write.
fn emit_refcell_store_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: refcell_store ---");
    emitter.label_global("__rt_refcell_store");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before spilling the new triple
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved cell and value triple
    emitter.instruction("sub rsp, 32");                                         // reserve local slots for the cell pointer, tag, low word, and high word
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the reference cell pointer across helper calls
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the new value tag across helper calls
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the new value low word across helper calls
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the new value high word across helper calls

    // -- release the value currently referenced by the cell --
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the cell's current inner value tag
    emitter.instruction("cmp r10, 1");                                          // is the current inner value a string?
    emitter.instruction("je __rt_refcell_store_rel_string");                    // strings release through heap_free_safe
    emitter.instruction("cmp r10, 4");                                          // does the current inner value hold a heap-backed child?
    emitter.instruction("jb __rt_refcell_store_retain");                        // scalars/bools/floats/null need no release
    emitter.instruction("cmp r10, 7");                                          // do heap-backed inner tags stay within the supported range?
    emitter.instruction("jbe __rt_refcell_store_rel_any");                      // tags 4-7 release through the uniform dispatcher
    emitter.instruction("cmp r10, 10");                                         // is the current inner value a callable descriptor?
    emitter.instruction("je __rt_refcell_store_rel_callable");                  // callable descriptors release through the helper
    emitter.instruction("jmp __rt_refcell_store_retain");                       // unknown tags need no release

    emitter.label("__rt_refcell_store_rel_string");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the cell pointer to read its inner string pointer
    emitter.instruction("mov rax, QWORD PTR [rax + 8]");                        // load the current inner string pointer
    emitter.instruction("call __rt_heap_free_safe");                            // release the previously referenced string payload
    emitter.instruction("jmp __rt_refcell_store_retain");                       // continue with retaining the new value

    emitter.label("__rt_refcell_store_rel_any");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the cell pointer to read its inner child pointer
    emitter.instruction("mov rax, QWORD PTR [rax + 8]");                        // load the current inner heap child pointer
    emitter.instruction("call __rt_decref_any");                                // release the previously referenced heap child
    emitter.instruction("jmp __rt_refcell_store_retain");                       // continue with retaining the new value

    emitter.label("__rt_refcell_store_rel_callable");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the cell pointer to read its inner descriptor pointer
    emitter.instruction("mov rax, QWORD PTR [rax + 8]");                        // load the current inner callable descriptor pointer
    emitter.instruction("call __rt_callable_descriptor_release");               // release the previously referenced descriptor

    // -- retain/persist the new value before storing it --
    emitter.label("__rt_refcell_store_retain");
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the new value tag
    emitter.instruction("cmp r10, 1");                                          // does the new value hold a string?
    emitter.instruction("je __rt_refcell_store_new_string");                    // strings must be persisted for the cell owner
    emitter.instruction("cmp r10, 4");                                          // does the new value hold a heap-backed child?
    emitter.instruction("jb __rt_refcell_store_write");                         // scalars/bools/floats/null need no retention
    emitter.instruction("cmp r10, 7");                                          // do heap-backed new tags stay within the supported range?
    emitter.instruction("jbe __rt_refcell_store_new_retain");                   // tags 4-7 must be retained for the cell owner
    emitter.instruction("cmp r10, 10");                                         // does the new value hold a callable descriptor?
    emitter.instruction("je __rt_refcell_store_new_retain");                    // callable descriptors are retained for the cell owner
    emitter.instruction("jmp __rt_refcell_store_write");                        // scalars can be written without retention

    emitter.label("__rt_refcell_store_new_string");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // move the new string pointer into the string helper input register
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // move the new string length into the paired string helper register
    emitter.instruction("call __rt_str_persist");                               // duplicate the new string payload for the cell owner
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // replace the saved low word with the owned string pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // replace the saved high word with the owned string length
    emitter.instruction("jmp __rt_refcell_store_write");                        // continue once the string payload is safely owned

    emitter.label("__rt_refcell_store_new_retain");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // load the new heap child pointer
    emitter.instruction("call __rt_incref");                                    // retain the shared new child pointer for the cell owner

    // -- overwrite the cell's inner triple in place --
    emitter.label("__rt_refcell_store_write");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the reference cell pointer
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the new value tag
    emitter.instruction("mov QWORD PTR [rax], r10");                            // store the new value tag at cell[0]
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the normalized new low word
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // store the new low word at cell[8]
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the normalized new high word
    emitter.instruction("mov QWORD PTR [rax + 16], r10");                       // store the new high word at cell[16]
    emitter.instruction("add rsp, 32");                                         // deallocate the stack frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.instruction("ret");                                                 // return after writing through the reference cell
}
