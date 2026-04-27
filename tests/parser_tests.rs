use elephc::lexer::tokenize;
use elephc::names::Name;
use elephc::parser::ast::{
    BinOp, CallableTarget, CatchClause, Expr, ExprKind, StaticReceiver, Stmt, StmtKind,
    TraitAdaptation, TypeExpr, UseKind, Visibility,
};
use elephc::parser::parse;

fn parse_source(src: &str) -> Vec<Stmt> {
    let tokens = tokenize(src).unwrap();
    parse(&tokens).unwrap()
}

fn parse_fails(src: &str) -> bool {
    let tokens = match tokenize(src) {
        Ok(t) => t,
        Err(_) => return true,
    };
    parse(&tokens).is_err()
}

// --- Echo ---

#[test]
fn test_echo_string_literal() {
    let stmts = parse_source("<?php echo \"hello\";");
    assert_eq!(stmts, vec![Stmt::echo(Expr::string_lit("hello"))]);
}

#[test]
fn test_echo_integer() {
    let stmts = parse_source("<?php echo 42;");
    assert_eq!(stmts, vec![Stmt::echo(Expr::int_lit(42))]);
}

#[test]
fn test_parse_try_catch_finally() {
    let stmts = parse_source(
        "<?php try { throw $e; } catch (MyException $err) { echo 1; } finally { echo 2; }",
    );
    assert_eq!(
        stmts,
        vec![Stmt::new(
            StmtKind::Try {
                try_body: vec![Stmt::new(
                    StmtKind::Throw(Expr::var("e")),
                    elephc::span::Span::dummy(),
                )],
                catches: vec![CatchClause {
                    exception_types: vec!["MyException".into()],
                    variable: Some("err".into()),
                    body: vec![Stmt::echo(Expr::int_lit(1))],
                }],
                finally_body: Some(vec![Stmt::echo(Expr::int_lit(2))]),
            },
            elephc::span::Span::dummy(),
        )]
    );
}

#[test]
fn test_parse_multi_catch() {
    let stmts = parse_source(
        "<?php try { throw $e; } catch (FooException | BarException $err) { echo 1; }",
    );
    assert_eq!(
        stmts,
        vec![Stmt::new(
            StmtKind::Try {
                try_body: vec![Stmt::new(
                    StmtKind::Throw(Expr::var("e")),
                    elephc::span::Span::dummy(),
                )],
                catches: vec![CatchClause {
                    exception_types: vec!["FooException".into(), "BarException".into()],
                    variable: Some("err".into()),
                    body: vec![Stmt::echo(Expr::int_lit(1))],
                }],
                finally_body: None,
            },
            elephc::span::Span::dummy(),
        )]
    );
}

#[test]
fn test_parse_catch_without_variable() {
    let stmts = parse_source("<?php try { throw $e; } catch (Exception) { echo 1; }");
    assert_eq!(
        stmts,
        vec![Stmt::new(
            StmtKind::Try {
                try_body: vec![Stmt::new(
                    StmtKind::Throw(Expr::var("e")),
                    elephc::span::Span::dummy(),
                )],
                catches: vec![CatchClause {
                    exception_types: vec!["Exception".into()],
                    variable: None,
                    body: vec![Stmt::echo(Expr::int_lit(1))],
                }],
                finally_body: None,
            },
            elephc::span::Span::dummy(),
        )]
    );
}

#[test]
fn test_parse_string_indexing_uses_array_access_ast() {
    let stmts = parse_source("<?php echo $name[1];");
    assert_eq!(stmts.len(), 1);
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::ArrayAccess { array, index } => {
                assert_eq!(array.kind, ExprKind::Variable("name".into()));
                assert_eq!(index.kind, ExprKind::IntLiteral(1));
            }
            other => panic!("expected array access, got {:?}", other),
        },
        other => panic!("expected echo, got {:?}", other),
    }
}

#[test]
fn test_parse_throw_expression_in_null_coalesce() {
    let stmts = parse_source("<?php $value = $maybe ?? throw new Exception();");
    assert_eq!(stmts.len(), 1);
    match &stmts[0].kind {
        StmtKind::Assign { name, value } => {
            assert_eq!(name, "value");
            match &value.kind {
                ExprKind::NullCoalesce { value, default } => {
                    assert_eq!(value.kind, ExprKind::Variable("maybe".into()));
                    match &default.kind {
                        ExprKind::Throw(inner) => match &inner.kind {
                            ExprKind::NewObject { class_name, .. } => {
                                assert_eq!(class_name, "Exception");
                            }
                            other => panic!("expected throw new Exception(), got {:?}", other),
                        },
                        other => panic!("expected throw expression, got {:?}", other),
                    }
                }
                other => panic!("expected null coalesce, got {:?}", other),
            }
        }
        other => panic!("expected assignment, got {:?}", other),
    }
}

#[test]
fn test_parse_ifdef_statement() {
    let stmts = parse_source("<?php ifdef DEBUG { echo 1; }");
    assert_eq!(
        stmts,
        vec![Stmt::new(
            StmtKind::IfDef {
                symbol: "DEBUG".into(),
                then_body: vec![Stmt::echo(Expr::int_lit(1))],
                else_body: None,
            },
            elephc::span::Span::dummy(),
        )]
    );
}

#[test]
fn test_parse_ifdef_else_statement() {
    let stmts = parse_source("<?php ifdef DEBUG { echo 1; } else { echo 2; }");
    assert_eq!(
        stmts,
        vec![Stmt::new(
            StmtKind::IfDef {
                symbol: "DEBUG".into(),
                then_body: vec![Stmt::echo(Expr::int_lit(1))],
                else_body: Some(vec![Stmt::echo(Expr::int_lit(2))]),
            },
            elephc::span::Span::dummy(),
        )]
    );
}

#[test]
fn test_parse_namespace_semicolon_and_use_group() {
    let stmts = parse_source(
        "<?php namespace App\\Core; use Lib\\Utils\\{Formatter, function render as draw, const ANSWER};",
    );
    assert_eq!(stmts.len(), 2);
    match &stmts[0].kind {
        StmtKind::NamespaceDecl { name } => {
            assert_eq!(name.as_ref().map(Name::as_str), Some("App\\Core"));
        }
        other => panic!("expected namespace decl, got {:?}", other),
    }
    match &stmts[1].kind {
        StmtKind::UseDecl { imports } => {
            assert_eq!(imports.len(), 3);
            assert_eq!(imports[0].kind, UseKind::Class);
            assert_eq!(imports[0].name.as_str(), "Lib\\Utils\\Formatter");
            assert_eq!(imports[0].alias, "Formatter");
            assert_eq!(imports[1].kind, UseKind::Function);
            assert_eq!(imports[1].name.as_str(), "Lib\\Utils\\render");
            assert_eq!(imports[1].alias, "draw");
            assert_eq!(imports[2].kind, UseKind::Const);
            assert_eq!(imports[2].name.as_str(), "Lib\\Utils\\ANSWER");
            assert_eq!(imports[2].alias, "ANSWER");
        }
        other => panic!("expected use decl, got {:?}", other),
    }
}

#[test]
fn test_parse_namespace_block_with_qualified_names() {
    let stmts = parse_source(
        "<?php namespace App\\Models { class User extends Base\\Record implements \\Contracts\\Jsonable { use Shared\\Loggable; public function make() { return Factory\\UserFactory::build(); } } }",
    );
    assert_eq!(stmts.len(), 1);
    match &stmts[0].kind {
        StmtKind::NamespaceBlock { name, body } => {
            assert_eq!(name.as_ref().map(Name::as_str), Some("App\\Models"));
            assert_eq!(body.len(), 1);
            match &body[0].kind {
                StmtKind::ClassDecl {
                    extends,
                    implements,
                    trait_uses,
                    methods,
                    ..
                } => {
                    assert_eq!(extends.as_ref().map(Name::as_str), Some("Base\\Record"));
                    assert_eq!(implements.len(), 1);
                    assert!(implements[0].is_fully_qualified());
                    assert_eq!(implements[0].as_str(), "Contracts\\Jsonable");
                    assert_eq!(trait_uses[0].trait_names[0].as_str(), "Shared\\Loggable");
                    match &methods[0].body[0].kind {
                        StmtKind::Return(Some(expr)) => match &expr.kind {
                            ExprKind::StaticMethodCall {
                                receiver, method, ..
                            } => {
                                match receiver {
                                    StaticReceiver::Named(name) => {
                                        assert_eq!(name.as_str(), "Factory\\UserFactory");
                                    }
                                    other => panic!("expected named receiver, got {:?}", other),
                                }
                                assert_eq!(method, "build");
                            }
                            other => panic!("expected static method call, got {:?}", other),
                        },
                        other => panic!("expected return stmt, got {:?}", other),
                    }
                }
                other => panic!("expected class decl, got {:?}", other),
            }
        }
        other => panic!("expected namespace block, got {:?}", other),
    }
}

#[test]
fn test_parse_packed_class_and_typed_buffer_decl() {
    let stmts = parse_source(
        "<?php packed class Vec2 { public float $x; public float $y; } buffer<Vec2> $points = buffer_new<Vec2>(4);",
    );
    assert_eq!(stmts.len(), 2);

    match &stmts[0].kind {
        StmtKind::PackedClassDecl { name, fields } => {
            assert_eq!(name, "Vec2");
            assert_eq!(fields.len(), 2);
            assert_eq!(fields[0].name, "x");
            assert_eq!(fields[0].type_expr, TypeExpr::Float);
            assert_eq!(fields[1].name, "y");
            assert_eq!(fields[1].type_expr, TypeExpr::Float);
        }
        other => panic!("expected packed class decl, got {:?}", other),
    }

    match &stmts[1].kind {
        StmtKind::TypedAssign {
            type_expr,
            name,
            value,
        } => {
            assert_eq!(name, "points");
            assert_eq!(
                type_expr,
                &TypeExpr::Buffer(Box::new(TypeExpr::Named(Name::unqualified("Vec2"))))
            );
            match &value.kind {
                ExprKind::BufferNew { element_type, len } => {
                    assert_eq!(element_type, &TypeExpr::Named(Name::unqualified("Vec2")));
                    assert_eq!(len.kind, ExprKind::IntLiteral(4));
                }
                other => panic!("expected buffer_new, got {:?}", other),
            }
        }
        other => panic!("expected typed assign, got {:?}", other),
    }
}

