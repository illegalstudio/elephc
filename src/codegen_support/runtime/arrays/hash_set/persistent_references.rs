//! Purpose:
//! Preserves persistent PHP reference identity during native hash assignment.
//!
//! Called from:
//! - The ordinary hash-set and bucket-replacing array-builder emitters.
//!
//! Key details:
//! - Ordinary assignment replaces the wrapper child, preserving every alias.
//! - Array builders detach reference entries before publishing independent values.
//! - Incoming owners retire before the displaced child can run a destructor.

use super::{hash_layout, Arch, Emitter};

/// Routes a persistent wrapper to write-through while ordinary values keep the existing path.
pub(super) fn emit_probe(emitter: &mut Emitter) {
    let (ordinary, persistent) = match emitter.target.arch {
        Arch::AArch64 => ("__rt_hash_set_check_reference", "__rt_hash_set_persistent_write"),
        Arch::X86_64 => ("__rt_hash_set_check_reference_x", "__rt_hash_set_persistent_write_x"),
    };
    elephc_memoir::values::hash_bucket::emit_probe_persistent_wrapper(emitter, ordinary, persistent);
}

/// Publishes a detached incoming value through the existing persistent wrapper.
pub(super) fn emit_write(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.label("__rt_hash_set_persistent_write");
            emitter.instruction("ldr x0, [sp, #40]");                           // recover the incoming value's runtime tag
            emitter.instruction("cmp x0, #7");                                  // an owned Mixed input is already boxed
            emitter.instruction("b.eq __rt_hash_set_persistent_ready");         // retain its owner until publication finishes
            emitter.instruction("ldr x1, [sp, #24]");                           // supply the owned raw low payload
            emitter.instruction("ldr x2, [sp, #32]");                           // supply the owned raw high payload
            emitter.instruction("bl __rt_hash_to_mixed_box_owned");             // convert and consume the raw input owner
            emitter.instruction("str x0, [sp, #24]");                           // remember the prepared Mixed owner
            emitter.label("__rt_hash_set_persistent_ready");
            emitter.instruction("ldr x5, [sp]");                                // recover the unique destination table
            emitter.instruction("ldr x9, [sp, #48]");                           // recover the destination bucket index
            hash_layout::emit_entry_address(emitter, "x12", "x5", "x9");
            emitter.instruction("ldr x0, [x12, #24]");                          // retain the destination wrapper in its bucket
            emitter.instruction("ldr x1, [sp, #24]");                           // lend the prepared value to the shared publication helper
            elephc_memoir::reference_cells::mixed_payload::emit_replace_persistent_owner(emitter, "[sp, #40]");
            emitter.instruction("b __rt_hash_set_done");                        // return the unchanged wrapper-bearing table
        }
        Arch::X86_64 => {
            emitter.label("__rt_hash_set_persistent_write_x");
            emitter.instruction("cmp QWORD PTR [rbp - 48], 7");                 // an owned Mixed input is already boxed
            emitter.instruction("je __rt_hash_set_persistent_ready_x");         // retain its owner until publication finishes
            emitter.instruction("mov rax, QWORD PTR [rbp - 48]");               // recover the incoming value's runtime tag
            emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");               // supply the owned raw low payload
            emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");               // supply the owned raw high payload
            emitter.instruction("call __rt_hash_to_mixed_x86_box_owned");       // convert and consume the raw input owner
            emitter.instruction("mov QWORD PTR [rbp - 32], rax");               // remember the prepared Mixed owner
            emitter.label("__rt_hash_set_persistent_ready_x");
            emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                // recover the unique destination table
            emitter.instruction("mov r11, QWORD PTR [rbp - 56]");               // recover the destination bucket index
            hash_layout::emit_entry_address(emitter, "r12", "r10", "r11");
            emitter.instruction("mov rax, QWORD PTR [r12 + 24]");               // retain the destination wrapper in its bucket
            emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");               // lend the prepared value to the shared publication helper
            elephc_memoir::reference_cells::mixed_payload::emit_replace_persistent_owner(emitter, "[rbp - 32]");
            emitter.instruction("jmp __rt_hash_set_reference_done_x");          // return the unchanged wrapper-bearing table
        }
    }
}

/// Routes occupied buckets through the extractor before an independent-value replacement.
pub(super) fn emit_value_detach_probe(emitter: &mut Emitter) {
    let (detach, forward) = match emitter.target.arch {
        Arch::AArch64 => ("__rt_hash_set_value_detach", "__rt_hash_set_value_forward"),
        Arch::X86_64 => ("__rt_hash_set_value_detach_x", "__rt_hash_set_value_forward_x"),
    };
    elephc_memoir::values::hash_bucket::emit_value_detach_probe(emitter, detach, forward);
    emitter.label(detach);
}
