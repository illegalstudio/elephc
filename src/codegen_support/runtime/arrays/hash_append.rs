//! Purpose:
//! Appends owned values through PHP's persistent automatic integer hash index.
//!
//! Called from:
//! - Runtime emission and Mixed associative-array append.
//!
//! Key details:
//! - Query registration shares the nonthrowing index probe.
//! - Failed appends retire incoming ownership before raising the catchable Error.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits C-ABI hash/value-low/value-high/tag append and transfers the supplied value owner.
pub fn emit_hash_append(emitter: &mut Emitter) {
    let arm = emitter.target.arch == Arch::AArch64;
    super::hash_next_index::emit(emitter);
    emitter.label_global("__rt_hash_append");
    if arm {
        emitter.instruction("sub sp, sp, #48");                                 // retain inputs and linkage across append-key lookup
        emitter.instruction("stp x29, x30, [sp, #32]");                         // preserve the caller across lookup and insertion
        emitter.instruction("stp x0, x1, [sp]");                                // retain the hash and owned value payload
        emitter.instruction("stp x2, x3, [sp, #16]");                           // retain the high payload and concrete value tag
    } else {
        emitter.instruction("push rbp");                                        // preserve linkage and align nested calls
        emitter.instruction("mov rbp, rsp");                                    // establish the append frame
        emitter.instruction("sub rsp, 32");                                     // retain the hash and all owned value words
        for (offset, reg) in [(0, "rdi"), (8, "rsi"), (16, "rdx"), (24, "rcx")] {
            emitter.instruction(&format!("mov QWORD PTR [rsp + {offset}], {reg}")); // preserve one append input across the shared probe
        }
    }
    abi::emit_call_label(emitter, "__rt_hash_try_next_index");
    if arm {
        emitter.instruction("cbz x1, __rt_hash_append_failed");                 // do not overwrite an occupied saturated key
        emitter.instruction("mov x1, x0");                                      // supply the checked next integer index
        emitter.instruction("ldr x0, [sp]");                                    // restore the hash that owns insertion history
        emitter.instruction("mov x2, #-1");                                     // select integer-key insertion
        emitter.instruction("ldr x3, [sp, #8]");                                // transfer the owned value payload
        emitter.instruction("ldp x4, x5, [sp, #16]");                           // supply the high word and concrete value tag
    } else {
        emitter.instruction("test edx, edx");                                   // inspect the shared availability result
        emitter.instruction("jz __rt_hash_append_failed");                      // preserve an occupied maximum key and its existing owner
        emitter.instruction("mov rsi, rax");                                    // supply the checked integer index
        emitter.instruction("mov rdi, QWORD PTR [rsp]");                        // restore the current hash
        emitter.instruction("mov rdx, -1");                                     // select integer-key insertion
        emitter.instruction("mov rcx, QWORD PTR [rsp + 8]");                    // transfer the owned low payload
        emitter.instruction("mov r8, QWORD PTR [rsp + 16]");                    // supply the high payload word
        emitter.instruction("mov r9, QWORD PTR [rsp + 24]");                    // supply the concrete value tag
    }
    abi::emit_call_label(emitter, "__rt_hash_set");
    restore(emitter);
    emitter.instruction("ret");                                                 // return the hash after any COW split or growth
    emitter.label("__rt_hash_append_failed");
    emitter.instruction(if arm { "ldr x9, [sp, #24]" } else { "mov r10, QWORD PTR [rsp + 24]" }); // inspect the uninserted value's ownership
    for (tag, arm_condition, x86_condition, label) in [
        (1, "eq", "je", "release"), (8, "eq", "je", "throw"),
        (10, "eq", "je", "release_callable"), (4, "lo", "jb", "throw"),
    ] {
        emitter.instruction(&if arm { format!("cmp x9, #{tag}") } else { format!("cmp r10, {tag}") }); // classify string, null, callable, and scalar ownership
        emitter.instruction(&format!("{} __rt_hash_append_{label}", if arm { format!("b.{arm_condition}") } else { x86_condition.to_owned() })); // release only transferred heap owners
    }
    emitter.label("__rt_hash_append_release");
    emitter.instruction(if arm { "ldr x0, [sp, #8]" } else { "mov rax, QWORD PTR [rsp + 8]" }); // consume the uninserted string or refcounted payload
    abi::emit_call_label(emitter, "__rt_decref_any");
    emitter.instruction(if arm { "b __rt_hash_append_throw" } else { "jmp __rt_hash_append_throw" }); // share exception entry after releasing ownership
    emitter.label("__rt_hash_append_release_callable");
    emitter.instruction(if arm { "ldr x0, [sp, #8]" } else { "mov rax, QWORD PTR [rsp + 8]" }); // consume the uninserted callable descriptor
    abi::emit_call_label(emitter, "__rt_callable_descriptor_release");
    emitter.label("__rt_hash_append_throw");
    restore(emitter);
    emitter.instruction(if arm { "b __rt_hash_append_error" } else { "jmp __rt_hash_append_error" }); // raise Error after retiring local transferred ownership
}

/// Restores append linkage while preserving the returned hash or completed release outcome.
fn restore(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction("ldp x29, x30, [sp, #32]");                         // restore caller linkage after the nested operation
        emitter.instruction("add sp, sp, #48");                                 // retire append input storage
    } else {
        emitter.instruction("leave");                                           // release input storage and restore the caller frame
    }
}
