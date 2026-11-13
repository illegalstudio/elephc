//! Purpose:
//! Verifies concrete scalar hash foreach values at the EIR/backend boundary.
//!
//! Called from:
//! - The AST-to-EIR unit test harness.
//!
//! Key details:
//! - Every supported target emits the same typed append and return contract.

use std::path::Path;

use crate::codegen::platform::Target;
use crate::ir::Op;
use crate::types::PhpType;

/// Builtin-derived hash bindings retain their runtime tags until a typed builder consumes them.
#[test]
fn scalar_hash_foreach_flip_values_stay_mixed_on_all_targets() {
    let source = r#"<?php
function collectFlipped(bool $reindex): void {
    $source = ['b' => 2, 'a' => 1];
    if ($reindex) { sort($source); }
    foreach (array_flip($source) as $value) { echo gettype($value); }
}
"#;
    for target_name in [
        "macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64",
    ] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(target_name).unwrap(),
        );
        let function = module.functions.iter().find(|f| f.name == "collectFlipped").unwrap();
        let flip = function.instructions.iter().find(|i| {
            i.immediate == Some(crate::ir::Immediate::RuntimeCall(
                crate::ir::RuntimeCallTarget::Function(crate::ir::RuntimeFnId::ArrayFlip)))
        }).expect("fixture must call array_flip");
        assert!(matches!(flip.result_php_type, PhpType::AssocArray { .. }), "{target_name}");
        let current = function.instructions.iter().find(|i| i.op == Op::IterCurrentValue).unwrap();
        assert_eq!(current.result_php_type, PhpType::Mixed, "{target_name}");
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|e| panic!("{target_name}: {e}"));
    }
}

/// Concrete hash values preserve the builder layout and emit successfully on all five targets.
#[test]
fn scalar_hash_foreach_builders_emit_on_all_targets() {
    for (name, ty) in [
        ("int", PhpType::Int),
        ("float", PhpType::Float),
        ("bool", PhpType::Bool),
        ("string", PhpType::Str),
        ("callable", PhpType::Callable),
    ] {
        let source = format!(r#"<?php
function collect(array<string, {name}> $source, string $wanted): array<{name}> {{
    array<{name}> $kept = [];
    foreach ($source as $key => $value) {{
        if ($key === $wanted) {{ $kept[] = $value; }}
    }}
    return $kept;
}}
"#);
        for target_name in [
            "macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64",
        ] {
            let module = super::lower_source_at_for_target(
                &source, Path::new("main.php"), Path::new("."),
                Target::parse(target_name).unwrap(),
            );
            let function = module.functions.iter().find(|f| f.name == "collect").unwrap();
            let append = function.instructions.iter().find(|i| i.op == Op::ArrayPush)
                .expect("fixture must append the foreach value");
            assert_eq!(function.value(append.operands[1]).unwrap().php_type, ty,
                "{name} on {target_name}");
            assert!(!function.instructions.iter().any(|i| i.op == Op::ArrayToMixed),
                "{name} on {target_name}: the pinned builder must not widen");
            crate::codegen::generate_user_asm_from_ir(&module, false, false)
                .unwrap_or_else(|e| panic!("{name} on {target_name}: {e}"));
        }
    }
}
