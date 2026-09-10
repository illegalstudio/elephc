//! Purpose:
//! Verifies storage-neutral array predicate lowering and ownership on every target.
//!
//! Called from:
//! - The AST-to-EIR unit suite.
//!
//! Key details:
//! - Keyed callbacks use the descriptor runtime, and find results own independent Mixed cells.

use crate::codegen::platform::Target;
use crate::ir::{Effects, Immediate, RuntimeCallTarget, RuntimeFnId};
use crate::types::PhpType;
use std::path::Path;

/// All supported ABIs use boxed value/key dispatch and retain the typed result and effect contracts.
#[test]
fn boxed_array_predicates_use_keyed_owned_runtime_on_every_target() {
    let source = r#"<?php
function targetArrayPredicates(array $values): void {
    $found = array_find($values, static fn(mixed $value, mixed $key): bool => $key === "item");
    echo gettype($found), array_any($values, static fn(mixed $value): bool => true);
    echo array_all($values, static fn(mixed $value, mixed $key): bool => true);
}

targetArrayPredicates(["item" => false]);
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let mut calls = 0;
        for function in &module.functions {
            for inst in &function.instructions {
                // String-callback predicates carry a strict-PHP visibility profile, so backend-neutral
                // lowering upgrades them to `ProfiledFunction`; accept both spellings of the same target.
                let target = match inst.immediate {
                    Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(target)))
                    | Some(Immediate::RuntimeCall(RuntimeCallTarget::ProfiledFunction { target, .. })) => target,
                    _ => continue,
                };
                if !matches!(target, RuntimeFnId::ArrayFind | RuntimeFnId::ArrayAny | RuntimeFnId::ArrayAll) { continue; }
                calls += 1;
                assert!(inst.effects.contains(Effects::MAY_THROW | Effects::REFCOUNT_OP), "{name}");
                assert_eq!(inst.result_php_type.codegen_repr(),
                    if target == RuntimeFnId::ArrayFind { PhpType::Mixed } else { PhpType::Bool }, "{name}");
            }
        }
        assert_eq!(calls, 3, "{name}");
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert_eq!(asm.matches("__rt_array_predicate_boxed").count(), 3, "{name}");
        assert!(!asm.contains("__rt_array_find_any_all"), "{name}: no scalar-only predicate path");
    }
}

/// Every ABI materializes filter defaults and returns an owned boxed array for preserved keys.
#[test]
fn boxed_array_filter_defaults_and_callback_modes_lower_on_every_target() {
    let source = r#"<?php
function targetArrayFilter(array $values, int $mode): void {
    $plain = array_filter($values);
    $null = array_filter($values, null);
    $callback = array_filter($values, static fn(mixed $value, mixed $key = null): bool => true, $mode);
    echo count($plain), count($null), count($callback);
}
targetArrayFilter(["item" => false], 1);
$filter = array_filter(...);
$first = $filter([0, "kept"]);
$second = call_user_func("array_filter", ["key" => true]);
$third = $filter(array: [1 => "kept"], callback: null);
echo count($first), count($second), count($third);
"#;
    assert_eq!(RuntimeFnId::ArrayFilter.result_ownership(),
        crate::builtins::semantics::BuiltinResultOwnership::Fresh);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        // `array_filter` inspects a string-callable operand, so its call site carries the strict-PHP
        // profile and lowers as `ProfiledFunction`; match both spellings of the same runtime target.
        let calls: Vec<_> = module.functions.iter().flat_map(|function| &function.instructions)
            .filter(|inst| matches!(inst.immediate,
                Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(RuntimeFnId::ArrayFilter)))
                | Some(Immediate::RuntimeCall(RuntimeCallTarget::ProfiledFunction {
                    target: RuntimeFnId::ArrayFilter, ..
                }))))
            .collect();
        assert_eq!(calls.len(), 6, "{name}");
        for inst in calls {
            assert_eq!(inst.operands.len(), 3, "{name}: callback and mode defaults must be materialized");
            assert_eq!(inst.result_php_type.codegen_repr(), PhpType::Mixed, "{name}");
            assert!(inst.effects.contains(Effects::MAY_THROW | Effects::REFCOUNT_OP), "{name}");
        }
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        // The first-class descriptor also emits one synthetic wrapper directly into assembly.
        // It is not part of the six source call sites in the original EIR module.
        assert_eq!(asm.matches("__rt_array_predicate_boxed").count(), 7,
            "{name}: six source calls and one first-class descriptor wrapper");
        assert!(!asm.contains("__rt_array_filter"), "{name}: no scalar-only filter runtime");
    }
}
