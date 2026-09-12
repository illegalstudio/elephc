//! Purpose:
//! Verifies array operand roots across throwing validation and warning handlers.
//!
//! Called from:
//! - The AST-to-EIR unit test module.
//!
//! Key details:
//! - Direct and statically resolved callable aggregates share the same scoped unwind ledger.
//! - Checks all supported emitters without depending on host executable availability.

use crate::ir::{Immediate, Op, RuntimeCallTarget, RuntimeFnId};

/// Merge descriptor extraction owners survive validation throws on every supported target.
#[test]
fn merge_descriptor_extracted_operands_have_scoped_unwind_roots_on_all_targets() {
    let source = r#"<?php
function mergeOperandDescriptor(callable $callback, array $left, array $right): mixed {
    return $callback($left, $right);
}
$callback = array_merge(...);
echo count(mergeOperandDescriptor($callback, [$argc], ["key" => $argc]));
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let mut module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let signature = crate::types::first_class_callable_builtin_sig("array_merge").unwrap();
        let wrapper = crate::ir_lower::lower_array_merge_callable(
            &mut module, "test_merge_operand_roots", &signature, false,
        );
        crate::ir::validate_function(&wrapper).unwrap();
        let (call_index, call) = wrapper.instructions.iter().enumerate().find(|(_, inst)| {
            matches!(inst.immediate, Some(Immediate::RuntimeCall(
                RuntimeCallTarget::Function(RuntimeFnId::ArrayMerge)
                | RuntimeCallTarget::ProfiledFunction { target: RuntimeFnId::ArrayMerge, .. }
            )))
        }).expect("wrapper must lower the two-input merge");
        assert_eq!(call.operands.len(), 2, "{target}");
        let mut roots = Vec::new();
        for operand in &call.operands {
            let store = wrapper.instructions[..call_index].iter().find(|inst| {
                inst.op == Op::StoreLocal && inst.operands.as_slice() == [*operand]
            }).expect("each extracted argument needs its own owner slot");
            let Some(Immediate::LocalSlot(slot)) = store.immediate else { panic!("root slot"); };
            assert!(!roots.contains(&slot), "{target}: inputs cannot share an owner slot");
            roots.push(slot);
            assert_eq!(wrapper.instructions[..call_index].iter().filter(|inst| {
                inst.op == Op::PushCallOperandOwner && inst.immediate == store.immediate
            }).count(), 1, "{target}: register each extraction before validation");
            let cleanup = &wrapper.instructions[call_index + 1..];
            let pop = cleanup.iter().position(|inst| {
                inst.op == Op::PopCallOperandOwner && inst.immediate == store.immediate
            }).expect("detach each scoped root after successful validation");
            assert_eq!(cleanup[pop + 1].op, Op::ReleaseLocalSlot, "{target}");
            assert_eq!(cleanup[pop + 1].immediate, store.immediate, "{target}");
            assert_eq!(cleanup.iter().filter(|inst| {
                inst.op == Op::ReleaseLocalSlot && inst.immediate == store.immediate
            }).count(), 1, "{target}: retire each extraction exactly once");
        }
        module.functions.push(wrapper);
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(asm.contains("__rt_cleanup_call_operand_owner"), "{target}");
        assert!(asm.contains("__rt_array_merge_boxed"), "{target}");
    }
}

/// Temporary aggregate inputs receive balanced scoped roots on every supported target.
#[test]
fn aggregate_warning_calls_root_temporary_sources_on_all_targets() {
    let source = r#"<?php
function ownedAggregateValues(): array { return [str_repeat("bad", 8)]; }
array_sum(ownedAggregateValues());
array_product(array: ownedAggregateValues());
call_user_func("array_sum", ownedAggregateValues());
$callback = array_product(...);
$callback(ownedAggregateValues());
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let mut calls = 0;
        for function in &module.functions {
            for (index, inst) in function.instructions.iter().enumerate() {
                if !matches!(inst.immediate, Some(Immediate::RuntimeCall(
                    RuntimeCallTarget::Function(RuntimeFnId::ArraySum | RuntimeFnId::ArrayProduct)
                    | RuntimeCallTarget::ProfiledFunction {
                        target: RuntimeFnId::ArraySum | RuntimeFnId::ArrayProduct, ..
                    }
                ))) { continue; }
                calls += 1;
                let push = function.instructions[..index].iter().rev()
                    .find(|candidate| candidate.op == Op::PushCallOperandOwner)
                    .expect("aggregate source needs a scoped owner before warning dispatch");
                let cleanup = &function.instructions[index + 1..];
                assert_eq!(cleanup[0].op, Op::PopCallOperandOwner, "{target}");
                assert_eq!(cleanup[0].immediate, push.immediate, "{target}");
                assert_eq!(cleanup[1].op, Op::ReleaseLocalSlot, "{target}");
                assert_eq!(cleanup[1].immediate, push.immediate, "{target}");
            }
        }
        assert_eq!(calls, 4, "{target}");
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(asm.contains("__rt_cleanup_call_operand_owner"), "{target}");
    }
}