#[test]
fn test_parse_buffer_packed_element_field_access() {
    let stmts = parse_source("<?php echo $points[0]->x;");
    assert_eq!(stmts.len(), 1);
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::PropertyAccess { object, property } => {
                assert_eq!(property, "x");
                match &object.kind {
                    ExprKind::ArrayAccess { array, index } => {
                        assert_eq!(array.kind, ExprKind::Variable("points".into()));
                        assert_eq!(index.kind, ExprKind::IntLiteral(0));
                    }
                    other => panic!("expected packed buffer element access, got {:?}", other),
                }
            }
            other => panic!("expected property access, got {:?}", other),
        },
        other => panic!("expected echo, got {:?}", other),
    }
}

// --- Assignment ---

#[test]
fn test_variable_assignment() {
    let stmts = parse_source("<?php $x = 10;");
    assert_eq!(stmts, vec![Stmt::assign("x", Expr::int_lit(10))]);
}

#[test]
fn test_echo_variable() {
    let stmts = parse_source("<?php $x = 5; echo $x;");
    assert_eq!(stmts.len(), 2);
    assert_eq!(stmts[1], Stmt::echo(Expr::var("x")));
}

// --- Unary ---

#[test]
fn test_negative_integer() {
    let stmts = parse_source("<?php echo -7;");
    assert_eq!(stmts, vec![Stmt::echo(Expr::negate(Expr::int_lit(7)))]);
}

// --- Operator precedence ---

