//! Purpose:
//! Verifies PHP arithmetic and array projections in shared default metadata.
//!
//! Called from:
//! - The const-default resolver unit tests.
//!
//! Key details:
//! - Exact integer division and fitting powers stay integers on every supported target.
//! - Fractional, floating and overflow cases preserve PHP's float representation.

use super::*;
use crate::codegen::platform::Target;
use crate::span::Span;

/// Scalar results retain enum evaluation effects so compact ABI consumers cannot erase them.
#[test]
fn enum_condition_defaults_retain_materialization_on_all_targets() {
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let mut module = Module::new(Target::parse(target).unwrap());
        module.enum_infos.insert("E".into(), crate::types::EnumInfo {
            backing_type: None,
            cases: vec![crate::types::EnumCaseInfo {
                name: "A".into(), value: None, attribute_names: Vec::new(), attribute_args: Vec::new(),
            }],
        });
        let expr = |kind| Expr::new(kind, Span::dummy());
        let expression = expr(ExprKind::Ternary {
            condition: Box::new(expr(ExprKind::ScopedConstantAccess {
                receiver: StaticReceiver::Named("E".into()), name: "A".into(),
            })),
            then_expr: Box::new(expr(ExprKind::IntLiteral(7))),
            else_expr: Box::new(expr(ExprKind::IntLiteral(9))),
        });
        let context = ConstDefaultContext { module: &module, current_class: None };
        let value = resolve_const_default(&expression, &context).unwrap();
        assert!(const_default_contains_enum(&value), "{target}");
        assert!(without_enum_effects(value.clone()) == const_default_int_value(7), "{target}");
        assert!(resolved_literal_default(value).is_none(), "{target}: an enum read became a pure literal");
    }
}

/// Constant offset folding preserves normalized keys, overwrites and the next append index.
#[test]
fn array_offset_defaults_preserve_key_semantics_on_all_targets() {
    use crate::parser::ast::ArrayEntry;
    let expr = |kind| Expr::new(kind, Span::dummy());
    let array = ExprKind::ArrayLiteralMixed(vec![
        ArrayEntry::Keyed(expr(ExprKind::IntLiteral(2)), expr(ExprKind::IntLiteral(10))),
        ArrayEntry::Value(expr(ExprKind::IntLiteral(20))),
        ArrayEntry::Keyed(expr(ExprKind::StringLiteral("3".into())), expr(ExprKind::IntLiteral(30))),
        ArrayEntry::Value(expr(ExprKind::IntLiteral(40))),
        ArrayEntry::Keyed(expr(ExprKind::StringLiteral("named".into())), expr(ExprKind::IntLiteral(50))),
        ArrayEntry::Keyed(expr(ExprKind::StringLiteral("named".into())), expr(ExprKind::IntLiteral(60))),
    ]);
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = Module::new(Target::parse(target).unwrap());
        let context = ConstDefaultContext { module: &module, current_class: None };
        for (index, expected) in [
            (ExprKind::IntLiteral(2), Some(10)),
            (ExprKind::StringLiteral("3".into()), Some(30)),
            (ExprKind::IntLiteral(4), Some(40)),
            (ExprKind::StringLiteral("named".into()), Some(60)),
            (ExprKind::IntLiteral(0), None),
        ] {
            let access = expr(ExprKind::ArrayAccess {
                array: Box::new(expr(array.clone())), index: Box::new(expr(index)),
            });
            assert!(resolve_const_default(&access, &context) == expected.map(const_default_int_value), "{target}");
        }
    }
}

/// Folding a projection cannot discard construction of other array elements.
#[test]
fn array_offset_defaults_decline_object_construction() {
    let module = Module::new(Target::parse("linux-x86_64").unwrap());
    let context = ConstDefaultContext { module: &module, current_class: None };
    let expr = |kind| Expr::new(kind, Span::dummy());
    let access = expr(ExprKind::ArrayAccess {
        array: Box::new(expr(ExprKind::ArrayLiteral(vec![
            expr(ExprKind::IntLiteral(7)),
            expr(ExprKind::NewObject { class_name: "Effectful".into(), args: Vec::new() }),
        ]))),
        index: Box::new(expr(ExprKind::IntLiteral(0))),
    });
    assert!(resolve_const_default(&access, &context).is_none());
}

/// Resolves one numeric default under every target without changing its PHP value type.
fn assert_arithmetic(op: BinOp, left: ExprKind, right: ExprKind, expected: Option<ConstDefaultValue>) {
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = Module::new(Target::parse(target).unwrap());
        let context = ConstDefaultContext { module: &module, current_class: None };
        let expression = Expr::new(ExprKind::BinaryOp {
            left: Box::new(Expr::new(left.clone(), Span::dummy())), op: op.clone(),
            right: Box::new(Expr::new(right.clone(), Span::dummy())),
        }, Span::dummy());
        assert!(resolve_const_default(&expression, &context) == expected, "{target}: {op:?}");
    }
}

/// Integer division and powers use exact checked integer arithmetic before float fallback.
#[test]
fn numeric_defaults_preserve_integer_results_on_all_targets() {
    for (op, left, right, expected) in [
        (BinOp::Div, 6, 2, 3), (BinOp::Div, -6, 2, -3),
        (BinOp::Pow, 6, 2, 36), (BinOp::Pow, -2, 3, -8),
        (BinOp::Pow, 3, 34, 16_677_181_699_666_569), (BinOp::Pow, 0, 0, 1),
    ] {
        assert_arithmetic(op, ExprKind::IntLiteral(left), ExprKind::IntLiteral(right),
            Some(const_default_int_value(expected)));
    }
}

