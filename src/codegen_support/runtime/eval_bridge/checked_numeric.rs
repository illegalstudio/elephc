//! Purpose:
//! Contains explicit checked numeric callbacks beneath the Magician Rust stack.
//!
//! Called from:
//! - The shared eval bridge emitter on every supported target.
//!
//! Key details:
//! - Version one borrows two cells and transfers one owner through the fourth C argument.
//! - Unsupported selectors and runtimes return status one without touching the output.

use super::{abi, Arch, Emitter};
use crate::codegen_support::runtime::exceptions::emit_protected_status;
use crate::codegen_support::try_handlers::TRY_HANDLER_SLOT_SIZE;
use elephc_semantics::{numeric::CheckedNumericOp, StrictFeature};

/// Emits an always-available symbol whose capabilities match the immutable linked runtime.
pub(super) fn emit(emitter: &mut Emitter) {
    let symbol = emitter.target.extern_symbol("__elephc_eval_checked_numeric_v1");
    emit_protected_status(emitter, &symbol, body);
}

/// Dispatches only reviewed checked entries and publishes a result after normal completion.
fn body(emitter: &mut Emitter) {
    let prefix = "__rt_eval_checked_numeric_v1";
    let result = abi::int_result_reg(emitter);
    let scratch = abi::symbol_scratch_reg(emitter);
    let right = if emitter.target.arch == Arch::AArch64 { "x1" } else { "rdi" };
    let done = format!("{prefix}_done");
    let success = format!("{prefix}_success");
    let supported = emitter.semantic_profile.contains(StrictFeature::Overflow);
    let operations = [
        (CheckedNumericOp::Add, "__rt_strict_mixed_numeric_add"),
        (CheckedNumericOp::Sub, "__rt_strict_mixed_numeric_sub"),
        (CheckedNumericOp::Mul, "__rt_strict_mixed_numeric_mul"),
        (CheckedNumericOp::Pow, "__rt_strict_mixed_numeric_pow"),
        (CheckedNumericOp::Abs, "__rt_strict_abs_mixed"),
        (CheckedNumericOp::ArraySum, "__rt_strict_array_sum_boxed"),
        (CheckedNumericOp::ArrayProduct, "__rt_strict_array_product_boxed"),
    ];
    if supported {
        abi::emit_load_temporary_stack_slot(emitter, result, TRY_HANDLER_SLOT_SIZE);
        for (op, _) in operations {
            match emitter.target.arch {
                Arch::AArch64 => emitter.instruction(&format!("cmp {result}, #{}", op as u64)), // match the explicit versioned selector
                Arch::X86_64 => emitter.instruction(&format!("cmp {result}, {}", op as u64)), // match the explicit versioned selector
            }
            abi::emit_branch_if_equal_wide(emitter, &format!("{prefix}_{}", op as u64));
        }
    }
    abi::emit_load_int_immediate(emitter, result, 1);
    abi::emit_jump(emitter, &done);
    if supported {
        for (op, target) in operations {
            emitter.label(&format!("{prefix}_{}", op as u64));
            abi::emit_load_temporary_stack_slot(emitter, result, TRY_HANDLER_SLOT_SIZE + 8);
            abi::emit_load_temporary_stack_slot(emitter, right, TRY_HANDLER_SLOT_SIZE + 16);
            abi::emit_call_label(emitter, target);
            abi::emit_jump(emitter, &success);
        }
        emitter.label(&success);
        abi::emit_load_temporary_stack_slot(emitter, scratch, TRY_HANDLER_SLOT_SIZE + 24);
        abi::emit_store_to_address(emitter, result, scratch, 0);
        abi::emit_load_int_immediate(emitter, result, 0);
    }
    emitter.label(&done);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;
    use elephc_semantics::SemanticProfile;

    /// Every target returns native exceptions, retains PHP helpers and rejects unsupported runtimes.
    #[test]
    fn strict_eval_numeric_boundary_contains_all_checked_kernels() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            for checked in [false, true] {
                let mut emitter = Emitter::new(Target::parse(name).unwrap());
                emitter.semantic_profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, checked);
                emit(&mut emitter);
                let symbol = emitter.target.extern_symbol("__elephc_eval_checked_numeric_v1");
                let asm = emitter.output();
                assert!(asm.contains(&symbol), "{name}");
                assert!(asm.contains("_exc_handler_top") && asm.contains("_exc_call_frame_top"), "{name}");
                assert!(!asm.contains("__rt_throw_current"), "{name}: native throws must return to Rust");
                for entry in ["add", "sub", "mul", "pow"] {
                    assert_eq!(asm.contains(&format!("__rt_strict_mixed_numeric_{entry}")), checked, "{name}");
                }
                assert_eq!(asm.contains("__rt_strict_abs_mixed"), checked, "{name}");
                for op in ["sum", "product"] {
                    assert_eq!(asm.contains(&format!("__rt_strict_array_{op}_boxed")), checked, "{name}");
                }
            }
        }
    }
}
