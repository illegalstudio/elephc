//! Purpose:
//! Adapts native physical matching, PHP identity and protected retirement to the validation ABI.
//!
//! Called from:
//! - Native reference publishers and executable validation ABI coverage.
//!
//! Key details:
//! - Shape probes authenticate managed canonical cells without PHP callbacks or ownership changes.
//! - Identity writes exactly one C int, preserving adjacent caller-owned bytes.
//! - Candidate retirement contains Throwable exits in the validation context's pending slot.

use super::*;

/// Emits the three native callbacks paired with the protected declaration coercer.
pub(super) fn emit(emitter: &mut Emitter) {
    emit_is_current(emitter);
    emit_identical(emitter);
    emit_release(emitter);
}

/// Authenticates a copied physical binding and compares its current canonical wrapper identity.
fn emit_is_current(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let other = abi::secondary_scratch_reg(emitter);
    let miss = "__rt_reference_validation_is_current_miss";
    let done = "__rt_reference_validation_is_current_done";
    emitter.label_global("__rt_reference_validation_is_current");
    abi::emit_frame_prologue(emitter, 48);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 1), 8);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 2), 16);
    abi::load_at_offset(emitter, result, 8);
    abi::emit_branch_if_int_result_zero(emitter, miss);
    abi::emit_load_from_address(emitter, result, result, 8);
    abi::emit_branch_if_int_result_zero(emitter, miss);
    abi::emit_load_from_address(emitter, result, result, 0);
    abi::load_at_offset(emitter, other, 8);
    abi::emit_load_from_address(emitter, other, other, 16);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {result}, {other}"));             // the physical binding must still contain this exact native cell
            emitter.instruction(&format!("b.ne {miss}"));                       // reject a retired or replaced binding before cell inspection
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {result}, {other}"));             // the physical binding must still contain this exact native cell
            emitter.instruction(&format!("jne {miss}"));                        // reject a retired or replaced binding before cell inspection
        }
    }
    abi::emit_call_label(emitter, "__rt_reference_cell_canonical");
    abi::emit_branch_if_int_result_zero(emitter, miss);
    abi::load_at_offset(emitter, other, 16);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {result}, {other}"));             // the weak wrapper index must match the authenticated live payload
            emitter.instruction("cset x0, eq");                                 // return one only for the same physical reference
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {result}, {other}"));             // the weak wrapper index must match the authenticated live payload
            emitter.instruction("sete al");                                     // produce the physical-match predicate
            emitter.instruction("movzx eax, al");                               // normalize the C int result
        }
    }
    abi::emit_jump(emitter, done);
    emitter.label(miss);
    abi::emit_load_int_immediate(emitter, result, 0);
    emitter.label(done);
    abi::emit_frame_restore(emitter, 48);
    abi::emit_return(emitter);
}

/// Compares borrowed candidates without PHP, writing only the C int output word.
fn emit_identical(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let output = abi::secondary_scratch_reg(emitter);
    let failed = "__rt_reference_validation_identical_failed";
    let done = "__rt_reference_validation_identical_done";
    emitter.label_global("__rt_reference_validation_identical");
    abi::emit_frame_prologue(emitter, 48);
    for (index, offset) in [(1, 8), (2, 16), (3, 24)] {
        abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    abi::load_at_offset(emitter, result, 24);
    abi::emit_branch_if_int_result_zero(emitter, failed);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), 8);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 1), 16);
    abi::emit_call_label(emitter, "__rt_mixed_strict_eq");
    abi::load_at_offset(emitter, output, 24);
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("str w0, [{output}]")),   // the ABI output is four bytes, not a native pointer word
        Arch::X86_64 => emitter.instruction(&format!("mov DWORD PTR [{output}], eax")), // preserve the caller's adjacent stack bytes
    }
    abi::emit_load_int_immediate(emitter, result, 0);
    abi::emit_jump(emitter, done);
    emitter.label(failed);
    abi::emit_load_int_immediate(emitter, result, 1);
    emitter.label(done);
    abi::emit_frame_restore(emitter, 48);
    abi::emit_return(emitter);
}

/// Consumes a candidate inside the same Throwable boundary used for declaration coercion.
fn emit_release(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let done = "__rt_reference_validation_release_done";
    emitter.label_global("__rt_reference_validation_release");
    abi::emit_frame_prologue(emitter, 32);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), 8);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 1), 16);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 2), 8);
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("add x2, x2, #8"),                 // accumulate a contained Throwable in this validation's owned slot
        Arch::X86_64 => emitter.instruction("add rdx, 8"),                      // accumulate a contained Throwable in this validation's owned slot
    }
    abi::emit_symbol_address(emitter, abi::int_arg_reg_name(emitter.target, 0), "__rt_decref_mixed");
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 1), 16);
    abi::emit_call_label(emitter, "__rt_cleanup_invoke");
    abi::emit_branch_if_int_result_zero(emitter, done);
    abi::emit_load_int_immediate(emitter, result, 2);
    emitter.label(done);
    abi::emit_frame_restore(emitter, 32);
    abi::emit_return(emitter);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// All supported targets authenticate cells, use the four-byte output and contain retirement.
    #[test]
    fn native_validation_operation_callbacks_follow_the_c_abi() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit(&mut emitter);
            let assembly = emitter.output();
            assert!(assembly.contains("__rt_reference_cell_canonical"), "{name}: authenticate cell identity");
            let (current, rest) = assembly.split_once("__rt_reference_validation_identical:").unwrap();
            assert!(!current.contains("__rt_incref") && !current.contains("__rt_cleanup"), "{name}: pure borrowed shape probe");
            let (identical, release) = rest.split_once("__rt_reference_validation_release:").unwrap();
            assert!(identical.contains(if name == "linux-x86_64" { "mov DWORD PTR" } else { "str w0," }), "{name}: C int output width");
            assert!(release.contains("__rt_cleanup_invoke") && !release.contains("__rt_throw_current"), "{name}: contain retirement across Rust");
        }
    }
}
