//! Purpose:
//! Verifies ownership roots for aggregate operands that survive throwing warning handlers.
//!
//! Called from:
//! - The AST-to-EIR unit test module.
//!
//! Key details:
//! - Direct and statically resolved callable aggregates share the same scoped unwind ledger.
//! - Checks all supported emitters without depending on host executable availability.

use crate::ir::{Immediate, Op, RuntimeCallTarget, RuntimeFnId};

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