#[test]
fn test_arithmetic_precedence() {
    let stmts = parse_source("<?php echo 2 + 3 * 4;");
    let expected = Stmt::echo(Expr::binop(
        Expr::int_lit(2),
        BinOp::Add,
        Expr::binop(Expr::int_lit(3), BinOp::Mul, Expr::int_lit(4)),
    ));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_parenthesized_expr() {
    let stmts = parse_source("<?php echo (2 + 3) * 4;");
    let expected = Stmt::echo(Expr::binop(
        Expr::binop(Expr::int_lit(2), BinOp::Add, Expr::int_lit(3)),
        BinOp::Mul,
        Expr::int_lit(4),
    ));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_left_associativity() {
    let stmts = parse_source("<?php echo 1 - 2 - 3;");
    let expected = Stmt::echo(Expr::binop(
        Expr::binop(Expr::int_lit(1), BinOp::Sub, Expr::int_lit(2)),
        BinOp::Sub,
        Expr::int_lit(3),
    ));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_concat_operator() {
    let stmts = parse_source("<?php echo \"a\" . \"b\";");
    let expected = Stmt::echo(Expr::binop(
        Expr::string_lit("a"),
        BinOp::Concat,
        Expr::string_lit("b"),
    ));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_comparison_lower_than_arithmetic() {
    // 1 + 2 == 3 should parse as (1 + 2) == 3
    let stmts = parse_source("<?php echo 1 + 2 == 3;");
    let expected = Stmt::echo(Expr::binop(
        Expr::binop(Expr::int_lit(1), BinOp::Add, Expr::int_lit(2)),
        BinOp::Eq,
        Expr::int_lit(3),
    ));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_concat_higher_than_comparison() {
    // "x" . 1 < 2 should parse as ("x" . 1) < 2 — PHP precedence
    let stmts = parse_source("<?php echo \"x\" . 1 < 2;");
    let expected = Stmt::echo(Expr::binop(
        Expr::binop(Expr::string_lit("x"), BinOp::Concat, Expr::int_lit(1)),
        BinOp::Lt,
        Expr::int_lit(2),
    ));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_modulo_same_as_multiply() {
    // 10 % 3 * 2 should parse as (10 % 3) * 2
    let stmts = parse_source("<?php echo 10 % 3 * 2;");
    let expected = Stmt::echo(Expr::binop(
        Expr::binop(Expr::int_lit(10), BinOp::Mod, Expr::int_lit(3)),
        BinOp::Mul,
        Expr::int_lit(2),
    ));
    assert_eq!(stmts, vec![expected]);
}

// --- Control flow ---

#[test]
fn test_if_parses() {
    let stmts = parse_source("<?php if (1 == 1) { echo \"yes\"; }");
    assert_eq!(stmts.len(), 1);
    assert!(matches!(&stmts[0].kind, StmtKind::If { .. }));
}

#[test]
fn test_if_else_parses() {
    let stmts = parse_source("<?php if (1) { echo \"a\"; } else { echo \"b\"; }");
    if let StmtKind::If { else_body, .. } = &stmts[0].kind {
        assert!(else_body.is_some());
    } else {
        panic!("expected If");
    }
}

#[test]
fn test_if_elseif_else_parses() {
    let stmts = parse_source(
        "<?php if (1) { echo \"a\"; } elseif (2) { echo \"b\"; } else { echo \"c\"; }",
    );
    if let StmtKind::If {
        elseif_clauses,
        else_body,
        ..
    } = &stmts[0].kind
    {
        assert_eq!(elseif_clauses.len(), 1);
        assert!(else_body.is_some());
    } else {
        panic!("expected If");
    }
}

#[test]
fn test_while_parses() {
    let stmts = parse_source("<?php while (1) { echo \"loop\"; }");
    assert!(matches!(&stmts[0].kind, StmtKind::While { .. }));
}

#[test]
fn test_do_while_parses() {
    let stmts = parse_source("<?php do { echo \"loop\"; } while (1);");
    assert!(matches!(&stmts[0].kind, StmtKind::DoWhile { .. }));
}

#[test]
fn test_for_parses() {
    let stmts = parse_source("<?php for ($i = 0; $i < 10; $i++) { echo $i; }");
    assert!(matches!(&stmts[0].kind, StmtKind::For { .. }));
}

#[test]
fn test_break_parses() {
    let stmts = parse_source("<?php while (1) { break; }");
    if let StmtKind::While { body, .. } = &stmts[0].kind {
        assert!(matches!(&body[0].kind, StmtKind::Break));
    }
}

#[test]
fn test_continue_parses() {
    let stmts = parse_source("<?php while (1) { continue; }");
    if let StmtKind::While { body, .. } = &stmts[0].kind {
        assert!(matches!(&body[0].kind, StmtKind::Continue));
    }
}

// --- Functions ---

#[test]
fn test_function_declaration_parses() {
    let stmts = parse_source("<?php function foo($a, $b) { return $a; }");
    if let StmtKind::FunctionDecl {
        name, params, body, ..
    } = &stmts[0].kind
    {
        assert_eq!(name, "foo");
        let param_names: Vec<&str> = params.iter().map(|(n, _, _, _)| n.as_str()).collect();
        assert_eq!(param_names, &["a", "b"]);
        assert_eq!(body.len(), 1);
    } else {
        panic!("expected FunctionDecl");
    }
}

#[test]
fn test_function_no_params() {
    let stmts = parse_source("<?php function noop() { return; }");
    if let StmtKind::FunctionDecl { params, .. } = &stmts[0].kind {
        assert!(params.is_empty());
    }
}

#[test]
fn test_return_value_parses() {
    let stmts = parse_source("<?php function f() { return 42; }");
    if let StmtKind::FunctionDecl { body, .. } = &stmts[0].kind {
        assert!(matches!(&body[0].kind, StmtKind::Return(Some(_))));
    }
}

#[test]
fn test_return_void_parses() {
    let stmts = parse_source("<?php function f() { return; }");
    if let StmtKind::FunctionDecl { body, .. } = &stmts[0].kind {
        assert!(matches!(&body[0].kind, StmtKind::Return(None)));
    }
}

#[test]
fn test_multiple_statements() {
    let stmts = parse_source("<?php $a = 1; $b = 2; echo $a;");
    assert_eq!(stmts.len(), 3);
}

// --- Parse errors ---

#[test]
fn test_missing_semicolon() {
    assert!(parse_fails("<?php echo \"hi\""));
}

#[test]
fn test_missing_closing_brace() {
    assert!(parse_fails("<?php if (1) { echo \"a\";"));
}

#[test]
fn test_missing_condition_parens() {
    assert!(parse_fails("<?php if 1 { echo \"a\"; }"));
}

// --- Strict comparison ---

#[test]
fn test_strict_equal_parses() {
    let stmts = parse_source("<?php echo 1 === 1;");
    let expected = Stmt::echo(Expr::binop(
        Expr::int_lit(1),
        BinOp::StrictEq,
        Expr::int_lit(1),
    ));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_strict_not_equal_parses() {
    let stmts = parse_source("<?php echo 1 !== 2;");
    let expected = Stmt::echo(Expr::binop(
        Expr::int_lit(1),
        BinOp::StrictNotEq,
        Expr::int_lit(2),
    ));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_strict_equal_same_precedence_as_loose() {
    // 1 + 2 === 3 should parse as (1 + 2) === 3
    let stmts = parse_source("<?php echo 1 + 2 === 3;");
    let expected = Stmt::echo(Expr::binop(
        Expr::binop(Expr::int_lit(1), BinOp::Add, Expr::int_lit(2)),
        BinOp::StrictEq,
        Expr::int_lit(3),
    ));
    assert_eq!(stmts, vec![expected]);
}

// --- Include/Require ---

#[test]
fn test_include_parses() {
    let stmts = parse_source("<?php include 'file.php';");
    assert_eq!(stmts.len(), 1);
    if let StmtKind::Include {
        path,
        once,
        required,
    } = &stmts[0].kind
    {
        assert_eq!(path, "file.php");
        assert!(!once);
        assert!(!required);
    } else {
        panic!("expected Include");
    }
}

#[test]
fn test_require_parses() {
    let stmts = parse_source("<?php require 'file.php';");
    if let StmtKind::Include {
        path,
        once,
        required,
    } = &stmts[0].kind
    {
        assert_eq!(path, "file.php");
        assert!(!once);
        assert!(required);
    } else {
        panic!("expected Include (require)");
    }
}

#[test]
fn test_include_once_parses() {
    let stmts = parse_source("<?php include_once 'file.php';");
    if let StmtKind::Include { once, required, .. } = &stmts[0].kind {
        assert!(once);
        assert!(!required);
    } else {
        panic!("expected Include (include_once)");
    }
}

#[test]
fn test_require_once_parses() {
    let stmts = parse_source("<?php require_once 'file.php';");
    if let StmtKind::Include { once, required, .. } = &stmts[0].kind {
        assert!(once);
        assert!(required);
    } else {
        panic!("expected Include (require_once)");
    }
}

#[test]
fn test_include_with_parens_parses() {
    let stmts = parse_source("<?php include('file.php');");
    if let StmtKind::Include { path, .. } = &stmts[0].kind {
        assert_eq!(path, "file.php");
    } else {
        panic!("expected Include");
    }
}

// --- Exponentiation ---

#[test]
fn test_pow_operator_parses() {
    let stmts = parse_source("<?php echo 2 ** 3;");
    let expected = Stmt::echo(Expr::binop(Expr::int_lit(2), BinOp::Pow, Expr::int_lit(3)));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_pow_right_associative_parse() {
    // 2 ** 3 ** 2 should parse as 2 ** (3 ** 2)
    let stmts = parse_source("<?php echo 2 ** 3 ** 2;");
    let expected = Stmt::echo(Expr::binop(
        Expr::int_lit(2),
        BinOp::Pow,
        Expr::binop(Expr::int_lit(3), BinOp::Pow, Expr::int_lit(2)),
    ));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_pow_higher_than_mul_parse() {
    // 3 * 2 ** 3 should parse as 3 * (2 ** 3)
    let stmts = parse_source("<?php echo 3 * 2 ** 3;");
    let expected = Stmt::echo(Expr::binop(
        Expr::int_lit(3),
        BinOp::Mul,
        Expr::binop(Expr::int_lit(2), BinOp::Pow, Expr::int_lit(3)),
    ));
    assert_eq!(stmts, vec![expected]);
}

// --- Type casting ---

#[test]
fn test_cast_int_parses() {
    let stmts = parse_source("<?php echo (int)3.14;");
    assert_eq!(stmts.len(), 1);
}

#[test]
fn test_cast_not_confused_with_parens() {
    // (1 + 2) should NOT be parsed as a cast
    let stmts = parse_source("<?php echo (1 + 2);");
    assert_eq!(stmts.len(), 1);
}

// --- Float ---

#[test]
fn test_float_literal() {
    let stmts = parse_source("<?php echo 3.14;");
    assert_eq!(stmts, vec![Stmt::echo(Expr::float_lit(3.14))]);
}

#[test]
fn test_negative_float() {
    let stmts = parse_source("<?php echo -3.14;");
    assert_eq!(stmts, vec![Stmt::echo(Expr::negate(Expr::float_lit(3.14)))]);
}

// --- Associative arrays ---

#[test]
fn test_parse_assoc_array() {
    let stmts = parse_source("<?php $m = [\"a\" => 1];");
    assert_eq!(stmts.len(), 1);
    if let StmtKind::Assign { value, .. } = &stmts[0].kind {
        assert!(matches!(&value.kind, ExprKind::ArrayLiteralAssoc(_)));
    } else {
        panic!("expected Assign");
    }
}

// --- Switch ---

#[test]
fn test_parse_switch() {
    let stmts =
        parse_source("<?php switch ($x) { case 1: echo \"one\"; break; default: echo \"other\"; }");
    assert_eq!(stmts.len(), 1);
    assert!(matches!(&stmts[0].kind, StmtKind::Switch { .. }));
}

// --- Match ---

#[test]
fn test_parse_match() {
    let stmts = parse_source("<?php $x = match(1) { 1 => \"a\" };");
    assert_eq!(stmts.len(), 1);
    if let StmtKind::Assign { value, .. } = &stmts[0].kind {
        assert!(matches!(&value.kind, ExprKind::Match { .. }));
    } else {
        panic!("expected Assign containing Match");
    }
}

// --- Foreach with key => value ---

#[test]
fn test_parse_foreach_key_value() {
    let stmts = parse_source("<?php foreach ($a as $k => $v) {}");
    assert_eq!(stmts.len(), 1);
    if let StmtKind::Foreach {
        key_var, value_var, ..
    } = &stmts[0].kind
    {
        assert_eq!(key_var, &Some("k".to_string()));
        assert_eq!(value_var, "v");
    } else {
        panic!("expected Foreach");
    }
}

#[test]
fn test_parse_foreach_value_only() {
    let stmts = parse_source("<?php foreach ($a as $value) {}");
    assert_eq!(stmts.len(), 1);
    if let StmtKind::Foreach {
        key_var, value_var, ..
    } = &stmts[0].kind
    {
        assert_eq!(key_var, &None);
        assert_eq!(value_var, "value");
    } else {
        panic!("expected Foreach");
    }
}

#[test]
fn test_print_parses_as_echo_statement() {
    let stmts = parse_source("<?php print \"hello\";");
    assert_eq!(stmts, vec![Stmt::echo(Expr::string_lit("hello"))]);
}

#[test]
fn test_parse_closure() {
    let stmts = parse_source("<?php $fn = function($x) { return $x; };");
    assert_eq!(stmts.len(), 1);
    if let StmtKind::Assign { value, .. } = &stmts[0].kind {
        if let ExprKind::Closure {
            params, is_arrow, ..
        } = &value.kind
        {
            let param_names: Vec<&str> = params.iter().map(|(n, _, _, _)| n.as_str()).collect();
            assert_eq!(param_names, &["x"]);
            assert!(!is_arrow);
        } else {
            panic!("expected Closure");
        }
    } else {
        panic!("expected Assign");
    }
}

#[test]
fn test_parse_typed_closure_param() {
    let stmts = parse_source("<?php $fn = function(int &$x) { return $x; };");
    assert_eq!(stmts.len(), 1);
    if let StmtKind::Assign { value, .. } = &stmts[0].kind {
        if let ExprKind::Closure {
            params, is_arrow, ..
        } = &value.kind
        {
            assert_eq!(params.len(), 1);
            assert_eq!(params[0].0, "x");
            assert_eq!(params[0].1, Some(TypeExpr::Int));
            assert!(params[0].3);
            assert!(!is_arrow);
        } else {
            panic!("expected Closure");
        }
    } else {
        panic!("expected Assign");
    }
}

#[test]
fn test_parse_arrow_function() {
    let stmts = parse_source("<?php $fn = fn($x) => $x * 2;");
    assert_eq!(stmts.len(), 1);
    if let StmtKind::Assign { value, .. } = &stmts[0].kind {
        if let ExprKind::Closure {
            params, is_arrow, ..
        } = &value.kind
        {
            let param_names: Vec<&str> = params.iter().map(|(n, _, _, _)| n.as_str()).collect();
            assert_eq!(param_names, &["x"]);
            assert!(is_arrow);
        } else {
            panic!("expected Closure (arrow)");
        }
    } else {
        panic!("expected Assign");
    }
}

#[test]
fn test_parse_typed_arrow_function_param() {
    let stmts = parse_source("<?php $fn = fn(string $label) => $label;");
    assert_eq!(stmts.len(), 1);
    if let StmtKind::Assign { value, .. } = &stmts[0].kind {
        if let ExprKind::Closure {
            params, is_arrow, ..
        } = &value.kind
        {
            assert_eq!(params.len(), 1);
            assert_eq!(params[0].0, "label");
            assert_eq!(params[0].1, Some(TypeExpr::Str));
            assert!(is_arrow);
        } else {
            panic!("expected Closure (arrow)");
        }
    } else {
        panic!("expected Assign");
    }
}

#[test]
fn test_parse_closure_call() {
    let stmts = parse_source("<?php $fn(1, 2);");
    assert_eq!(stmts.len(), 1);
    if let StmtKind::ExprStmt(expr) = &stmts[0].kind {
        if let ExprKind::ClosureCall { var, args } = &expr.kind {
            assert_eq!(var, "fn");
            assert_eq!(args.len(), 2);
        } else {
            panic!("expected ClosureCall");
        }
    } else {
        panic!("expected ExprStmt");
    }
}

#[test]
fn test_parse_named_function_call() {
    let stmts = parse_source("<?php greet(name: \"Alice\", age: 30);");
    if let StmtKind::ExprStmt(expr) = &stmts[0].kind {
        if let ExprKind::FunctionCall { name, args } = &expr.kind {
            assert_eq!(name.as_str(), "greet");
            assert_eq!(args.len(), 2);
            assert!(matches!(
                args[0].kind,
                ExprKind::NamedArg { ref name, .. } if name == "name"
            ));
            assert!(matches!(
                args[1].kind,
                ExprKind::NamedArg { ref name, .. } if name == "age"
            ));
        } else {
            panic!("expected FunctionCall");
        }
    } else {
        panic!("expected ExprStmt");
    }
}

#[test]
fn test_parse_named_constructor_call() {
    let stmts = parse_source("<?php $user = new User(id: 42);");
    if let StmtKind::Assign { value: expr, .. } = &stmts[0].kind {
        if let ExprKind::NewObject { class_name, args } = &expr.kind {
            assert_eq!(class_name.as_str(), "User");
            assert_eq!(args.len(), 1);
            assert!(matches!(
                args[0].kind,
                ExprKind::NamedArg { ref name, .. } if name == "id"
            ));
        } else {
            panic!("expected NewObject");
        }
    } else {
        panic!("expected Assign");
    }
}

// --- Default parameter values ---

#[test]
fn test_parse_function_default_params() {
    let stmts = parse_source("<?php function foo($a, $b = 10) { return $a + $b; }");
    if let StmtKind::FunctionDecl { params, .. } = &stmts[0].kind {
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].0, "a");
        assert!(params[0].2.is_none());
        assert_eq!(params[1].0, "b");
        assert!(params[1].2.is_some());
    } else {
        panic!("expected FunctionDecl");
    }
}

// --- Bitwise operator precedence ---

#[test]
fn test_bitwise_and_lower_than_equality() {
    // 1 == 1 & 0 should parse as (1 == 1) & 0 — PHP precedence
    let stmts = parse_source("<?php echo 1 == 1 & 0;");
    let expected = Stmt::echo(Expr::binop(
        Expr::binop(Expr::int_lit(1), BinOp::Eq, Expr::int_lit(1)),
        BinOp::BitAnd,
        Expr::int_lit(0),
    ));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_shift_higher_than_comparison() {
    // 1 << 2 < 10 should parse as (1 << 2) < 10 — PHP precedence
    let stmts = parse_source("<?php echo 1 << 2 < 10;");
    let expected = Stmt::echo(Expr::binop(
        Expr::binop(Expr::int_lit(1), BinOp::ShiftLeft, Expr::int_lit(2)),
        BinOp::Lt,
        Expr::int_lit(10),
    ));
    assert_eq!(stmts, vec![expected]);
}

// --- Null coalescing precedence ---

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

#[test]
fn test_const_decl_int() {
    let stmts = parse_source("<?php const MAX = 100;");
    assert_eq!(stmts.len(), 1);
    match &stmts[0].kind {
        StmtKind::ConstDecl { name, value } => {
            assert_eq!(name, "MAX");
            assert_eq!(value.kind, ExprKind::IntLiteral(100));
        }
        _ => panic!("Expected ConstDecl"),
    }
}

#[test]
fn test_const_decl_string() {
    let stmts = parse_source("<?php const NAME = \"hello\";");
    assert_eq!(stmts.len(), 1);
    match &stmts[0].kind {
        StmtKind::ConstDecl { name, value } => {
            assert_eq!(name, "NAME");
            assert_eq!(value.kind, ExprKind::StringLiteral("hello".into()));
        }
        _ => panic!("Expected ConstDecl"),
    }
}

#[test]
fn test_const_ref_in_echo() {
    let stmts = parse_source("<?php echo MAX;");
    assert_eq!(stmts.len(), 1);
    match &stmts[0].kind {
        StmtKind::Echo(expr) => {
            assert_eq!(expr.kind, ExprKind::ConstRef("MAX".into()));
        }
        _ => panic!("Expected Echo"),
    }
}

#[test]
fn test_parse_backed_enum_decl() {
    let stmts = parse_source("<?php enum Color: int { case Red = 1; case Green = 2; }");
    assert_eq!(stmts.len(), 1);
    match &stmts[0].kind {
        StmtKind::EnumDecl {
            name,
            backing_type,
            cases,
        } => {
            assert_eq!(name, "Color");
            assert_eq!(backing_type, &Some(TypeExpr::Int));
            assert_eq!(cases.len(), 2);
            assert_eq!(cases[0].name, "Red");
            assert_eq!(
                cases[0].value.as_ref().map(|expr| &expr.kind),
                Some(&ExprKind::IntLiteral(1))
            );
            assert_eq!(cases[1].name, "Green");
            assert_eq!(
                cases[1].value.as_ref().map(|expr| &expr.kind),
                Some(&ExprKind::IntLiteral(2))
            );
        }
        other => panic!("Expected EnumDecl, got {:?}", other),
    }
}

#[test]
fn test_parse_enum_case_expr() {
    let stmts = parse_source("<?php echo Color::Red;");
    assert_eq!(stmts.len(), 1);
    match &stmts[0].kind {
        StmtKind::Echo(expr) => {
            assert_eq!(
                expr.kind,
                ExprKind::EnumCase {
                    enum_name: Name::from("Color"),
                    case_name: "Red".to_string(),
                }
            );
        }
        other => panic!("Expected Echo, got {:?}", other),
    }
}

#[test]
fn test_list_unpack() {
    let stmts = parse_source("<?php [$a, $b] = [1, 2];");
    assert_eq!(stmts.len(), 1);
    match &stmts[0].kind {
        StmtKind::ListUnpack { vars, .. } => {
            assert_eq!(vars, &["a".to_string(), "b".to_string()]);
        }
        _ => panic!("Expected ListUnpack"),
    }
}

#[test]
fn test_list_unpack_three_vars() {
    let stmts = parse_source("<?php [$x, $y, $z] = [10, 20, 30];");
    assert_eq!(stmts.len(), 1);
    match &stmts[0].kind {
        StmtKind::ListUnpack { vars, .. } => {
            assert_eq!(vars, &["x".to_string(), "y".to_string(), "z".to_string()]);
        }
        _ => panic!("Expected ListUnpack"),
    }
}

// --- Global ---

#[test]
fn test_parse_global_single() {
    let stmts = parse_source("<?php global $x;");
    assert_eq!(stmts.len(), 1);
    match &stmts[0].kind {
        StmtKind::Global { vars } => {
            assert_eq!(vars, &["x".to_string()]);
        }
        _ => panic!("Expected Global"),
    }
}

#[test]
fn test_parse_global_multiple() {
    let stmts = parse_source("<?php global $a, $b, $c;");
    assert_eq!(stmts.len(), 1);
    match &stmts[0].kind {
        StmtKind::Global { vars } => {
            assert_eq!(vars, &["a".to_string(), "b".to_string(), "c".to_string()]);
        }
        _ => panic!("Expected Global"),
    }
}

// --- Static variable ---

#[test]
fn test_parse_static_var() {
    let stmts = parse_source("<?php static $count = 0;");
    assert_eq!(stmts.len(), 1);
    match &stmts[0].kind {
        StmtKind::StaticVar { name, init } => {
            assert_eq!(name, "count");
            assert_eq!(init.kind, ExprKind::IntLiteral(0));
        }
        _ => panic!("Expected StaticVar"),
    }
}

// --- Pass by reference ---

#[test]
fn test_parse_ref_param() {
    let stmts = parse_source("<?php function foo(&$x) { }");
    assert_eq!(stmts.len(), 1);
    match &stmts[0].kind {
        StmtKind::FunctionDecl { name, params, .. } => {
            assert_eq!(name, "foo");
            assert_eq!(params.len(), 1);
            assert_eq!(params[0].0, "x");
            assert!(params[0].3, "Expected param to be pass-by-reference");
        }
        _ => panic!("Expected FunctionDecl"),
    }
}

#[test]
fn test_parse_mixed_ref_params() {
    let stmts = parse_source("<?php function foo(&$a, $b, &$c) { }");
    assert_eq!(stmts.len(), 1);
    match &stmts[0].kind {
        StmtKind::FunctionDecl { params, .. } => {
            assert_eq!(params.len(), 3);
            assert!(params[0].3, "First param should be ref");
            assert!(!params[1].3, "Second param should not be ref");
            assert!(params[2].3, "Third param should be ref");
        }
        _ => panic!("Expected FunctionDecl"),
    }
}

#[test]
fn test_parse_non_ref_param() {
    let stmts = parse_source("<?php function foo($x) { }");
    match &stmts[0].kind {
        StmtKind::FunctionDecl { params, .. } => {
            assert!(!params[0].3, "Normal param should not be ref");
        }
        _ => panic!("Expected FunctionDecl"),
    }
}

#[test]
fn test_parse_typed_function_param_and_return_type() {
    let stmts = parse_source("<?php function foo(int $x): string { return \"ok\"; }");
    match &stmts[0].kind {
        StmtKind::FunctionDecl {
            name,
            params,
            return_type,
            ..
        } => {
            assert_eq!(name, "foo");
            assert_eq!(params.len(), 1);
            assert_eq!(params[0].0, "x");
            assert_eq!(params[0].1, Some(TypeExpr::Int));
            assert_eq!(return_type.as_ref(), Some(&TypeExpr::Str));
        }
        other => panic!("Expected FunctionDecl, got {:?}", other),
    }
}

#[test]
fn test_parse_union_and_nullable_function_types() {
    let stmts = parse_source("<?php function describe(int|string $value): ?int { return null; }");
    match &stmts[0].kind {
        StmtKind::FunctionDecl {
            params,
            return_type,
            ..
        } => {
            assert_eq!(
                params[0].1,
                Some(TypeExpr::Union(vec![TypeExpr::Int, TypeExpr::Str]))
            );
            assert_eq!(
                return_type.as_ref(),
                Some(&TypeExpr::Nullable(Box::new(TypeExpr::Int)))
            );
        }
        other => panic!("Expected FunctionDecl, got {:?}", other),
    }
}

#[test]
fn test_parse_typed_ref_param() {
    let stmts = parse_source("<?php function bump(int &$x) { }");
    match &stmts[0].kind {
        StmtKind::FunctionDecl { params, .. } => {
            assert_eq!(params.len(), 1);
            assert_eq!(params[0].0, "x");
            assert_eq!(params[0].1, Some(TypeExpr::Int));
            assert!(params[0].3);
        }
        other => panic!("Expected FunctionDecl, got {:?}", other),
    }
}

// --- Variadic and Spread ---

#[test]
fn test_parse_variadic_function() {
    let stmts = parse_source("<?php function foo(...$args) { }");
    match &stmts[0].kind {
        StmtKind::FunctionDecl {
            name,
            params,
            variadic,
            ..
        } => {
            assert_eq!(name, "foo");
            assert!(params.is_empty());
            assert_eq!(variadic.as_deref(), Some("args"));
        }
        _ => panic!("Expected FunctionDecl"),
    }
}

#[test]
fn test_parse_variadic_with_regular_params() {
    let stmts = parse_source("<?php function foo($a, $b, ...$rest) { }");
    match &stmts[0].kind {
        StmtKind::FunctionDecl {
            params, variadic, ..
        } => {
            assert_eq!(params.len(), 2);
            assert_eq!(params[0].0, "a");
            assert_eq!(params[1].0, "b");
            assert_eq!(variadic.as_deref(), Some("rest"));
        }
        _ => panic!("Expected FunctionDecl"),
    }
}

#[test]
fn test_parse_no_variadic() {
    let stmts = parse_source("<?php function foo($a) { }");
    match &stmts[0].kind {
        StmtKind::FunctionDecl { variadic, .. } => {
            assert!(variadic.is_none());
        }
        _ => panic!("Expected FunctionDecl"),
    }
}

#[test]
fn test_parse_typed_variadic_param_fails() {
    assert!(parse_fails("<?php function foo(int ...$xs) { }"));
}

#[test]
fn test_parse_spread_in_function_call() {
    let stmts = parse_source("<?php foo(...$arr);");
    match &stmts[0].kind {
        StmtKind::ExprStmt(expr) => match &expr.kind {
            ExprKind::FunctionCall { args, .. } => {
                assert_eq!(args.len(), 1);
                assert!(matches!(&args[0].kind, ExprKind::Spread(_)));
            }
            _ => panic!("Expected FunctionCall"),
        },
        _ => panic!("Expected ExprStmt"),
    }
}

#[test]
fn test_parse_spread_in_array_literal() {
    let stmts = parse_source("<?php $x = [...$a, ...$b];");
    match &stmts[0].kind {
        StmtKind::Assign { value, .. } => match &value.kind {
            ExprKind::ArrayLiteral(elems) => {
                assert_eq!(elems.len(), 2);
                assert!(matches!(&elems[0].kind, ExprKind::Spread(_)));
                assert!(matches!(&elems[1].kind, ExprKind::Spread(_)));
            }
            _ => panic!("Expected ArrayLiteral"),
        },
        _ => panic!("Expected Assign"),
    }
}

#[test]
fn test_parse_class_decl() {
    let stmts = parse_source("<?php class Point { public $x; private $y = 1; public function get() { return $this->x; } public static function origin() { return new Point(); } }");
    match &stmts[0].kind {
        StmtKind::ClassDecl {
            name,
            extends,
            implements,
            is_abstract,
            trait_uses,
            properties,
            methods,
            ..
        } => {
            assert_eq!(name, "Point");
            assert_eq!(extends, &None);
            assert!(implements.is_empty());
            assert!(!is_abstract);
            assert!(trait_uses.is_empty());
            assert_eq!(properties.len(), 2);
            assert_eq!(properties[0].name, "x");
            assert_eq!(properties[0].visibility, Visibility::Public);
            assert_eq!(properties[1].name, "y");
            assert_eq!(properties[1].visibility, Visibility::Private);
            assert!(properties[1].default.is_some());
            assert_eq!(methods.len(), 2);
            assert_eq!(methods[0].name, "get");
            assert!(!methods[0].is_static);
            assert_eq!(methods[1].name, "origin");
            assert!(methods[1].is_static);
        }
        _ => panic!("Expected ClassDecl"),
    }
}

#[test]
fn test_parse_trait_decl_and_use_adaptations() {
    let stmts = parse_source(
        "<?php trait A { public function foo() { return 1; } } class Box { use A { A::foo as private bar; } }",
    );
    match &stmts[0].kind {
        StmtKind::TraitDecl {
            name,
            trait_uses,
            properties,
            methods,
        } => {
            assert_eq!(name, "A");
            assert!(trait_uses.is_empty());
            assert!(properties.is_empty());
            assert_eq!(methods.len(), 1);
            assert_eq!(methods[0].name, "foo");
        }
        _ => panic!("Expected TraitDecl"),
    }
    match &stmts[1].kind {
        StmtKind::ClassDecl {
            name,
            extends,
            implements,
            is_abstract,
            trait_uses,
            properties,
            methods,
            ..
        } => {
            assert_eq!(name, "Box");
            assert_eq!(extends, &None);
            assert!(implements.is_empty());
            assert!(!is_abstract);
            assert!(properties.is_empty());
            assert!(methods.is_empty());
            assert_eq!(trait_uses.len(), 1);
            assert_eq!(trait_uses[0].trait_names, vec!["A".to_string()]);
            assert_eq!(trait_uses[0].adaptations.len(), 1);
            match &trait_uses[0].adaptations[0] {
                TraitAdaptation::Alias {
                    trait_name,
                    method,
                    alias,
                    visibility,
                } => {
                    assert_eq!(trait_name.as_deref(), Some("A"));
                    assert_eq!(method, "foo");
                    assert_eq!(alias.as_deref(), Some("bar"));
                    assert_eq!(*visibility, Some(Visibility::Private));
                }
                _ => panic!("Expected trait alias adaptation"),
            }
        }
        _ => panic!("Expected ClassDecl"),
    }
}

#[test]
fn test_parse_trait_use_as_protected() {
    let stmts = parse_source(
        "<?php trait A { public function foo() { return 1; } } class Box { use A { A::foo as protected; } }",
    );
    match &stmts[1].kind {
        StmtKind::ClassDecl { trait_uses, .. } => {
            assert_eq!(trait_uses.len(), 1);
            assert_eq!(trait_uses[0].adaptations.len(), 1);
            match &trait_uses[0].adaptations[0] {
                TraitAdaptation::Alias {
                    trait_name,
                    method,
                    alias,
                    visibility,
                } => {
                    assert_eq!(trait_name.as_deref(), Some("A"));
                    assert_eq!(method, "foo");
                    assert_eq!(alias, &None);
                    assert_eq!(*visibility, Some(Visibility::Protected));
                }
                _ => panic!("Expected trait alias adaptation"),
            }
        }
        _ => panic!("Expected ClassDecl"),
    }
}

#[test]
fn test_parse_trait_use_insteadof() {
    let stmts = parse_source(
        "<?php trait A { public function foo() { return 1; } } trait B { public function foo() { return 2; } } class Box { use A, B { A::foo insteadof B; } }",
    );
    match &stmts[2].kind {
        StmtKind::ClassDecl { trait_uses, .. } => {
            assert_eq!(trait_uses.len(), 1);
            assert_eq!(trait_uses[0].adaptations.len(), 1);
            match &trait_uses[0].adaptations[0] {
                TraitAdaptation::InsteadOf {
                    trait_name,
                    method,
                    instead_of,
                } => {
                    assert_eq!(trait_name.as_deref(), Some("A"));
                    assert_eq!(method, "foo");
                    assert_eq!(instead_of, &vec!["B".to_string()]);
                }
                _ => panic!("Expected trait insteadof adaptation"),
            }
        }
        _ => panic!("Expected ClassDecl"),
    }
}

#[test]
fn test_parse_new_object() {
    let stmts = parse_source("<?php $p = new Point(1, 2);");
    match &stmts[0].kind {
        StmtKind::Assign { value, .. } => match &value.kind {
            ExprKind::NewObject { class_name, args } => {
                assert_eq!(class_name, "Point");
                assert_eq!(args.len(), 2);
            }
            _ => panic!("Expected NewObject"),
        },
        _ => panic!("Expected Assign"),
    }
}

#[test]
fn test_parse_property_access() {
    let stmts = parse_source("<?php echo $obj->prop;");
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::PropertyAccess { object, property } => {
                assert_eq!(property, "prop");
                assert!(matches!(object.kind, ExprKind::Variable(_)));
            }
            _ => panic!("Expected PropertyAccess"),
        },
        _ => panic!("Expected Echo"),
    }
}

#[test]
fn test_parse_property_array_access() {
    let stmts = parse_source("<?php echo $obj->items[0];");
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::ArrayAccess { array, index } => {
                assert!(matches!(index.kind, ExprKind::IntLiteral(0)));
                match &array.kind {
                    ExprKind::PropertyAccess { object, property } => {
                        assert_eq!(property, "items");
                        assert!(matches!(object.kind, ExprKind::Variable(_)));
                    }
                    other => panic!("Expected PropertyAccess, got {:?}", other),
                }
            }
            other => panic!("Expected ArrayAccess, got {:?}", other),
        },
        other => panic!("Expected Echo, got {:?}", other),
    }
}

#[test]
fn test_parse_method_call() {
    let stmts = parse_source("<?php $obj->run(1, 2);");
    match &stmts[0].kind {
        StmtKind::ExprStmt(expr) => match &expr.kind {
            ExprKind::MethodCall {
                object,
                method,
                args,
            } => {
                assert_eq!(method, "run");
                assert_eq!(args.len(), 2);
                assert!(matches!(object.kind, ExprKind::Variable(_)));
            }
            _ => panic!("Expected MethodCall"),
        },
        _ => panic!("Expected ExprStmt"),
    }
}

#[test]
fn test_parse_static_method_call() {
    let stmts = parse_source("<?php Factory::make(1);");
    match &stmts[0].kind {
        StmtKind::ExprStmt(expr) => match &expr.kind {
            ExprKind::StaticMethodCall {
                receiver,
                method,
                args,
            } => {
                assert_eq!(receiver, &StaticReceiver::Named("Factory".into()));
                assert_eq!(method, "make");
                assert_eq!(args.len(), 1);
            }
            _ => panic!("Expected StaticMethodCall"),
        },
        _ => panic!("Expected ExprStmt"),
    }
}

#[test]
fn test_parse_static_property_access() {
    let stmts = parse_source("<?php echo Counter::$count;");
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::StaticPropertyAccess { receiver, property } => {
                assert_eq!(receiver, &StaticReceiver::Named("Counter".into()));
                assert_eq!(property, "count");
            }
            _ => panic!("Expected StaticPropertyAccess"),
        },
        _ => panic!("Expected Echo"),
    }
}

#[test]
fn test_parse_static_property_assignment() {
    let stmts = parse_source("<?php self::$count = 2;");
    match &stmts[0].kind {
        StmtKind::StaticPropertyAssign {
            receiver,
            property,
            value,
        } => {
            assert_eq!(receiver, &StaticReceiver::Self_);
            assert_eq!(property, "count");
            assert!(matches!(value.kind, ExprKind::IntLiteral(2)));
        }
        _ => panic!("Expected StaticPropertyAssign"),
    }
}

#[test]
fn test_parse_static_property_array_push() {
    let stmts = parse_source("<?php Counter::$items[] = 2;");
    match &stmts[0].kind {
        StmtKind::StaticPropertyArrayPush {
            receiver,
            property,
            value,
        } => {
            assert_eq!(receiver, &StaticReceiver::Named("Counter".into()));
            assert_eq!(property, "items");
            assert!(matches!(value.kind, ExprKind::IntLiteral(2)));
        }
        other => panic!("Expected StaticPropertyArrayPush, got {:?}", other),
    }
}

#[test]
fn test_parse_static_property_array_assign() {
    let stmts = parse_source("<?php Counter::$items[1] = 3;");
    match &stmts[0].kind {
        StmtKind::StaticPropertyArrayAssign {
            receiver,
            property,
            index,
            value,
        } => {
            assert_eq!(receiver, &StaticReceiver::Named("Counter".into()));
            assert_eq!(property, "items");
            assert!(matches!(index.kind, ExprKind::IntLiteral(1)));
            assert!(matches!(value.kind, ExprKind::IntLiteral(3)));
        }
        other => panic!("Expected StaticPropertyArrayAssign, got {:?}", other),
    }
}

#[test]
fn test_parse_static_property_declaration() {
    let stmts = parse_source("<?php class Counter { public static int $count = 1; }");
    match &stmts[0].kind {
        StmtKind::ClassDecl { properties, .. } => {
            assert_eq!(properties.len(), 1);
            assert_eq!(properties[0].name, "count");
            assert_eq!(properties[0].visibility, Visibility::Public);
            assert!(properties[0].is_static);
            assert!(properties[0].type_expr.is_some());
            assert!(properties[0].default.is_some());
        }
        _ => panic!("Expected ClassDecl"),
    }
}

#[test]
fn test_parse_class_decl_with_extends() {
    let stmts =
        parse_source("<?php class Child extends Base { public function run() { return 1; } }");
    match &stmts[0].kind {
        StmtKind::ClassDecl {
            name,
            extends,
            methods,
            ..
        } => {
            assert_eq!(name, "Child");
            assert_eq!(extends.as_deref(), Some("Base"));
            assert_eq!(methods.len(), 1);
            assert_eq!(methods[0].name, "run");
        }
        _ => panic!("Expected ClassDecl"),
    }
}

#[test]
fn test_parse_interface_decl() {
    let stmts = parse_source(
        "<?php interface Named extends Renderable, Jsonable { public function name(); }",
    );
    match &stmts[0].kind {
        StmtKind::InterfaceDecl {
            name,
            extends,
            methods,
        } => {
            assert_eq!(name, "Named");
            assert_eq!(
                extends,
                &vec!["Renderable".to_string(), "Jsonable".to_string()]
            );
            assert_eq!(methods.len(), 1);
            assert_eq!(methods[0].name, "name");
            assert!(methods[0].is_abstract);
            assert!(!methods[0].has_body);
            assert!(methods[0].body.is_empty());
        }
        _ => panic!("Expected InterfaceDecl"),
    }
}

#[test]
fn test_parse_abstract_class_with_implements() {
    let stmts = parse_source(
        "<?php abstract class Base implements Named, Tagged { abstract protected function load(); }",
    );
    match &stmts[0].kind {
        StmtKind::ClassDecl {
            name,
            implements,
            is_abstract,
            methods,
            ..
        } => {
            assert_eq!(name, "Base");
            assert_eq!(implements, &vec!["Named".to_string(), "Tagged".to_string()]);
            assert!(*is_abstract);
            assert_eq!(methods.len(), 1);
            assert_eq!(methods[0].name, "load");
            assert!(methods[0].is_abstract);
            assert!(!methods[0].has_body);
            assert!(methods[0].body.is_empty());
        }
        _ => panic!("Expected ClassDecl"),
    }
}

#[test]
fn test_parse_parent_static_receiver() {
    let stmts = parse_source("<?php parent::boot();");
    match &stmts[0].kind {
        StmtKind::ExprStmt(expr) => match &expr.kind {
            ExprKind::StaticMethodCall {
                receiver,
                method,
                args,
            } => {
                assert_eq!(receiver, &StaticReceiver::Parent);
                assert_eq!(method, "boot");
                assert!(args.is_empty());
            }
            _ => panic!("Expected StaticMethodCall"),
        },
        _ => panic!("Expected ExprStmt"),
    }
}

#[test]
fn test_parse_self_static_receiver() {
    let stmts = parse_source("<?php self::boot();");
    match &stmts[0].kind {
        StmtKind::ExprStmt(expr) => match &expr.kind {
            ExprKind::StaticMethodCall {
                receiver,
                method,
                args,
            } => {
                assert_eq!(receiver, &StaticReceiver::Self_);
                assert_eq!(method, "boot");
                assert!(args.is_empty());
            }
            _ => panic!("Expected StaticMethodCall"),
        },
        _ => panic!("Expected ExprStmt"),
    }
}

#[test]
fn test_parse_static_static_receiver() {
    let stmts = parse_source("<?php static::boot();");
    match &stmts[0].kind {
        StmtKind::ExprStmt(expr) => match &expr.kind {
            ExprKind::StaticMethodCall {
                receiver,
                method,
                args,
            } => {
                assert_eq!(receiver, &StaticReceiver::Static);
                assert_eq!(method, "boot");
                assert!(args.is_empty());
            }
            _ => panic!("Expected StaticMethodCall"),
        },
        _ => panic!("Expected ExprStmt"),
    }
}

#[test]
fn test_parse_property_assign() {
    let stmts = parse_source("<?php $obj->prop = 42;");
    match &stmts[0].kind {
        StmtKind::PropertyAssign {
            object,
            property,
            value,
        } => {
            assert_eq!(property, "prop");
            assert!(matches!(object.kind, ExprKind::Variable(_)));
            assert!(matches!(value.kind, ExprKind::IntLiteral(42)));
        }
        _ => panic!("Expected PropertyAssign"),
    }
}

#[test]
fn test_parse_property_array_push() {
    let stmts = parse_source("<?php $obj->entries[] = $item;");
    match &stmts[0].kind {
        StmtKind::PropertyArrayPush {
            object,
            property,
            value,
        } => {
            assert_eq!(property, "entries");
            assert!(matches!(object.kind, ExprKind::Variable(_)));
            assert!(matches!(value.kind, ExprKind::Variable(_)));
        }
        other => panic!("Expected PropertyArrayPush, got {:?}", other),
    }
}

#[test]
fn test_parse_property_array_assign() {
    let stmts = parse_source("<?php $obj->items[0] = 42;");
    match &stmts[0].kind {
        StmtKind::PropertyArrayAssign {
            object,
            property,
            index,
            value,
        } => {
            assert_eq!(property, "items");
            assert!(matches!(object.kind, ExprKind::Variable(_)));
            assert!(matches!(index.kind, ExprKind::IntLiteral(0)));
            assert!(matches!(value.kind, ExprKind::IntLiteral(42)));
        }
        other => panic!("Expected PropertyArrayAssign, got {:?}", other),
    }
}

#[test]
fn test_parse_deep_property_assign_after_array_access() {
    let stmts = parse_source("<?php $catalog->palette->colors[$i]->r = 12;");
    match &stmts[0].kind {
        StmtKind::PropertyAssign {
            object,
            property,
            value,
        } => {
            assert_eq!(property, "r");
            assert!(matches!(value.kind, ExprKind::IntLiteral(12)));
            match &object.kind {
                ExprKind::ArrayAccess { array, index } => {
                    assert!(matches!(index.kind, ExprKind::Variable(ref name) if name == "i"));
                    match &array.kind {
                        ExprKind::PropertyAccess { object, property } => {
                            assert_eq!(property, "colors");
                            match &object.kind {
                                ExprKind::PropertyAccess { object, property } => {
                                    assert_eq!(property, "palette");
                                    assert!(
                                        matches!(object.kind, ExprKind::Variable(ref name) if name == "catalog")
                                    );
                                }
                                other => {
                                    panic!("Expected nested PropertyAccess, got {:?}", other)
                                }
                            }
                        }
                        other => panic!("Expected PropertyAccess, got {:?}", other),
                    }
                }
                other => panic!("Expected ArrayAccess, got {:?}", other),
            }
        }
        other => panic!("Expected PropertyAssign, got {:?}", other),
    }
}

#[test]
fn test_parse_deep_property_array_assign_after_array_access() {
    let stmts = parse_source("<?php $catalog->palette->colors[$i]->shades[1] = 12;");
    match &stmts[0].kind {
        StmtKind::PropertyArrayAssign {
            object,
            property,
            index,
            value,
        } => {
            assert_eq!(property, "shades");
            assert!(matches!(index.kind, ExprKind::IntLiteral(1)));
            assert!(matches!(value.kind, ExprKind::IntLiteral(12)));
            match &object.kind {
                ExprKind::ArrayAccess { array, index } => {
                    assert!(matches!(index.kind, ExprKind::Variable(ref name) if name == "i"));
                    match &array.kind {
                        ExprKind::PropertyAccess { object, property } => {
                            assert_eq!(property, "colors");
                            match &object.kind {
                                ExprKind::PropertyAccess { object, property } => {
                                    assert_eq!(property, "palette");
                                    assert!(
                                        matches!(object.kind, ExprKind::Variable(ref name) if name == "catalog")
                                    );
                                }
                                other => {
                                    panic!("Expected nested PropertyAccess, got {:?}", other)
                                }
                            }
                        }
                        other => panic!("Expected PropertyAccess, got {:?}", other),
                    }
                }
                other => panic!("Expected ArrayAccess, got {:?}", other),
            }
        }
        other => panic!("Expected PropertyArrayAssign, got {:?}", other),
    }
}

#[test]
fn test_parse_deep_property_array_push_after_array_access() {
    let stmts = parse_source("<?php $catalog->palette->colors[$i]->shades[] = 12;");
    match &stmts[0].kind {
        StmtKind::PropertyArrayPush {
            object,
            property,
            value,
        } => {
            assert_eq!(property, "shades");
            assert!(matches!(value.kind, ExprKind::IntLiteral(12)));
            match &object.kind {
                ExprKind::ArrayAccess { array, index } => {
                    assert!(matches!(index.kind, ExprKind::Variable(ref name) if name == "i"));
                    match &array.kind {
                        ExprKind::PropertyAccess { object, property } => {
                            assert_eq!(property, "colors");
                            match &object.kind {
                                ExprKind::PropertyAccess { object, property } => {
                                    assert_eq!(property, "palette");
                                    assert!(
                                        matches!(object.kind, ExprKind::Variable(ref name) if name == "catalog")
                                    );
                                }
                                other => {
                                    panic!("Expected nested PropertyAccess, got {:?}", other)
                                }
                            }
                        }
                        other => panic!("Expected PropertyAccess, got {:?}", other),
                    }
                }
                other => panic!("Expected ArrayAccess, got {:?}", other),
            }
        }
        other => panic!("Expected PropertyArrayPush, got {:?}", other),
    }
}

#[test]
fn test_parse_chained_access() {
    let stmts = parse_source("<?php echo $obj->make()->prop;");
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::PropertyAccess { object, property } => {
                assert_eq!(property, "prop");
                match &object.kind {
                    ExprKind::MethodCall {
                        object,
                        method,
                        args,
                    } => {
                        assert_eq!(method, "make");
                        assert!(args.is_empty());
                        assert!(matches!(object.kind, ExprKind::Variable(_)));
                    }
                    _ => panic!("Expected MethodCall inside chained access"),
                }
            }
            _ => panic!("Expected PropertyAccess"),
        },
        _ => panic!("Expected Echo"),
    }
}

#[test]
fn test_parse_property_access_after_array_index() {
    let stmts = parse_source("<?php echo $items[0]->name;");
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::PropertyAccess { object, property } => {
                assert_eq!(property, "name");
                match &object.kind {
                    ExprKind::ArrayAccess { array, index } => {
                        assert!(matches!(array.kind, ExprKind::Variable(_)));
                        assert!(matches!(index.kind, ExprKind::IntLiteral(0)));
                    }
                    other => panic!("Expected ArrayAccess, got {:?}", other),
                }
            }
            other => panic!("Expected PropertyAccess, got {:?}", other),
        },
        other => panic!("Expected Echo, got {:?}", other),
    }
}

