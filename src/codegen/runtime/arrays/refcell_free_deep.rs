//! Purpose:
//! Emits the `__rt_refcell_free_deep` runtime helper assembly for freeing a reference cell.
//! Releases the value held inside a heap-kind-6 reference cell, then frees the cell storage.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//! - `__rt_decref_refcell` once a reference cell's refcount reaches zero.
//!
//! Key details:
//! - The inner triple uses the same value-tag layout as a Mixed cell, so the release dispatch
//!   mirrors `__rt_mixed_free_deep` exactly; the inner tag is never 11 (references never nest).

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// refcell_free_deep: release a reference cell's owned inner value and free the cell.
/// Input: x0 = reference cell pointer
/// Output: none
pub fn emit_refcell_free_deep(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_refcell_free_deep_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: refcell_free_deep ---");
    emitter.label_global("__rt_refcell_free_deep");

    emitter.instruction("cbz x0, __rt_refcell_free_deep_done");                 // skip null reference cells immediately
    emitter.instruction("sub sp, sp, #32");                                     // allocate a small frame to preserve the cell pointer
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // set up the new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the cell pointer across inner child release
    emitter.instruction("ldr x9, [x0]");                                        // load the boxed inner value tag
    emitter.instruction("cmp x9, #1");                                          // is the inner payload a string?
    emitter.instruction("b.eq __rt_refcell_free_deep_string");                  // strings release through heap_free_safe
    emitter.instruction("cmp x9, #4");                                          // does the inner payload hold a heap-backed child?
    emitter.instruction("b.lo __rt_refcell_free_deep_box");                     // scalars/bools/floats/null need no nested release
    emitter.instruction("cmp x9, #7");                                          // do heap-backed inner tags stay within the supported range?
    emitter.instruction("b.eq __rt_refcell_free_deep_value_any");               // boxed mixed cells release through the dispatcher
    emitter.instruction("cmp x9, #10");                                         // does the inner payload hold a callable descriptor?
    emitter.instruction("b.eq __rt_refcell_free_deep_callable");                // callable descriptors release through the helper
    emitter.instruction("cmp x9, #7");                                          // restore the heap-backed upper-bound comparison for array/hash/object tags
    emitter.instruction("b.hi __rt_refcell_free_deep_box");                     // unknown tags are ignored by reference deep-free
    emitter.label("__rt_refcell_free_deep_value_any");
    emitter.instruction("ldr x0, [x0, #8]");                                    // load the boxed heap child pointer
    emitter.instruction("bl __rt_decref_any");                                  // release the boxed child through the uniform dispatcher
    emitter.instruction("b __rt_refcell_free_deep_box");                        // free the cell storage after releasing the child

    emitter.label("__rt_refcell_free_deep_callable");
    emitter.instruction("ldr x0, [x0, #8]");                                    // load the boxed callable descriptor pointer
    emitter.instruction("bl __rt_callable_descriptor_release");                 // release the callable descriptor owned by the cell
    emitter.instruction("b __rt_refcell_free_deep_box");                        // free the cell storage after releasing the descriptor

    emitter.label("__rt_refcell_free_deep_string");
    emitter.instruction("ldr x0, [x0, #8]");                                    // load the boxed string pointer
    emitter.instruction("bl __rt_heap_free_safe");                              // release the boxed string payload

    emitter.label("__rt_refcell_free_deep_box");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the cell pointer after child release
    emitter.instruction("bl __rt_heap_free");                                   // free the reference cell storage itself
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate the reference-free frame

    emitter.label("__rt_refcell_free_deep_done");
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the x86_64 Linux variant of `__rt_refcell_free_deep`.
/// Input: rax = reference cell pointer
/// Output: none
/// ABI: preserves rbp, uses rax for input, calls `__rt_decref_any` and `__rt_heap_free` as needed.
fn emit_refcell_free_deep_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: refcell_free_deep ---");
    emitter.label_global("__rt_refcell_free_deep");

    emitter.instruction("test rax, rax");                                       // skip null reference cells immediately because they own no storage
    emitter.instruction("jz __rt_refcell_free_deep_done");                      // null cells need no release work
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before spilling the cell pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved cell pointer
    emitter.instruction("sub rsp, 16");                                         // reserve local storage for the cell pointer across nested helper calls
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the cell pointer across any nested child release helper call
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // load the boxed inner value tag to decide whether the child owns storage
    emitter.instruction("cmp r10, 1");                                          // detect string payloads that need their owned storage released explicitly
    emitter.instruction("je __rt_refcell_free_deep_string");                    // string payloads release through heap_free_safe first
    emitter.instruction("cmp r10, 4");                                          // does the cell point at a heap-backed child such as array/hash/object/mixed?
    emitter.instruction("jl __rt_refcell_free_deep_box");                       // scalar, bool, float, and null payloads skip directly to freeing the cell
    emitter.instruction("cmp r10, 7");                                          // do the heap-backed inner tags stay within the supported runtime range?
    emitter.instruction("je __rt_refcell_free_deep_value_any");                 // boxed mixed cells release through the uniform dispatcher
    emitter.instruction("cmp r10, 10");                                         // does the inner payload hold a callable descriptor?
    emitter.instruction("je __rt_refcell_free_deep_callable");                  // callable descriptors release through the descriptor helper
    emitter.instruction("cmp r10, 7");                                          // restore the heap-backed upper-bound comparison for array/hash/object tags
    emitter.instruction("jg __rt_refcell_free_deep_box");                       // unknown tags are ignored by the x86_64 reference deep-free helper
    emitter.label("__rt_refcell_free_deep_value_any");
    emitter.instruction("mov rax, QWORD PTR [rax + 8]");                        // load the boxed heap child pointer from the cell payload
    emitter.instruction("call __rt_decref_any");                                // release the boxed heap-backed child through the uniform dispatcher
    emitter.instruction("jmp __rt_refcell_free_deep_box");                      // free the cell storage itself after releasing the child

    emitter.label("__rt_refcell_free_deep_callable");
    emitter.instruction("mov rax, QWORD PTR [rax + 8]");                        // load the boxed callable descriptor pointer from the cell payload
    emitter.instruction("call __rt_callable_descriptor_release");               // release the callable descriptor owned by the cell
    emitter.instruction("jmp __rt_refcell_free_deep_box");                      // free the cell storage itself after releasing the descriptor

    emitter.label("__rt_refcell_free_deep_string");
    emitter.instruction("mov rax, QWORD PTR [rax + 8]");                        // load the boxed string pointer from the cell payload
    emitter.instruction("call __rt_heap_free_safe");                            // release the boxed string payload when the cell owns a persisted string

    emitter.label("__rt_refcell_free_deep_box");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the cell pointer after the optional child release helper call
    emitter.instruction("call __rt_heap_free");                                 // release the reference cell storage itself through the shared heap wrapper
    emitter.instruction("add rsp, 16");                                         // release the spill slot reserved for the cell pointer
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.label("__rt_refcell_free_deep_done");
    emitter.instruction("ret");                                                 // return to the caller after releasing the cell and its optional child
}
