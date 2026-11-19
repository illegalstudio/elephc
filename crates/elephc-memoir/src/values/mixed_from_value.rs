//! Purpose:
//! Emits retained Mixed boxes and direct owned-string boxing.
//!
//! Called from:
//! - Compiler runtime emission and memory integration tests.
//!
//! Key details:
//! - Resource identity and inventory policy are emitted by compiler host hooks.

use crate::Emitter;
use crate::Arch;
use crate::layout::{emit_branch_if_null_container, POINTER_ARRAY_VALUE_TYPE};

/// Retains or persists a runtime value and boxes it into a Mixed cell.
///
/// Container-shaped tags with a zero or null-sentinel payload are normalized to
/// canonical PHP null before retain/allocation, preventing malformed boxes from
/// escaping into downstream consumers.
/// Input:  x0=value_tag, x1=value_lo, x2=value_hi
/// Output: x0=boxed mixed pointer
pub fn emit_mixed_from_value(emitter: &mut impl Emitter) {
    if emitter.arch() == Arch::X86_64 {
        emit_mixed_from_value_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: mixed_from_value ---");
    emitter.label_global("__rt_mixed_from_value");
    // -- a ?ptr<T> container word: address 0 is PHP null, any other address an integer --
    emitter.instruction(&format!("cmp x0, #{}", POINTER_ARRAY_VALUE_TYPE));     // is this a raw ?ptr<T> word from a pointer container?
    emitter.instruction("b.ne __rt_mixed_from_value_tagged");                   // every other tag boxes as it is
    emitter.instruction("mov x9, #8");                                          // runtime tag 8 = PHP null for the null pointer word
    emitter.instruction("cmp x1, #0");                                          // is this the null pointer word?
    emitter.instruction("csel x0, x9, xzr, eq");                                // box 0 as PHP null and any address as an integer
    emitter.label("__rt_mixed_from_value_tagged");

    emitter.instruction("sub sp, sp, #48");                                     // allocate stack frame for the incoming payload and boxed result
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up the new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the runtime value tag across helper calls
    emitter.instruction("stp x1, x2, [sp, #8]");                                // save the incoming payload words across helper calls
    emitter.instruction("str xzr, [sp, #24]");                                  // only genuine tracked resources bind a canonical cell

    emitter.instruction("cmp x0, #4");                                          // do only container-shaped tags need null-payload normalization?
    emitter.instruction("b.lt __rt_mixed_from_value_dispatch");                 // scalar tags preserve every payload bit pattern
    emitter.instruction("cmp x0, #6");                                          // indexed arrays, hashes, and objects are the container-shaped tags
    emitter.instruction("b.gt __rt_mixed_from_value_dispatch");                 // nested Mixed/callable tags use ordinary pointer ownership
    emit_branch_if_null_container(
        emitter,
        "x1",
        "x9",
        "__rt_mixed_from_value_null_container",
    );
    emitter.label("__rt_mixed_from_value_dispatch");
    emitter.instruction("cmp x0, #1");                                          // does this mixed payload hold a string?
    emitter.instruction("b.eq __rt_mixed_from_value_string");                   // strings must be persisted for the boxed owner
    emitter.instruction("cmp x0, #4");                                          // does this mixed payload hold an indexed array?
    emitter.instruction("b.eq __rt_mixed_from_value_retain");                   // refcounted child pointers must be retained for the boxed owner
    emitter.instruction("cmp x0, #5");                                          // does this mixed payload hold an associative array?
    emitter.instruction("b.eq __rt_mixed_from_value_retain");                   // refcounted child pointers must be retained for the boxed owner
    emitter.instruction("cmp x0, #6");                                          // does this mixed payload hold an object?
    emitter.instruction("b.eq __rt_mixed_from_value_retain");                   // refcounted child pointers must be retained for the boxed owner
    emitter.instruction("cmp x0, #7");                                          // does this mixed payload hold another mixed cell?
    emitter.instruction("b.eq __rt_mixed_from_value_retain");                   // nested mixed cells must also be retained
    emitter.instruction("cmp x0, #10");                                         // does this mixed payload hold a callable descriptor?
    emitter.instruction("b.eq __rt_mixed_from_value_retain");                   // callable descriptors are retained for the boxed owner
    emitter.instruction("cmp x0, #9");                                          // does this mixed payload hold a PHP resource?
    emitter.instruction("b.eq __rt_mixed_from_value_resource");                 // resources need a display id bound before they can be shown
    emitter.instruction("b __rt_mixed_from_value_alloc");                       // scalars can be boxed without additional retention

    emitter.emit_mixed_resource_identity();

    emitter.label("__rt_mixed_from_value_null_container");
    emitter.instruction("mov x9, #8");                                          // runtime tag 8 is the canonical boxed PHP null
    emitter.instruction("str x9, [sp, #0]");                                    // replace the container-shaped tag before allocation
    emitter.instruction("stp xzr, xzr, [sp, #8]");                              // canonical null has no payload words or ownership
    emitter.instruction("b __rt_mixed_from_value_alloc");                       // allocate the normalized Mixed null without retaining the sentinel

    emitter.label("__rt_mixed_from_value_string");
    emitter.instruction("bl __rt_str_persist");                                 // duplicate the string payload for the boxed owner
    emitter.instruction("stp x1, x2, [sp, #8]");                                // replace the saved payload with the owned string pointer and length
    emitter.instruction("b __rt_mixed_from_value_alloc");                       // continue with allocation after persisting the string

    emitter.label("__rt_mixed_from_value_retain");
    emitter.instruction("mov x0, x1");                                          // move the child heap pointer into the incref argument register
    emitter.instruction("bl __rt_incref");                                      // retain the shared child pointer for the boxed owner

    emitter.label("__rt_mixed_from_value_alloc");
    emitter.instruction(&format!("mov x0, #{}", crate::layout::MIXED_CELL_SIZE)); // mixed cells store tag plus two payload words
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the mixed cell storage
    emitter.instruction(&format!("mov x9, #{}", crate::layout::MIXED_HEAP_KIND)); // low byte 5 = mixed cell heap kind
    emitter.instruction("str x9, [x0, #-8]");                                   // install the mixed-cell heap kind in the uniform header
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the saved runtime value tag
    emitter.instruction("str x10, [x0]");                                       // store the runtime value tag at mixed[0]
    emitter.instruction("ldp x11, x12, [sp, #8]");                              // reload the normalized payload words
    emitter.instruction("stp x11, x12, [x0, #8]");                              // store the payload words at mixed[8] and mixed[16]
    super::resource_weak::emit_publish(emitter, 24, "__rt_mixed_from_value_done");
    emitter.label("__rt_mixed_from_value_done");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate the stack frame
    emitter.instruction("ret");                                                 // return the boxed mixed pointer in x0
}

/// Emits `__rt_mixed_from_owned_string`, which boxes an already-owned string pair.
///
/// AArch64 input: `x1`/`x2` = owned string pointer/length; x86_64 input: `rax`/`rdx`.
/// Output: `x0`/`rax` = a fresh Mixed cell whose string tag owns the transferred bytes.
/// The transfer is direct: no persistence, identity lookup, or byte copy is performed.
pub fn emit_mixed_from_owned_string(emitter: &mut impl Emitter) {
    if emitter.arch() == Arch::X86_64 {
        emitter.blank();
        emitter.comment("--- runtime: mixed_from_owned_string ---");
        emitter.label_global("__rt_mixed_from_owned_string");
        emitter.instruction("push rbp");                                        // preserve the caller frame across cell allocation
        emitter.instruction("mov rbp, rsp");                                    // establish a stable boxer frame
        emitter.instruction("sub rsp, 32");                                     // reserve the transferred string pair
        emitter.instruction("mov QWORD PTR [rbp - 8], rax");                    // save the owned string pointer
        emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                   // save the owned string length
        emitter.instruction(&format!("mov rax, {}", crate::layout::MIXED_CELL_SIZE)); // mixed cells store tag plus two payload words
        emitter.instruction("call __rt_heap_alloc");                            // allocate the boxed cell
        emitter.instruction(&format!("mov r10, 0x{:x}", crate::layout::x86_64_heap_kind_word(crate::layout::MIXED_HEAP_KIND))); // stamp the mixed-cell heap kind
        emitter.instruction("mov QWORD PTR [rax - 8], r10");                    // install the uniform heap header
        emitter.instruction("mov r10, 1");                                      // runtime tag 1 = string
        emitter.instruction("mov QWORD PTR [rax], r10");                        // publish the string tag
        emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                    // reload the owned string pointer
        emitter.instruction("mov QWORD PTR [rax + 8], r10");                    // transfer the string pointer into the cell
        emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                   // reload the owned string length
        emitter.instruction("mov QWORD PTR [rax + 16], r10");                   // transfer the string length into the cell
        emitter.instruction("leave");                                           // restore the caller frame pointer
        emitter.instruction("ret");                                             // return the boxed string cell
        return;
    }
    emitter.blank();
    emitter.comment("--- runtime: mixed_from_owned_string ---");
    emitter.label_global("__rt_mixed_from_owned_string");
    emitter.instruction("sub sp, sp, #48");                                     // reserve the transferred string pair and linkage
    emitter.instruction("stp x29, x30, [sp, #32]");                             // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #32");                                    // establish a stable boxer frame
    emitter.instruction("stp x1, x2, [sp, #0]");                                // save the owned string pointer and length
    emitter.instruction(&format!("mov x0, #{}", crate::layout::MIXED_CELL_SIZE)); // mixed cells store tag plus two payload words
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the boxed cell
    emitter.instruction(&format!("mov x9, #{}", crate::layout::MIXED_HEAP_KIND)); // low byte 5 = mixed cell heap kind
    emitter.instruction("str x9, [x0, #-8]");                                   // install the uniform heap header
    emitter.instruction("mov x9, #1");                                          // runtime tag 1 = string
    emitter.instruction("str x9, [x0]");                                        // publish the string tag
    emitter.instruction("ldp x10, x11, [sp, #0]");                              // reload the transferred string pointer and length
    emitter.instruction("stp x10, x11, [x0, #8]");                              // transfer the string pair into the cell
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore the caller frame and return address
    emitter.instruction("add sp, sp, #48");                                     // release the boxer frame
    emitter.instruction("ret");                                                 // return the boxed string cell
}

/// x86_64 Linux implementation of `__rt_mixed_from_value`.
/// Detects the runtime value tag in `rax`, applies ownership normalization (string persistence or refcount retention),
/// allocates a 24-byte mixed cell on the heap, and writes the tagged payload at offsets 0, 8, and 16.
/// Input:  rax=value_tag, rdi=value_lo, rsi=value_hi
/// Output: rax=boxed mixed pointer
/// Clobbers: r10 as temporary scratch during header stamping and payload installation.
fn emit_mixed_from_value_linux_x86_64(emitter: &mut impl Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_from_value ---");
    emitter.label_global("__rt_mixed_from_value");
    // -- a ?ptr<T> container word: address 0 is PHP null, any other address an integer --
    emitter.instruction(&format!("cmp rax, {}", POINTER_ARRAY_VALUE_TYPE));     // is this a raw ?ptr<T> word from a pointer container?
    emitter.instruction("jne __rt_mixed_from_value_x86_tagged");                // every other tag boxes as it is
    emitter.instruction("xor eax, eax");                                        // runtime tag 0 = integer for a live pointer address
    emitter.instruction("mov r10, 8");                                          // runtime tag 8 = PHP null for the null pointer word
    emitter.instruction("test rdi, rdi");                                       // is this the null pointer word?
    emitter.instruction("cmovz rax, r10");                                      // box 0 as PHP null
    emitter.label("__rt_mixed_from_value_x86_tagged");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before boxing the mixed payload
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the temporary payload spill
    emitter.instruction("sub rsp, 32");                                         // reserve local slots for tag, value_lo, value_hi, and future helper-preserved scratch state
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the runtime value tag across helper-driven ownership normalization
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // save the low payload word across helper calls and the final heap allocation
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                       // save the high payload word across helper calls and the final heap allocation
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // only genuine tracked resources bind a canonical cell
    emitter.instruction("cmp rax, 4");                                          // do only container-shaped tags need null-payload normalization?
    emitter.instruction("jl __rt_mixed_from_value_dispatch");                   // scalar tags preserve every payload bit pattern
    emitter.instruction("cmp rax, 6");                                          // indexed arrays, hashes, and objects are the container-shaped tags
    emitter.instruction("jg __rt_mixed_from_value_dispatch");                   // nested Mixed/callable tags use ordinary pointer ownership
    emit_branch_if_null_container(
        emitter,
        "rdi",
        "r10",
        "__rt_mixed_from_value_null_container",
    );
    emitter.label("__rt_mixed_from_value_dispatch");
    emitter.instruction("cmp rax, 1");                                          // detect string payloads that need their own owned copy inside the mixed box
    emitter.instruction("je __rt_mixed_from_value_string");                     // strings must be persisted so the mixed cell owns a stable payload
    emitter.instruction("cmp rax, 4");                                          // detect indexed arrays that participate in refcounted ownership
    emitter.instruction("je __rt_mixed_from_value_retain");                     // retain indexed arrays before storing them inside the mixed cell
    emitter.instruction("cmp rax, 5");                                          // detect associative arrays that participate in refcounted ownership
    emitter.instruction("je __rt_mixed_from_value_retain");                     // retain associative arrays before storing them inside the mixed cell
    emitter.instruction("cmp rax, 6");                                          // detect objects that participate in refcounted ownership
    emitter.instruction("je __rt_mixed_from_value_retain");                     // retain objects before storing them inside the mixed cell
    emitter.instruction("cmp rax, 7");                                          // detect nested mixed cells that participate in refcounted ownership
    emitter.instruction("je __rt_mixed_from_value_retain");                     // retain nested mixed cells before storing them inside the parent mixed cell
    emitter.instruction("cmp rax, 10");                                         // detect callable descriptors that participate in callable ownership
    emitter.instruction("je __rt_mixed_from_value_retain");                     // retain callable descriptors before storing them inside the mixed cell
    emitter.instruction("cmp rax, 9");                                          // detect PHP resources, which carry a displayed id rather than ownership
    emitter.instruction("je __rt_mixed_from_value_resource");                   // resources need a display id bound before they can be shown
    emitter.instruction("jmp __rt_mixed_from_value_alloc");                     // scalars can be boxed directly without additional ownership work

    emitter.emit_mixed_resource_identity();

    emitter.label("__rt_mixed_from_value_null_container");
    emitter.instruction("mov QWORD PTR [rbp - 8], 8");                          // replace the container-shaped tag with canonical Mixed null
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // canonical null has no low payload or ownership
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // canonical null has no high payload
    emitter.instruction("jmp __rt_mixed_from_value_alloc");                     // allocate the normalized Mixed null without retaining the sentinel

    emitter.label("__rt_mixed_from_value_string");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // move the source string pointer into the x86_64 string helper input register
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // move the source string length into the paired x86_64 string helper register
    emitter.instruction("call __rt_str_persist");                               // duplicate the string payload so the new mixed owner receives heap-backed storage
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // replace the saved low payload word with the owned string pointer returned by str_persist
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // replace the saved high payload word with the owned string length returned by str_persist
    emitter.instruction("jmp __rt_mixed_from_value_alloc");                     // continue boxing once the string payload is safely owned

    emitter.label("__rt_mixed_from_value_retain");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // move the shared heap child into the x86_64 refcount helper input register
    emitter.instruction("call __rt_incref");                                    // retain the shared heap child for the new mixed owner

    emitter.label("__rt_mixed_from_value_alloc");
    emitter.instruction(&format!("mov rax, {}", crate::layout::MIXED_CELL_SIZE)); // mixed cells store tag plus two payload words in the owned heap allocation
    emitter.instruction("call __rt_heap_alloc");                                // allocate the mixed cell storage through the x86_64 heap wrapper
    emitter.instruction(&format!("mov r10, 0x{:x}", crate::layout::x86_64_heap_kind_word(crate::layout::MIXED_HEAP_KIND))); // materialize the mixed-cell heap kind word with the x86_64 heap marker
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the allocated payload as a mixed cell in the uniform heap header
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the saved runtime value tag after helper-driven ownership normalization
    emitter.instruction("mov QWORD PTR [rax], r10");                            // store the runtime value tag at mixed[0]
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the normalized low payload word after ownership helpers completed
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // store the low payload word at mixed[8]
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the normalized high payload word after ownership helpers completed
    emitter.instruction("mov QWORD PTR [rax + 16], r10");                       // store the high payload word at mixed[16]
    super::resource_weak::emit_publish(emitter, 32, "__rt_mixed_from_value_done");
    emitter.label("__rt_mixed_from_value_done");
    emitter.instruction("add rsp, 32");                                         // release the temporary payload spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.instruction("ret");                                                 // return the boxed mixed pointer in rax
}
