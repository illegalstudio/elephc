//! Purpose:
//! Classifies borrowed numeric operands without losing exact integer payloads.
//!
//! Called from:
//! - Checked boxed arithmetic before computation and warning dispatch.
//!
//! Key details:
//! - Returns canonical numeric tag, payload and prefix/invalid status without boxing.
//! - Complete numeric strings and arithmetic prefixes share the existing bounded parser.
//! - The caller snapshots both results before a warning callback can mutate source storage.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use elephc_semantics::{SemanticProfile, StrictFeature};

const TYPE_MESSAGE: &str = "Unsupported operand types for numeric arithmetic";
const TYPE_LABEL: &str = "_strict_numeric_operand_type_message";
pub(in crate::codegen_support::runtime) const TYPE_FAIL: &str = "__rt_strict_numeric_operand_type_fail";
pub(super) const PREFIX_WARNING: &str = "Warning: A non-numeric value encountered\n";
pub(super) const PREFIX_LABEL: &str = "_strict_numeric_prefix_warning";

/// Emits diagnostics from the same byte strings used to materialize runtime lengths.
pub(in crate::codegen_support::runtime) fn data(profile: SemanticProfile) -> String {
    if !profile.contains(StrictFeature::Overflow) { return String::new(); }
    format!("\n.data\n{TYPE_LABEL}:\n.ascii {TYPE_MESSAGE:?}\n{PREFIX_LABEL}:\n.ascii {PREFIX_WARNING:?}\n{}",
        super::strict_numeric_div::data())
}

/// Borrows one cell and returns numeric tag/payload plus status zero, prefix one or invalid two.
pub(super) fn emit(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let (low, high) = match emitter.target.arch { Arch::AArch64 => ("x1", "x2"), Arch::X86_64 => ("rdi", "rdx") };
    let prefix = "__rt_strict_numeric_operand";
    let number = format!("{prefix}_number");
    let boolean = format!("{prefix}_boolean");
    let null = format!("{prefix}_null");
    let string = format!("{prefix}_string");
    let done = format!("{prefix}_done");
    let invalid = format!("{prefix}_invalid");
    emitter.label_global(prefix);
    abi::emit_frame_prologue(emitter, 16);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    for (tag, label) in [(0, &number), (2, &number), (3, &boolean), (8, &null), (1, &string)] {
        ins(emitter, &format!("cmp x0, #{tag}"), &format!("cmp rax, {tag}"));
        abi::emit_branch_if_equal_wide(emitter, label);
    }
    abi::emit_jump(emitter, &invalid);
    emitter.label(&null);
    abi::emit_load_int_immediate(emitter, low, 0);
    emitter.label(&boolean);
    abi::emit_load_int_immediate(emitter, result, 0);
    emitter.label(&number);
    abi::emit_load_int_immediate(emitter, high, 0);
    abi::emit_jump(emitter, &done);
    emitter.label(&string);
    abi::emit_reg_move(emitter, abi::int_arg_reg_name(emitter.target, 0), low);
    abi::emit_reg_move(emitter, abi::int_arg_reg_name(emitter.target, 1), high);
    abi::emit_call_label(emitter, "__rt_str_numeric_value");
    ins(emitter, "cmp x2, #2", "cmp rdx, 2");
    abi::emit_branch_if_not_equal_wide(emitter, &done);
    emitter.label(&invalid);
    abi::emit_load_int_immediate(emitter, result, 0);
    abi::emit_load_int_immediate(emitter, low, 0);
    abi::emit_load_int_immediate(emitter, high, 2);
    emitter.label(&done);
    abi::emit_frame_restore(emitter, 16);
    abi::emit_return(emitter);
    emitter.label_global(TYPE_FAIL);
    match emitter.target.arch {
        Arch::AArch64 => super::value_error::emit_throw_static_message_aarch64(
            emitter, "_spl_type_error_class_id", TYPE_LABEL, TYPE_MESSAGE.len()),
        Arch::X86_64 => {
            emitter.instruction("sub rsp, 8");                                  // adapt a jump-only failure to the shared throw helper's return-slot alignment
            super::value_error::emit_throw_static_message_x86_64(
                emitter, "_spl_type_error_class_id", TYPE_LABEL, TYPE_MESSAGE.len());
        }
    }
}

/// Emits equivalent instructions without duplicating the numeric classification graph.
fn ins(emitter: &mut Emitter, arm: &str, x86: &str) {
    emitter.instruction(if emitter.target.arch == Arch::AArch64 { arm } else { x86 }); // preserve exact numeric kinds on every target
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Snapshot classification returns invalid status as data without dispatching an early failure.
    #[test]
    fn strict_numeric_diagnostics_runtime_classification_defers_kind_failures() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit(&mut emitter);
            let asm = emitter.output();
            let classifier = asm.split(&format!(".globl {TYPE_FAIL}\n")).next().unwrap();
            assert!(!classifier.lines().any(|line| !line.trim_start().starts_with('.')
                && line.contains(TYPE_FAIL)), "{name}");
            assert!(!classifier.contains("__rt_diag_warning"), "{name}");
            assert!(classifier.contains("__rt_strict_numeric_operand_invalid:"), "{name}");
        }
    }
}
