//! Purpose:
//! Computes protected eval remainders with PHP numeric conversion and diagnostic ordering.
//!
//! Called from:
//! - Checked numeric runtime emission and the explicit Mod eval callback selector.
//!
//! Key details:
//! - Guarded input copies preserve quoted string bytes across arbitrary warning callbacks.
//! - Float values use modulo conversion; numeric strings use the capped string conversion.
//! - Zero divisors throw and the signed-minimum modulo-minus-one case returns zero.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use super::strict_mixed_numeric::{box_number, capture_values, dispatch_operand_status, load_float};

const FRAME: usize = 192;
const PREFIX: &str = "__rt_strict_mixed_numeric_mod";
const MESSAGES: &[(&str, &str)] = &[
    ("_strict_mod_zero_message", "Modulo by zero"),
    ("_strict_mod_string_prefix", "Deprecated: Implicit conversion from float-string \""),
    ("_strict_mod_string_suffix", "\" to int loses precision\n"),
];

/// Emits static messages from the same fragments that determine diagnostic byte counts.
pub(super) fn data() -> String {
    MESSAGES.iter().map(|(label, message)| format!("{label}:\n.ascii {message:?}\n")).collect()
}

/// Captures independent inputs, converts in source order and transfers only a completed result.
pub(super) fn emit(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let low = if emitter.target.arch == Arch::AArch64 { "x1" } else { "rdi" };
    emitter.label_global(PREFIX);
    abi::emit_frame_prologue(emitter, FRAME);
    abi::emit_store_to_sp(emitter, result, 0);
    abi::emit_store_to_sp(emitter, low, 8);
    for (input, owner, guard) in [(0, 80, 96), (8, 88, 128)] {
        abi::emit_load_temporary_stack_slot(emitter, result, input);
        abi::emit_call_label(emitter, "__rt_mixed_clone");
        abi::emit_store_to_sp(emitter, result, owner);
        abi::emit_guard_owned_at_stack(emitter, result, "__rt_exception_release_owned", guard);
    }
    abi::emit_load_temporary_stack_slot(emitter, result, 80);
    abi::emit_load_temporary_stack_slot(emitter, low, 88);
    capture_values(emitter);
    for (input, payload, tag, status) in [(0, 16, 24, 48), (8, 32, 40, 56)] {
        dispatch_operand_status(emitter, PREFIX, status);
        convert_operand(emitter, input, payload, tag);
    }
    abi::emit_load_temporary_stack_slot(emitter, result, 32);
    abi::emit_branch_if_int_result_zero(emitter, &format!("{PREFIX}_zero"));
    ins(emitter, "cmn x0, #1", "cmp rax, -1");
    abi::emit_branch_if_equal_wide(emitter, &format!("{PREFIX}_minus_one"));
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_reg_move(emitter, "x2", result);
            abi::emit_load_temporary_stack_slot(emitter, "x1", 16);
            emitter.instruction("sdiv x9, x1, x2");                             // compute a signed quotient after excluding zero and the minimum/-1 overflow
            emitter.instruction("msub x1, x9, x2, x1");                         // retain the dividend-signed remainder as the integer payload
        }
        Arch::X86_64 => {
            abi::emit_reg_move(emitter, "rcx", result);
            abi::emit_load_temporary_stack_slot(emitter, result, 16);
            emitter.instruction("cqo");                                         // extend the signed dividend before a non-overflowing divide
            emitter.instruction("idiv rcx");                                    // zero and the hardware-overflow case have already been excluded
            emitter.instruction("mov rdi, rdx");                                // retain the dividend-signed remainder as the integer payload
        }
    }
    abi::emit_jump(emitter, &format!("{PREFIX}_box"));
    emitter.label(&format!("{PREFIX}_minus_one"));
    abi::emit_load_int_immediate(emitter, low, 0);
    emitter.label(&format!("{PREFIX}_box"));
    box_number(emitter, 0);
    abi::emit_store_to_sp(emitter, result, 72);
    for (owner, guard) in [(88, 128), (80, 96)] {
        abi::emit_unguard_owned_at_stack(emitter, guard);
        abi::emit_load_temporary_stack_slot(emitter, result, owner);
        abi::emit_call_label(emitter, "__rt_decref_mixed");
    }
    abi::emit_load_temporary_stack_slot(emitter, result, 72);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);
    emitter.label(&format!("{PREFIX}_zero"));
    super::strict_numeric_div::emit_zero_failure(emitter, MESSAGES[0].0, MESSAGES[0].1.len());
}

