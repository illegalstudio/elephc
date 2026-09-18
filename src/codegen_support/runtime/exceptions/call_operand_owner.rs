//! Purpose:
//! Retires one short-lived call operand while exception cleanup walks its scope record.
//!
//! Called from:
//! - The activation-chain walker through the record's unary C callback.
//!
//! Key details:
//! - The argument points to an owning slot, not directly to the payload.
//! - Clearing the slot prevents later PHP-frame cleanup from releasing the owner twice.
//! - Destructor exceptions are chained without escaping the cleanup walker.

use crate::codegen_support::{abi, emit::Emitter};

/// Emits the non-escaping unary callback for a temporary owner slot on every native ABI.
pub fn emit_cleanup_call_operand_owner(emitter: &mut Emitter) {
    emit_owner_cleanup_entry(emitter, "__rt_cleanup_call_operand_owner", "__rt_decref_any");
    emit_owner_cleanup_entry(emitter, "__rt_cleanup_call_operand_descriptor", "__rt_callable_descriptor_release");
}

/// Emits one bounded cleanup entry with the release discipline of its owning slot.
fn emit_owner_cleanup_entry(emitter: &mut Emitter, label: &str, release: &str) {
    emitter.blank();
    emitter.label_global(label);
    abi::emit_frame_prologue(emitter, 16);
    let address = abi::secondary_scratch_reg(emitter);
    let result = abi::int_result_reg(emitter);
    abi::emit_reg_move(emitter, address, abi::int_arg_reg_name(emitter.target, 0));
    abi::emit_load_from_address(emitter, result, address, 0);
    abi::emit_store_zero_to_address(emitter, address, 0);
    abi::emit_unary_cleanup_preserving_exception(emitter, release, result);
    abi::emit_frame_restore(emitter, 16);
    abi::emit_return(emitter);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Arch, Target};

    /// Slot retirement precedes bounded release, including when the payload's destructor throws.
    #[test]
    fn call_operand_cleanup_clears_before_release_on_all_targets() {
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(target).unwrap());
            emit_cleanup_call_operand_owner(&mut emitter);
            let clear = if emitter.target.arch == Arch::AArch64 {
                "str xzr, [x10]"
            } else {
                "mov QWORD PTR [r10], 0"
            };
            let asm = emitter.output();
            assert!(asm.find(clear).unwrap() < asm.find("__rt_cleanup_preserve_exception").unwrap(), "{target}");
            assert!(!asm.contains("__rt_throw_current"), "{target}");
        }
    }
}
