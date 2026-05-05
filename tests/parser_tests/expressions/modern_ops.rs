use super::*;

#[test]
fn test_word_logical_and_lower_than_oror() {
    // $a || $b and $c should parse as ($a || $b) and $c — PHP precedence
    let stmts = parse_source("<?php echo $a || $b and $c;");
    let expected = Stmt::echo(Expr::binop(
        Expr::binop(Expr::var("a"), BinOp::Or, Expr::var("b")),
        BinOp::And,
        Expr::var("c"),
    ));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_word_logical_or_lower_than_andand() {
    // $a && $b or $c should parse as ($a && $b) or $c — PHP precedence
    let stmts = parse_source("<?php echo $a && $b or $c;");
    let expected = Stmt::echo(Expr::binop(
        Expr::binop(Expr::var("a"), BinOp::And, Expr::var("b")),
        BinOp::Or,
        Expr::var("c"),
    ));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_word_logical_xor_precedence() {
    // $a xor $b and $c should parse as $a xor ($b and $c)
    let stmts = parse_source("<?php echo $a xor $b and $c;");
    let expected = Stmt::echo(Expr::binop(
        Expr::var("a"),
        BinOp::Xor,
        Expr::binop(Expr::var("b"), BinOp::And, Expr::var("c")),
    ));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_word_logical_xor_higher_than_or() {
    // $a or $b xor $c should parse as $a or ($b xor $c)
    let stmts = parse_source("<?php echo $a or $b xor $c;");
    let expected = Stmt::echo(Expr::binop(
        Expr::var("a"),
        BinOp::Or,
        Expr::binop(Expr::var("b"), BinOp::Xor, Expr::var("c")),
    ));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_print_expression_binds_tighter_than_word_and() {
    let stmts = parse_source("<?php echo print $a and $b;");
    let expected = Stmt::echo(Expr::binop(
        Expr::print(Expr::var("a")),
        BinOp::And,
        Expr::var("b"),
    ));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_print_expression_operand_accepts_short_ternary() {
    let stmts = parse_source("<?php echo print $a ?: $b;");
    let expected = Stmt::echo(Expr::print(Expr::new(
        ExprKind::ShortTernary {
            value: Box::new(Expr::var("a")),
            default: Box::new(Expr::var("b")),
        },
        elephc::span::Span::dummy(),
    )));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_parse_instanceof_expression() {
    let stmts = parse_source("<?php echo $a instanceof Foo;");
    let expected = Stmt::echo(Expr::instance_of(
        Expr::var("a"),
        Name::unqualified("Foo"),
    ));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_parse_dynamic_instanceof_variable_target() {
    let stmts = parse_source("<?php echo $a instanceof $className;");
    let expected = Stmt::echo(Expr::dynamic_instance_of(
        Expr::var("a"),
        Expr::var("className"),
    ));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_parse_dynamic_instanceof_property_and_array_targets() {
    let stmts = parse_source("<?php echo $a instanceof $holder->className; echo $a instanceof $names[0];");
    let property_target = Expr::new(
        ExprKind::PropertyAccess {
            object: Box::new(Expr::var("holder")),
            property: "className".to_string(),
        },
        elephc::span::Span::dummy(),
    );
    let array_target = Expr::new(
        ExprKind::ArrayAccess {
            array: Box::new(Expr::var("names")),
            index: Box::new(Expr::int_lit(0)),
        },
        elephc::span::Span::dummy(),
    );
    assert_eq!(
        stmts,
        vec![
            Stmt::echo(Expr::dynamic_instance_of(Expr::var("a"), property_target)),
            Stmt::echo(Expr::dynamic_instance_of(Expr::var("a"), array_target)),
        ]
    );
}

#[test]
fn test_parse_parenthesized_dynamic_instanceof_expression_target() {
    let stmts = parse_source("<?php echo $a instanceof ($prefix . $suffix);");
    let target = Expr::binop(Expr::var("prefix"), BinOp::Concat, Expr::var("suffix"));
    let expected = Stmt::echo(Expr::dynamic_instance_of(Expr::var("a"), target));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_instanceof_binds_tighter_than_not() {
    let stmts = parse_source("<?php echo !$a instanceof Foo;");
    let expected = Stmt::echo(Expr::new(
        ExprKind::Not(Box::new(Expr::instance_of(
            Expr::var("a"),
            Name::unqualified("Foo"),
        ))),
        elephc::span::Span::dummy(),
    ));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_instanceof_binds_tighter_than_addition() {
    let stmts = parse_source("<?php echo 1 + $a instanceof Foo;");
    let expected = Stmt::echo(Expr::binop(
        Expr::int_lit(1),
        BinOp::Add,
        Expr::instance_of(Expr::var("a"), Name::unqualified("Foo")),
    ));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_dynamic_instanceof_binds_tighter_than_concat() {
    let stmts = parse_source("<?php echo $a instanceof $className . \"!\";");
    let expected = Stmt::echo(Expr::binop(
        Expr::dynamic_instance_of(Expr::var("a"), Expr::var("className")),
        BinOp::Concat,
        Expr::string_lit("!"),
    ));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_instanceof_chains_left_to_right() {
    let stmts = parse_source("<?php echo $a instanceof Foo instanceof Bar;");
    let expected = Stmt::echo(Expr::instance_of(
        Expr::instance_of(Expr::var("a"), Name::unqualified("Foo")),
        Name::unqualified("Bar"),
    ));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_dynamic_instanceof_chains_left_to_right() {
    let stmts = parse_source("<?php echo $a instanceof $className instanceof Foo;");
    let expected = Stmt::echo(Expr::instance_of(
        Expr::dynamic_instance_of(Expr::var("a"), Expr::var("className")),
        Name::unqualified("Foo"),
    ));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_instanceof_accepts_special_class_targets() {
    let stmts = parse_source("<?php echo $a instanceof self; echo $a instanceof parent; echo $a instanceof static;");
    assert_eq!(
        stmts,
        vec![
            Stmt::echo(Expr::instance_of(Expr::var("a"), Name::unqualified("self"))),
            Stmt::echo(Expr::instance_of(Expr::var("a"), Name::unqualified("parent"))),
            Stmt::echo(Expr::instance_of(Expr::var("a"), Name::unqualified("static"))),
        ]
    );
}

#[test]
fn test_parenthesized_word_logical_assignment_rhs() {
    let stmts = parse_source("<?php $x = (true and false);");
    match &stmts[0].kind {
        StmtKind::Assign { value, .. } => match &value.kind {
            ExprKind::BinaryOp { op, .. } => assert_eq!(op, &BinOp::And),
            other => panic!("expected BinaryOp, got {:?}", other),
        },
        other => panic!("expected Assign, got {:?}", other),
    }
}

#[test]
fn test_assignment_expression_binds_tighter_than_word_and() {
    let stmts = parse_source("<?php $x = true and false;");
    match &stmts[0].kind {
        StmtKind::ExprStmt(expr) => match &expr.kind {
            ExprKind::BinaryOp { left, op, right } => {
                assert_eq!(op, &BinOp::And);
                assert!(matches!(right.kind, ExprKind::BoolLiteral(false)));
                match &left.kind {
                    ExprKind::Assignment { target, value, .. } => {
                        assert!(matches!(target.kind, ExprKind::Variable(ref name) if name == "x"));
                        assert!(matches!(value.kind, ExprKind::BoolLiteral(true)));
                    }
                    other => panic!("expected assignment expression, got {:?}", other),
                }
            }
            other => panic!("expected BinaryOp, got {:?}", other),
        },
        other => panic!("expected ExprStmt, got {:?}", other),
    }
}

#[test]
fn test_assignment_expression_is_right_associative() {
    let stmts = parse_source("<?php $x = $y = 1;");
    match &stmts[0].kind {
        StmtKind::Assign { name, value } => {
            assert_eq!(name, "x");
            match &value.kind {
                ExprKind::Assignment { target, value, .. } => {
                    assert!(matches!(target.kind, ExprKind::Variable(ref name) if name == "y"));
                    assert!(matches!(value.kind, ExprKind::IntLiteral(1)));
                }
                other => panic!("expected nested assignment expression, got {:?}", other),
            }
        }
        other => panic!("expected Assign, got {:?}", other),
    }
}

#[test]
fn test_non_local_assignment_expression_parses_array_target() {
    let stmts = parse_source("<?php echo ($items[$i] = 2);");
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::Assignment { target, value, prelude, .. } => {
                assert_eq!(prelude.len(), 1);
                assert!(matches!(value.kind, ExprKind::Variable(_)));
                match &target.kind {
                    ExprKind::ArrayAccess { array, index } => {
                        assert!(matches!(array.kind, ExprKind::Variable(ref name) if name == "items"));
                        assert!(matches!(index.kind, ExprKind::Variable(ref name) if name == "i"));
                    }
                    other => panic!("expected array target, got {:?}", other),
                }
            }
            other => panic!("expected assignment expression, got {:?}", other),
        },
        other => panic!("expected Echo, got {:?}", other),
    }
}

#[test]
fn test_non_local_assignment_expression_parses_property_target() {
    let stmts = parse_source("<?php echo ($box->value += 2);");
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::Assignment { target, value, .. } => {
                assert!(matches!(target.kind, ExprKind::PropertyAccess { .. }));
                assert!(matches!(value.kind, ExprKind::BinaryOp { op: BinOp::Add, .. }));
            }
            other => panic!("expected assignment expression, got {:?}", other),
        },
        other => panic!("expected Echo, got {:?}", other),
    }
}

#[test]
fn test_non_local_assignment_expression_parses_static_property_target() {
    let stmts = parse_source("<?php echo (Registry::$count ??= 1);");
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::Assignment { target, value, .. } => {
                assert!(matches!(target.kind, ExprKind::StaticPropertyAccess { .. }));
                assert!(matches!(value.kind, ExprKind::NullCoalesce { .. }));
            }
            other => panic!("expected assignment expression, got {:?}", other),
        },
        other => panic!("expected Echo, got {:?}", other),
    }
}

#[test]
fn test_non_local_assignment_expression_stabilizes_effectful_index() {
    let stmts = parse_source("<?php echo ($items[idx()] = value());");
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::Assignment {
                target,
                value,
                result_target,
                prelude,
                ..
            } => {
                assert_eq!(prelude.len(), 2);
                assert!(result_target.is_some());
                assert!(matches!(value.kind, ExprKind::Variable(_)));
                match &target.kind {
                    ExprKind::ArrayAccess { index, .. } => {
                        assert!(matches!(index.kind, ExprKind::Variable(_)));
                    }
                    other => panic!("expected stabilized array target, got {:?}", other),
                }
            }
            other => panic!("expected assignment expression, got {:?}", other),
        },
        other => panic!("expected Echo, got {:?}", other),
    }
}

#[test]
fn test_non_local_assignment_expression_delays_simple_variable_index() {
    let stmts = parse_source("<?php echo ($items[$i] = ($i = 1));");
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::Assignment {
                target,
                value,
                result_target,
                prelude,
                ..
            } => {
                assert_eq!(prelude.len(), 1);
                assert!(result_target.is_some());
                assert!(matches!(value.kind, ExprKind::Variable(_)));
                match &target.kind {
                    ExprKind::ArrayAccess { index, .. } => {
                        assert!(matches!(index.kind, ExprKind::Variable(ref name) if name == "i"));
                    }
                    other => panic!("expected array target, got {:?}", other),
                }
            }
            other => panic!("expected assignment expression, got {:?}", other),
        },
        other => panic!("expected Echo, got {:?}", other),
    }
}

#[test]
fn test_null_coalesce_assignment_expression_uses_conditional_value_temp() {
    let stmts = parse_source("<?php echo ($items[$i] ??= ($i = 1));");
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::Assignment {
                target,
                value,
                result_target,
                prelude,
                conditional_value_temp,
            } => {
                assert!(prelude.is_empty());
                assert!(result_target.is_some());
                assert!(conditional_value_temp.is_some());
                assert!(matches!(value.kind, ExprKind::NullCoalesce { .. }));
                match &target.kind {
                    ExprKind::ArrayAccess { index, .. } => {
                        assert!(matches!(index.kind, ExprKind::Variable(ref name) if name == "i"));
                    }
                    other => panic!("expected array target, got {:?}", other),
                }
            }
            other => panic!("expected assignment expression, got {:?}", other),
        },
        other => panic!("expected Echo, got {:?}", other),
    }
}

#[test]
fn test_null_coalesce_assignment_expression_stabilizes_computed_mutated_index() {
    let stmts = parse_source("<?php echo ($items[$i + 0] ??= ($i = 1));");
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::Assignment {
                target,
                value,
                result_target,
                prelude,
                conditional_value_temp,
            } => {
                assert_eq!(prelude.len(), 1);
                assert!(result_target.is_some());
                assert!(conditional_value_temp.is_some());
                assert!(matches!(value.kind, ExprKind::NullCoalesce { .. }));
                match &target.kind {
                    ExprKind::ArrayAccess { index, .. } => {
                        assert!(matches!(index.kind, ExprKind::Variable(_)));
                    }
                    other => panic!("expected stabilized array target, got {:?}", other),
                }
            }
            other => panic!("expected assignment expression, got {:?}", other),
        },
        other => panic!("expected Echo, got {:?}", other),
    }
}