/// Converts one captured number only after delivering that operand's prefix or kind diagnostic.
fn convert_operand(emitter: &mut Emitter, input: usize, payload: usize, tag: usize) {
    let result = abi::int_result_reg(emitter);
    let done = format!("{PREFIX}_converted_{payload}");
    let string = format!("{PREFIX}_string_{payload}");
    abi::emit_load_temporary_stack_slot(emitter, result, tag);
    abi::emit_branch_if_int_result_zero(emitter, &done);
    abi::emit_load_temporary_stack_slot(emitter, result, input);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    ins(emitter, "cmp x0, #1", "cmp rax, 1");
    abi::emit_branch_if_equal_wide(emitter, &string);
    load_float(emitter, payload, tag, PREFIX);
    abi::emit_call_label(emitter, "__rt_float_key_to_int");
    abi::emit_store_to_sp(emitter, result, payload);
    abi::emit_jump(emitter, &done);
    emitter.label(&string);
    convert_string(emitter, payload);
    emitter.label(&done);
}

/// Retains a string's capped integer and quotes the original bytes if its numeric value changes.
fn convert_string(emitter: &mut Emitter, payload: usize) {
    let result = abi::int_result_reg(emitter);
    let low = if emitter.target.arch == Arch::AArch64 { "x1" } else { "rdi" };
    let high = if emitter.target.arch == Arch::AArch64 { "x2" } else { "rdx" };
    let ready = format!("{PREFIX}_string_ready_{payload}");
    abi::emit_store_to_sp(emitter, low, 160);
    abi::emit_store_to_sp(emitter, high, 168);
    if emitter.target.arch == Arch::X86_64 { abi::emit_reg_move(emitter, result, low); }
    abi::emit_call_label(emitter, "__rt_str_to_int");
    abi::emit_store_to_sp(emitter, result, 64);
    abi::emit_int_result_to_float_result(emitter);
    ins(emitter, "fmov d1, d0", "movapd xmm1, xmm0");
    abi::emit_load_temporary_stack_slot(emitter, abi::float_result_reg(emitter), payload);
    ins(emitter, "fcmp d0, d1", "ucomisd xmm0, xmm1");
    abi::emit_branch_if_equal_wide(emitter, &ready);
    emit_fragment(emitter, 1, false);
    let (pointer, length) = if emitter.target.arch == Arch::AArch64 { ("x1", "x2") } else { ("rdi", "rsi") };
    abi::emit_load_temporary_stack_slot(emitter, pointer, 160);
    abi::emit_load_temporary_stack_slot(emitter, length, 168);
    abi::emit_call_label(emitter, "__rt_diag_warning_fragment");
    emit_fragment(emitter, 2, true);
    emitter.label(&ready);
    abi::emit_load_temporary_stack_slot(emitter, result, 64);
    abi::emit_store_to_sp(emitter, result, payload);
}

/// Sends a complete or partial diagnostic using one authoritative static fragment table.
fn emit_fragment(emitter: &mut Emitter, index: usize, complete: bool) {
    let (pointer, length) = if emitter.target.arch == Arch::AArch64 { ("x1", "x2") } else { ("rdi", "rsi") };
    abi::emit_symbol_address(emitter, pointer, MESSAGES[index].0);
    abi::emit_load_int_immediate(emitter, length, MESSAGES[index].1.len() as i64);
    abi::emit_call_label(emitter, if complete { "__rt_diag_warning" } else { "__rt_diag_warning_fragment" });
}

/// Emits equivalent scalar-register and floating instructions on each supported instruction set.
fn ins(emitter: &mut Emitter, arm: &str, x86: &str) {
    emitter.instruction(if emitter.target.arch == Arch::AArch64 { arm } else { x86 });// preserve identical integer conversion and remainder behavior
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Native snapshots have unwind guards and preserve distinct PHP conversion entries everywhere.
    #[test]
    fn strict_eval_modulo_runtime_owns_snapshots_and_safe_remainder_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit(&mut emitter);
            let asm = emitter.output();
            assert_eq!(asm.matches(" __rt_mixed_clone\n").count(), 2, "{name}");
            assert_eq!(asm.matches(" __rt_exception_guard_owned\n").count(), 2, "{name}");
            assert!(asm.find(" __rt_exception_guard_owned\n").unwrap() < asm.find("__rt_strict_numeric_operand").unwrap(), "{name}");
            for helper in ["__rt_str_to_int", "__rt_float_key_to_int", "__rt_diag_warning_fragment",
                "_spl_division_by_zero_error_class_id", "__rt_exception_unguard_owned"] {
                assert!(asm.contains(helper), "{name}: {helper}");
            }
            assert!(asm.contains(&format!("{PREFIX}_minus_one:")), "{name}");
            assert!(!asm.contains("__rt_mixed_cast_int"), "{name}");
        }
    }
}
