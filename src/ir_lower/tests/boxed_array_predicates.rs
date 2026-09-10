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
                let Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(target))) = inst.immediate else { continue; };
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