#[test]
fn test_parse_array_access_on_function_call_result() {
    let stmts = parse_source("<?php echo getColor()[0];");
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::ArrayAccess { array, index } => {
                assert!(matches!(index.kind, ExprKind::IntLiteral(0)));
                match &array.kind {
                    ExprKind::FunctionCall { name, args } => {
                        assert_eq!(name.as_str(), "getColor");
                        assert!(args.is_empty());
                    }
                    other => panic!("Expected FunctionCall, got {:?}", other),
                }
            }
            other => panic!("Expected ArrayAccess, got {:?}", other),
        },
        other => panic!("Expected Echo, got {:?}", other),
    }
}

#[test]
fn test_parse_deep_mixed_property_and_array_chain() {
    let stmts = parse_source("<?php echo $catalog->palette->colors[$i]->r;");
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::PropertyAccess { object, property } => {
                assert_eq!(property, "r");
                match &object.kind {
                    ExprKind::ArrayAccess { array, index } => {
                        assert!(matches!(index.kind, ExprKind::Variable(ref name) if name == "i"));
                        match &array.kind {
                            ExprKind::PropertyAccess { object, property } => {
                                assert_eq!(property, "colors");
                                match &object.kind {
                                    ExprKind::PropertyAccess { object, property } => {
                                        assert_eq!(property, "palette");
                                        assert!(matches!(object.kind, ExprKind::Variable(ref name) if name == "catalog"));
                                    }
                                    other => {
                                        panic!("Expected nested PropertyAccess, got {:?}", other)
                                    }
                                }
                            }
                            other => panic!("Expected PropertyAccess, got {:?}", other),
                        }
                    }
                    other => panic!("Expected ArrayAccess, got {:?}", other),
                }
            }
            other => panic!("Expected PropertyAccess, got {:?}", other),
        },
        other => panic!("Expected Echo, got {:?}", other),
    }
}

