//! Purpose:
//! Emits shared catchable numeric failures for explicitly checked runtime operations.
//!
//! Called from:
//! - Runtime helper/data emission and checked scalar/boxed arithmetic entries.
//!
//! Key details:
//! - PHP-promoting entries remain available in the same strict runtime.
//! - Failure publishes ArithmeticError before ordinary unwind records retire operands.

use crate::codegen_support::{emit::Emitter, platform::Arch};
use elephc_semantics::{SemanticProfile, StrictFeature, numeric::INTEGER_OVERFLOW_MESSAGE};

/// Emits the checked overflow message only when its runtime policy is present.
pub(crate) fn data(profile: SemanticProfile) -> String {
    let mut data = super::arrays::strict_numeric_operand::data();
    data.push_str(&super::arrays::array_numeric_policy::data(profile));
    if !profile.contains(StrictFeature::Overflow) { return data; }
    data.push_str(&format!("\n.data\n_strict_integer_overflow_message:\n.ascii \"{INTEGER_OVERFLOW_MESSAGE}\"\n"));
    data.push_str(&format!("\n_strict_eval_null_decrement_warning:\n.ascii {:?}\n",
        elephc_semantics::numeric::NULL_DECREMENT_WARNING));
    data.push_str(&super::arrays::strict_abs::data(profile));
    data.push_str(&super::arrays::strict_numeric_operand::checked_data());
    data
}

/// Emits a nonreturning overflow entry through the existing native exception machinery.
pub(in crate::codegen_support::runtime) fn emit(emitter: &mut Emitter) {
    if !emitter.semantic_profile.contains(StrictFeature::Overflow) { return; }
    emitter.label_global("__rt_strict_integer_overflow_fail");
    match emitter.target.arch {
        Arch::AArch64 => super::arrays::value_error::emit_throw_static_message_aarch64(
            emitter, "_spl_arithmetic_error_class_id", "_strict_integer_overflow_message", INTEGER_OVERFLOW_MESSAGE.len()),
        Arch::X86_64 => {
            // This jump-only entry inherits an aligned helper body, without a call return slot.
            emitter.instruction("sub rsp, 8");                                  // supply the return-slot alignment expected by the throwing helper
            super::arrays::value_error::emit_throw_static_message_x86_64(
                emitter, "_spl_arithmetic_error_class_id", "_strict_integer_overflow_message", INTEGER_OVERFLOW_MESSAGE.len());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// PHP and strict entries coexist with explicit policy storage and catchable failure edges.
    #[test]
    fn strict_boxed_overflow_runtime_entries_cover_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            for checked in [false, true] {
                let mut emitter = Emitter::new(Target::parse(name).unwrap());
                emitter.semantic_profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, checked);
                super::super::arrays::emit_mixed_numeric_binops(&mut emitter);
                super::super::arrays::emit_int_pow_checked(&mut emitter);
                super::super::arrays::emit_mixed_numeric_pow(&mut emitter);
                emit(&mut emitter);
                let asm = emitter.output();
                for helper in ["__rt_mixed_numeric_add", "__rt_mixed_numeric_pow", "__rt_int_pow_checked"] {
                    assert!(asm.contains(&format!("{helper}:")), "{name} {checked} {helper}");
                }
                for helper in ["__rt_strict_mixed_numeric_add", "__rt_strict_mixed_numeric_sub", "__rt_strict_mixed_numeric_mul",
                    "__rt_strict_mixed_numeric_pow", "__rt_int_pow_or_throw", "__rt_strict_integer_overflow_fail"] {
                    assert_eq!(asm.contains(&format!("{helper}:")), checked, "{name} {checked} {helper}");
                }
                if checked {
                    assert!(asm.contains("__rt_throw_current"), "{name}");
                    assert!(asm.contains("_spl_arithmetic_error_class_id"), "{name}");
                    assert!(asm.matches("__rt_strict_integer_overflow_fail").count() >= 4, "{name}");
                }
            }
        }
    }
}
