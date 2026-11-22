//! Purpose:
//! Calls the native Rust validation coordinator with a complete protected operation table.
//!
//! Called from:
//! - Native reference publishers when the native_reference_validation capability is enabled.
//!
//! Key details:
//! - Arguments are borrowed reference, original RHS, context and writable outcome.
//! - The publisher owns context, outcome and invisible roots through every callback and commit.
//! - All PHP exits are contained by operation callbacks before crossing Rust stack frames.

use super::*;

const FRAME: usize = 128;
const OPS: usize = 104;
const PROPOSED: usize = 40;

/// Emits a C-ABI bridge preserving the coordinator's success/failure/Throwable/conflict statuses.
pub(in crate::codegen_support::runtime) fn emit(emitter: &mut Emitter, eval_bridge: bool) {
    let result = abi::int_result_reg(emitter);
    emitter.label_global("__rt_reference_validate_native");
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter.target, 4), 0);
    abi::emit_jump(emitter, "__rt_reference_validate_native_body");
    emitter.label_global("__rt_reference_validate_native_with_source");
    emitter.label_shared("__rt_reference_validate_native_body");
    abi::emit_frame_prologue(emitter, FRAME);
    for (index, offset) in [8, 16, 24, 32, PROPOSED].into_iter().enumerate() {
        abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    if eval_bridge && (emitter.semantic_profile.checked_property_references()
        || emitter.semantic_profile.contains(elephc_semantics::StrictFeature::Locals)) {
        // Live contents and exact eval sources inspect the original RHS before scalar coercion.
        abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), 8);
        abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 1), 16);
        abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 2), 24);
        let pending = abi::int_arg_reg_name(emitter.target, 2);
        match emitter.target.arch {
            Arch::AArch64 => emitter.instruction(&format!("add {pending}, {pending}, #8")),  // output owns a raw contained Throwable
            Arch::X86_64 => emitter.instruction(&format!("add {pending}, 8")),  // output owns a raw contained Throwable
        }
        let entry = emitter.target.extern_symbol("__elephc_eval_reference_validate_v1");
        abi::emit_call_label(emitter, &entry);
        abi::emit_branch_if_int_result_nonzero(emitter, "__rt_reference_validate_native_done");
    }
    let version = elephc_reference::native_reference_validation::VALIDATION_ABI_VERSION_V2 as i64;
    let bytes = std::mem::size_of::<elephc_reference::native_reference_validation::NativeReferenceValidationOpsV2>() as i64;
    debug_assert_eq!(bytes, 56, "the native operation frame follows validation ABI version two");
    for (offset, value) in [(OPS, version), (OPS - 8, bytes)] {
        abi::emit_load_int_immediate(emitter, result, value);
        abi::store_at_offset(emitter, result, offset);
    }
    for (offset, entry) in [
        (OPS - 16, "__rt_reference_validation_is_current"),
        (OPS - 24, "__rt_reference_validation_coerce"),
        (OPS - 32, "__rt_reference_validation_identical"),
        (OPS - 40, "__rt_reference_validation_release"),
    ] {
        abi::emit_symbol_address(emitter, result, entry);
        abi::store_at_offset(emitter, result, offset);
    }
    let weak_eval = eval_bridge && emitter.semantic_profile.checked_property_references()
        && !emitter.semantic_profile.contains(elephc_semantics::StrictFeature::Coercions);
    if weak_eval {
        abi::emit_symbol_address(emitter, result, "__rt_reference_validation_eval_coerce");
        abi::store_at_offset(emitter, result, OPS - 48);
    } else { abi::emit_store_zero_to_local_slot(emitter, OPS - 48); }
    for (index, offset) in [8, 16, 24].into_iter().enumerate() {
        abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    abi::emit_frame_slot_address(emitter, abi::int_arg_reg_name(emitter.target, 3), OPS);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 4), 32);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 5), PROPOSED);
    let entry = emitter.target.extern_symbol("__elephc_reference_validate_v2");
    abi::emit_call_label(emitter, &entry);
    emitter.label("__rt_reference_validate_native_done");
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);
    if weak_eval { emit_eval_coercer(emitter); }
}

