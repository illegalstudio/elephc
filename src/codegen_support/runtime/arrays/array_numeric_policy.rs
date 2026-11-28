//! Purpose:
//! Preserves explicit numeric domain and overflow policies in array reduction runtime helpers.
//!
//! Called from:
//! - Numeric aggregate helper emission and shared numeric data emission.
//!
//! Key details:
//! - The third internal ABI argument carries numeric semantic bits, independent of the module default.
//! - Error entries are called with the ordinary target ABI and never return.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use elephc_semantics::{numeric_domains::{NumericDomain, NumericDomainRule}, SemanticProfile, StrictFeature};

pub(super) const POLICY: usize = 128;
const DOMAINS: [(StrictFeature, &str); 2] = [(StrictFeature::Arithmetic, "arithmetic"), (StrictFeature::Coercions, "coercions")];

/// Materializes neutral numeric diagnostic text only for linked domain capabilities.
pub(in crate::codegen_support::runtime) fn data(profile: SemanticProfile) -> String {
    let mut data = String::new();
    for (feature, suffix) in DOMAINS {
        if profile.contains(feature) {
            data.push_str(&format!("\n.data\n_strict_aggregate_{suffix}_message:\n.ascii {:?}\n",
                NumericDomainRule { feature, domain: NumericDomain::Number }.message()));
        }
    }
    data
}

/// Saves the explicit caller policy before unboxing can overwrite C argument registers.
pub(super) fn save_argument(emitter: &mut Emitter) {
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 2), POLICY);
}

/// Accepts float and native numeric adapters after the integer tag was already handled.
pub(super) fn dispatch(emitter: &mut Emitter, prefix: &str) {
    for (tag, suffix) in [(2, "combine"), (6, "object")] {
        ins(emitter, &format!("cmp x0, #{tag}"), &format!("cmp rax, {tag}"));
        abi::emit_branch_if_equal_wide(emitter, &format!("{prefix}_{suffix}"));
    }
    abi::emit_jump(emitter, &format!("{prefix}_domain_error"));
}

/// Selects a policy bit without treating the emitter's linked capability as an operation request.
pub(super) fn branch_if_feature(emitter: &mut Emitter, feature: StrictFeature, label: &str) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::symbol_scratch_reg(emitter);
    abi::load_at_offset(emitter, result, POLICY);
    abi::emit_load_int_immediate(emitter, scratch, SemanticProfile::PERMISSIVE.with(feature, true).bits() as i64);
    ins(emitter, &format!("and x0, x0, {scratch}"), &format!("and rax, {scratch}"));
    abi::emit_branch_if_int_result_nonzero(emitter, label);
}

/// Chooses the same overlapping-domain diagnostic as the compiler and eval guard.
pub(super) fn emit_failure(emitter: &mut Emitter, prefix: &str) {
    emitter.label(&format!("{prefix}_domain_error"));
    let arithmetic = emitter.semantic_profile.contains(StrictFeature::Arithmetic);
    let coercions = emitter.semantic_profile.contains(StrictFeature::Coercions);
    let arithmetic_label = format!("{prefix}_arithmetic_error");
    if arithmetic && coercions { branch_if_feature(emitter, StrictFeature::Arithmetic, &arithmetic_label); }
    if coercions { abi::emit_call_label(emitter, "__rt_aggregate_coercions_type_error"); }
    if arithmetic {
        emitter.label(&arithmetic_label);
        abi::emit_call_label(emitter, "__rt_aggregate_arithmetic_type_error");
    }
}

/// Emits call-entry TypeError helpers beneath the aggregate's existing owner cleanup boundary.
pub(super) fn emit_error_entries(emitter: &mut Emitter) {
    for (feature, suffix) in DOMAINS {
        if !emitter.semantic_profile.contains(feature) { continue; }
        let message = NumericDomainRule { feature, domain: NumericDomain::Number }.message();
        emitter.label_global(&format!("__rt_aggregate_{suffix}_type_error"));
        let label = format!("_strict_aggregate_{suffix}_message");
        match emitter.target.arch {
            Arch::AArch64 => super::value_error::emit_throw_static_message_aarch64(
                emitter, "_spl_type_error_class_id", &label, message.len()),
            Arch::X86_64 => super::value_error::emit_throw_static_message_x86_64(
                emitter, "_spl_type_error_class_id", &label, message.len()),
        }
    }
}

/// Spells one shared reduction-policy operation for both supported architectures.
fn ins(emitter: &mut Emitter, arm: &str, x86: &str) {
    emitter.instruction(if emitter.target.arch == Arch::AArch64 { arm } else { x86 }); // retain explicit reduction policy across native targets
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Linked numeric capabilities never silently enable overflow for a profiled aggregate call.
    #[test]
    fn strict_numeric_aggregate_profiled_runtime_capabilities_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            for overflow in [false, true] {
                let mut emitter = Emitter::new(Target::parse(name).unwrap());
                emitter.semantic_profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Arithmetic, true)
                    .with(StrictFeature::Coercions, true).with(StrictFeature::Overflow, overflow);
                let messages = data(emitter.semantic_profile);
                super::super::array_numeric_aggregate::emit_array_numeric_aggregate(&mut emitter);
                let asm = emitter.output();
                for name in ["sum", "product"] {
                    assert!(asm.contains(&format!("__rt_profiled_array_{name}_boxed_domain_error")));
                    assert_eq!(asm.contains(&format!("__rt_profiled_array_{name}_boxed_checked_combine")), overflow);
                }
                for (_, suffix) in DOMAINS {
                    assert!(asm.contains(&format!("__rt_aggregate_{suffix}_type_error")));
                    assert!(messages.contains(&format!("_strict_aggregate_{suffix}_message")));
                }
                assert!(asm.contains("__rt_mixed_numeric_add") && asm.contains("__rt_mixed_numeric_mul"));
                assert!(asm.contains("__rt_exception_chain") && asm.contains("__rt_throw_current"));
            }
        }
    }
}
