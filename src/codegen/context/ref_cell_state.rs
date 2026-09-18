//! Purpose:
//! Emits runtime state transitions for locals that may store managed reference-cell pointers.
//! Keeps cleanup ownership distinct from the slot's active runtime representation.
//!
//! Called from:
//! - `super::FunctionContext::release_counted_ref_binding()`.
//!
//! Key details:
//! - State zero is raw storage, state one is an ordinary managed-cell pointer, and larger values
//!   are retained hash-entry cell addresses.
//! - Releasing counted managed-cell provenance must preserve state one so later cleanup does not pass
//!   the reference-cell header to a payload-specific destructor.
//! - Counted provenance becomes state one before decref, keeping exception and epilogue cleanup
//!   from reinterpreting the still-present cell pointer as raw payload storage.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Releases only counted managed-cell provenance, preserving raw and ordinary-cell state.
pub(super) fn emit_release_counted_ref_binding(
    emitter: &mut Emitter,
    state_offset: usize,
    done: &str,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::load_at_offset(emitter, "x9", state_offset);
            emitter.instruction("cmp x9, #1");                                  // distinguish a counted cell address from raw and ordinary ref-cell states
            emitter.instruction(&format!("b.ls {done}"));                       // preserve raw state and the ordinary managed-cell marker
            emitter.instruction("mov x10, #1");                                 // leave the slot marked as holding a managed cell during and after release
            abi::store_at_offset_scratch(emitter, "x10", state_offset, "x11");
            abi::emit_reg_move(emitter, "x0", "x9");
            abi::emit_call_label(emitter, "__rt_decref_any");
        }
        Arch::X86_64 => {
            abi::load_at_offset(emitter, "r10", state_offset);
            emitter.instruction("cmp r10, 1");                                  // distinguish a counted cell address from raw and ordinary ref-cell states
            emitter.instruction(&format!("jbe {done}"));                        // preserve raw state and the ordinary managed-cell marker
            emitter.instruction("mov r11, 1");                                  // leave the slot marked as holding a managed cell during and after release
            abi::store_at_offset(emitter, "r11", state_offset);
            abi::emit_reg_move(emitter, "rax", "r10");
            abi::emit_call_label(emitter, "__rt_decref_any");
        }
    }
    emitter.label(done);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// State one reaches the join without being rewritten, while counted provenance is retired.
    #[test]
    fn counted_release_preserves_the_ordinary_cell_marker_on_every_target() {
        for name in [
            "macos-aarch64",
            "ios-arm64",
            "ios-sim-arm64",
            "linux-aarch64",
            "linux-x86_64",
        ] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_release_counted_ref_binding(&mut emitter, 24, "release_done");
            let assembly = emitter.output();
            let branch = assembly
                .find(if name == "linux-x86_64" {
                    "jbe release_done"
                } else {
                    "b.ls release_done"
                })
                .unwrap();
            let marker = assembly
                .find(if name == "linux-x86_64" {
                    "mov QWORD PTR [rbp - 24], r11"
                } else {
                    "stur x10, [x29, #-24]"
                })
                .unwrap();
            let release = assembly.find("__rt_decref_any").unwrap();
            let done = assembly.find("release_done:").unwrap();
            assert!(
                branch < marker && marker < release && release < done,
                "{name}: {assembly}"
            );
            assert_eq!(
                assembly.matches(if name == "linux-x86_64" {
                    "mov QWORD PTR [rbp - 24], r11"
                } else {
                    "stur x10, [x29, #-24]"
                }).count(),
                1,
                "{name}: counted provenance must become the ordinary-cell marker"
            );
        }
    }
}
