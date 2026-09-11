//! Purpose:
//! Exposes persistent automatic array indices to Magician without unwinding across Rust.
//!
//! Called from:
//! - Eval runtime emission and the RuntimeValueOps array-next-index callback.
//!
//! Key details:
//! - The C callback borrows a boxed array and writes a signed index on status zero.
//! - Status one reports exhaustion; status two reports unsupported storage.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Copies exact source history into a freshly rebuilt hash without acquiring PHP owners.
fn emit_copy_history(emitter: &mut Emitter) {
    use crate::codegen_support::runtime::arrays::hash_layout::NEXT_INDEX_OFFSET;
    let arm = emitter.target.arch == Arch::AArch64;
    super::label_c_global(emitter, "__elephc_eval_value_array_copy_index_history");
    if arm {
        emitter.instruction("sub sp, sp, #32");                                 // retain destination, source history, and linkage
        emitter.instruction("stp x29, x30, [sp, #16]");                         // preserve the caller across reference resolution
        emitter.instruction("str x1, [sp]");                                    // retain the freshly reconstructed destination
    } else {
        emitter.instruction("push rbp");                                        // preserve linkage and align calls
        emitter.instruction("mov rbp, rsp");                                    // establish the metadata callback frame
        emitter.instruction("sub rsp, 16");                                     // retain destination and source history
        emitter.instruction("mov QWORD PTR [rsp], rsi");                        // retain the reconstructed destination
        emitter.instruction("mov rax, rdi");                                    // adapt the source to private dereferencing
    }
    abi::emit_call_label(emitter, "__rt_mixed_deref");
    emitter.instruction(if arm { "cbz x0, __rt_eval_copy_index_invalid" } else { "test rax, rax" }); // validate the borrowed source cell
    if !arm { emitter.instruction("jz __rt_eval_copy_index_invalid"); }         // reject a missing source
    emitter.instruction(if arm { "ldr x9, [x0]" } else { "mov r11, QWORD PTR [rax]" }); // inspect the source representation
    emitter.instruction(if arm { "ldr x0, [x0, #8]" } else { "mov rax, QWORD PTR [rax + 8]" }); // borrow source array storage
    emitter.instruction(if arm { "cmp x9, #4" } else { "cmp r11, 4" });         // indexed arrays preserve their dense length
    emitter.instruction(if arm { "b.eq __rt_eval_copy_index_dense" } else { "je __rt_eval_copy_index_dense" }); // derive history before deleting a dense tail
    emitter.instruction(if arm { "cmp x9, #5" } else { "cmp r11, 5" });         // associative arrays own the signed counter
    emitter.instruction(if arm { "b.ne __rt_eval_copy_index_invalid" } else { "jne __rt_eval_copy_index_invalid" }); // reject unsupported source values
    super::emit_branch_if_null_container(emitter, if arm { "x0" } else { "rax" }, if arm { "x10" } else { "r10" }, "__rt_eval_copy_index_empty");
    emitter.instruction(&if arm { format!("ldr x0, [x0, #{NEXT_INDEX_OFFSET}]") } else { format!("mov rax, QWORD PTR [rax + {NEXT_INDEX_OFFSET}]") }); // retain the exact counter, including its initial sentinel
    emitter.instruction(if arm { "b __rt_eval_copy_index_resolve" } else { "jmp __rt_eval_copy_index_resolve" }); // preserve associative history across destination resolution
    emitter.label("__rt_eval_copy_index_dense");
    super::emit_branch_if_null_container(emitter, if arm { "x0" } else { "rax" }, if arm { "x10" } else { "r10" }, "__rt_eval_copy_index_empty");
    emitter.instruction(if arm { "ldr x0, [x0]" } else { "mov rax, QWORD PTR [rax]" }); // keep the original dense append index
    emitter.instruction(if arm { "cbnz x0, __rt_eval_copy_index_resolve" } else { "test rax, rax" }); // preserve the nonempty dense length
    if !arm { emitter.instruction("jnz __rt_eval_copy_index_resolve"); }        // an empty dense source has no integer insertion history
    emitter.label("__rt_eval_copy_index_empty");
    abi::emit_load_int_immediate(emitter, if arm { "x0" } else { "rax" }, i64::MIN);
    emitter.label("__rt_eval_copy_index_resolve");
    emitter.instruction(if arm { "str x0, [sp, #8]" } else { "mov QWORD PTR [rsp + 8], rax" }); // retain exact metadata across destination dereferencing
    emitter.instruction(if arm { "ldr x0, [sp]" } else { "mov rax, QWORD PTR [rsp]" }); // resolve the newly reconstructed cell
    abi::emit_call_label(emitter, "__rt_mixed_deref");
    emitter.instruction(if arm { "cbz x0, __rt_eval_copy_index_invalid" } else { "test rax, rax" }); // require a valid destination cell
    if !arm { emitter.instruction("jz __rt_eval_copy_index_invalid"); }         // reject an absent destination
    emitter.instruction(if arm { "ldr x9, [x0]" } else { "mov r11, QWORD PTR [rax]" }); // inspect destination storage
    emitter.instruction(if arm { "cmp x9, #5" } else { "cmp r11, 5" });         // arbitrary history requires associative storage
    emitter.instruction(if arm { "b.ne __rt_eval_copy_index_invalid" } else { "jne __rt_eval_copy_index_invalid" }); // preserve unsupported destinations unchanged
    emitter.instruction(if arm { "ldr x0, [x0, #8]" } else { "mov rax, QWORD PTR [rax + 8]" }); // borrow the fresh destination hash
    super::emit_branch_if_null_container(emitter, if arm { "x0" } else { "rax" }, if arm { "x10" } else { "r10" }, "__rt_eval_copy_index_invalid");
    emitter.instruction(if arm { "ldr x9, [sp, #8]" } else { "mov r10, QWORD PTR [rsp + 8]" }); // recover the original insertion history
    emitter.instruction(&if arm { format!("str x9, [x0, #{NEXT_INDEX_OFFSET}]") } else { format!("mov QWORD PTR [rax + {NEXT_INDEX_OFFSET}], r10") }); // retain deleted-key history after copying live entries
    emitter.instruction(if arm { "mov x0, #0" } else { "xor eax, eax" });       // report successful metadata transfer
    emitter.instruction(if arm { "b __rt_eval_copy_index_done" } else { "jmp __rt_eval_copy_index_done" }); // share callback cleanup
    emitter.label("__rt_eval_copy_index_invalid");
    emitter.instruction(if arm { "mov x0, #2" } else { "mov eax, 2" });         // return unsupported storage without running PHP
    emitter.label("__rt_eval_copy_index_done");
    if arm {
        emitter.instruction("ldp x29, x30, [sp, #16]");                         // restore caller linkage
        emitter.instruction("add sp, sp, #32");                                 // retire borrowed metadata storage
    } else {
        emitter.instruction("leave");                                           // restore the caller frame and stack
    }
    emitter.instruction("ret");                                                 // return the independent host status
}

