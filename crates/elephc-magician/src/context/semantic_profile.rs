//! Purpose:
//! Owns the immutable semantic profile attached to one eval activation context.
//!
//! Called from:
//! - Generated native context setup and profile-aware interpreter operations.
//!
//! Key details:
//! - A context cannot switch policies after it accepts a profile.

use super::ElephcEvalContext;
use crate::errors::EvalStatus;
use elephc_semantics::SemanticProfile;

impl ElephcEvalContext {
    /// Returns this activation's effective guarantees, defaulting for standalone embedders.
    pub fn semantic_profile(&self) -> SemanticProfile {
        self.semantic_profile.unwrap_or(SemanticProfile::PERMISSIVE)
    }

    /// Pins the default when an embedder starts executing without installing a profile.
    pub(crate) fn activate_semantic_profile(&mut self) -> SemanticProfile {
        *self.semantic_profile.get_or_insert(SemanticProfile::PERMISSIVE)
    }

    /// Installs a profile once and refuses incompatible native re-entry on the same context.
    pub fn install_semantic_profile(&mut self, profile: SemanticProfile) -> Result<(), EvalStatus> {
        if self.semantic_profile.is_some_and(|current| current != profile) {
            return Err(EvalStatus::AbiMismatch);
        }
        self.semantic_profile = Some(profile);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use elephc_semantics::StrictFeature;

    /// Separate eval contexts retain independent profiles and reject policy changes.
    #[test]
    fn strict_profile_eval_context_is_immutable_and_independent() {
        let strict = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, true);
        let mut context = ElephcEvalContext::new();
        let other = ElephcEvalContext::new();
        assert_eq!(context.semantic_profile(), SemanticProfile::PERMISSIVE);
        assert_eq!(context.install_semantic_profile(strict), Ok(()));
        assert_eq!(context.install_semantic_profile(strict), Ok(()));
        assert_eq!(context.install_semantic_profile(SemanticProfile::PERMISSIVE), Err(EvalStatus::AbiMismatch));
        assert_eq!(context.semantic_profile(), strict);
        assert_eq!(other.semantic_profile(), SemanticProfile::PERMISSIVE);
        let mut activated = ElephcEvalContext::new();
        assert_eq!(activated.activate_semantic_profile(), SemanticProfile::PERMISSIVE);
        assert_eq!(activated.install_semantic_profile(strict), Err(EvalStatus::AbiMismatch));
    }

    /// Native profile installation rejects unknown versions/bits without changing the context.
    #[test]
    fn strict_profile_eval_abi_validates_version_bits_and_context() {
        use crate::ffi::context::__elephc_eval_set_semantic_profile;
        use elephc_semantics::SEMANTICS_ABI_VERSION;
        let mut context = ElephcEvalContext::new();
        let pointer = &mut context as *mut _;
        let mut incompatible = ElephcEvalContext::for_abi_version(crate::abi::ABI_VERSION + 1);
        // SAFETY: pointer names this test's live, exclusively borrowed context.
        unsafe {
            assert_eq!(__elephc_eval_set_semantic_profile(pointer, SEMANTICS_ABI_VERSION + 1, 0), EvalStatus::AbiMismatch.code());
            assert_eq!(__elephc_eval_set_semantic_profile(pointer, SEMANTICS_ABI_VERSION, 1u64 << 63), EvalStatus::AbiMismatch.code());
            assert_eq!(__elephc_eval_set_semantic_profile(std::ptr::null_mut(), SEMANTICS_ABI_VERSION, 0), EvalStatus::RuntimeFatal.code());
            assert_eq!(__elephc_eval_set_semantic_profile(&mut incompatible, SEMANTICS_ABI_VERSION, 0), EvalStatus::AbiMismatch.code());
            assert_eq!(__elephc_eval_set_semantic_profile(pointer, SEMANTICS_ABI_VERSION, StrictFeature::Locals.bit()), EvalStatus::Ok.code());
        }
        assert_eq!(context.semantic_profile(), SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true));
    }

    /// Rejects opaque source before parsing, including the deferred-catalog FFI entry point.
    #[test]
    fn strict_profile_dynamic_code_ffi_cannot_execute_source() {
        use crate::ffi::execute::{__elephc_eval_execute, __elephc_eval_execute_with_deferred_catalog};
        let source = b"echo 'must not execute';";
        for feature in [StrictFeature::DynamicCode, StrictFeature::Casts] {
            let mut context = ElephcEvalContext::new();
            context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(feature, true)).unwrap();
            let mut scope = crate::scope::ElephcEvalScope::new();
            // SAFETY: both opaque handles and the source slice remain live throughout each call.
            unsafe {
                for execute in [__elephc_eval_execute, __elephc_eval_execute_with_deferred_catalog] {
                    assert_eq!(execute(&mut context, &mut scope, source.as_ptr(), source.len() as u64,
                        std::ptr::null_mut()), EvalStatus::UnsupportedConstruct.code());
                }
            }
        }
    }
}
