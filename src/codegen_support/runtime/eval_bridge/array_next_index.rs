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

/// Emits a C2 boxed-array/output-index callback sharing native hash history and reference resolution.
pub(crate) fn emit(emitter: &mut Emitter) {
    elephc_memoir::values::hash_next_index::emit_copy_array_history(emitter, "__elephc_eval_value_array_copy_index_history");
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
    let result = abi::int_result_reg(emitter);
    elephc_memoir::values::packed_slots::emit_load_length(emitter, result, result);
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
