//! Purpose:
//! Checks original comparator result kinds before native callback adapters consume them.
//!
//! Called from:
//! - User-sort adapters and comparator-driven set operations once registered by the runtime.
//!
//! Key details:
//! - Borrowed and owned entries share the same noncoercive integer check.
//! - Consumed cells remain published through catchable failures and destructor cleanup.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use elephc_semantics::{SemanticProfile, StrictFeature, comparisons::COMPARATOR_MESSAGE};

const BORROWED_FRAME: usize = 32;
const OWNED_FRAME: usize = 96;
const OWNER_RECORD: usize = 80;
/// Shared jump-only TypeError entry used by comparator result guards.
pub(in crate::codegen_support::runtime) const FAIL: &str = "__rt_strict_comparator_fail";

/// Adds the checked comparator diagnostic only to runtimes that can invoke the guard.
pub(crate) fn data(profile: SemanticProfile) -> String {
    if !profile.contains(StrictFeature::Comparisons) { return String::new(); }
    format!("\n.data\n_strict_comparator_message:\n.ascii \"{COMPARATOR_MESSAGE}\"\n")
}

/// Emits equivalent borrowed and consuming comparator adapters for every supported target.
pub(in crate::codegen_support::runtime) fn emit(emitter: &mut Emitter) {
    if !emitter.semantic_profile.contains(StrictFeature::Comparisons) { return; }
    let result = abi::int_result_reg(emitter);
    emitter.label_global("__rt_strict_comparator");
    abi::emit_frame_prologue(emitter, BORROWED_FRAME);
    abi::emit_branch_if_int_result_zero(emitter, FAIL);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    // The storage ABI tags integers with zero; reference wrappers are unboxed first.
    abi::emit_branch_if_int_result_nonzero(emitter, FAIL);
    let payload = match emitter.target.arch { Arch::AArch64 => "x1", Arch::X86_64 => "rdi" };
    abi::emit_reg_move(emitter, result, payload);
    abi::emit_frame_restore(emitter, BORROWED_FRAME);
    abi::emit_return(emitter);

    emitter.label_global("__rt_strict_comparator_owned");
    abi::emit_frame_prologue(emitter, OWNED_FRAME);
    abi::store_at_offset(emitter, result, 8);
    let owner = abi::tertiary_scratch_reg(emitter);
    abi::emit_frame_slot_address(emitter, owner, 8);
    abi::emit_link_call_operand_owner_in_frame(emitter, owner, OWNER_RECORD);
    abi::emit_call_label(emitter, "__rt_strict_comparator");
    abi::store_at_offset(emitter, result, 16);
    abi::emit_unlink_call_operand_owner_in_frame(emitter, OWNER_RECORD);
    abi::load_at_offset(emitter, result, 8);
    abi::emit_call_label(emitter, "__rt_decref_mixed");
    abi::load_at_offset(emitter, result, 16);
    abi::emit_frame_restore(emitter, OWNED_FRAME);
    abi::emit_return(emitter);

    emitter.label_global(FAIL);
    match emitter.target.arch {
        Arch::AArch64 => super::arrays::value_error::emit_throw_static_message_aarch64(
            emitter, "_spl_type_error_class_id", "_strict_comparator_message", COMPARATOR_MESSAGE.len()),
        Arch::X86_64 => {
            emitter.instruction("sub rsp, 8");                                  // adapt the jump-only entry to the throwing helper's return-slot alignment
            super::arrays::value_error::emit_throw_static_message_x86_64(
                emitter, "_spl_type_error_class_id", "_strict_comparator_message", COMPARATOR_MESSAGE.len());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every target checks the unboxed original tag and publishes the consumed return owner.
    #[test]
    fn strict_comparison_comparator_runtime_guards_cover_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit(&mut emitter);
            assert!(emitter.output().is_empty(), "{name}");
            assert!(data(SemanticProfile::PERMISSIVE).is_empty());
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Comparisons, true);
            let mut emitter = Emitter::new(target);
            emitter.semantic_profile = profile;
            emit(&mut emitter);
            let asm = emitter.output();
            for symbol in ["__rt_strict_comparator", "__rt_strict_comparator_owned",
                "__rt_mixed_unbox", "__rt_cleanup_call_operand_owner", "__rt_decref_mixed",
                "_spl_type_error_class_id", "__rt_throw_current"]
            {
                assert!(asm.contains(symbol), "{name}: {symbol}");
            }
            assert!(!asm.contains("__rt_mixed_cast_int"), "{name}");
            assert!(data(profile).contains(COMPARATOR_MESSAGE), "{name}");
            if target.arch == Arch::X86_64 {
                let failure = asm.split(&format!(".globl {FAIL}\n")).nth(1).unwrap();
                assert!(failure.lines().take(5).any(|line| line.trim() == "sub rsp, 8"), "{failure}");
            }
        }
    }

    /// Cross-assembles the actual guard entries without creating persistent object artifacts.
    #[test]
    #[ignore = "requires clang with ELF and Apple AArch64 assembler support"]
    fn strict_comparison_comparator_runtime_guards_assemble_all_targets() {
        use std::{io::Write, process::{Command, Stdio}};
        for (name, triple) in [
            ("macos-aarch64", "arm64-apple-macosx13.0"),
            ("ios-arm64", "arm64-apple-ios15.0"),
            ("ios-sim-arm64", "arm64-apple-ios15.0-simulator"),
            ("linux-aarch64", "aarch64-unknown-linux-gnu"),
            ("linux-x86_64", "x86_64-unknown-linux-gnu"),
        ] {
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Comparisons, true);
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emitter.semantic_profile = profile;
            emit(&mut emitter);
            let syntax = if emitter.target.arch == Arch::X86_64 { ".intel_syntax noprefix\n" } else { "" };
            let assembly = format!("{syntax}{}\n.text\n{}", data(profile), emitter.output());
            let mut child = Command::new("clang").args(["-target", triple, "-x", "assembler",
                "-c", "-o", "/dev/null", "-"]).stdin(Stdio::piped()).stdout(Stdio::piped())
                .stderr(Stdio::piped()).spawn().expect("launch target assembler");
            child.stdin.take().unwrap().write_all(assembly.as_bytes()).unwrap();
            let output = child.wait_with_output().unwrap();
            assert!(output.status.success(), "{name}: {}", String::from_utf8_lossy(&output.stderr));
        }
    }
}
