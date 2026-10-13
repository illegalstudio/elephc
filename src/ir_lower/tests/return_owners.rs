//! Purpose:
//! Verifies ownership when boxed return expressions become concrete heap results.
//!
//! Called from:
//! - The AST-to-EIR unit test module.
//!
//! Key details:
//! - Extraction retains the returned payload before retiring an owned source box.
//! - Borrowed receivers are not released by return conversion on any supported target.

/// Reflection getters release their owned Mixed read after retaining the concrete return value.
#[test]
fn concrete_reflection_returns_retire_owned_source_boxes_on_all_targets() {
    use crate::ir::{Op, Ownership};

    let source = r#"<?php
class ReturnParent { public function value(): int { return 1; } }
class ReturnChild extends ReturnParent { public function value(): int { return 2; } }
$method = new ReflectionMethod(ReturnChild::class, "value");
$declaring = $method->getDeclaringClass();
$prototype = $method->getPrototype();
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        for name in ["ReflectionMethod::getDeclaringClass", "ReflectionMethod::getPrototype"] {
            let function = module.class_methods.iter().find(|function| function.name == name)
                .expect("reachable Reflection getter");
            let (index, extraction) = function.instructions.iter().enumerate().find(|(_, inst)| {
                inst.op == Op::RuntimeCall && inst.operands.len() == 1
                    && function.value(inst.operands[0]).unwrap().php_type == crate::types::PhpType::Mixed
            }).expect("boxed concrete-return extraction");
            let source = extraction.operands[0];
            let result = extraction.result.expect("owned concrete result");
            assert_eq!(function.value(source).unwrap().ownership, Ownership::Owned, "{target}: {name}");
            assert_eq!(function.value(result).unwrap().ownership, Ownership::Owned, "{target}: {name}");
            let receiver = function.instructions.iter().find(|inst| {
                inst.op == Op::PropGet && inst.result == Some(source)
            }).expect("owned property read").operands[0];
            let releases = function.instructions.iter().enumerate().filter(|(_, inst)| {
                inst.op == Op::Release && inst.operands == [source]
            }).collect::<Vec<_>>();
            assert_eq!(releases.len(), 1, "{target}: {name}: source box must retire once");
            assert!(releases[0].0 > index, "{target}: {name}: extraction precedes box retirement");
            assert!(!function.instructions.iter().any(|inst| {
                inst.op == Op::Release && inst.operands == [result]
            }), "{target}: {name}: return payload transfers to the caller");
            assert!(!function.instructions.iter().any(|inst| {
                inst.op == Op::Release && inst.operands == [receiver]
            }), "{target}: {name}: the borrowed receiver stays with the caller");
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}
