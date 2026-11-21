//! Purpose:
//! Verifies ownership when boxed return expressions become concrete heap results.
//!
//! Called from:
//! - The AST-to-EIR unit test module.
//!
//! Key details:
//! - Extraction retains the returned payload before retiring an owned source box.
//! - Borrowed receivers are not released by return conversion on any supported target.

/// Opaque array returns validate their tags without selecting indexed-only storage on any target.
#[test]
fn declared_php_array_returns_emit_runtime_guard_on_all_targets() {
    let source = r#"<?php
        function forwardArray(mixed $frame): array { return $frame['args']; }
        echo json_encode(forwardArray(['args' => ['label' => 42]]));
    "#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let printed = crate::ir::print_module(&module);
        assert!(printed.contains("return.array_rejected"), "{target}: tag guard must be present");
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}

/// Both raw property cells and local payload views retain a checked array boundary on every target.
#[test]
fn declared_php_array_reference_returns_emit_guards_on_all_targets() {
    let source = r#"<?php
        function &arrayReference(mixed &$value): array { return $value; }
        class ArrayReferenceOwner {
            public mixed $value;
            public function __construct(mixed $value) { $this->value = $value; }
            public function &get(): array { return $this->value; }
        }
        $value = ['label' => 1];
        $alias =& arrayReference($value);
        $owner = new ArrayReferenceOwner(['label' => 2]);
        $property =& $owner->get();
    "#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let printed = crate::ir::print_module(&module);
        assert_eq!(printed.matches("return.array_rejected:").count(), 2, "{target}: each reference needs a value guard");
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}

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
            // The extraction is the single-operand instruction that unboxes the returned payload;
            // argument snapshots of earlier runtime calls are not return extractions.
            let returned = function.blocks.iter().find_map(|block| match block.terminator {
                Some(crate::ir::Terminator::Return { value: Some(value) }) => Some(value),
                _ => None,
            }).expect("concrete object return");
            let (index, extraction) = function.instructions.iter().enumerate().find(|(_, inst)| {
                inst.result == Some(returned) && inst.operands.len() == 1
                    && function.value(inst.operands[0]).unwrap().php_type == crate::types::PhpType::Mixed
            }).expect("boxed concrete-return extraction");
            let source = extraction.operands[0];
            let result = extraction.result.expect("owned concrete result");
            assert_eq!(function.value(source).unwrap().ownership, Ownership::Owned, "{target}: {name}");
            assert_eq!(function.value(result).unwrap().ownership, Ownership::Owned, "{target}: {name}");
            // The source box is an owned property read, directly or through the merged local
            // that carries it out of a conditional build. The receiver is the method's `this`.
            let receivers = function.instructions.iter().filter(|inst| {
                inst.op == Op::LoadLocal
                    && inst.immediate == Some(crate::ir::Immediate::LocalSlot(crate::ir::LocalSlotId::from_raw(0)))
            }).filter_map(|inst| inst.result).collect::<Vec<_>>();
            assert!(!receivers.is_empty(), "{target}: {name}: the method loads its receiver");
            assert!(function.instructions.iter().any(|inst| inst.op == Op::PropGet),
                "{target}: {name}: the boxed return comes from an owned property read");
            let releases = function.instructions.iter().enumerate().filter(|(_, inst)| {
                inst.op == Op::Release && inst.operands == [source]
            }).collect::<Vec<_>>();
            assert_eq!(releases.len(), 1, "{target}: {name}: source box must retire once");
            assert!(releases[0].0 > index, "{target}: {name}: extraction precedes box retirement");
            assert!(!function.instructions.iter().any(|inst| {
                inst.op == Op::Release && inst.operands == [result]
            }), "{target}: {name}: return payload transfers to the caller");
            assert!(!function.instructions.iter().any(|inst| {
                inst.op == Op::Release && receivers.iter().any(|receiver| inst.operands == [*receiver])
            }), "{target}: {name}: the borrowed receiver stays with the caller");
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}
