//! Purpose:
//! Pins borrowed versus owning Mixed payload extraction on every supported ABI.
//!
//! Called from:
//! - The EIR backend unit-test harness.
//!
//! Key details:
//! - Borrowed class-dispatch receivers must not acquire a reference without a release owner.
//! - Ordinary owning unboxes retain their existing payload lease.

use super::*;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Target;
use crate::codegen::shared_state::SharedCodegenState;

/// Guarded receiver borrows omit retains while owning payloads keep exactly one retain.
#[test]
fn mixed_object_unbox_respects_borrowed_ownership_on_every_target() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for ownership in [Ownership::Borrowed, Ownership::Owned, Ownership::MaybeOwned] {
            let target = Target::parse(name).unwrap();
            let module = Module::new(target);
            let mut function = crate::codegen::shared_helper::helper_function("unbox_owner", PhpType::Void);
            {
                let entry = function.entry;
                let mut builder = crate::ir::Builder::new(&mut function);
                builder.position_at_end(entry);
                let ty = PhpType::Object("stdClass".into());
                builder.emit_with_effects(Op::MixedUnbox,
                    vec![crate::codegen::shared_helper::helper_value()], None,
                    crate::ir::IrType::Heap(crate::ir::IrHeapKind::Object), ty.clone(),
                    ownership, Op::mixed_unbox_effects(&ty), None).expect("an object unbox produces a value");
                builder.terminate(crate::ir::Terminator::Return { value: None });
            }
            let inst = function.instructions[0].clone();
            let layout = crate::codegen::frame::layout_for_function(&function, target, false, false, false);
            let mut emitter = Emitter::new(target);
            let mut data = DataSection::new();
            let mut shared = SharedCodegenState::default();
            {
                let mut ctx = FunctionContext::new(&module, &function, &mut emitter, &mut data,
                    &mut shared, layout, false, false, false, None);
                super::reference_arguments::lower_mixed_unbox(&mut ctx, &inst).unwrap();
            }
            let assembly = emitter.output();
            assert_eq!(assembly.matches("__rt_mixed_unbox").count(), 1, "{name}: {ownership:?}");
            assert_eq!(assembly.matches("__rt_incref").count(),
                usize::from(ownership != Ownership::Borrowed), "{name}: {ownership:?}\n{assembly}");
        }
    }
}

/// String unboxes preserve canonical null as the typed missing-string marker on every target.
#[test]
fn mixed_string_unbox_restores_null_sentinel_and_owns_payload_on_every_target() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let module = Module::new(target);
        let mut function = crate::codegen::shared_helper::helper_function("unbox_string", PhpType::Void);
        {
            let entry = function.entry;
            let mut builder = crate::ir::Builder::new(&mut function);
            builder.position_at_end(entry);
            builder.emit_with_effects(
                Op::MixedUnbox,
                vec![crate::codegen::shared_helper::helper_value()],
                None,
                crate::ir::IrType::Str,
                PhpType::Str,
                Ownership::Owned,
                Op::mixed_unbox_effects(&PhpType::Str),
                None,
            ).expect("a string unbox produces a value");
            builder.terminate(crate::ir::Terminator::Return { value: None });
        }
        let inst = function.instructions[0].clone();
        let layout = crate::codegen::frame::layout_for_function(
            &function, target, false, false, false,
        );
        let mut emitter = Emitter::new(target);
        let mut data = DataSection::new();
        let mut shared = SharedCodegenState::default();
        {
            let mut ctx = FunctionContext::new(
                &module, &function, &mut emitter, &mut data, &mut shared, layout,
                false, false, false, None,
            );
            super::reference_arguments::lower_mixed_unbox(&mut ctx, &inst).unwrap();
        }
        let assembly = emitter.output();
        assert!(assembly.contains("mixed_unbox_string_done"), "{name}: {assembly}");
        assert!(assembly.contains("__rt_str_persist"), "{name}: {assembly}");
        assert!(!assembly.contains("__rt_incref"), "{name}: {assembly}");
    }
}
