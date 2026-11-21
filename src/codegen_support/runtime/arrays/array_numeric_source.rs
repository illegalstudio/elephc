//! Purpose:
//! Shares array reduction source diagnostics between native calls and protected eval callbacks.
//!
//! Called from:
//! - Aggregate codegen, numeric runtime data and checked eval boundary emission.
//!
//! Key details:
//! - Kernel zero means an invalid borrowed source, never a successful result owner.
//! - Failure entries use the ordinary call ABI beneath the caller's exception boundary.

use crate::codegen_support::{emit::Emitter, platform::Arch};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Returns the shared PHP array parameter failure without depending on operand storage.
pub fn numeric_aggregate_source_message(product: bool) -> &'static str {
    if product { "array_product(): Argument #1 ($array) must be of type array" }
    else { "array_sum(): Argument #1 ($array) must be of type array" }
}

/// Returns one stable internal failure symbol for a typed sum or product callback.
pub(in crate::codegen_support::runtime) fn symbol(product: bool) -> &'static str {
    if product { "__rt_array_product_source_type_error" } else { "__rt_array_sum_source_type_error" }
}

/// Emits source failure data when the checked aggregate callback capability is linked.
pub(in crate::codegen_support::runtime) fn data(profile: SemanticProfile) -> String {
    if !profile.contains(StrictFeature::Overflow) { return String::new(); }
    [false, true].into_iter().map(|product| format!("\n.data\n{}_message:\n.ascii {:?}\n",
        symbol(product), numeric_aggregate_source_message(product))).collect()
}

/// Emits normal call-entry TypeErrors so protected callbacks contain every invalid source failure.
pub(super) fn emit(emitter: &mut Emitter) {
    if !emitter.semantic_profile.contains(StrictFeature::Overflow) { return; }
    for product in [false, true] {
        let message = numeric_aggregate_source_message(product);
        let label = format!("{}_message", symbol(product));
        emitter.label_global(symbol(product));
        match emitter.target.arch {
            Arch::AArch64 => super::value_error::emit_throw_static_message_aarch64(
                emitter, "_spl_type_error_class_id", &label, message.len()),
            Arch::X86_64 => super::value_error::emit_throw_static_message_x86_64(
                emitter, "_spl_type_error_class_id", &label, message.len()),
        }
    }
}