/// Fractions, negative powers, overflow and explicit floats keep float results; zero division defers.
#[test]
fn numeric_defaults_preserve_float_and_error_boundaries_on_all_targets() {
    for (op, left, right, expected) in [
        (BinOp::Div, ExprKind::IntLiteral(6), ExprKind::IntLiteral(4), 1.5),
        (BinOp::Pow, ExprKind::IntLiteral(2), ExprKind::IntLiteral(-2), 0.25),
        (BinOp::Div, ExprKind::FloatLiteral(6.0), ExprKind::IntLiteral(2), 3.0),
        (BinOp::Pow, ExprKind::FloatLiteral(2.0), ExprKind::IntLiteral(2), 4.0),
        (BinOp::Div, ExprKind::IntLiteral(i64::MIN), ExprKind::IntLiteral(-1), -(i64::MIN as f64)),
        (BinOp::Add, ExprKind::IntLiteral(i64::MAX), ExprKind::IntLiteral(1), -(i64::MIN as f64)),
    ] {
        assert_arithmetic(op, left, right, Some(const_default_float(expected)));
    }
    assert_arithmetic(BinOp::Div, ExprKind::IntLiteral(1), ExprKind::IntLiteral(0), None);
}

/// PHP's MIN_INT remainder by negative one is zero, including after exact integer division.
#[test]
fn numeric_defaults_remainder_overflow_on_all_targets() {
    let divided = ExprKind::BinaryOp {
        left: Box::new(Expr::new(ExprKind::IntLiteral(i64::MIN), Span::dummy())),
        op: BinOp::Div,
        right: Box::new(Expr::new(ExprKind::IntLiteral(1), Span::dummy())),
    };
    for left in [ExprKind::IntLiteral(i64::MIN), divided] {
        assert_arithmetic(BinOp::Mod, left, ExprKind::IntLiteral(-1), Some(const_default_int_value(0)));
    }
    assert_arithmetic(BinOp::Mod, ExprKind::IntLiteral(1), ExprKind::IntLiteral(0), None);
}

/// Enum-dependent keys and offsets resolve and keep their reads in PHP evaluation order.
#[test]
fn enum_dependent_keys_and_offsets_retain_ordered_reads_on_all_targets() {
    let expr = |kind| Expr::new(kind, Span::dummy());
    let case = |name: &str| expr(ExprKind::ScopedConstantAccess {
        receiver: StaticReceiver::Named("E".into()), name: name.into(),
    });
    let selector = |condition: &str, then_key: ExprKind, else_key: ExprKind| expr(ExprKind::Ternary {
        condition: Box::new(case(condition)),
        then_expr: Box::new(expr(then_key)),
        else_expr: Box::new(expr(else_key)),
    });
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let mut module = Module::new(Target::parse(target).unwrap());
        module.enum_infos.insert("E".into(), crate::types::EnumInfo {
            backing_type: None,
            cases: ["A", "B", "C"].into_iter().map(|name| crate::types::EnumCaseInfo {
                name: name.into(), value: None, attribute_names: Vec::new(), attribute_args: Vec::new(),
            }).collect(),
        });
        let context = ConstDefaultContext { module: &module, current_class: None };
        // [E::C ? 1 : 0] reads the list [E::A, E::B] first, then the offset's E::C.
        let offset = expr(ExprKind::ArrayAccess {
            array: Box::new(expr(ExprKind::ArrayLiteral(vec![case("A"), case("B")]))),
            index: Box::new(selector("C", ExprKind::IntLiteral(1), ExprKind::IntLiteral(0))),
        });
        let selected = resolve_enum_case_default(&offset, &context).expect(target);
        assert_eq!(selected.case_name, "B", "{target}");
        let order = |reads: &[(String, String)]| reads.iter().map(|(_, case)| case.clone()).collect::<Vec<_>>();
        // The selected case is reloaded last so the final load leaves it in the result register.
        assert_eq!(order(&selected.evaluated), ["A", "B", "C", "B"], "{target}");
        // Each key's reads precede its value's: [E::C ? 'x' : 'y' => E::A, E::B ? 'z' : 'w' => 5].
        let keyed = expr(ExprKind::ArrayLiteralAssoc(vec![
            (selector("C", ExprKind::StringLiteral("x".into()), ExprKind::StringLiteral("y".into())), case("A")),
            (selector("B", ExprKind::StringLiteral("z".into()), ExprKind::StringLiteral("w".into())), expr(ExprKind::IntLiteral(5))),
        ]));
        let value = resolve_const_default(&keyed, &context).expect(target);
        let mut reads = Vec::new();
        collect_enum_reads(&value, &mut reads);
        assert_eq!(order(&reads), ["C", "A", "B"], "{target}");
        let ConstDefaultValue::Array(elements) = without_enum_effects(value) else { panic!("{target}") };
        assert!(elements[0].key == Some(ConstDefaultArrayKey::String("x".into())), "{target}");
        assert!(elements[1].key == Some(ConstDefaultArrayKey::String("z".into())), "{target}");
    }
}