#[test]
fn test_short_ternary_expression() {
    let stmts = parse_source("<?php echo $a ?: $b;");
    let expected = Stmt::echo(Expr::new(
        ExprKind::ShortTernary {
            value: Box::new(Expr::var("a")),
            default: Box::new(Expr::var("b")),
        },
        elephc::span::Span::dummy(),
    ));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_short_ternary_lower_than_symbolic_or() {
    let stmts = parse_source("<?php echo $a || $b ?: $c;");
    let expected = Stmt::echo(Expr::new(
        ExprKind::ShortTernary {
            value: Box::new(Expr::binop(Expr::var("a"), BinOp::Or, Expr::var("b"))),
            default: Box::new(Expr::var("c")),
        },
        elephc::span::Span::dummy(),
    ));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_short_ternary_default_accepts_null_coalesce() {
    let stmts = parse_source("<?php echo $a ?: $b ?? $c;");
    let expected = Stmt::echo(Expr::new(
        ExprKind::ShortTernary {
            value: Box::new(Expr::var("a")),
            default: Box::new(Expr::new(
                ExprKind::NullCoalesce {
                    value: Box::new(Expr::var("b")),
                    default: Box::new(Expr::var("c")),
                },
                elephc::span::Span::dummy(),
            )),
        },
        elephc::span::Span::dummy(),
    ));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_short_ternary_can_nest_in_full_ternary_else_branch() {
    let stmts = parse_source("<?php echo $a ? $b : $c ?: $d;");
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::Ternary { else_expr, .. } => {
                assert!(matches!(else_expr.kind, ExprKind::ShortTernary { .. }));
            }
            other => panic!("expected Ternary, got {:?}", other),
        },
        other => panic!("expected Echo, got {:?}", other),
    }
}

#[test]
fn test_null_coalesce_parse() {
    let stmts = parse_source("<?php echo $x ?? 0;");
    assert_eq!(stmts.len(), 1);
    if let StmtKind::Echo(expr) = &stmts[0].kind {
        if let ExprKind::NullCoalesce { .. } = &expr.kind {
            // good
        } else {
            panic!("expected NullCoalesce, got {:?}", expr.kind);
        }
    } else {
        panic!("expected Echo");
    }
}

#[test]
fn test_null_coalesce_assignment_parse() {
    let stmts = parse_source("<?php $x ??= 10;");
    assert_eq!(stmts.len(), 1);
    match &stmts[0].kind {
        StmtKind::Assign { name, value } => {
            assert_eq!(name, "x");
            match &value.kind {
                ExprKind::NullCoalesce { value, default } => {
                    assert_eq!(value.kind, ExprKind::Variable("x".into()));
                    assert_eq!(default.kind, ExprKind::IntLiteral(10));
                }
                other => panic!("expected NullCoalesce, got {:?}", other),
            }
        }
        other => panic!("expected Assign, got {:?}", other),
    }
}

#[test]
fn test_null_coalesce_assignment_rhs_is_expression() {
    let stmts = parse_source("<?php $x ??= $fallback ?? 10;");
    match &stmts[0].kind {
        StmtKind::Assign { value, .. } => match &value.kind {
            ExprKind::NullCoalesce { default, .. } => {
                assert!(matches!(default.kind, ExprKind::NullCoalesce { .. }));
            }
            other => panic!("expected outer NullCoalesce, got {:?}", other),
        },
        other => panic!("expected Assign, got {:?}", other),
    }
}

// --- Spaceship operator ---

#[test]
fn test_spaceship_parse() {
    let stmts = parse_source("<?php echo 1 <=> 2;");
    let expected = Stmt::echo(Expr::binop(
        Expr::int_lit(1),
        BinOp::Spaceship,
        Expr::int_lit(2),
    ));
    assert_eq!(stmts, vec![expected]);
}

// --- Constants ---
