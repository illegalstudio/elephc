//! Purpose:
//! Keeps a managed hash alive across reentrant construction writes without changing PHP COW.
//!
//! Called from:
//! - The managed runtime emitter and internal writers with a borrowed live hash.
//!
//! Key details:
//! - Each pin contributes one physical reference and one non-owner count in the hash header.
//! - The collector sees a root; ordinary mutation subtracts pins when deciding to copy.
//! - Unpinning may destroy the hash and must use the caller's exception/cleanup boundary.

use crate::codegen_support::{emit::Emitter, platform::Arch};
use super::hash_layout::PINS_OFFSET;

/// Emits paired pins for nonnull managed hashes, taking a standard C ABI pointer.
/// Pin returns the same hash. Every successful pin requires exactly one unpin;
/// unpin has no return value and releases its physical root through normal deep cleanup.
pub fn emit_hash_pin(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash lifetime pins ---");
    emitter.label_global("__rt_hash_pin");
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rax, rdi");                                    // retain and return the C ABI hash argument
        emitter.instruction("add DWORD PTR [rax - 12], 1");                     // keep the header and its graph reachable during callbacks
        emitter.instruction(&format!("add QWORD PTR [rax + {PINS_OFFSET}], 1")); // exclude this root from logical PHP ownership
        emitter.instruction("ret");                                             // return the pinned stable header
        emitter.label_global("__rt_hash_unpin");
        emitter.instruction("mov rax, rdi");                                    // adapt the C ABI pointer to the private release ABI
        emitter.instruction(&format!("sub QWORD PTR [rax + {PINS_OFFSET}], 1")); // remove the non-owner count before any destruction
        emitter.instruction("jmp __rt_decref_hash");                            // release the physical root through ordinary hash cleanup
    } else {
        emitter.instruction("ldr w9, [x0, #-12]");                              // read the current physical reference count
        emitter.instruction("add w9, w9, #1");                                  // keep the header and its graph reachable during callbacks
        emitter.instruction("str w9, [x0, #-12]");                              // publish the retained physical root
        emitter.instruction(&format!("ldr x9, [x0, #{PINS_OFFSET}]"));          // read the existing number of internal roots
        emitter.instruction("add x9, x9, #1");                                  // exclude this root from logical PHP ownership
        emitter.instruction(&format!("str x9, [x0, #{PINS_OFFSET}]"));          // publish the lifetime pin
        emitter.instruction("ret");                                             // return the pinned stable header
        emitter.label_global("__rt_hash_unpin");
        emitter.instruction(&format!("ldr x9, [x0, #{PINS_OFFSET}]"));          // read the live internal root count
        emitter.instruction("sub x9, x9, #1");                                  // retire exactly this lifetime pin
        emitter.instruction(&format!("str x9, [x0, #{PINS_OFFSET}]"));          // remove the non-owner count before any destruction
        emitter.instruction("b __rt_decref_hash");                              // release the physical root through ordinary hash cleanup
    }
}