#[test]
fn test_parse_property_access_after_array_access_on_method_call_result() {
    let stmts = parse_source("<?php echo $shop->getItems()[0]->name;");
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::PropertyAccess { object, property } => {
                assert_eq!(property, "name");
                match &object.kind {
                    ExprKind::ArrayAccess { array, index } => {
                        assert!(matches!(index.kind, ExprKind::IntLiteral(0)));
                        match &array.kind {
                            ExprKind::MethodCall {
                                object,
                                method,
                                args,
                            } => {
                                assert_eq!(method, "getItems");
                                assert!(args.is_empty());
                                assert!(
                                    matches!(object.kind, ExprKind::Variable(ref name) if name == "shop")
                                );
                            }
                            other => panic!("Expected MethodCall, got {:?}", other),
                        }
                    }
                    other => panic!("Expected ArrayAccess, got {:?}", other),
                }
            }
            other => panic!("Expected PropertyAccess, got {:?}", other),
        },
        other => panic!("Expected Echo, got {:?}", other),
    }
}

#[test]
fn test_parse_ptr_cast() {
    let stmts = parse_source("<?php $q = ptr_cast<Point>($p);");
    match &stmts[0].kind {
        StmtKind::Assign { value, .. } => match &value.kind {
            ExprKind::PtrCast { target_type, expr } => {
                assert_eq!(target_type, "Point");
                assert!(matches!(expr.kind, ExprKind::Variable(_)));
            }
            _ => panic!("Expected PtrCast"),
        },
        _ => panic!("Expected Assign"),
    }
}

