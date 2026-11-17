//! Purpose:
//! Keeps native Mixed reference cells and persistent PHP value aliases on the same storage.
//!
//! Called from:
//! - Native reference-property capture, local/property stores and eval property replacement.
//!
//! Key details:
//! - Kind-eight cells retain their existing descriptor seven and low-word sentinel.
//! - A requested reference lazily wraps the current box without copying its PHP value.
//! - Writers follow the wrapper's immediate child slot and preserve the wrapper identity.

use crate::codegen_support::emit::Emitter;
pub(super) use elephc_memoir::reference_cells::mixed_payload::emit_retain_mixed;
pub(crate) use elephc_memoir::reference_cells::mixed_payload::follow_mixed_reference_slot;

/// Keeps native type-source association optional and authoritative in the compiler.
pub(super) fn emit_promote_mixed(emitter: &mut Emitter, native_validation: bool) {
    let associate = native_validation.then_some(
        super::source_lifecycle::associate_result as fn(&mut Emitter));
    elephc_memoir::reference_cells::mixed_payload::emit_promote_mixed(emitter, associate);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every target owns a promoted Mixed constructor cell and retires its replaced placeholder.
    #[test]
    fn promoted_mixed_constructor_reference_ownership_on_every_target() {
        let source = "<?php class OwnedMixedConstructor { \
            public function __construct(public mixed &$value = 'default') {} } \
            class OwnedMixedChild extends OwnedMixedConstructor {} \
            class OwnedPrivateMixedConstructor { \
                public function __construct(private mixed &$value) {} } \
            class OwnedPrivateMixedShadow extends OwnedPrivateMixedConstructor { \
                public string $value = 'shadow'; } \
            function makeOwnedMixed(mixed $value): OwnedMixedChild { \
                return new OwnedMixedChild($value); } \
            function makeFreshOwnedMixed(): OwnedMixedConstructor { \
                return new OwnedMixedConstructor($fresh); } \
            function makeDynamicOwnedMixed(string $class): mixed { \
                $value = 'before'; $object = new $class($value); \
                $value = 'after'; return $object; } \
            $object = makeOwnedMixed('initial'); $object->value = 'updated'; \
            $freshObject = makeFreshOwnedMixed();";
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let module = crate::ir_lower::tests::lower_source_at_for_target(
                source, std::path::Path::new("main.php"), std::path::Path::new("."), target,
            );
            for class in ["OwnedMixedConstructor", "OwnedMixedChild", "OwnedPrivateMixedConstructor", "OwnedPrivateMixedShadow"] {
                assert!(module.class_infos[class].property_slot_owns_reference(0), "{name}: {class}");
            }
            let dynamic = module.functions.iter().find(|function| function.name == "makeDynamicOwnedMixed").unwrap();
            assert_eq!(dynamic.instructions.iter().filter(|inst| inst.op == crate::ir::Op::ObjectNew).count(), 4,
                "{name}: inherited constructors retain normalization beneath private property shadows");
            let user = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
            assert!(user.contains("__rt_reference_cell_retain_mixed"), "{name}: constructor retains its binding");
            assert!(user.contains("__rt_reference_cell_release"), "{name}: placeholder owner retires");
            let mut emitter = Emitter::new(target);
            emit_retain_mixed(&mut emitter);
            let runtime = emitter.output();
            assert!(runtime.contains("__rt_reference_cell_owner"), "{name}: reuse exact managed owners");
            assert!(runtime.contains("__rt_reference_cell_promote_mixed"), "{name}: borrowed slots share a persistent wrapper");
            assert!(!runtime.contains("__rt_mixed_clone"), "{name}: keep the original PHP reference identity");
        }
    }
}
