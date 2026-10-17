//! Purpose:
//! Checks the shared native catalog initialization boundary on all supported targets.
//!
//! Called from:
//! - The compiler unit test harness.
//!
//! Key details:
//! - A private aligned token identifies the module, with fresh context allocation per frame.

use super::*;
use crate::ir::IrType;
use crate::codegen::data_section::DataSection;
use crate::codegen::platform::Target;
use crate::codegen::emit::Emitter;
use crate::codegen::shared_helper::helper_function;
use crate::codegen::shared_state::SharedCodegenState;
use crate::codegen::frame;

/// Emits two activation initializers sharing one module identity on each target.
#[test]
fn native_catalog_initialization_is_target_aware_and_module_private() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let module = Module::new(target);
        let mut function = helper_function("catalog_probe", PhpType::Void);
        function.add_local(Some("context".into()), IrType::I64, PhpType::Int, LocalKind::EvalContext);
        let layout = frame::layout_for_function(&function, target, false, false, false);
        let mut emitter = Emitter::new(target);
        let mut data = DataSection::new();
        let mut shared = SharedCodegenState::default();
        let mut context = FunctionContext::new(
            &module, &function, &mut emitter, &mut data, &mut shared, layout,
            false, false, false, None,
        );
        ensure_eval_context(&mut context).unwrap();
        ensure_eval_context(&mut context).unwrap();
        let asm = emitter.output();
        let call = if matches!(target.arch, Arch::X86_64) { "call" } else { "bl" };
        for symbol in ["__elephc_eval_context_new_with_native_catalog",
            "__elephc_eval_context_native_catalog_ready", "__elephc_eval_context_publish_native_catalog"] {
            let instruction = format!("{call} {}", target.extern_symbol(symbol));
            assert_eq!(asm.lines().filter(|line| line.trim() == instruction).count(), 2, "{name}: {symbol}");
        }
        assert!(asm.contains("eval_native_catalog_registered"), "{name}");
        let section = data.emit(target);
        assert_eq!(section.matches("L__elephc_eval_native_catalog:").count(), 1, "{name}");
        assert!(section.contains(".p2align 3\nL__elephc_eval_native_catalog:\n    .quad 0"), "{name}");
        assert!(!section.contains(".globl L__elephc_eval_native_catalog"), "{name}");
    }
}
