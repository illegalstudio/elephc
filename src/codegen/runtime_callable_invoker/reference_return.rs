//! Purpose:
//! Converts a native reference-returning callable's transferred cell into the invoker's owned value.
//!
//! Called from:
//! - The native descriptor invoker after restoring the caller's concat storage.
//!
//! Key details:
//! - The raw return always uses the integer result register, including string and float references.
//! - Boxing acquires the value before the returned cell lease is retired.

use super::{abi, Emitter, PhpType};

/// Copies the referenced value into an owned Mixed return and consumes the callee's cell lease.
pub(super) fn emit_boxed_reference_return(emitter: &mut Emitter, php_type: &PhpType) {
    let result = abi::int_result_reg(emitter);
    let cell = abi::secondary_scratch_reg(emitter);
    let repr = php_type.codegen_repr();
    abi::emit_push_reg(emitter, result);
    abi::emit_reg_move(emitter, cell, result);
    super::load_array_element_to_result(emitter, &repr, cell, 0);
    if matches!(repr, PhpType::Mixed | PhpType::Union(_)) {
        abi::emit_call_label(emitter, "__rt_mixed_clone");
    } else {
        super::emit_box_current_value_as_mixed(emitter, &repr);
    }
    abi::emit_push_reg(emitter, result);
    abi::emit_load_temporary_stack_slot(emitter, result, 16);
    abi::emit_call_label(emitter, "__rt_reference_cell_owner");
    abi::emit_call_label(emitter, "__rt_reference_cell_release");
    abi::emit_pop_reg(emitter, result);
    abi::emit_release_temporary_stack(emitter, 16);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Descriptor return boxing precedes cell retirement for all supported target ABIs and payload shapes.
    #[test]
    fn descriptor_reference_returns_copy_before_retiring_the_cell() {
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            for php_type in [PhpType::Str, PhpType::Float, PhpType::Mixed, PhpType::Int] {
                let mut emitter = Emitter::new(Target::parse(target).unwrap());
                emit_boxed_reference_return(&mut emitter, &php_type);
                let assembly = emitter.output();
                let box_entry = if php_type == PhpType::Mixed { "__rt_mixed_clone" } else { "__rt_mixed_from_value" };
                assert!(assembly.find(box_entry).unwrap() < assembly.find("__rt_reference_cell_release").unwrap(),
                    "{target}: {php_type:?}");
            }
        }
    }
}
