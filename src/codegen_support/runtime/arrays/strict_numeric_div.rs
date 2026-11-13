//! Purpose:
//! Divides borrowed eval numbers beneath the protected native callback boundary.
//!
//! Called from:
//! - Checked numeric runtime emission and the explicit Div eval selector.
//!
//! Key details:
//! - Reuses exact operand snapshots, including numeric-string warning callbacks.
//! - Retains the eval division Float result contract and throws for either signed zero.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use super::strict_mixed_numeric::{box_number, capture_operands, load_float, FRAME};

const ZERO_MESSAGE: &str = "Division by zero";
const ZERO_LABEL: &str = "_strict_numeric_div_zero_message";

/// Supplies the shared static message only in a runtime that emits this checked kernel.
pub(super) fn data() -> String {
    format!("{ZERO_LABEL}:\n.ascii {ZERO_MESSAGE:?}\n")
}

/// Emits an explicit division kernel without changing ordinary PHP implementation hooks.
pub(super) fn emit(emitter: &mut Emitter) {
    let prefix = "__rt_strict_mixed_numeric_div";
    let zero = format!("{prefix}_zero");
    emitter.label_global(prefix);
    abi::emit_frame_prologue(emitter, FRAME);
    capture_operands(emitter, prefix);
    load_float(emitter, 16, 24, prefix);
    abi::emit_store_to_sp(emitter, abi::float_result_reg(emitter), 0);
    load_float(emitter, 32, 40, prefix);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("fcmp d0, #0.0");                               // reject either signed zero while retaining unordered NaN
            abi::emit_branch_if_equal_wide(emitter, &zero);
            emitter.instruction("fmov d1, d0");                                 // preserve the captured divisor before restoring the dividend
            abi::emit_load_temporary_stack_slot(emitter, "d0", 0);
            emitter.instruction("fdiv d0, d0, d1");                             // retain the existing floating eval division result contract
            emitter.instruction("fmov x1, d0");                                 // transfer the exact IEEE result bits to the independent result box
        }
        Arch::X86_64 => {
            emitter.instruction("movq rax, xmm0");                              // inspect divisor bits without treating unordered NaN as zero
            emitter.instruction("shl rax, 1");                                  // discard only the sign bit so both signed zero payloads match
            abi::emit_branch_if_int_result_zero(emitter, &zero);
            emitter.instruction("movapd xmm1, xmm0");                           // preserve the captured divisor before restoring the dividend
            abi::emit_load_temporary_stack_slot(emitter, "xmm0", 0);
            emitter.instruction("divsd xmm0, xmm1");                            // preserve IEEE division for nonzero finite and nonfinite operands
            emitter.instruction("movq rdi, xmm0");                              // transfer the exact IEEE result bits to the independent result box
        }
    }
    box_number(emitter, 2);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);
    emitter.label(&zero);
    // Calling the shared throw emitter creates the standard Throwable with existing unwind roots.
    match emitter.target.arch {
        Arch::AArch64 => super::value_error::emit_throw_static_message_aarch64(
            emitter, "_spl_division_by_zero_error_class_id", ZERO_LABEL, ZERO_MESSAGE.len()),
        Arch::X86_64 => {
            emitter.instruction("sub rsp, 8");                                  // adapt the jump-only failure to the shared throw emitter's call alignment
            super::value_error::emit_throw_static_message_x86_64(
                emitter, "_spl_division_by_zero_error_class_id", ZERO_LABEL, ZERO_MESSAGE.len());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every target captures both operands before callbacks and preserves catchable zero failures.
    #[test]
    fn strict_eval_division_runtime_covers_supported_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit(&mut emitter);
            let asm = emitter.output();
            let inputs = asm.match_indices("__rt_strict_numeric_operand").map(|(index, _)| index).collect::<Vec<_>>();
            assert_eq!(inputs.len(), 2, "{name}");
            assert!(inputs[1] < asm.find("__rt_diag_warning").unwrap(), "{name}");
            assert!(asm.contains("_spl_division_by_zero_error_class_id"), "{name}");
            assert!(asm.contains("__rt_throw_current"), "{name}");
            assert!(!asm.contains("__rt_mixed_cast_"), "{name}");
        }
    }
}
