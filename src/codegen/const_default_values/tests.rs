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
                name: "A".into(), value: None, span: None, deprecated_mt_rand_php: false,
                attribute_names: Vec::new(), attribute_args: Vec::new(),
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
                name: name.into(), value: None, span: None, deprecated_mt_rand_php: false,
                attribute_names: Vec::new(), attribute_args: Vec::new(),
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

/// Builds an enum module with cases `A` and `B` of enum `E` for comparison folds.
fn comparison_module(target: &str) -> Module {
    let mut module = Module::new(Target::parse(target).unwrap());
    let case = |name: &str| crate::types::EnumCaseInfo {
        name: name.into(), value: None, span: None, deprecated_mt_rand_php: false,
        attribute_names: Vec::new(), attribute_args: Vec::new(),
    };
    module.enum_infos.insert("E".into(), crate::types::EnumInfo {
        backing_type: None, cases: vec![case("A"), case("B")],
    });
    module
}

/// Folds `left op right` and returns the bool plus the enum reads the fold retained.
fn compare(
    module: &Module, left: ExprKind, op: BinOp, right: ExprKind,
) -> Option<(bool, Vec<(String, String)>)> {
    let expr = |kind| Expr::new(kind, Span::dummy());
    let expression = expr(ExprKind::BinaryOp {
        left: Box::new(expr(left)), op, right: Box::new(expr(right)),
    });
    let context = ConstDefaultContext { module, current_class: None };
    let value = resolve_const_default(&expression, &context)?;
    let mut reads = Vec::new();
    collect_enum_reads(&value, &mut reads);
    match without_enum_effects(value) {
        ConstDefaultValue::Scalar { kind: CONST_DEFAULT_BOOL, payload } => Some((payload != 0, reads)),
        _ => panic!("a comparison must resolve to a bool"),
    }
}

/// Builds the `E::name` enum-case expression used as a comparison operand.
fn case(name: &str) -> ExprKind {
    ExprKind::ScopedConstantAccess { receiver: StaticReceiver::Named("E".into()), name: name.into() }
}

/// Enum cases are identical only to themselves, and the fold keeps both reads in order.
#[test]
fn enum_comparison_defaults_fold_identity_and_keep_reads_on_all_targets() {
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = comparison_module(target);
        for (op, left, right, expected) in [
            (BinOp::StrictEq, "A", "A", true), (BinOp::StrictEq, "A", "B", false),
            (BinOp::StrictNotEq, "A", "B", true), (BinOp::StrictNotEq, "B", "B", false),
            (BinOp::Eq, "B", "B", true), (BinOp::Eq, "A", "B", false),
            (BinOp::NotEq, "A", "B", true), (BinOp::NotEq, "A", "A", false),
        ] {
            let (value, reads) = compare(&module, case(left), op.clone(), case(right)).unwrap();
            assert_eq!(value, expected, "{target}: {left} {op:?} {right}");
            let mut expected_reads = vec![("E".to_string(), left.to_string())];
            if left != right { expected_reads.push(("E".to_string(), right.to_string())); }
            assert_eq!(reads, expected_reads, "{target}: reads of {left} {op:?} {right}");
        }
    }
}

/// An enum case against a non-enum is false under strict comparison and declines under loose.
#[test]
fn enum_comparison_defaults_decline_loose_cross_type_on_all_targets() {
    for target in ["macos-aarch64", "linux-x86_64"] {
        let module = comparison_module(target);
        assert_eq!(
            compare(&module, case("A"), BinOp::StrictEq, ExprKind::IntLiteral(1)).map(|c| c.0),
            Some(false), "{target}",
        );
        assert_eq!(
            compare(&module, ExprKind::StringLiteral("x".into()), BinOp::StrictNotEq, case("A")).map(|c| c.0),
            Some(true), "{target}",
        );
        assert!(compare(&module, case("A"), BinOp::Eq, ExprKind::IntLiteral(1)).is_none(), "{target}");
        assert!(compare(&module, case("A"), BinOp::NotEq, ExprKind::BoolLiteral(true)).is_none(), "{target}");
    }
}

/// Scalar comparisons keep PHP's type rules: strict needs the same type, loose strings may be numeric.
#[test]
fn scalar_comparison_defaults_follow_php_type_rules_on_all_targets() {
    let module = comparison_module("linux-x86_64");
    let string = |value: &str| ExprKind::StringLiteral(value.into());
    let check = |left, op, right| compare(&module, left, op, right).map(|result| result.0);
    assert_eq!(check(string("abc"), BinOp::StrictEq, string("abc")), Some(true));
    assert_eq!(check(string("abc"), BinOp::StrictEq, string("abd")), Some(false));
    assert_eq!(check(string("abc"), BinOp::Eq, string("abc")), Some(true));
    // "1e1" == "10" holds in PHP, so a loose string mismatch cannot be decided by bytes.
    assert_eq!(check(string("1e1"), BinOp::Eq, string("10")), None);
    assert_eq!(check(string("1e1"), BinOp::StrictEq, string("10")), Some(false));
    assert_eq!(check(ExprKind::IntLiteral(1), BinOp::StrictEq, ExprKind::FloatLiteral(1.0)), Some(false));
    assert_eq!(check(ExprKind::IntLiteral(1), BinOp::Eq, ExprKind::FloatLiteral(1.0)), None);
    assert_eq!(check(ExprKind::FloatLiteral(1.5), BinOp::StrictEq, ExprKind::FloatLiteral(1.5)), Some(true));
    assert_eq!(check(ExprKind::FloatLiteral(f64::NAN), BinOp::StrictEq, ExprKind::FloatLiteral(f64::NAN)), Some(false));
    assert_eq!(check(ExprKind::Null, BinOp::StrictEq, ExprKind::Null), Some(true));
    assert_eq!(check(ExprKind::Null, BinOp::StrictEq, ExprKind::BoolLiteral(false)), Some(false));
    assert_eq!(check(ExprKind::Null, BinOp::Eq, ExprKind::BoolLiteral(false)), None);
}

/// `PHP_BINARY` is a string only known at run time: a comparison with a string literal must decline,
/// while a comparison with a type that is never a string stays decidable under `===`.
#[test]
fn executable_path_comparison_defaults_decline_string_operands_on_all_targets() {
    for target in ["macos-aarch64", "linux-x86_64"] {
        let module = comparison_module(target);
        let path = || ExprKind::ConstRef("PHP_BINARY".into());
        let string = |value: &str| ExprKind::StringLiteral(value.into());
        for op in [BinOp::StrictEq, BinOp::StrictNotEq, BinOp::Eq, BinOp::NotEq] {
            assert!(compare(&module, path(), op.clone(), string("/opt/app")).is_none(), "{target}: path {op:?} string");
            assert!(compare(&module, string("/opt/app"), op.clone(), path()).is_none(), "{target}: string {op:?} path");
        }
        assert_eq!(
            compare(&module, path(), BinOp::StrictEq, ExprKind::IntLiteral(1)).map(|c| c.0),
            Some(false), "{target}",
        );
        assert!(compare(&module, path(), BinOp::Eq, ExprKind::IntLiteral(1)).is_none(), "{target}");
    }
}
