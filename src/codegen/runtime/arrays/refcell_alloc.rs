//! Purpose:
//! Emits the `__rt_refcell_alloc` runtime helper assembly for allocating a reference cell.
//! A reference cell (heap kind 6) boxes one value triple so several array elements, object
//! properties, and source variables can share and mutate a single referenced value.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - The cell body is laid out exactly like a Mixed cell (`[tag@0][lo@8][hi@16]`), so the inner
//!   triple obeys the same ownership rules; the only difference is the heap-kind byte (6, not 5).
//! - The cell starts with refcount 1 (the allocating owner); each additional owner increfs it.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// High 32 bits of the x86_64 heap-block kind word, stamped so the runtime can tell managed
/// allocations from foreign pointers. Value: `"ELPH"` in ASCII.
const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

/// refcell_alloc: retain/persist a value triple and box it into a fresh reference cell.
/// Input:  x0=value_tag, x1=value_lo, x2=value_hi
/// Output: x0=reference cell pointer (refcount 1, heap kind 6)
pub fn emit_refcell_alloc(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_refcell_alloc_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: refcell_alloc ---");
    emitter.label_global("__rt_refcell_alloc");

    emitter.instruction("sub sp, sp, #48");                                     // allocate a frame for the incoming triple and the new cell
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up the new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the referenced value tag across helper calls
    emitter.instruction("stp x1, x2, [sp, #8]");                                // save the referenced payload words across helper calls

    emitter.instruction("cmp x0, #1");                                          // does the referenced value hold a string?
    emitter.instruction("b.eq __rt_refcell_alloc_string");                      // strings must be persisted for the cell owner
    emitter.instruction("cmp x0, #4");                                          // does the referenced value hold an indexed array?
    emitter.instruction("b.eq __rt_refcell_alloc_retain");                      // refcounted children must be retained for the cell
    emitter.instruction("cmp x0, #5");                                          // does the referenced value hold an associative array?
    emitter.instruction("b.eq __rt_refcell_alloc_retain");                      // refcounted children must be retained for the cell
    emitter.instruction("cmp x0, #6");                                          // does the referenced value hold an object?
    emitter.instruction("b.eq __rt_refcell_alloc_retain");                      // refcounted children must be retained for the cell
    emitter.instruction("cmp x0, #7");                                          // does the referenced value hold a boxed mixed cell?
    emitter.instruction("b.eq __rt_refcell_alloc_retain");                      // nested mixed cells must also be retained
    emitter.instruction("cmp x0, #10");                                         // does the referenced value hold a callable descriptor?
    emitter.instruction("b.eq __rt_refcell_alloc_retain");                      // callable descriptors are retained for the cell
    emitter.instruction("b __rt_refcell_alloc_store");                          // scalars can be boxed without additional retention

    emitter.label("__rt_refcell_alloc_string");
    emitter.instruction("bl __rt_str_persist");                                 // duplicate the string payload for the cell owner
    emitter.instruction("stp x1, x2, [sp, #8]");                                // replace the saved payload with the owned string pointer and length
    emitter.instruction("b __rt_refcell_alloc_store");                          // continue with allocation after persisting the string

    emitter.label("__rt_refcell_alloc_retain");
    emitter.instruction("mov x0, x1");                                          // move the child heap pointer into the incref argument register
    emitter.instruction("bl __rt_incref");                                      // retain the shared child pointer for the cell owner

    emitter.label("__rt_refcell_alloc_store");
    emitter.instruction("mov x0, #24");                                         // reference cells store a tag plus two payload words
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the reference cell storage (refcount initialized to 1)
    emitter.instruction("mov x9, #6");                                          // low byte 6 = reference cell heap kind
    emitter.instruction("str x9, [x0, #-8]");                                   // install the reference-cell heap kind in the uniform header
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the saved referenced value tag
    emitter.instruction("str x10, [x0]");                                       // store the referenced value tag at cell[0]
    emitter.instruction("ldp x11, x12, [sp, #8]");                              // reload the normalized payload words
    emitter.instruction("stp x11, x12, [x0, #8]");                              // store the payload words at cell[8] and cell[16]
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate the stack frame
    emitter.instruction("ret");                                                 // return the reference cell pointer in x0
}

