//! Purpose:
//! Verifies eval-to-native callable descriptors preserve generated-function stack arguments.
//!
//! Called from:
//! - The eval callable helper library tests.
//!
//! Key details:
//! - ARM64 callees require a caller gap after overflow slots are materialized.
//! - Both receiver-bound and static descriptors use the same target ABI contract.

use super::*;
use crate::codegen_support::platform::{Arch, Target};

/// Eval callable arrays include the native static factory even when it has no EIR body.
#[test]
fn eval_static_factory_descriptor_cases_exist_on_every_target() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let module = crate::ir_lower::tests::lower_source_at_for_target(
            "<?php $factory = SplFixedArray::fromArray(...); $factory([1]);",
            std::path::Path::new("main.php"), std::path::Path::new("."), target,
        );
        let mut emitter = Emitter::new(target);
        let mut data = DataSection::new();
        let cases = eval_static_method_callable_cases(&module, &mut emitter, &mut data,
            &mut EvalCallableEmitState::new());
        assert!(cases.iter().any(|case| case.class_name == "SplFixedArray"
            && case.method_name.eq_ignore_ascii_case("fromArray")), "{name}: native factory case");
        assert!(emitter.output().contains(&static_method_symbol("SplFixedArray", "fromarray")),
            "{name}: the descriptor enters the reviewed native method wrapper");
    }
}

/// Eval descriptors reserve the ARM64 caller gap for every method with overflow arguments.
#[test]
fn eval_method_descriptors_preserve_overflow_stack_arguments_on_every_target() {
    let visible = vec![PhpType::Int; 9];
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        for is_static in [false, true] {
            let mut emitter = Emitter::new(target);
            let symbol = if is_static {
                emit_eval_static_method_descriptor_entry_wrapper_body(&mut emitter, "StackTarget", "sum", &visible, 1);
                static_method_symbol("StackTarget", "sum")
            } else {
                emit_eval_instance_method_descriptor_entry_wrapper_body(&mut emitter, "StackTarget", "sum", &visible);
                method_symbol("StackTarget", "sum")
            };
            let instruction = if target.arch == Arch::AArch64 { "bl" } else { "call" };
            let assembly = emitter.output();
            let (before, after) = assembly.split_once(&format!("{instruction} {symbol}")).unwrap();
            if target.arch == Arch::AArch64 {
                assert!(before.trim_end().ends_with("sub sp, sp, #16"), "{name} static={is_static}: {assembly}");
                assert!(after.trim_start().starts_with("add sp, sp, #16"), "{name} static={is_static}: {assembly}");
            } else {
                assert!(!before.trim_end().ends_with("sub rsp, 16"), "{name}: {assembly}");
            }
        }
    }
}