/// Emits a C2 boxed-array/output-index callback sharing native hash history and reference resolution.
pub(crate) fn emit(emitter: &mut Emitter) {
    emit_copy_history(emitter);
    let arm = emitter.target.arch == Arch::AArch64;
    super::label_c_global(emitter, "__elephc_eval_value_array_next_index");
    if arm {
        emitter.instruction("sub sp, sp, #32");                                 // preserve the borrowed output pointer and caller linkage
        emitter.instruction("stp x29, x30, [sp, #16]");                         // retain linkage across reference resolution
        emitter.instruction("str x1, [sp]");                                    // retain writable index output across nested helpers
    } else {
        emitter.instruction("push rbp");                                        // preserve linkage and align nested calls
        emitter.instruction("mov rbp, rsp");                                    // establish the callback frame
        emitter.instruction("sub rsp, 16");                                     // preserve the writable index output
        emitter.instruction("mov QWORD PTR [rsp], rsi");                        // keep the output pointer across reference resolution
        emitter.instruction("mov rax, rdi");                                    // adapt the boxed receiver to the private dereference convention
    }
    abi::emit_call_label(emitter, "__rt_mixed_deref");
    if arm {
        emitter.instruction("cbz x0, __rt_eval_next_index_invalid");            // reject malformed absent boxed cells
        emitter.instruction("ldr x9, [x0]");                                    // inspect the referenced PHP value
        emitter.instruction("ldr x0, [x0, #8]");                                // borrow its array storage
        emitter.instruction("cmp x9, #4");                                      // indexed arrays use their dense logical length
        emitter.instruction("b.eq __rt_eval_next_index_dense");                 // share the checked output publication
        emitter.instruction("cmp x9, #5");                                      // hashes own persistent automatic-key history
        emitter.instruction("b.ne __rt_eval_next_index_invalid");               // leave non-array values untouched
        super::emit_branch_if_null_container(emitter, "x0", "x10", "__rt_eval_next_index_zero");
    } else {
        emitter.instruction("test rax, rax");                                   // reject malformed absent boxed cells
        emitter.instruction("jz __rt_eval_next_index_invalid");                 // avoid dereferencing a missing receiver
        emitter.instruction("mov r10, QWORD PTR [rax]");                        // inspect the referenced PHP value
        emitter.instruction("mov rax, QWORD PTR [rax + 8]");                    // borrow its array storage
        emitter.instruction("cmp r10, 4");                                      // indexed arrays use their dense logical length
        emitter.instruction("je __rt_eval_next_index_dense");                   // share checked output publication
        emitter.instruction("cmp r10, 5");                                      // hashes own persistent automatic-key history
        emitter.instruction("jne __rt_eval_next_index_invalid");                // leave non-array values untouched
        super::emit_branch_if_null_container(emitter, "rax", "r10", "__rt_eval_next_index_zero");
        emitter.instruction("mov rdi, rax");                                    // pass the current hash through the C argument convention
    }
    abi::emit_call_label(emitter, "__rt_hash_try_next_index");
    emitter.instruction(if arm { "cbz x1, __rt_eval_next_index_exhausted" } else { "test edx, edx" }); // preserve silent exhaustion across the Rust boundary
    if !arm { emitter.instruction("jz __rt_eval_next_index_exhausted"); }       // let Magician construct the PHP Error in its own exception context
    emitter.instruction(if arm { "b __rt_eval_next_index_publish" } else { "jmp __rt_eval_next_index_publish" }); // return the native index without creating a detached array snapshot
    emitter.label("__rt_eval_next_index_dense");
    super::emit_branch_if_null_container(emitter, if arm { "x0" } else { "rax" }, if arm { "x10" } else { "r10" }, "__rt_eval_next_index_zero");
    emitter.instruction(if arm { "ldr x0, [x0]" } else { "mov rax, QWORD PTR [rax]" }); // dense arrays append at their logical length
    emitter.instruction(if arm { "b __rt_eval_next_index_publish" } else { "jmp __rt_eval_next_index_publish" }); // preserve the computed dense index
    emitter.label("__rt_eval_next_index_zero");
    emitter.instruction(if arm { "mov x0, #0" } else { "xor eax, eax" });       // legacy null container payloads start at zero
    emitter.label("__rt_eval_next_index_publish");
    emitter.instruction(if arm { "ldr x9, [sp]" } else { "mov r10, QWORD PTR [rsp]" }); // recover the caller's writable signed-index slot
    emitter.instruction(if arm { "str x0, [x9]" } else { "mov QWORD PTR [r10], rax" }); // publish the index only for a successful lookup
    emitter.instruction(if arm { "mov x0, #0" } else { "xor eax, eax" });       // report successful lookup without transferring an owner
    emitter.instruction(if arm { "b __rt_eval_next_index_done" } else { "jmp __rt_eval_next_index_done" }); // share callback teardown
    emitter.label("__rt_eval_next_index_exhausted");
    emitter.instruction(if arm { "mov x0, #1" } else { "mov eax, 1" });         // report saturation without executing PHP or unwinding
    emitter.instruction(if arm { "b __rt_eval_next_index_done" } else { "jmp __rt_eval_next_index_done" }); // preserve the exhaustion status
    emitter.label("__rt_eval_next_index_invalid");
    emitter.instruction(if arm { "mov x0, #2" } else { "mov eax, 2" });         // reject unsupported storage without an index payload
    emitter.label("__rt_eval_next_index_done");
    if arm {
        emitter.instruction("ldp x29, x30, [sp, #16]");                         // restore linkage after reference resolution and lookup
        emitter.instruction("add sp, sp, #32");                                 // retire callback-local output storage
    } else {
        emitter.instruction("leave");                                           // restore the caller and release local output storage
    }
    emitter.instruction("ret");                                                 // return the independent host status to Rust
}
