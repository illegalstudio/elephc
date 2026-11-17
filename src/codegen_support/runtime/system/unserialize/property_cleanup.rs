//! Purpose:
//! Supplies compiler hydration facts to the shared property-owner emitter.
//!
//! Called from:
//! - The unserialize orchestrator before its target-specific property stores.
//!
//! Key details:
//! - Property lookup, type validation and parser completion remain compiler-owned.
//! - Memoir preserves stored owners and box aliases using the supplied scope callbacks.

use crate::codegen_support::{emit::Emitter, sentinels::UNINITIALIZED_TYPED_PROPERTY_SENTINEL};
use elephc_memoir::hydration_owners::{self, Context};

/// Supplies the decoder's existing marker and scope/payload ownership protocols.
const OWNER_CONTEXT: Context<'static> = Context {
    uninitialized: UNINITIALIZED_TYPED_PROPERTY_SENTINEL,
    defer_data: "__rt_unserialize_defer_data",
    active_depth: "_unser_active",
    release_payload: "__rt_unser_release_payload",
};

/// Emits the shared owner primitives at their original point in parser emission.
pub(super) fn emit(emitter: &mut Emitter) {
    hydration_owners::emit(emitter, OWNER_CONTEXT);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every target releases the value a hydration store overwrites, skipping only the
    /// uninitialized typed-property sentinel, and routes each owner kind to its helper.
    #[test]
    fn previous_property_release_contract_holds_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit(&mut emitter);
            let asm = emitter.output();
            let block = asm
                .split_once("__rt_unser_release_prev_prop").unwrap().1
                .split_once("__rt_unser_reuse_box").unwrap().0;
            assert!(block.contains("__rt_decref_any"), "{name}");
            assert!(block.contains("__rt_callable_descriptor_release"), "{name}");
            assert!(!block.contains("__rt_unserialize_defer_data"), "{name}: Mixed slots are handled by the store");
            assert!(asm.contains("__rt_unser_reuse_box"), "{name}");
        }
    }

    /// Every target keeps container payloads alive through the context and frees copied
    /// scalar/string box shells after the property slot has taken what it needs.
    #[test]
    fn stored_property_finisher_contract_holds_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains("__rt_unser_finish_stored_prop"), "{name}");
            assert!(asm.contains("__rt_incref"), "{name}");
            assert!(asm.contains("__rt_unserialize_defer_data"), "{name}");
            assert!(asm.contains("__rt_heap_free"), "{name}");
        }
    }

    /// Host callbacks and the uninitialized marker stay explicit on every target and mode.
    #[test]
    fn memoir_hydration_uses_host_context_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            for mode in 0..6 {
                let mut emitter = Emitter::new(Target::parse(name).unwrap());
                let mut changed = Emitter::new(Target::parse(name).unwrap());
                for state in [&mut emitter, &mut changed] {
                    state.pic_data_refs = mode == 1 || mode == 2;
                    state.cdylib_boundary = mode == 2 || mode == 3;
                    state.phpext_output = mode == 4;
                    state.dead_strip = mode == 5;
                }
                let context = Context {
                    defer_data: "__memoir_hydration_defer_probe",
                    release_payload: "__memoir_hydration_release_probe",
                    ..OWNER_CONTEXT
                };
                hydration_owners::emit(&mut emitter, context);
                hydration_owners::emit(&mut changed, Context {
                    uninitialized: context.uninitialized ^ 1, ..context
                });
                let asm = emitter.output();
                assert!(asm.contains(context.defer_data), "{name}, mode={mode}");
                assert!(asm.contains(context.release_payload), "{name}, mode={mode}");
                assert!(!asm.contains(OWNER_CONTEXT.defer_data), "{name}, mode={mode}");
                assert!(!asm.contains(OWNER_CONTEXT.release_payload), "{name}, mode={mode}");
                let finisher = asm.split_once("__rt_unser_release_prev_prop").unwrap().0;
                assert!(finisher.contains(context.defer_data), "{name}, mode={mode}");
                assert!(!finisher.contains(context.release_payload), "{name}, mode={mode}");
                assert_ne!(asm, changed.output(), "host marker ignored: {name}, mode={mode}");
            }
        }
    }
}
