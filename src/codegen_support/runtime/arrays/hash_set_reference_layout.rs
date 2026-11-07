//! Purpose:
//! Selects shared nullable-reference hash writes before copy-on-write consumes the caller's owner.
//!
//! Called from:
//! - The framed entry of both target variants of `__rt_hash_set`.
//!
//! Key details:
//! - Ordinary unique hashes avoid an extra lookup.
//! - Shape12 writes leave the marker and hash unchanged, so their existing writer can run before COW.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch, sentinels::NULL_SENTINEL};
use crate::codegen_support::callable_invoker_args::{INVOKER_ARG_REF_CELL_TAG, INVOKER_ARG_TAGGED_SCALAR_SOURCE};
use super::hash_layout::PINS_OFFSET;

/// Routes a shared hash's shape12 marker to its validating writer before cloning any storage.
pub(super) fn emit_shared_tagged_reference_selection(emitter: &mut Emitter) {
    let done = "__rt_hash_set_reference_layout_selected";
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbz x0, {done}"));                    // a missing table cannot contain an existing reference marker
            abi::emit_load_int_immediate(emitter, "x9", NULL_SENTINEL);
            emitter.instruction("cmp x0, x9");                                  // a sentinel table has no heap header to inspect
            emitter.instruction(&format!("b.eq {done}"));                       // preserve the existing null-container normalization path
            emitter.instruction("ldr w9, [x0, #-12]");                          // count logical PHP owners before COW can consume this one
            emitter.instruction(&format!("ldr x10, [x0, #{PINS_OFFSET}]"));     // lifetime pins do not require copy-on-write separation
            emitter.instruction("sub x9, x9, x10");                             // use the same logical owner count as hash_ensure_unique
            emitter.instruction("cmp x9, #1");                                  // a unique hash keeps its header even if its entries grow
            emitter.instruction(&format!("b.ls {done}"));                       // unique ordinary writes need no preliminary hash lookup
            abi::emit_call_label(emitter, "__rt_hash_get");
            emitter.instruction(&format!("cbz x4, {done}"));                    // an absent key cannot write through a marker
            emitter.instruction("ldr x9, [x4, #40]");                           // inspect the original entry rather than its dereferenced value
            emitter.instruction("cmp x9, #7");                                  // invoker markers inhabit boxed Mixed hash entries
            emitter.instruction(&format!("b.ne {done}"));                       // preserve ordinary value and managed-reference updates
            emitter.instruction("ldr x14, [x4, #24]");                          // keep the marker box in the existing tagged writer's input register
            emitter.instruction(&format!("cbz x14, {done}"));                   // empty Mixed slots do not expose caller storage
            emitter.instruction("ldr x9, [x14]");                               // inspect the private descriptor marker tag
            emitter.instruction(&format!("cmp x9, #{INVOKER_ARG_REF_CELL_TAG}")); // ordinary boxes do not alias a caller slot
            emitter.instruction(&format!("b.ne {done}"));                       // leave their ownership with the ordinary setter
            emitter.instruction("ldr x9, [x14, #16]");                          // inspect the physical representation of the aliased slot
            emitter.instruction(&format!("cmp x9, #{INVOKER_ARG_TAGGED_SCALAR_SOURCE}")); // only inline nullable integers use the new refusal path
            emitter.instruction("b.eq __rt_hash_set_invoker_reference_tagged"); // validate and write the cell before any hash-owner transfer
            emitter.label(done);
            emitter.instruction("ldr x0, [sp]");                                // restore the original table for normal COW and null normalization
            emitter.instruction("ldr x9, [sp, #56]");                           // restore the selected-payload mode after lookup scratch use
        }
        Arch::X86_64 => {
            emitter.instruction("test rdi, rdi");                               // a missing table cannot contain an existing reference marker
            emitter.instruction(&format!("jz {done}"));                         // preserve null-table allocation without probing metadata
            abi::emit_load_int_immediate(emitter, "r11", NULL_SENTINEL);
            emitter.instruction("cmp rdi, r11");                                // a sentinel table has no heap header to inspect
            emitter.instruction(&format!("je {done}"));                         // preserve the existing null-container normalization path
            emitter.instruction("mov r10d, DWORD PTR [rdi - 12]");              // count logical PHP owners before COW can consume this one
            emitter.instruction(&format!("sub r10, QWORD PTR [rdi + {PINS_OFFSET}]")); // lifetime pins do not require copy-on-write separation
            emitter.instruction("cmp r10, 1");                                  // a unique hash keeps its header even if its entries grow
            emitter.instruction(&format!("jbe {done}"));                        // unique ordinary writes need no preliminary hash lookup
            abi::emit_call_label(emitter, "__rt_hash_get");
            emitter.instruction("test r8, r8");                                 // an absent key cannot write through a marker
            emitter.instruction(&format!("jz {done}"));                         // preserve ordinary insertion into the hash
            emitter.instruction("cmp QWORD PTR [r8 + 40], 7");                  // inspect the original entry rather than its dereferenced value
            emitter.instruction(&format!("jne {done}"));                        // preserve ordinary value and managed-reference updates
            emitter.instruction("mov r14, QWORD PTR [r8 + 24]");                // keep the marker box in the existing tagged writer's input register
            emitter.instruction("test r14, r14");                               // empty Mixed slots do not expose caller storage
            emitter.instruction(&format!("jz {done}"));                         // leave their ownership with the ordinary setter
            emitter.instruction(&format!("cmp QWORD PTR [r14], {INVOKER_ARG_REF_CELL_TAG}")); // ordinary boxes do not alias a caller slot
            emitter.instruction(&format!("jne {done}"));                        // only a descriptor marker can select its caller storage
            emitter.instruction(&format!("cmp QWORD PTR [r14 + 16], {INVOKER_ARG_TAGGED_SCALAR_SOURCE}")); // recognize inline nullable-integer storage
            emitter.instruction("je __rt_hash_set_invoker_reference_tagged_x"); // validate and write the cell before any hash-owner transfer
            emitter.label(done);
            emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                // restore the original table for normal COW and null normalization
            emitter.instruction("mov r10, QWORD PTR [rbp - 64]");               // restore the selected-payload mode after lookup scratch use
        }
    }
}
