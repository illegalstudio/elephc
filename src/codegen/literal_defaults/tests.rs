//! Purpose:
//! Verifies recursive literal-default planning and target-independent ownership emission.
//!
//! Called from:
//! - The literal-default module's Rust test harness.
//!
//! Key details:
//! - All supported targets emit the same nested container graph without assembling or linking it.

use super::*;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Target;
use crate::codegen::shared_state::SharedCodegenState;
use crate::ir::Module;
use crate::span::Span;

/// Wraps one default expression in an irrelevant source span for planning tests.
fn expression(kind: ExprKind) -> Expr {
    Expr::new(kind, Span::new(1, 1))
}

/// Mixed defaults preserve nested indexed/hash shapes instead of rejecting their child expressions.
#[test]
fn nested_mixed_literal_defaults_preserve_container_shapes() {
    let nested = ExprKind::ArrayLiteral(vec![
        expression(ExprKind::IntLiteral(1)),
        expression(ExprKind::ArrayLiteralAssoc(vec![(
            expression(ExprKind::StringLiteral("leaf".to_string())),
            expression(ExprKind::ArrayLiteral(vec![expression(ExprKind::BoolLiteral(true))])),
        )])),
        expression(ExprKind::ArrayLiteral(Vec::new())),
    ]);
    let default = literal_default_value("test", &PhpType::Mixed, &nested, "test").unwrap();
    let LiteralDefaultValue::BoxedArray { elem_type, elements } = default else {
        panic!("Mixed indexed defaults must own a boxed array");
    };
    assert_eq!(elem_type, PhpType::Mixed);
    assert!(matches!(&elements[0], LiteralArrayElement::Int(1)));
    let LiteralArrayElement::AssocArray { value_type, entries } = &elements[1] else {
        panic!("explicit keys must select hash storage");
    };
    assert_eq!(*value_type, PhpType::Mixed);
    assert!(matches!(&entries[0].value, LiteralArrayElement::Array { .. }));
    assert!(matches!(&elements[2], LiteralArrayElement::Array { elements, .. } if elements.is_empty()));
}

/// Recursion must not silently accept nested arrays in scalar-only element storage.
#[test]
fn nested_literal_defaults_reject_incompatible_scalar_storage() {
    let nested = ExprKind::ArrayLiteral(vec![expression(ExprKind::ArrayLiteral(Vec::new()))]);
    assert!(literal_default_value(
        "test", &PhpType::Array(Box::new(PhpType::Int)), &nested, "test",
    ).is_err());
}

/// Nested defaults stamp typed arrays and transfer each allocated container owner on every target.
#[test]
fn nested_literal_defaults_emit_owned_containers_on_all_targets() {
    let entries = vec![LiteralAssocEntry {
        key: LiteralArrayKey::Str("tree".to_string()),
        value: LiteralArrayElement::Array {
            elem_type: PhpType::Mixed,
            elements: vec![
                LiteralArrayElement::Array {
                    elem_type: PhpType::Float,
                    elements: vec![LiteralArrayElement::Float(2.5)],
                },
                LiteralArrayElement::AssocArray { value_type: PhpType::Mixed, entries: Vec::new() },
                LiteralArrayElement::Array { elem_type: PhpType::Mixed, elements: Vec::new() },
            ],
        },
    }];
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let module = Module::new(target);
        let mut emitter = Emitter::new(target);
        let mut data = DataSection::new();
        let mut shared = SharedCodegenState::default();
        crate::codegen::shared_helper::emit_shared_helper(
            &module, &mut emitter, &mut data, &mut shared, false,
            "nested_defaults", PhpType::Mixed, "nested default fixture",
            |ctx| emit_boxed_assoc_array_literal_to_result(ctx, &PhpType::Mixed, &entries),
        ).unwrap();
        let assembly = emitter.output();
        assert_eq!(assembly.matches("__rt_array_new").count(), 3, "{name}");
        assert_eq!(assembly.matches("__rt_hash_new").count(), 2, "{name}");
        assert_eq!(assembly.matches("__rt_array_push_refcounted").count(), 3, "{name}");
        assert_eq!(assembly.matches("__rt_hash_set").count(), 1, "{name}");
        let float_tag = match target.arch {
            Arch::AArch64 => "mov x11, #2",
            Arch::X86_64 => "mov r12, 2",
        };
        assert!(assembly.contains(float_tag), "{name}: missing float element metadata");
        assert!(assembly.contains("__rt_decref"), "{name}: missing transferred-owner cleanup");
    }
}
