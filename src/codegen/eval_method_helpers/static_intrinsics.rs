//! Purpose:
//! Verifies that eval dispatch can reach reviewed native static intrinsic wrappers.
//!
//! Called from:
//! - Eval method helper unit tests on every supported target.
//!
//! Key details:
//! - Fiber suspension retains the ordinary method signature and private result-owner marker.
//! - The runtime wrapper clones the consumed value and omits the hidden called-class word.

use super::*;
use crate::codegen::platform::Target;

/// Generic eval suspension reaches the same reviewed wrapper as native coroutine callables.
#[test]
fn eval_fiber_suspend_dispatch_selects_the_owned_wrapper_on_every_target() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let module = crate::ir_lower::tests::lower_source_at_for_target(
            "<?php function nativeEvalSuspend(): void { Fiber::suspend(7); }",
            std::path::Path::new("main.php"), std::path::Path::new("."), target,
        );
        assert!(module.required_runtime_features.fiber, "{name}: select the native coroutine runtime");
        let slots = collect_eval_static_method_slots(&module);
        let slot = slots.iter().find(|slot| slot.class_name == "Fiber" && slot.method == "suspend")
            .unwrap_or_else(|| panic!("{name}: expose the native static suspension entry to eval"));
        assert_eq!(slot.entry_symbol, static_method_symbol("Fiber", "suspend"));
        assert_eq!(slot.params, [PhpType::Mixed]);
        assert!(slot.return_uses_marker && !slot.by_ref_return, "{name}: adopt the returned boxed owner");
        let mut emitter = Emitter::new(target);
        crate::codegen::emit_intrinsic_method_wrappers(&module, &mut emitter);
        let assembly = emitter.output();
        let entry = format!("{}:", slot.entry_symbol);
        let body = assembly.split_once(&entry).unwrap().1;
        let before_suspend = body.split_once("__rt_fiber_suspend").unwrap().0;
        assert!(before_suspend.contains("__rt_mixed_clone"), "{name}: consume an independent suspension value");
    }
}
