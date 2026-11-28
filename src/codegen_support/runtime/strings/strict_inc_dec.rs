//! Purpose:
//! Emits policy-specific checked PHP increment/decrement entries.
//!
//! Called from:
//! - The shared string and boxed increment runtime emitters.
//!
//! Key details:
//! - Only signed integer step overflow changes behavior under Overflow alone.
//! - Existing PHP entries remain available for trusted implementation operations.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use elephc_semantics::StrictFeature;

/// Selects an explicit entry policy before the string helper allocates its shared frame.
pub(super) fn string_entries(emitter: &mut Emitter) {
    emitter.label_global("__rt_str_inc_dec");
    if !emitter.semantic_profile.contains(StrictFeature::Overflow) { return; }
    let flag = if emitter.target.arch == Arch::AArch64 { "x9" } else { "r10" };
    abi::emit_load_int_immediate(emitter, flag, 0);
    abi::emit_jump(emitter, "__rt_str_inc_dec_policy_body");
    emitter.label_global("__rt_strict_str_inc_dec");
    abi::emit_load_int_immediate(emitter, flag, 1);
    abi::emit_jump(emitter, "__rt_str_inc_dec_policy_body");
    emitter.label_shared("__rt_str_inc_dec_policy_body");
}

/// Saves the selected policy in the existing frame's unused final local word.
pub(super) fn save_string_policy(emitter: &mut Emitter) {
    if !emitter.semantic_profile.contains(StrictFeature::Overflow) { return; }
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("str x9, [sp, #40]"),              // retain the entry policy beyond string scanning and conversion calls
        Arch::X86_64 => emitter.instruction("mov QWORD PTR [rbp - 48], r10"),   // retain the entry policy beyond string scanning and conversion calls
    }
}

/// Names only the actual integer step overflow edge, leaving float classification unchanged.
pub(super) fn string_overflow_target(emitter: &Emitter) -> &'static str {
    if emitter.semantic_profile.contains(StrictFeature::Overflow) { "__rt_str_inc_dec_integer_overflow" }
    else if emitter.target.arch == Arch::AArch64 { "__rt_sid_float" } else { "__rt_sid_float_x86" }
}

/// Throws only for a checked entry; PHP entries continue through the existing float fallback.
pub(super) fn string_overflow_dispatch(emitter: &mut Emitter) {
    if !emitter.semantic_profile.contains(StrictFeature::Overflow) { return; }
    emitter.label("__rt_str_inc_dec_integer_overflow");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x9, [sp, #40]");                           // inspect the entry policy after the integer step overflowed
            emitter.instruction("cmp x9, #0");                                  // checked entries reject integer-to-float promotion
        }
        Arch::X86_64 => emitter.instruction("cmp QWORD PTR [rbp - 48], 0"),     // checked entries reject integer-to-float promotion
    }
    abi::emit_branch_if_not_equal_wide(emitter, "__rt_strict_integer_overflow_fail");
}

/// Checks a genuine boxed integer before the PHP helper can mutate its storage.
pub(super) fn mixed_entry(emitter: &mut Emitter) {
    if !emitter.semantic_profile.contains(StrictFeature::Overflow) { return; }
    let (input, delta, payload, scratch) = match emitter.target.arch {
        Arch::AArch64 => ("x0", "x1", "x1", "x9"),
        Arch::X86_64 => ("rax", "rdi", "rdi", "r10"),
    };
    emitter.label_global("__rt_strict_mixed_inc_dec");
    abi::emit_frame_prologue(emitter, 64);
    abi::emit_store_to_sp(emitter, input, 0);
    abi::emit_store_to_sp(emitter, delta, 8);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("cmp x0, #1"),                     // route string payloads through their checked numeric-string step
        Arch::X86_64 => emitter.instruction("cmp rax, 1"),                      // route string payloads through their checked numeric-string step
    }
    abi::emit_branch_if_equal_wide(emitter, "__rt_strict_mixed_inc_dec_string");
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("cmp x0, #0"),                     // genuine integer payloads need a checked step before any mutation
        Arch::X86_64 => emitter.instruction("cmp rax, 0"),                      // genuine integer payloads need a checked step before any mutation
    }
    abi::emit_branch_if_equal_wide(emitter, "__rt_strict_mixed_inc_dec_integer");
    for tag in [2, 3, 8] {
        match emitter.target.arch {
            Arch::AArch64 => emitter.instruction(&format!("cmp x0, #{tag}")),   // retain the reviewed scalar PHP update domains
            Arch::X86_64 => emitter.instruction(&format!("cmp rax, {tag}")),    // retain the reviewed scalar PHP update domains
        }
        abi::emit_branch_if_equal_wide(emitter, "__rt_strict_mixed_inc_dec_php");
    }
    abi::emit_jump(emitter, super::super::arrays::strict_numeric_operand::TYPE_FAIL);
    emitter.label("__rt_strict_mixed_inc_dec_integer");
    abi::emit_load_temporary_stack_slot(emitter, scratch, 8);
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("adds x9, x1, x9"),                // test the signed integer step without changing the source cell
        Arch::X86_64 => emitter.instruction("add r10, rdi"),                    // test the signed integer step without changing the source cell
    }
    abi::emit_branch_if_overflow_wide(emitter, "__rt_strict_integer_overflow_fail");
    emitter.label("__rt_strict_mixed_inc_dec_php");
    abi::emit_load_temporary_stack_slot(emitter, input, 0);
    abi::emit_load_temporary_stack_slot(emitter, delta, 8);
    abi::emit_frame_restore(emitter, 64);
    abi::emit_jump(emitter, "__rt_mixed_inc_dec");
    emitter.label("__rt_strict_mixed_inc_dec_string");
    if emitter.target.arch == Arch::X86_64 { abi::emit_reg_move(emitter, "rax", payload); }
    let string_delta = if emitter.target.arch == Arch::AArch64 { "x3" } else { "rcx" };
    abi::emit_load_temporary_stack_slot(emitter, string_delta, 8);
    abi::emit_call_label(emitter, "__rt_strict_str_inc_dec");
    abi::emit_frame_restore(emitter, 64);
    abi::emit_return(emitter);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;
    use elephc_semantics::SemanticProfile;

    /// Checked entries coexist with PHP entries on every supported target and retain wide error edges.
    #[test]
    fn strict_overflow_update_runtime_entries_cover_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            for checked in [false, true] {
                let mut emitter = Emitter::new(Target::parse(name).unwrap());
                emitter.semantic_profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, checked);
                super::super::emit_str_inc_dec(&mut emitter);
                super::super::emit_mixed_inc_dec(&mut emitter);
                let arch = emitter.target.arch;
                let asm = emitter.output();
                for helper in ["__rt_str_inc_dec", "__rt_mixed_inc_dec"] {
                    assert!(asm.contains(&format!("{helper}:")), "{name} {checked}");
                }
                for helper in ["__rt_strict_str_inc_dec", "__rt_strict_mixed_inc_dec", "__rt_str_inc_dec_integer_overflow"] {
                    assert_eq!(asm.contains(&format!("{helper}:")), checked, "{name} {checked}: {helper}");
                }
                if checked {
                    assert!(asm.contains("__rt_strict_integer_overflow_fail"), "{name}");
                    if arch == Arch::AArch64 { assert!(asm.contains("b.vc 1f"), "{name}"); }
                }
            }
        }
    }
}
