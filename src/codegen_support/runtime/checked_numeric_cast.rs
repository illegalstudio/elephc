//! Purpose:
//! Emits checked explicit scalar casts with catchable PHP type and value failures.
//!
//! Called from:
//! - Typed EIR cast calls and shared managed runtime emission.
//!
//! Key details:
//! - Source cells stay borrowed, and string parsing retires private scratch before failure.
//! - Integer decimal conversion uses exact digits rather than rounded double payloads.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use elephc_semantics::{numeric_cast::{NumericCastFailure, NumericCastTarget}, SemanticProfile, StrictFeature};

mod decimal_int;
#[cfg(test)]
mod tests;

const RANGE: &str = "__rt_checked_cast_range_fail";
const STRING: &str = "__rt_checked_cast_string_fail";
const TYPE: &str = "__rt_checked_cast_type_fail";

/// Selects an explicit checked conversion without inferring behavior from global runtime state.
pub(crate) fn symbol(target: NumericCastTarget) -> &'static str {
    match target { NumericCastTarget::Int => "__rt_checked_cast_int", NumericCastTarget::Float => "__rt_checked_cast_float" }
}

/// Supplies shared failure text only to runtimes carrying the checked-cast capability.
pub(in crate::codegen_support::runtime) fn data(profile: SemanticProfile) -> String {
    if !profile.contains(StrictFeature::Casts) { return String::new(); }
    [NumericCastFailure::Range, NumericCastFailure::InvalidString, NumericCastFailure::UnsupportedKind]
        .into_iter().map(|failure| format!("\n.data\n{}:\n.ascii {:?}\n", message_label(failure), failure.message())).collect()
}

/// Emits both scalar result kinds and one exact decimal parser on every supported target.
pub(in crate::codegen_support::runtime) fn emit(emitter: &mut Emitter) {
    if !emitter.semantic_profile.contains(StrictFeature::Casts) { return; }
    entry(emitter, NumericCastTarget::Int);
    entry(emitter, NumericCastTarget::Float);
    decimal_int::emit(emitter);
    for (label, failure) in [(RANGE, NumericCastFailure::Range), (STRING, NumericCastFailure::InvalidString),
        (TYPE, NumericCastFailure::UnsupportedKind)] {
        emitter.label_global(label);
        let class = if failure == NumericCastFailure::UnsupportedKind { "_spl_type_error_class_id" }
            else { "_spl_value_error_class_id" };
        match emitter.target.arch {
            Arch::AArch64 => super::arrays::value_error::emit_throw_static_message_aarch64(
                emitter, class, message_label(failure), failure.message().len()),
            Arch::X86_64 => {
                emitter.instruction("sub rsp, 8");                              // adapt an aligned helper body to the shared throw entry's call layout
                super::arrays::value_error::emit_throw_static_message_x86_64(
                    emitter, class, message_label(failure), failure.message().len());
            }
        }
    }
}

/// Keeps static message identity paired with the same neutral failure as its byte length.
fn message_label(failure: NumericCastFailure) -> &'static str {
    match failure {
        NumericCastFailure::Range => "_checked_cast_range_message",
        NumericCastFailure::InvalidString => "_checked_cast_string_message",
        NumericCastFailure::UnsupportedKind => "_checked_cast_type_message",
    }
}

/// Inspects original scalar tags before applying the selected checked result contract.
fn entry(emitter: &mut Emitter, target: NumericCastTarget) {
    let prefix = symbol(target);
    let integer = format!("{prefix}_integer"); let floating = format!("{prefix}_floating");
    let string = format!("{prefix}_string"); let null = format!("{prefix}_null");
    let done = format!("{prefix}_done");
    let result = abi::int_result_reg(emitter);
    let (low, high) = if emitter.target.arch == Arch::AArch64 { ("x1", "x2") } else { ("rdi", "rdx") };
    emitter.label_global(prefix);
    abi::emit_frame_prologue(emitter, 32);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    for (tag, label) in [(0, &integer), (3, &integer), (2, &floating), (1, &string), (8, &null)] {
        ins(emitter, &format!("cmp x0, #{tag}"), &format!("cmp rax, {tag}"));
        abi::emit_branch_if_equal_wide(emitter, label);
    }
    abi::emit_jump(emitter, TYPE);
    emitter.label(&null);
    abi::emit_load_int_immediate(emitter, low, 0);
    emitter.label(&integer);
    abi::emit_reg_move(emitter, result, low);
    if target == NumericCastTarget::Float { abi::emit_int_result_to_float_result(emitter); }
    abi::emit_jump(emitter, &done);
    emitter.label(&floating);
    ins(emitter, "fmov d0, x1", "movq xmm0, rdi");
    if target == NumericCastTarget::Int { checked_float(emitter); }
    abi::emit_jump(emitter, &done);
    emitter.label(&string);
    abi::emit_store_to_sp(emitter, low, 0); abi::emit_store_to_sp(emitter, high, 8);
    abi::emit_reg_move(emitter, abi::int_arg_reg_name(emitter.target, 0), low);
    abi::emit_reg_move(emitter, abi::int_arg_reg_name(emitter.target, 1), high);
    abi::emit_call_label(emitter, "__rt_str_numeric_value");
    ins(emitter, "cmp x2, #0", "cmp rdx, 0");
    abi::emit_branch_if_not_equal_wide(emitter, STRING);
    if target == NumericCastTarget::Int {
        abi::emit_load_temporary_stack_slot(emitter, abi::int_arg_reg_name(emitter.target, 0), 0);
        abi::emit_load_temporary_stack_slot(emitter, abi::int_arg_reg_name(emitter.target, 1), 8);
        abi::emit_call_label(emitter, "__rt_checked_decimal_to_int");
    } else {
        ins(emitter, "cmp x0, #0", "cmp rax, 0");
        abi::emit_branch_if_equal_wide(emitter, &integer);
        abi::emit_jump(emitter, &floating);
    }
    emitter.label(&done);
    abi::emit_frame_restore(emitter, 32);
    abi::emit_return(emitter);
}

/// Rejects unordered and out-of-range doubles before the common PHP truncation helper runs.
fn checked_float(emitter: &mut Emitter) {
    let scratch = abi::secondary_scratch_reg(emitter);
    for (bound, inclusive) in [(-9_223_372_036_854_775_808.0f64, true), (9_223_372_036_854_775_808.0, false)] {
        abi::emit_load_int_immediate(emitter, scratch, bound.to_bits() as i64);
        ins(emitter, "fmov d1, x10", "movq xmm1, r10");
        ins(emitter, "fcmp d0, d1", "ucomisd xmm0, xmm1");
        if inclusive {
            range_if(emitter, "vc", "jp");
            range_if(emitter, "ge", "jb");
        } else { range_if(emitter, "lt", "jae"); }
    }
    abi::emit_float_result_to_int_result(emitter);
}

/// Keeps Apple conditional edges inside their symbol atom before a wide failure transfer.
fn range_if(emitter: &mut Emitter, inverse_arm: &str, x86: &str) {
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction(&format!("b.{inverse_arm} 1f"));                    // keep the inverse condition inside the current Apple symbol atom
        abi::emit_jump(emitter, RANGE);
        emitter.label("1");
    } else { emitter.instruction(&format!("{x86} {RANGE}")); }                  // take the checked range failure directly on x86_64
}

/// Emits equivalent steps while keeping the scalar conversion graph shared between architectures.
fn ins(emitter: &mut Emitter, arm: &str, x86: &str) {
    emitter.instruction(if emitter.target.arch == Arch::AArch64 { arm } else { x86 }); // execute the same checked conversion step under the target's register convention
}
