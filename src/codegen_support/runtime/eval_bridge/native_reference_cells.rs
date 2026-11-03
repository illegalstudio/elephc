//! Purpose:
//! Allocates retainable native reference slots over live eval PHP references.
//!
//! Called from:
//! - Eval value-wrapper emission and native function reference staging.
//!
//! Key details:
//! - Descriptor seven owns the existing PHPRef without copying its current child.
//! - Staging owns an invisible pin; real native parameter and property bindings own the cell.
//! - The ordinary Mixed invoker marker keeps its established pointer-slot layout.

use super::*;

/// Gives staging one managed cell lease while keeping the caller's PHPRef identity intact.
pub(super) fn emit(emitter: &mut Emitter) {
    emitter.blank();
    label_c_global(emitter, "__elephc_eval_value_native_mixed_reference_cell");
    abi::emit_frame_prologue(emitter, 48);
    let result = abi::int_result_reg(emitter);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 1), 16);
    abi::emit_reg_move(emitter, result, abi::int_arg_reg_name(emitter.target, 0));
    abi::emit_call_label(emitter, "__rt_incref");
    abi::store_at_offset(emitter, result, 8);
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter.target, 0), 7);
    abi::emit_call_label(emitter, "__rt_reference_cell_new");
    let reference = abi::secondary_scratch_reg(emitter);
    abi::load_at_offset(emitter, reference, 8);
    abi::emit_store_to_address(emitter, reference, result, 0);
    abi::store_at_offset(emitter, result, 24);
    abi::emit_reg_move(emitter, abi::int_arg_reg_name(emitter.target, 1), result);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), 16);
    abi::emit_call_label(emitter, "__rt_reference_validation_pin_enter");
    // Transfer the fresh allocation's owner to its pin, which now keeps the cell alive.
    abi::load_at_offset(emitter, result, 24);
    abi::emit_call_label(emitter, "__rt_decref_any");
    abi::load_at_offset(emitter, result, 24);
    abi::emit_frame_restore(emitter, 48);
    abi::emit_return(emitter);
    super::release_boundary::emit_reference_pin_release(emitter);
}

#[cfg(test)]
mod tests {
    use crate::codegen_support::{emit::Emitter, platform::Target};

    /// Every target retains the original PHPRef before publishing a descriptor-seven native cell.
    #[test]
    fn native_mixed_reference_staging_cell_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            super::emit(&mut emitter);
            let assembly = emitter.output();
            let retain = assembly.find("__rt_incref").unwrap();
            let allocate = assembly.find("__rt_reference_cell_new").unwrap();
            assert!(retain < allocate, "{name}: the live PHPRef must remain owned across allocation");
            assert!(!assembly.contains("__rt_mixed_clone"), "{name}: staging must preserve reference identity");
            assert!(assembly.find("__rt_reference_validation_pin_enter").unwrap()
                < assembly.find("__rt_decref_any").unwrap(), "{name}: transfer the allocation owner only after pinning");
            assert!(assembly.contains("__rt_reference_validation_pin_take"), "{name}: staging cleanup retires the pin");
            assert!(assembly.contains(if name == "linux-x86_64" { "mov rdi, 7" } else { "mov x0, #7" }),
                "{name}: native cells must release and trace the PHPRef as Mixed");
        }
    }
}
