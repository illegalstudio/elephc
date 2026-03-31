use elephc::lexer::tokenize;
use elephc::names::Name;
use elephc::parser::ast::{
    BinOp, CatchClause, Expr, ExprKind, StaticReceiver, Stmt, StmtKind, TraitAdaptation, UseKind,
    Visibility,
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
                            ExprKind::StaticMethodCall { receiver, method, .. } => {
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
        let param_names: Vec<&str> = params.iter().map(|(n, _, _)| n.as_str()).collect();
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
            let param_names: Vec<&str> = params.iter().map(|(n, _, _)| n.as_str()).collect();
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
fn test_parse_arrow_function() {
    let stmts = parse_source("<?php $fn = fn($x) => $x * 2;");
    assert_eq!(stmts.len(), 1);
    if let StmtKind::Assign { value, .. } = &stmts[0].kind {
        if let ExprKind::Closure {
            params, is_arrow, ..
        } = &value.kind
        {
            let param_names: Vec<&str> = params.iter().map(|(n, _, _)| n.as_str()).collect();
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

// --- Default parameter values ---

#[test]
fn test_parse_function_default_params() {
    let stmts = parse_source("<?php function foo($a, $b = 10) { return $a + $b; }");
    if let StmtKind::FunctionDecl { params, .. } = &stmts[0].kind {
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].0, "a");
        assert!(params[0].1.is_none());
        assert_eq!(params[1].0, "b");
        assert!(params[1].1.is_some());
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
            assert!(params[0].2, "Expected param to be pass-by-reference");
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
            assert!(params[0].2, "First param should be ref");
            assert!(!params[1].2, "Second param should not be ref");
            assert!(params[2].2, "Third param should be ref");
        }
        _ => panic!("Expected FunctionDecl"),
    }
}

#[test]
fn test_parse_non_ref_param() {
    let stmts = parse_source("<?php function foo($x) { }");
    match &stmts[0].kind {
        StmtKind::FunctionDecl { params, .. } => {
            assert!(!params[0].2, "Normal param should not be ref");
        }
        _ => panic!("Expected FunctionDecl"),
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
fn test_parse_class_decl_with_extends() {
    let stmts = parse_source("<?php class Child extends Base { public function run() { return 1; } }");
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
            assert_eq!(extends, &vec!["Renderable".to_string(), "Jsonable".to_string()]);
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