#[test]
fn test_parse_ptr_builtins_as_function_calls() {
    let stmts = parse_source("<?php ptr_null(); ptr($x); ptr_is_null($p); ptr_get($p); ptr_set($p, 1); ptr_offset($p, 8); ptr_sizeof(\"int\");");
    // All should parse as FunctionCall
    for stmt in &stmts {
        match &stmt.kind {
            StmtKind::ExprStmt(expr) => match &expr.kind {
                ExprKind::FunctionCall { .. } => {}
                _ => panic!("Expected FunctionCall, got {:?}", expr.kind),
            },
            _ => panic!("Expected ExprStmt"),
        }
    }
}

#[test]
fn test_parse_extern_function() {
    let stmts = parse_source("<?php extern function abs(int $n): int;");
    match &stmts[0].kind {
        StmtKind::ExternFunctionDecl {
            name,
            params,
            return_type,
            library,
        } => {
            assert_eq!(name, "abs");
            assert_eq!(params.len(), 1);
            assert_eq!(params[0].name, "n");
            assert!(matches!(return_type, elephc::parser::ast::CType::Int));
            assert!(library.is_none());
        }
        _ => panic!("Expected ExternFunctionDecl"),
    }
}

#[test]
fn test_parse_extern_block() {
    let stmts = parse_source(
        r#"<?php extern "curl" { function init(): ptr; function cleanup(ptr $h): void; }"#,
    );
    assert_eq!(stmts.len(), 2);
    match &stmts[0].kind {
        StmtKind::ExternFunctionDecl { name, library, .. } => {
            assert_eq!(name, "init");
            assert_eq!(library.as_deref(), Some("curl"));
        }
        _ => panic!("Expected ExternFunctionDecl"),
    }
    match &stmts[1].kind {
        StmtKind::ExternFunctionDecl { name, library, .. } => {
            assert_eq!(name, "cleanup");
            assert_eq!(library.as_deref(), Some("curl"));
        }
        _ => panic!("Expected ExternFunctionDecl"),
    }
}

