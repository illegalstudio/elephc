//! Purpose:
//! Verifies the backend's null-only parameter contract on every supported target.
//!
//! Called from:
//! - The library unit-test harness through `codegen::lower_inst`.
//!
//! Key details:
//! - Typed EIR input metadata isolates null guards from the parser's standalone-null gap.
//! - Both boolean values must be rejected rather than coerced to the null ABI word.

use super::*;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Target;
use crate::codegen::shared_state::SharedCodegenState;

/// Null-only guards accept the null tag and emit distinct true/false rejection diagnostics.
#[test]
fn literal_null_parameter_guard_emits_null_contract_all_targets() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let module = Module::new(target);
        let function = crate::codegen::shared_helper::helper_function("null_guard", PhpType::Void);
        let layout = crate::codegen::frame::layout_for_function(&function, target, false, false, false);
        let mut emitter = Emitter::new(target);
        let mut data = DataSection::new();
        let mut shared = SharedCodegenState::default();
        {
            let mut ctx = FunctionContext::new(
                &module, &function, &mut emitter, &mut data, &mut shared, layout,
                false, false, false, None,
            );
            super::direct_calls::emit_literal_declared_parameter_guards(
                &mut ctx,
                &[crate::codegen::shared_helper::helper_value()],
                &[("value".to_string(), PhpType::Void)],
                "NullGuard::accept",
            ).unwrap();
        }
        let assembly = emitter.output();
        assert!(assembly.contains("__rt_mixed_unbox"), "{name}: {assembly}");
        assert!(assembly.matches("literal_param_ok").count() >= 2,
            "{name}: null acceptance branch or destination missing");
        assert!(assembly.contains("null_param_false"), "{name}: false rejection missing");
        let messages = data.emit(target);
        for value in ["false", "true"] {
            assert!(messages.contains(&format!("must be of type null, {value} given")),
                "{name}: missing null-only diagnostic for {value}: {messages}");
        }
        assert!(!messages.contains("must be of type false"), "{name}: null guard became false-only");
    }
}
