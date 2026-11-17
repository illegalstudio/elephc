//! Purpose:
//! Stages one borrowed string byte through temporary native stack storage.
//!
//! Called from:
//! - Compiler lowering for consumers that persist a string offset result.
//!
//! Key details:
//! - Empty and missing pairs pass through unchanged with the same stack shape.

use crate::{Arch, Emitter, TargetAbi};

/// Reserves a temporary slot and redirects present one-byte string views through it.
pub fn emit_stage(emitter: &mut impl Emitter, pointer_reg: &str, length_reg: &str, ready: &str) {
    emitter.emit_reserve_temporary_stack(16);
    match emitter.arch() {
        Arch::AArch64 => {
            // -- stage a present offset byte before string persistence --
            emitter.instruction(&format!("cmp {length_reg}, #1"));              // only a present offset has one byte to stage
            emitter.instruction(&format!("b.ne {ready}"));                      // preserve the original empty or missing string pair
            emitter.instruction(&format!("ldrb w9, [{pointer_reg}]"));          // read the selected byte before its receiver can be retired
            emitter.instruction("strb w9, [sp]");                               // stage the byte outside the managed heap
            emitter.instruction(&format!("mov {pointer_reg}, sp"));             // persistence must duplicate stack storage instead of taking over a heap view
        }
        Arch::X86_64 => {
            // -- stage a present offset byte before string persistence --
            emitter.instruction(&format!("cmp {length_reg}, 1"));               // only a present offset has one byte to stage
            emitter.instruction(&format!("jne {ready}"));                       // preserve the original empty or missing string pair
            emitter.instruction(&format!("movzx r10d, BYTE PTR [{pointer_reg}]")); // read the selected byte before its receiver can be retired
            emitter.instruction("mov BYTE PTR [rsp], r10b");                    // stage the byte outside the managed heap
            emitter.instruction(&format!("mov {pointer_reg}, rsp"));            // persistence must duplicate stack storage instead of taking over a heap view
        }
    }
    emitter.label(ready);
}

/// Releases the aligned scratch reserved by the byte-view stage.
pub fn emit_retire(emitter: &mut impl TargetAbi) {
    emitter.emit_release_temporary_stack(16);
}