#[test]
fn test_parse_extern_class() {
    let stmts = parse_source("<?php extern class Point { public int $x; public float $y; }");
    match &stmts[0].kind {
        StmtKind::ExternClassDecl { name, fields } => {
            assert_eq!(name, "Point");
            assert_eq!(fields.len(), 2);
            assert_eq!(fields[0].name, "x");
            assert_eq!(fields[1].name, "y");
        }
        _ => panic!("Expected ExternClassDecl"),
    }
}

#[test]
fn test_parse_extern_global() {
    let stmts = parse_source("<?php extern global int $errno;");
    match &stmts[0].kind {
        StmtKind::ExternGlobalDecl { name, c_type } => {
            assert_eq!(name, "errno");
            assert!(matches!(c_type, elephc::parser::ast::CType::Int));
        }
        _ => panic!("Expected ExternGlobalDecl"),
    }
}

#[test]
fn test_parse_extern_lib_function() {
    let stmts = parse_source(r#"<?php extern "m" function sin(float $x): float;"#);
    match &stmts[0].kind {
        StmtKind::ExternFunctionDecl { name, library, .. } => {
            assert_eq!(name, "sin");
            assert_eq!(library.as_deref(), Some("m"));
        }
        _ => panic!("Expected ExternFunctionDecl"),
    }
}

#[test]
fn test_parse_extern_callable_param() {
    let stmts = parse_source(r#"<?php extern function signal(int $sig, callable $handler): ptr;"#);
    match &stmts[0].kind {
        StmtKind::ExternFunctionDecl { params, .. } => {
            assert_eq!(params.len(), 2);
            assert!(matches!(
                params[1].c_type,
                elephc::parser::ast::CType::Callable
            ));
        }
        _ => panic!("Expected ExternFunctionDecl"),
    }
}

#[test]
fn test_parse_readonly_class_flag() {
    let stmts = parse_source("<?php readonly class User { public $id; }");
    match &stmts[0].kind {
        StmtKind::ClassDecl {
            name,
            is_readonly_class,
            properties,
            ..
        } => {
            assert_eq!(name, "User");
            assert!(*is_readonly_class);
            assert_eq!(properties.len(), 1);
            assert_eq!(properties[0].name, "id");
        }
        other => panic!("Expected readonly ClassDecl, got {:?}", other),
    }
}

#[test]
fn test_parse_final_class_flag() {
    let stmts = parse_source("<?php final class User { public function id() { return 1; } }");
    match &stmts[0].kind {
        StmtKind::ClassDecl {
            name,
            is_final,
            is_abstract,
            is_readonly_class,
            methods,
            ..
        } => {
            assert_eq!(name, "User");
            assert!(*is_final);
            assert!(!is_abstract);
            assert!(!is_readonly_class);
            assert_eq!(methods.len(), 1);
        }
        other => panic!("Expected final ClassDecl, got {:?}", other),
    }
}

#[test]
fn test_parse_final_readonly_class_flags() {
    for source in [
        "<?php final readonly class User {}",
        "<?php readonly final class User {}",
    ] {
        let stmts = parse_source(source);
        match &stmts[0].kind {
            StmtKind::ClassDecl {
                name,
                is_final,
                is_readonly_class,
                ..
            } => {
                assert_eq!(name, "User");
                assert!(*is_final);
                assert!(*is_readonly_class);
            }
            other => panic!("Expected final readonly ClassDecl, got {:?}", other),
        }
    }
}

#[test]
fn test_parse_abstract_readonly_class_flags() {
    let stmts = parse_source("<?php abstract readonly class User {}");
    match &stmts[0].kind {
        StmtKind::ClassDecl {
            name,
            is_abstract,
            is_readonly_class,
            ..
        } => {
            assert_eq!(name, "User");
            assert!(*is_abstract);
            assert!(*is_readonly_class);
        }
        other => panic!("Expected abstract readonly ClassDecl, got {:?}", other),
    }
}

#[test]
fn test_parse_final_method_flag() {
    let stmts = parse_source("<?php class User { final public function id() { return 1; } }");
    match &stmts[0].kind {
        StmtKind::ClassDecl { methods, .. } => {
            assert_eq!(methods.len(), 1);
            assert_eq!(methods[0].name, "id");
            assert!(methods[0].is_final);
            assert!(!methods[0].is_abstract);
            assert!(methods[0].has_body);
        }
        other => panic!("Expected ClassDecl with final method, got {:?}", other),
    }
}

#[test]
fn test_parse_final_property_flag() {
    let stmts = parse_source("<?php class User { final public $id = 1; }");
    match &stmts[0].kind {
        StmtKind::ClassDecl { properties, .. } => {
            assert_eq!(properties.len(), 1);
            assert_eq!(properties[0].name, "id");
            assert!(properties[0].is_final);
            assert!(!properties[0].readonly);
        }
        other => panic!("Expected ClassDecl with final property, got {:?}", other),
    }
}

#[test]
fn test_parse_typed_properties() {
    let stmts = parse_source(
        "<?php class User { public int $id; protected ?string $email = null; final public string $name = \"Ada\"; }",
    );
    match &stmts[0].kind {
        StmtKind::ClassDecl { properties, .. } => {
            assert_eq!(properties.len(), 3);
            assert_eq!(properties[0].name, "id");
            assert_eq!(properties[0].type_expr, Some(TypeExpr::Int));
            assert_eq!(properties[1].name, "email");
            assert_eq!(properties[1].visibility, Visibility::Protected);
            assert_eq!(
                properties[1].type_expr,
                Some(TypeExpr::Nullable(Box::new(TypeExpr::Str)))
            );
            assert_eq!(properties[2].name, "name");
            assert_eq!(properties[2].type_expr, Some(TypeExpr::Str));
            assert!(properties[2].is_final);
        }
        other => panic!("Expected ClassDecl with typed properties, got {:?}", other),
    }
}

#[test]
fn test_parse_constructor_promoted_properties() {
    let stmts = parse_source(
        "<?php class User { public function __construct(public int $id, private string $name, readonly ?int $rank = null, protected int &$score) { echo $id; } }",
    );
    match &stmts[0].kind {
        StmtKind::ClassDecl {
            properties,
            methods,
            ..
        } => {
            assert_eq!(properties.len(), 4);
            assert_eq!(properties[0].name, "id");
            assert_eq!(properties[0].visibility, Visibility::Public);
            assert_eq!(properties[0].type_expr, Some(TypeExpr::Int));
            assert!(!properties[0].readonly);
            assert_eq!(properties[1].name, "name");
            assert_eq!(properties[1].visibility, Visibility::Private);
            assert_eq!(properties[1].type_expr, Some(TypeExpr::Str));
            assert_eq!(properties[2].name, "rank");
            assert_eq!(properties[2].visibility, Visibility::Public);
            assert_eq!(
                properties[2].type_expr,
                Some(TypeExpr::Nullable(Box::new(TypeExpr::Int)))
            );
            assert!(properties[2].readonly);
            assert!(!properties[2].by_ref);
            assert_eq!(properties[3].name, "score");
            assert_eq!(properties[3].visibility, Visibility::Protected);
            assert_eq!(properties[3].type_expr, Some(TypeExpr::Int));
            assert!(properties[3].by_ref);

            assert_eq!(methods.len(), 1);
            let ctor = &methods[0];
            assert_eq!(ctor.name, "__construct");
            assert_eq!(ctor.params.len(), 4);
            assert_eq!(ctor.params[0].0, "id");
            assert_eq!(ctor.params[0].1, Some(TypeExpr::Int));
            assert_eq!(ctor.params[1].0, "name");
            assert_eq!(ctor.params[1].1, Some(TypeExpr::Str));
            assert_eq!(ctor.params[2].0, "rank");
            assert!(ctor.params[2].2.is_some());
            assert_eq!(ctor.params[3].0, "score");
            assert!(ctor.params[3].3);
            assert_eq!(ctor.body.len(), 5);
            assert_promoted_assignment(&ctor.body[0], "id");
            assert_promoted_assignment(&ctor.body[1], "name");
            assert_promoted_assignment(&ctor.body[2], "rank");
            assert_promoted_assignment(&ctor.body[3], "score");
            match &ctor.body[4].kind {
                StmtKind::Echo(expr) => assert_eq!(expr.kind, ExprKind::Variable("id".into())),
                other => panic!("Expected original constructor body after promotion, got {:?}", other),
            }
        }
        other => panic!("Expected ClassDecl with promoted properties, got {:?}", other),
    }
}

fn assert_promoted_assignment(stmt: &Stmt, expected: &str) {
    match &stmt.kind {
        StmtKind::PropertyAssign {
            object,
            property,
            value,
        } => {
            assert_eq!(object.kind, ExprKind::This);
            assert_eq!(property, expected);
            assert_eq!(value.kind, ExprKind::Variable(expected.into()));
        }
        other => panic!("Expected promoted property assignment, got {:?}", other),
    }
}

#[test]
fn test_parse_first_class_callable_function() {
    let stmts = parse_source("<?php $f = strlen(...);");
    match &stmts[0].kind {
        StmtKind::Assign { value, .. } => match &value.kind {
            ExprKind::FirstClassCallable(CallableTarget::Function(name)) => {
                assert_eq!(name.as_str(), "strlen");
            }
            other => panic!("Expected function first-class callable, got {:?}", other),
        },
        other => panic!("Expected assignment, got {:?}", other),
    }
}

#[test]
fn test_parse_first_class_callable_static_method() {
    let stmts = parse_source("<?php Foo::build(...);");
    match &stmts[0].kind {
        StmtKind::ExprStmt(expr) => match &expr.kind {
            ExprKind::FirstClassCallable(CallableTarget::StaticMethod { receiver, method }) => {
                assert_eq!(method, "build");
                match receiver {
                    StaticReceiver::Named(name) => assert_eq!(name.as_str(), "Foo"),
                    other => panic!("Expected named static receiver, got {:?}", other),
                }
            }
            other => panic!("Expected static first-class callable, got {:?}", other),
        },
        other => panic!("Expected expression statement, got {:?}", other),
    }
}

#[test]
fn test_parse_nullable_typed_assign() {
    let stmts = parse_source("<?php ?int $value = null;");
    match &stmts[0].kind {
        StmtKind::TypedAssign {
            type_expr,
            name,
            value,
        } => {
            assert_eq!(name, "value");
            assert_eq!(type_expr, &TypeExpr::Nullable(Box::new(TypeExpr::Int)));
            assert_eq!(value.kind, ExprKind::Null);
        }
        other => panic!("Expected typed assign, got {:?}", other),
    }
}

#[test]
fn test_parse_union_typed_assign() {
    let stmts = parse_source("<?php int|string $value = 1;");
    match &stmts[0].kind {
        StmtKind::TypedAssign {
            type_expr,
            name,
            value,
        } => {
            assert_eq!(name, "value");
            assert_eq!(type_expr, &TypeExpr::Union(vec![TypeExpr::Int, TypeExpr::Str]));
            assert_eq!(value.kind, ExprKind::IntLiteral(1));
        }
        other => panic!("Expected typed assign, got {:?}", other),
    }
}

#[test]
fn test_parse_nullable_shorthand_cannot_be_combined_with_union() {
    assert!(parse_fails("<?php ?int|string $value = null;"));
}
