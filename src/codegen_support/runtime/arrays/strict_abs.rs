//! Purpose:
//! Computes checked numeric absolute values while preserving integer and float kinds.
//!
//! Called from:
//! - The ordinary abs runtime emitter when the linked profile supports checked overflow.
//! - Strict numeric data emission for the stable builtin TypeError message.
//!
//! Key details:
//! - Borrows boxed or stack-cell inputs and returns one independently owned numeric cell.
//! - Numeric strings use the exact integer classifier before any floating-point conversion.
//! - Failure jumps enter the shared native exception machinery from an aligned helper body.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use elephc_semantics::{SemanticProfile, StrictFeature};

const FRAME: usize = 32;
const TYPE_MESSAGE: &str = "abs(): Argument #1 ($num) must be of type int|float";
const TYPE_LABEL: &str = "_strict_abs_numeric_type_error_message";
const TYPE_FAIL: &str = "__rt_strict_abs_numeric_type_error_fail";

/// Emits the checked helper's immutable diagnostic only for linked overflow profiles.
pub(in crate::codegen_support::runtime) fn data(profile: SemanticProfile) -> String {
    if !profile.contains(StrictFeature::Overflow) { return String::new(); }
    format!("\n.data\n{TYPE_LABEL}:\n.ascii \"{TYPE_MESSAGE}\"\n")
}

/// Emits a storage-neutral absolute value using genuine numeric runtime tags.
pub(super) fn emit(emitter: &mut Emitter) {
    if !emitter.semantic_profile.contains(StrictFeature::Overflow) { return; }
    let prefix = "__rt_strict_abs_mixed";
    let result = abi::int_result_reg(emitter);
    let (low, high) = match emitter.target.arch { Arch::AArch64 => ("x1", "x2"), Arch::X86_64 => ("rdi", "rdx") };
    let integer = format!("{prefix}_integer");
    let floating = format!("{prefix}_floating");
    let string = format!("{prefix}_string");
    let coerce = format!("{prefix}_coerce");
    let boxed = format!("{prefix}_box");
    emitter.label_global(prefix);
    abi::emit_frame_prologue(emitter, FRAME);
    abi::emit_store_to_sp(emitter, result, 0);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    branch_tag(emitter, 0, &integer);
    branch_tag(emitter, 2, &floating);
    branch_tag(emitter, 1, &string);
    branch_tag(emitter, 3, &coerce);
    branch_tag(emitter, 8, &coerce);
    abi::emit_jump(emitter, TYPE_FAIL);

    emitter.label(&string);
    abi::emit_reg_move(emitter, abi::int_arg_reg_name(emitter.target, 0), low);
    abi::emit_reg_move(emitter, abi::int_arg_reg_name(emitter.target, 1), high);
    abi::emit_call_label(emitter, "__rt_str_numeric_value");
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("cmp x2, #0"),                     // abs accepts only fully numeric string arguments
        Arch::X86_64 => emitter.instruction("cmp rdx, 0"),                      // abs accepts only fully numeric string arguments
    }
    abi::emit_branch_if_not_equal_wide(emitter, TYPE_FAIL);
    branch_tag(emitter, 2, &floating);
    abi::emit_jump(emitter, &integer);

    emitter.label(&coerce);
    abi::emit_load_temporary_stack_slot(emitter, result, 0);
    abi::emit_call_label(emitter, "__rt_mixed_cast_int");
    abi::emit_reg_move(emitter, low, result);
    emitter.label(&integer);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x1, #0");                                  // select the sign of the exact integer payload
            emitter.instruction("cneg x1, x1, lt");                             // negate only negative integers without converting to float
        }
        Arch::X86_64 => {
            emitter.instruction("mov r10, rdi");                                // retain the exact integer before branchless absolute value
            emitter.instruction("sar r10, 63");                                 // create the two's-complement sign mask
            emitter.instruction("xor rdi, r10");                                // invert negative payload bits before adding one
            emitter.instruction("sub rdi, r10");                                // complete the exact absolute-value magnitude
        }
    }
    let scratch = abi::secondary_scratch_reg(emitter);
    abi::emit_load_int_immediate(emitter, scratch, i64::MIN);
    emitter.instruction(&format!("cmp {low}, {scratch}"));                      // INT_MIN is the only unrepresentable absolute-value magnitude
    abi::emit_branch_if_equal_wide(emitter, "__rt_strict_integer_overflow_fail");
    abi::emit_load_int_immediate(emitter, result, 0);
    abi::emit_jump(emitter, &boxed);
    emitter.label(&floating);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("fmov d0, x1");                                 // preserve the IEEE-754 payload before clearing its sign
            emitter.instruction("fabs d0, d0");                                 // floating-point absolute value preserves the float kind
            emitter.instruction("fmov x1, d0");                                 // prepare the float bits for independent Mixed boxing
        }
        Arch::X86_64 => {
            emitter.instruction("mov r10, 0x7fffffffffffffff");                 // retain every IEEE-754 bit except the sign
            emitter.instruction("and rdi, r10");                                // floating-point absolute value preserves the float kind
        }
    }
    abi::emit_load_int_immediate(emitter, result, 2);
    emitter.label(&boxed);
    abi::emit_load_int_immediate(emitter, if emitter.target.arch == Arch::AArch64 { "x2" } else { "rsi" }, 0);
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);
    emit_type_error(emitter);
}

/// Compares a runtime tag without interpreting another kind as an integer payload.
fn branch_tag(emitter: &mut Emitter, tag: i64, label: &str) {
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("cmp x0, #{tag}")),       // inspect the canonical unboxed PHP value kind
        Arch::X86_64 => emitter.instruction(&format!("cmp rax, {tag}")),        // inspect the canonical unboxed PHP value kind
    }
    abi::emit_branch_if_equal_wide(emitter, label);
}

/// Raises a PHP TypeError from the same aligned body convention as checked numeric overflow.
fn emit_type_error(emitter: &mut Emitter) {
    emitter.label_global(TYPE_FAIL);
    match emitter.target.arch {
        Arch::AArch64 => super::value_error::emit_throw_static_message_aarch64(
            emitter, "_spl_type_error_class_id", TYPE_LABEL, TYPE_MESSAGE.len()),
        Arch::X86_64 => {
            emitter.instruction("sub rsp, 8");                                  // adapt the jump-only entry to the throwing helper's return-slot alignment
            super::value_error::emit_throw_static_message_x86_64(
                emitter, "_spl_type_error_class_id", TYPE_LABEL, TYPE_MESSAGE.len());
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Both runtime policies coexist and preserve numeric-string classification on every target.
    #[test]
    fn strict_abs_runtime_entries_keep_php_and_checked_paths_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            for checked in [false, true] {
                let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, checked);
                let mut emitter = Emitter::new(Target::parse(name).unwrap());
                emitter.semantic_profile = profile;
                super::super::mixed_abs::emit_mixed_abs(&mut emitter);
                let asm = emitter.output();
                assert!(asm.contains("__rt_abs_mixed:"), "{name}");
                assert_eq!(asm.contains("__rt_strict_abs_mixed:"), checked, "{name}");
                assert_eq!(data(profile).contains(TYPE_LABEL), checked, "{name}");
                if checked {
                    for helper in ["__rt_str_numeric_value", "__rt_strict_integer_overflow_fail", "_spl_type_error_class_id"] {
                        assert!(asm.contains(helper), "{name} {helper}");
                    }
                }
            }
        }
    }
}