/// x86_64 Linux implementation of `__rt_refcell_alloc`.
/// Normalizes ownership of the referenced triple (string persistence or refcount retention),
/// allocates a 24-byte reference cell, stamps heap kind 6, and writes the tagged payload.
/// Input:  rax=value_tag, rdi=value_lo, rsi=value_hi
/// Output: rax=reference cell pointer (refcount 1, heap kind 6)
/// Clobbers: r10 as scratch during header stamping and payload installation.
fn emit_refcell_alloc_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: refcell_alloc ---");
    emitter.label_global("__rt_refcell_alloc");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before boxing the referenced triple
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the temporary payload spill
    emitter.instruction("sub rsp, 32");                                         // reserve local slots for tag, value_lo, value_hi, and scratch state
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the referenced value tag across ownership normalization
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // save the low payload word across helper calls and allocation
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                       // save the high payload word across helper calls and allocation
    emitter.instruction("cmp rax, 1");                                          // detect string payloads that need their own owned copy inside the cell
    emitter.instruction("je __rt_refcell_alloc_string");                        // strings must be persisted so the cell owns a stable payload
    emitter.instruction("cmp rax, 4");                                          // detect indexed arrays that participate in refcounted ownership
    emitter.instruction("je __rt_refcell_alloc_retain");                        // retain indexed arrays before storing them inside the cell
    emitter.instruction("cmp rax, 5");                                          // detect associative arrays that participate in refcounted ownership
    emitter.instruction("je __rt_refcell_alloc_retain");                        // retain associative arrays before storing them inside the cell
    emitter.instruction("cmp rax, 6");                                          // detect objects that participate in refcounted ownership
    emitter.instruction("je __rt_refcell_alloc_retain");                        // retain objects before storing them inside the cell
    emitter.instruction("cmp rax, 7");                                          // detect nested mixed cells that participate in refcounted ownership
    emitter.instruction("je __rt_refcell_alloc_retain");                        // retain nested mixed cells before storing them inside the cell
    emitter.instruction("cmp rax, 10");                                         // detect callable descriptors that participate in callable ownership
    emitter.instruction("je __rt_refcell_alloc_retain");                        // retain callable descriptors before storing them inside the cell
    emitter.instruction("jmp __rt_refcell_alloc_store");                        // scalars can be boxed directly without additional ownership work

    emitter.label("__rt_refcell_alloc_string");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // move the source string pointer into the string helper input register
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // move the source string length into the paired string helper register
    emitter.instruction("call __rt_str_persist");                               // duplicate the string payload so the new cell owner owns heap-backed storage
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // replace the saved low word with the owned string pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // replace the saved high word with the owned string length
    emitter.instruction("jmp __rt_refcell_alloc_store");                        // continue boxing once the string payload is safely owned

    emitter.label("__rt_refcell_alloc_retain");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // move the shared heap child into the refcount helper input register
    emitter.instruction("call __rt_incref");                                    // retain the shared heap child for the new cell owner

    emitter.label("__rt_refcell_alloc_store");
    emitter.instruction("mov rax, 24");                                         // reference cells store a tag plus two payload words in owned heap storage
    emitter.instruction("call __rt_heap_alloc");                                // allocate the reference cell storage (refcount initialized to 1)
    emitter.instruction(&format!("mov r10, 0x{:x}", (X86_64_HEAP_MAGIC_HI32 << 32) | 6)); // materialize the reference-cell heap kind word with the x86_64 marker
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the allocated payload as a reference cell in the uniform header
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the saved referenced value tag after ownership normalization
    emitter.instruction("mov QWORD PTR [rax], r10");                            // store the referenced value tag at cell[0]
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the normalized low payload word after ownership helpers completed
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // store the low payload word at cell[8]
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the normalized high payload word after ownership helpers completed
    emitter.instruction("mov QWORD PTR [rax + 16], r10");                       // store the high payload word at cell[16]
    emitter.instruction("add rsp, 32");                                         // release the temporary payload spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.instruction("ret");                                                 // return the reference cell pointer in rax
}
