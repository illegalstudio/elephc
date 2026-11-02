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

use super::{abi, hash_layout, Arch, Emitter};

/// Recognizes a persistent wrapper after the boxed invoker-marker probe has declined it.
pub(super) fn emit_probe(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x15, #7");                                 // persistent references use a nested Mixed box
            emitter.instruction("b.ne __rt_hash_set_check_reference");          // ordinary boxes keep the existing replacement path
            emitter.instruction("ldr x15, [x14, #16]");                         // inspect the persistent-reference marker
            emitter.instruction("cmp x15, #1");                                 // only marker one carries PHP reference identity
            emitter.instruction("b.eq __rt_hash_set_persistent_write");         // preserve the wrapper and update its child
        }
        Arch::X86_64 => {
            emitter.instruction("cmp QWORD PTR [r14], 7");                      // persistent references use a nested Mixed box
            emitter.instruction("jne __rt_hash_set_check_reference_x");         // ordinary boxes keep the existing replacement path
            emitter.instruction("cmp QWORD PTR [r14 + 16], 1");                 // only marker one carries PHP reference identity
            emitter.instruction("je __rt_hash_set_persistent_write_x");         // preserve the wrapper and update its child
        }
    }
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
            emitter.instruction("bl __rt_reference_replace");                   // publish before returning the displaced child owner
            abi::emit_push_reg(emitter, "x0");
            emitter.instruction("ldr x0, [sp, #40]");                           // recover the incoming owner below the displaced-child spill
            abi::emit_call_label(emitter, "__rt_decref_any");
            abi::emit_pop_reg(emitter, "x0");
            abi::emit_call_label(emitter, "__rt_decref_any");
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
            emitter.instruction("call __rt_reference_replace");                 // publish before returning the displaced child owner
            abi::emit_push_reg(emitter, "rax");
            emitter.instruction("mov rax, QWORD PTR [rbp - 32]");               // recover the incoming owner independently of the displaced child
            abi::emit_call_label(emitter, "__rt_decref_any");
            abi::emit_pop_reg(emitter, "rax");
            abi::emit_call_label(emitter, "__rt_decref_any");
            emitter.instruction("jmp __rt_hash_set_reference_done_x");          // return the unchanged wrapper-bearing table
        }
    }
}

/// Selects reference entries that a bucket-replacing array builder must detach first.
pub(super) fn emit_value_detach_probe(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x9, #11");                                 // raw managed reference entries also detach
            emitter.instruction("b.eq __rt_hash_set_value_detach");             // clear the bucket before releasing its cell owner
            emitter.instruction("cmp x9, #7");                                  // boxed references carry the ordinary Mixed entry tag
            emitter.instruction("b.ne __rt_hash_set_value_forward");            // ordinary entries need no detachment
            emitter.instruction("ldr x10, [x4, #24]");                          // inspect the existing box without changing ownership
            emitter.instruction("cbz x10, __rt_hash_set_value_forward");        // a null Mixed handle is an ordinary value
            emitter.instruction("ldr x11, [x10]");                              // distinguish invoker markers and persistent wrappers
            emitter.instruction("cmp x11, #11");                                // boxed invoker references also detach for array builders
            emitter.instruction("b.eq __rt_hash_set_value_detach");             // preserve their caller's original storage
            emitter.instruction("cmp x11, #7");                                 // only a nested Mixed box can be a persistent reference
            emitter.instruction("b.ne __rt_hash_set_value_forward");            // ordinary boxed values keep their existing replacement path
            emitter.instruction("ldr x11, [x10, #16]");                         // inspect the persistent-reference marker
            emitter.instruction("cmp x11, #1");                                 // marker one identifies writable PHP reference identity
            emitter.instruction("b.ne __rt_hash_set_value_forward");            // ordinary nested values need no detachment
            emitter.label("__rt_hash_set_value_detach");
        }
        Arch::X86_64 => {
            emitter.instruction("cmp QWORD PTR [r8 + 40], 11");                 // raw managed reference entries also detach
            emitter.instruction("je __rt_hash_set_value_detach_x");             // clear the bucket before releasing its cell owner
            emitter.instruction("cmp QWORD PTR [r8 + 40], 7");                  // boxed references carry the ordinary Mixed entry tag
            emitter.instruction("jne __rt_hash_set_value_forward_x");           // ordinary entries need no detachment
            emitter.instruction("mov r10, QWORD PTR [r8 + 24]");                // inspect the existing box without changing ownership
            emitter.instruction("test r10, r10");                               // a null Mixed handle is an ordinary value
            emitter.instruction("jz __rt_hash_set_value_forward_x");            // skip absent boxed storage
            emitter.instruction("cmp QWORD PTR [r10], 11");                     // boxed invoker references also detach for array builders
            emitter.instruction("je __rt_hash_set_value_detach_x");             // preserve their caller's original storage
            emitter.instruction("cmp QWORD PTR [r10], 7");                      // only a nested Mixed box can be a persistent reference
            emitter.instruction("jne __rt_hash_set_value_forward_x");           // ordinary boxed values keep their existing replacement path
            emitter.instruction("cmp QWORD PTR [r10 + 16], 1");                 // marker one identifies writable PHP reference identity
            emitter.instruction("jne __rt_hash_set_value_forward_x");           // ordinary nested values need no detachment
            emitter.label("__rt_hash_set_value_detach_x");
        }
    }
}
