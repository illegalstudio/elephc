//! Purpose:
//! Registers stack-local exceptional owners used while native PHP callbacks run.
//!
//! Called from:
//! - Output handlers and mbregex callbacks when staging PHP argument owners.
//!
//! Key details:
//! - The shared exception activation protocol consumes owners only on an escaping throw.
//! - Normal cleanup detaches each guard before consuming its owner.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Registers the native result-register owner in a 32-byte stack record at the supplied offsets.
pub(crate) fn guard(emitter: &mut Emitter, arm: usize, x86: usize) {
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction(&format!("str x0, [sp, #{}]", arm + 24));           // publish exceptional ownership before running PHP
        abi::emit_symbol_address(emitter, "x9", "__rt_exception_release_owned");
        emitter.instruction(&format!("str x9, [sp, #{}]", arm + 8));            // use the protected heap-owner release callback
        emitter.instruction(&format!("add x0, sp, #{arm}"));                    // provide the aligned activation record
        emitter.instruction("mov x1, #0");                                      // insert this owner at the current activation head
    } else {
        emitter.instruction(&format!("mov QWORD PTR [rbp - {}], rax", x86 - 24)); // retain the owner in exceptional cleanup storage
        abi::emit_symbol_address(emitter, "r10", "__rt_exception_release_owned");
        emitter.instruction(&format!("mov QWORD PTR [rbp - {}], r10", x86 - 8)); // install protected release for the staged owner
        emitter.instruction(&format!("lea rdi, [rbp - {x86}]"));                // identify this stack-local activation
        emitter.instruction("xor esi, esi");                                    // insert ownership at the activation chain head
    }
    abi::emit_call_label(emitter, "__rt_exception_guard_owned");
}

/// Detaches one exceptional owner before ordinary release or transfer to the caller.
pub(crate) fn unguard(emitter: &mut Emitter, arm: usize, x86: usize) {
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction(&format!("add x0, sp, #{arm}"));                    // identify the completed exceptional ownership scope
    } else {
        emitter.instruction(&format!("lea rdi, [rbp - {x86}]"));                // identify the exact activation to retire
    }
    abi::emit_call_label(emitter, "__rt_exception_unguard_owned");
}