/// Adapts the protected eval provider to the version-two external candidate callback.
fn emit_eval_coercer(emitter: &mut Emitter) {
    emitter.label_global("__rt_reference_validation_eval_coerce");
    abi::emit_frame_prologue(emitter, 48);
    for (index, offset) in [8, 16, 24, 32].into_iter().enumerate() {
        abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    for (index, offset) in [16, 24, 32].into_iter().enumerate() {
        abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    let pending = abi::int_arg_reg_name(emitter.target, 3);
    abi::load_at_offset(emitter, pending, 8);
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("add {pending}, {pending}, #8")),// borrow the validation context's contained Throwable slot
        Arch::X86_64 => emitter.instruction(&format!("add {pending}, 8")),      // borrow the validation context's contained Throwable slot
    }
    let entry = emitter.target.extern_symbol("__elephc_eval_reference_coerce_v1");
    abi::emit_call_label(emitter, &entry);
    abi::emit_frame_restore(emitter, 48);
    abi::emit_return(emitter);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Casts-only publication supplies weak eval candidates without enabling exact Coercions.
    #[test]
    fn native_reference_coordinator_weak_eval_candidates_are_independent_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            for (casts, coercions, bridge) in [(true, false, true), (false, false, true),
                (true, true, true), (true, false, false)] {
                let mut emitter = Emitter::new(Target::parse(name).unwrap());
                emitter.semantic_profile = elephc_semantics::SemanticProfile::PERMISSIVE
                    .with(elephc_semantics::StrictFeature::Casts, casts)
                    .with(elephc_semantics::StrictFeature::Coercions, coercions);
                emit(&mut emitter, bridge);
                assert_eq!(emitter.output().contains("__elephc_eval_reference_coerce_v1"),
                    casts && !coercions && bridge, "{name}: casts={casts} coercions={coercions} bridge={bridge}");
            }
        }
    }

    /// Eval constraints inspect the original candidate before the native coercion coordinator.
    #[test]
    fn native_reference_coordinator_checks_eval_sources_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            for feature in [elephc_semantics::StrictFeature::Coercions, elephc_semantics::StrictFeature::Locals,
                elephc_semantics::StrictFeature::ArrayTypes, elephc_semantics::StrictFeature::Mixed,
                elephc_semantics::StrictFeature::Casts, elephc_semantics::StrictFeature::Nullability] {
                for enabled in [false, true] {
                    let target = Target::parse(name).unwrap();
                    let mut emitter = Emitter::new(target);
                    emitter.semantic_profile = elephc_semantics::SemanticProfile::PERMISSIVE
                        .with(feature, enabled);
                    emit(&mut emitter, true);
                    let assembly = emitter.output();
                    let eval = assembly.find(&target.extern_symbol("__elephc_eval_reference_validate_v1"));
                    assert_eq!(eval.is_some(), enabled, "{name} strict={enabled}");
                    if let Some(eval) = eval {
                        let native = assembly.find(&target.extern_symbol("__elephc_reference_validate_v2")).unwrap();
                        assert!(eval < native, "{name}: original candidate checked before native coercion");
                        assert!(assembly[eval..native].contains("__rt_reference_validate_native_done"),
                            "{name}: rejected candidates return before native validation or publication");
                    }
                    assert!(!assembly.contains("__rt_throw_current"), "{name}: exceptions cross Rust as status values");
                }
            }
        }
    }

    /// Real ELF and Mach-O assemblers accept the eval reference boundary on every target.
    #[test]
    #[ignore = "requires clang with ELF and Apple AArch64 assembler support"]
    fn native_reference_coordinator_eval_guard_assembles_all_targets() {
        use std::{io::Write, process::{Command, Stdio}};
        for (name, triple) in [("linux-x86_64", "x86_64-linux-gnu"),
            ("linux-aarch64", "aarch64-linux-gnu"), ("macos-aarch64", "arm64-apple-macos11"),
            ("ios-arm64", "arm64-apple-ios13"), ("ios-sim-arm64", "arm64-apple-ios13-simulator")] {
            for feature in [elephc_semantics::StrictFeature::Coercions, elephc_semantics::StrictFeature::Casts,
                elephc_semantics::StrictFeature::Locals] {
                let target = Target::parse(name).unwrap();
                let mut emitter = Emitter::new(target);
                emitter.semantic_profile = elephc_semantics::SemanticProfile::PERMISSIVE
                    .with(feature, true);
                emit(&mut emitter, true);
                let syntax = if target.arch == Arch::X86_64 { ".intel_syntax noprefix\n" } else { "" };
                let assembly = format!("{syntax}.text\n{}", emitter.output());
                let mut process = Command::new("clang").args(["-target", triple, "-x", "assembler",
                    "-c", "-o", "/dev/null", "-"]).stdin(Stdio::piped()).stderr(Stdio::piped())
                    .spawn().expect("clang is required for the explicit cross-target assembly test");
                process.stdin.take().unwrap().write_all(assembly.as_bytes()).unwrap();
                let output = process.wait_with_output().unwrap();
                assert!(output.status.success(), "{name}: {}", String::from_utf8_lossy(&output.stderr));
            }
        }
    }

    /// Every supported ABI supplies the versioned table without selecting PHP call conventions.
    #[test]
    fn native_reference_coordinator_has_all_protected_callbacks_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit(&mut emitter, false);
            let assembly = emitter.output();
            for entry in ["is_current", "coerce", "identical", "release"] {
                assert!(assembly.contains(&format!("__rt_reference_validation_{entry}")), "{name}: {entry}");
            }
            assert!(assembly.contains(&target.extern_symbol("__elephc_reference_validate_v2")), "{name}: native Rust ABI");
            assert!(!assembly.contains("__rt_throw_current"), "{name}: only contained statuses cross Rust");
        }
    }
}
