//! Purpose:
//! Checks the shared native catalog initialization boundary on all supported targets.
//!
//! Called from:
//! - The compiler unit test harness.
//!
//! Key details:
//! - A private aligned token identifies the module, with fresh context allocation per frame.
//! - The registration transaction itself lives in the shared catalog helper, so the
//!   caller is checked for the hand-off and the helper body for the guard and the
//!   single publishing step.

use super::*;
use crate::ir::IrType;
use crate::codegen::data_section::DataSection;
use crate::codegen::platform::Target;
use crate::codegen::emit::Emitter;
use crate::codegen::shared_helper::helper_function;
use crate::codegen::shared_state::SharedCodegenState;
use crate::codegen::frame;

/// Counts the direct calls to `symbol` on the target's own call encoding.
fn count_calls(asm: &str, target: Target, symbol: &str) -> usize {
    let call = if matches!(target.arch, Arch::X86_64) { "call" } else { "bl" };
    let instruction = format!("{call} {}", target.extern_symbol(symbol));
    asm.lines().filter(|line| line.trim() == instruction).count()
}

/// Emits two activation initializers sharing one module identity on each target.
#[test]
fn native_catalog_initialization_is_target_aware_and_module_private() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let mut module = Module::new(target);
        module.required_runtime_features.eval_bridge = true;
        let mut attributed = helper_function("FunctionAttributeProbe", PhpType::Void);
        attributed.attribute_names = vec!["RepeatedTag".into(), "RepeatedTag".into()];
        attributed.attribute_args = vec![Some(Vec::new()), Some(Vec::new())];
        module.functions.push(attributed);
        let mut function = helper_function("catalog_probe", PhpType::Void);
        function.add_local(Some("context".into()), IrType::I64, PhpType::Int, LocalKind::EvalContext);
        let layout = frame::layout_for_function(&function, target, false, false, false);
        let mut emitter = Emitter::new(target);
        let mut data = DataSection::new();
        let mut shared = SharedCodegenState::default();
        {
            let mut context = FunctionContext::new(
                &module, &function, &mut emitter, &mut data, &mut shared, layout,
                false, false, false, None,
            );
            let attributes = eval_native_member_attribute_registrations(&context);
            assert_eq!(attributes.len(), 2, "{name}: repeated function attributes");
            for attribute in attributes {
                let record = super::property_attribute_registration::eval_native_member_attribute_record(&attribute);
                assert_eq!(record[0], NATIVE_MEMBER_ATTRIBUTE_FUNCTION, "{name}: function owner tag");
                let owner_len = u32::from_le_bytes(record[1..5].try_into().unwrap()) as usize;
                assert_eq!(&record[5..5 + owner_len], b"FunctionAttributeProbe", "{name}: raw function key");
            }
            ensure_eval_context(&mut context).unwrap();
            ensure_eval_context(&mut context).unwrap();
        }
        let asm = emitter.output();
        // Each activation allocates a handle ...
        assert_eq!(
            count_calls(&asm, target, "__elephc_eval_context_new_with_native_catalog"),
            2,
            "{name}: handle allocation"
        );
        // ... and hands the catalog off to the shared helper.
        assert_eq!(
            asm.lines()
                .filter(|line| line.trim()
                    == format!("{} {}", if matches!(target.arch, Arch::X86_64) { "call" } else { "bl" },
                        crate::codegen::eval_inventory_helper::EVAL_INVENTORY_LABEL))
                .count(),
            2,
            "{name}: catalog helper hand-off"
        );
        for symbol in [
            "__elephc_eval_context_native_catalog_ready",
            "__elephc_eval_context_publish_native_catalog",
        ] {
            assert!(
                !asm.contains(&target.extern_symbol(symbol)),
                "{name}: {symbol} belongs to the helper, not the caller"
            );
        }

        // The helper body owns the readiness guard and the single publish step.
        let mut helper_emitter = Emitter::new(target);
        let mut helper_data = DataSection::new();
        let mut helper_shared = SharedCodegenState::default();
        crate::codegen::eval_inventory_helper::emit_eval_inventory_helper(
            &module,
            &mut helper_emitter,
            &mut helper_data,
            &mut helper_shared,
            false,
        )
        .unwrap();
        let helper_asm = helper_emitter.output();
        assert_eq!(count_calls(&helper_asm, target, "__elephc_eval_register_native_member_attribute"), 2,
            "{name}: every repeated function attribute must be registered");
        for symbol in [
            "__elephc_eval_context_native_catalog_ready",
            "__elephc_eval_context_publish_native_catalog",
        ] {
            assert_eq!(count_calls(&helper_asm, target, symbol), 1, "{name}: {symbol}");
        }
        assert!(helper_asm.contains("eval_native_catalog_registered"), "{name}");

        let section = data.emit(target);
        assert_eq!(section.matches("L__elephc_eval_native_catalog:").count(), 1, "{name}");
        assert!(section.contains(".p2align 3\nL__elephc_eval_native_catalog:\n    .quad 0"), "{name}");
        assert!(!section.contains(".globl L__elephc_eval_native_catalog"), "{name}");
    }
}
