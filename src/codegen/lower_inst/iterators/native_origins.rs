//! Purpose:
//! Reloads a reference foreach origin from its current canonical child or raw native cell.
//!
//! Called from:
//! - Iterator relocation checks after a by-reference loop body.
//!
//! Key details:
//! - Canonical PHP references lend their actual Mixed child without cloning it.
//! - Headerless raw cells keep their existing payload ABI and acquire no owner.

use crate::codegen_support::{abi, emit::Emitter};

/// Borrows the live origin value while preserving either representation's stack balance.
pub(super) fn load(emitter: &mut Emitter, destination: &str, label: &str) {
    let raw = format!("{label}_raw");
    let done = format!("{label}_done");
    let result = abi::int_result_reg(emitter);
    abi::emit_push_reg(emitter, destination);
    abi::emit_reg_move(emitter, result, destination);
    abi::emit_call_label(emitter, "__rt_reference_cell_canonical");
    abi::emit_branch_if_int_result_zero(emitter, &raw);
    abi::emit_call_label(emitter, "__rt_mixed_deref");
    abi::emit_reg_move(emitter, destination, result);
    abi::emit_release_temporary_stack(emitter, 16);
    abi::emit_jump(emitter, &done);
    emitter.label(&raw);
    abi::emit_pop_reg(emitter, destination);
    abi::emit_load_from_address(emitter, destination, destination, 0);
    emitter.label(&done);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Arch, Target};

    /// Origin reloads inspect canonical storage without manufacturing an unowned Mixed box.
    #[test]
    fn reference_origins_borrow_canonical_children_and_preserve_raw_cells_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            let destination = if target.arch == Arch::AArch64 { "x9" } else { "r11" };
            load(&mut emitter, destination, "origin_probe");
            let assembly = emitter.output();
            let canonical = assembly.find("__rt_reference_cell_canonical").unwrap();
            let dereference = assembly.find("__rt_mixed_deref").unwrap();
            let fallback = assembly.find("origin_probe_raw:").unwrap();
            assert!(canonical < dereference && dereference < fallback, "{name}: {assembly}");
            assert!(assembly.contains(if target.arch == Arch::AArch64 {
                "ldr x9, [x9]"
            } else { "mov r11, QWORD PTR [r11]" }), "{name}: {assembly}");
            let restore = if target.arch == Arch::AArch64 { "add sp, sp, #16" } else { "add rsp, 16" };
            assert!(assembly[..fallback].contains(restore), "{name}: canonical path restores the stack");
            let restore_raw = if target.arch == Arch::AArch64 { "ldr x9, [sp], #16" } else { restore };
            assert!(assembly[fallback..].contains(restore_raw), "{name}: raw path restores its saved pointer");
            for forbidden in ["__rt_mixed_clone", "__rt_mixed_from_value", "__rt_incref", "__rt_decref"] {
                assert!(!assembly.contains(forbidden), "{name}: origin reload cannot own a snapshot: {assembly}");
            }
        }
    }
}
