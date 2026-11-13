//! Purpose:
//! Exposes immutable native semantic identity through the eval value-hook C ABI.
//!
//! Called from:
//! - The eval bridge runtime emitter and Magician's native RuntimeValueOps adapter.
//!
//! Key details:
//! - Values come from the explicit runtime emitter profile and never mutable process state.
//! - Each leaf uses the target C symbol and integer return convention.

use super::{abi, label_c_global, Emitter};

/// Emits version and profile getters before native value hooks can be used by eval.
pub(super) fn emit(emitter: &mut Emitter) {
    let profile = emitter.semantic_profile.bits();
    for (name, value) in [
        ("__elephc_eval_runtime_semantics_version", u64::from(elephc_semantics::SEMANTICS_ABI_VERSION)),
        ("__elephc_eval_runtime_semantics_profile", profile),
    ] {
        label_c_global(emitter, name);
        let result = abi::int_result_reg(emitter);
        abi::emit_load_int_immediate(emitter, result, value as i64);
        abi::emit_return(emitter);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Arch, Target};
    use elephc_semantics::{SemanticProfile, StrictFeature};

    /// All targets expose the profile selected for this runtime, including explicit permissive zero.
    #[test]
    fn strict_runtime_semantic_identity_covers_all_supported_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            for profile in [SemanticProfile::PERMISSIVE, SemanticProfile::PERMISSIVE.with(StrictFeature::Bools, true)] {
                let mut emitter = Emitter::new(target);
                emitter.semantic_profile = profile;
                emit(&mut emitter);
                let asm = emitter.output();
                for symbol in ["__elephc_eval_runtime_semantics_version", "__elephc_eval_runtime_semantics_profile"] {
                    assert!(asm.contains(&format!("{}:", target.extern_symbol(symbol))), "{name}: {asm}");
                }
                let version = if target.arch == Arch::AArch64 { "mov x0, #1" } else { "mov rax, 1" };
                let bits = if target.arch == Arch::AArch64 { format!("mov x0, #{}", profile.bits()) }
                    else { format!("mov rax, {}", profile.bits()) };
                assert!(asm.contains(version) && asm.contains(&bits), "{name}: {asm}");
                assert_eq!(asm.lines().filter(|line| line.trim().starts_with("ret")).count(), 2, "{name}: {asm}");
            }
        }
    }
}
