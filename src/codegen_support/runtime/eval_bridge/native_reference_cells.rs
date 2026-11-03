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
    label_c_global(emitter, "__elephc_eval_value_native_reference_owner_pin_v1");
    let first = abi::int_arg_reg_name(emitter.target, 0);
    let second = abi::int_arg_reg_name(emitter.target, 1);
    let scratch = abi::secondary_scratch_reg(emitter);
    abi::emit_reg_move(emitter, scratch, first);
    abi::emit_reg_move(emitter, first, second);
    abi::emit_reg_move(emitter, second, scratch);
    abi::emit_jump(emitter, "__rt_reference_validation_pin_enter");
    label_c_global(emitter, "__elephc_eval_value_native_reference_owner_take_v1");
    abi::emit_jump(emitter, "__rt_reference_validation_pin_take");
    super::release_boundary::emit_reference_pin_release(emitter);
}

#[cfg(test)]
mod tests {
    use crate::codegen_support::{emit::Emitter, platform::Target};

    /// Outer argument leases use the same authenticated root ABI without another managed cell.
    #[test]
    fn native_argument_owner_roots_forward_the_exact_value_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            super::emit(&mut emitter);
            let assembly = emitter.output();
            let entry = target.extern_symbol("__elephc_eval_value_native_reference_owner_pin_v1");
            let body = assembly.split_once(&format!("{entry}:\n")).unwrap().1
                .split_once("__rt_reference_validation_pin_enter").unwrap().0;
            let swap = if name == "linux-x86_64" {
                ["mov r10, rdi", "mov rdi, rsi", "mov rsi, r10"]
            } else { ["mov x10, x0", "mov x0, x1", "mov x1, x10"] };
            let positions = swap.map(|instruction| body.find(instruction).expect("exact ABI argument forwarding"));
            assert!(positions[0] < positions[1] && positions[1] < positions[2], "{name}: preserve the value while swapping root arguments");
            assert!(!body.contains("__rt_reference_cell_new") && !body.contains("__rt_mixed_clone"),
                "{name}: the source owner stays attached to its original PHP reference");
        }
    }

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
