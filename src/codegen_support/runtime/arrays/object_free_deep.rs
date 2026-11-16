//! Purpose:
//! Preserves compiler imports and tests for Memoir value storage.
//!
//! Called from:
//! - Compiler memory emission and its integration tests.
//!
//! Key details:
//! - Runs only in the compiler and preserves the generated target assembly.

use crate::codegen_support::{emit::Emitter, RuntimeFeatures};
/// Emits the existing helper using only its reviewed memory feature gates.
pub fn emit_object_free_deep(emitter: &mut Emitter, features: RuntimeFeatures) {
    elephc_memoir::values::object_free_deep::emit_object_free_deep(emitter, features.into());
}

#[cfg(test)]
mod tests {
    use crate::codegen_support::emit::Emitter;
    use crate::codegen_support::platform::{Arch, Platform, Target};

    use super::*;

    /// Verifies that ARM64 object destruction dispatches callable properties to
    /// the descriptor-aware release helper instead of the generic heap decref.
    #[test]
    fn test_object_free_deep_releases_callable_properties_aarch64() {
        let mut emitter = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_object_free_deep(&mut emitter, RuntimeFeatures::all());
        let asm = emitter.output();

        assert!(asm.contains("cmp x15, #10\n"));
        assert!(asm.contains("__rt_object_free_deep_release_callable:\n"));
        assert!(asm.contains("__rt_callable_descriptor_release") && asm.contains("__rt_cleanup_invoke"));
    }

    /// Both targets clear a freed object's eval metadata (dynamic objects and registered
    /// reflectors) before its storage can be reused, and only when the program links the eval
    /// bridge, so a program without Magician references no `__elephc_eval_*` symbol.
    #[test]
    fn object_free_forgets_eval_identity_only_with_the_eval_bridge() {
        for (platform, arch, call) in [
            (Platform::MacOS, Arch::AArch64, "bl ___elephc_eval_dynamic_object_forget\n"),
            (Platform::Linux, Arch::AArch64, "bl __elephc_eval_dynamic_object_forget\n"),
            (Platform::Linux, Arch::X86_64, "call __elephc_eval_dynamic_object_forget\n"),
        ] {
            let mut with = Emitter::new(Target::new(platform, arch));
            emit_object_free_deep(&mut with, RuntimeFeatures::all());
            let with = with.output();
            let forget = with.find(call).unwrap_or_else(|| panic!("{arch:?}: forget call missing"));
            assert!(
                with[forget..].contains("__rt_heap_free"),
                "{arch:?}: the forget call must precede the heap free"
            );
            let mut without = Emitter::new(Target::new(platform, arch));
            let mut features = RuntimeFeatures::all();
            features.eval_bridge = false;
            emit_object_free_deep(&mut without, features);
            assert!(
                !without.output().contains("__elephc_eval_dynamic_object_forget"),
                "{arch:?}: no eval symbol without the bridge"
            );
        }
    }

    /// The stack-releasing arms appear only for a program that has the classes, and the guard has
    /// to check BOTH directions: an "is it absent" assertion alone passes just as well when the
    /// emitter is broken and emits nothing at all.
    ///
    /// Both arms are checked together because they share `__rt_fiber_free_stack`. Gating only the
    /// Fiber one left `munmap` imported through the Generator one — measured, not assumed, and the
    /// reason this test names both.
    #[test]
    fn the_stack_releasing_arms_follow_their_classes() {
        for (platform, arch, fiber_branch, generator_branch) in [
            (
                Platform::MacOS,
                Arch::AArch64,
                "b.ne __rt_object_free_deep_not_fiber\n",
                "b.ne __rt_object_free_deep_not_generator\n",
            ),
            (
                Platform::Linux,
                Arch::X86_64,
                "jne __rt_object_free_deep_not_fiber\n",
                "jne __rt_object_free_deep_not_generator\n",
            ),
        ] {
            let mut with = Emitter::new(Target::new(platform, arch));
            emit_object_free_deep(&mut with, RuntimeFeatures::all());
            let with = with.output();
            assert!(with.contains(fiber_branch), "{arch:?}: fiber arm must be emitted");
            assert!(
                with.contains(generator_branch),
                "{arch:?}: generator arm must be emitted"
            );
            assert!(
                with.contains("__rt_fiber_free_stack"),
                "{arch:?}: the stack helper is reached from those arms"
            );

            let mut without = Emitter::new(Target::new(platform, arch));
            emit_object_free_deep(&mut without, RuntimeFeatures::none());
            let without = without.output();
            assert!(!without.contains(fiber_branch), "{arch:?}: fiber arm must be gone");
            assert!(
                !without.contains(generator_branch),
                "{arch:?}: generator arm must be gone"
            );
            assert!(
                !without.contains("__rt_fiber_free_stack"),
                "{arch:?}: nothing may still reach the stack helper, or munmap stays imported"
            );
            assert!(
                without.contains("__rt_object_free_deep_struct"),
                "{arch:?}: the ordinary free path must survive the gating"
            );
        }
    }

    /// Verifies that x86_64 object destruction dispatches callable properties
    /// to the descriptor-aware release helper instead of the generic heap decref.
    #[test]
    fn test_object_free_deep_releases_callable_properties_x86_64() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_object_free_deep(&mut emitter, RuntimeFeatures::all());
        let asm = emitter.output();

        assert!(asm.contains("cmp r8, 10\n"));
        assert!(asm.contains("__rt_object_free_deep_release_callable:\n"));
        assert!(asm.contains("__rt_callable_descriptor_release") && asm.contains("__rt_cleanup_invoke"));
    }
}
