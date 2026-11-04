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

/// Renaming and deduplicating eval invokers cannot strand their shared owner callbacks.
#[test]
fn eval_invoker_owner_visitors_survive_body_deduplication_on_every_target() {
    let length = crate::types::first_class_callable_builtin_sig("strlen").unwrap();
    let kind = crate::types::first_class_callable_builtin_sig("gettype").unwrap();
    assert_eq!(length.params.len(), kind.params.len());
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let module = Module::new(target);
        let mut emitter = Emitter::new(target);
        let mut data = DataSection::new();
        let mut state = EvalCallableEmitState::new();
        let mut labels = Vec::new();
        for signature in [&length, &kind, &length] {
            labels.push(emit_eval_runtime_callable_invoker_inline(
                &module, &mut emitter, &mut data, &mut state, signature, &[], false, None));
        }
        assert_eq!(labels[0], labels[2], "{name}: identical invokers remain deduplicated");
        assert_ne!(labels[0], labels[1], "{name}: distinct decoding contracts retain separate bodies");
        let assembly = emitter.output();
        assert!(!assembly.contains(PENDING_INVOKER_LABEL), "{name}: unresolved placeholder: {assembly}");
        let callbacks = assembly.split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .filter(|symbol| symbol.ends_with("__gc_argument_owners")
                || symbol.ends_with("__gc_argument_owners_cleanup"))
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(callbacks.len(), 2, "{name}: all invokers must share one callback pair: {assembly}");
        for callback in callbacks {
            assert_eq!(assembly.matches(&format!("{callback}:\n")).count(), 1,
                "{name}: every referenced callback needs one real definition: {assembly}");
        }
        assert_eq!(assembly.matches("__rt_gc_frame_owner_slot").count(), length.params.len() + 2,
            "{name}: callbacks are shared without losing any owner slots: {assembly}");
    }
}

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
