//! Purpose:
//! Integration or regression tests for parser AST coverage of includes, including word logical typed assignment rhs requires parentheses, include parses, and require parses.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP snippets cover successful AST shapes plus malformed syntax that must fail during parsing.

use super::*;

/// Verifies that `<?php int $x = true or false;` fails to parse because the RHS of a
/// typed assignment requires parentheses — the `or` keyword has lower precedence than
/// the `=` sign, which would incorrectly parse as `(int $x = true) or false`.
#[test]
fn test_word_logical_typed_assignment_rhs_requires_parentheses() {
    assert!(parse_fails("<?php int $x = true or false;"));
}

/// Verifies that `<?php include 'file.php';` parses to an `Include` with path StringLiteral
/// "file.php", once=false, required=false.
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
        assert_path_string_literal(path, "file.php");
        assert!(!once);
        assert!(!required);
    } else {
        panic!("expected Include");
    }
}

/// Verifies that `<?php @include 'file.php';` parses with error suppression applied to the include.
#[test]
fn test_error_suppressed_include_parses() {
    let stmts = parse_source("<?php @include 'file.php';");
    assert_eq!(stmts.len(), 1);
    if let StmtKind::Include {
        path,
        once,
        required,
    } = &stmts[0].kind
    {
        assert_path_string_literal(path, "file.php");
        assert!(!once);
        assert!(!required);
    } else {
        panic!("expected Include");
    }
}

/// Verifies that `<?php require 'file.php';` parses with required=true, once=false.
#[test]
fn test_require_parses() {
    let stmts = parse_source("<?php require 'file.php';");
    if let StmtKind::Include {
        path,
        once,
        required,
    } = &stmts[0].kind
    {
        assert_path_string_literal(path, "file.php");
        assert!(!once);
        assert!(required);
    } else {
        panic!("expected Include (require)");
    }
}

/// Verifies that `<?php include_once 'file.php';` parses with once=true, required=false.
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

/// Verifies that `<?php require_once 'file.php';` parses with once=true, required=true.
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

/// Verifies that `<?php include('file.php');` (parenthesized path) parses to an `Include`
/// with a string literal path. Parenthesized include paths are valid PHP.
#[test]
fn test_include_with_parens_parses() {
    let stmts = parse_source("<?php include('file.php');");
    if let StmtKind::Include { path, .. } = &stmts[0].kind {
        assert_path_string_literal(path, "file.php");
    } else {
        panic!("expected Include");
    }
}

/// Verifies that `<?php require __DIR__ . '/lib/x.php';` parses with a binary concatenation
/// of `__DIR__` magic constant and a string literal as the include path.
#[test]
fn test_require_with_dunder_dir_concat_parses() {
    let stmts = parse_source("<?php require __DIR__ . '/lib/x.php';");
    if let StmtKind::Include { path, .. } = &stmts[0].kind {
        match &path.kind {
            ExprKind::BinaryOp { left, op: BinOp::Concat, right } => {
                assert_eq!(left.kind, ExprKind::MagicConstant(MagicConstant::Dir));
                assert_eq!(right.kind, ExprKind::StringLiteral("/lib/x.php".to_string()));
            }
            other => panic!("expected BinaryOp(Concat) path, got {:?}", other),
        }
    } else {
        panic!("expected Include");
    }
}

/// Verifies that `<?php require BASE . '/x.php';` parses with a binary concatenation of
/// a constant reference and a string literal as the include path.
#[test]
fn test_require_with_const_ref_parses() {
    let stmts = parse_source("<?php require BASE . '/x.php';");
    if let StmtKind::Include { path, .. } = &stmts[0].kind {
        match &path.kind {
            ExprKind::BinaryOp { left, op: BinOp::Concat, right } => {
                match &left.kind {
                    ExprKind::ConstRef(name) => assert_eq!(name.as_str(), "BASE"),
                    other => panic!("expected ConstRef left, got {:?}", other),
                }
                assert_eq!(right.kind, ExprKind::StringLiteral("/x.php".to_string()));
            }
            other => panic!("expected BinaryOp(Concat) path, got {:?}", other),
        }
    } else {
        panic!("expected Include");
    }
}

// --- Exponentiation ---

/// Asserts that `expr` is the transient `IncludeValue` marker the parser emits for an
/// expression-position include, returning its `(once, required)` flags for the caller to check.
/// The resolver later expands this marker into caller-scope statements.
fn assert_include_value(expr: &ExprKind) -> (bool, bool) {
    match expr {
        ExprKind::IncludeValue { once, required, .. } => (*once, *required),
        other => panic!("Expected IncludeValue, got {:?}", other),
    }
}

/// Verifies that `return require X;` parses to `return <IncludeValue>`, carrying the include's
/// once/required flags for the resolver to expand into the caller's scope.
#[test]
fn test_return_require_parses_as_include_value() {
    let stmts = parse_source("<?php function f() { return require 'helper.php'; }");
    let body = match &stmts[0].kind {
        StmtKind::FunctionDecl { body, .. } => body,
        other => panic!("Expected FunctionDecl, got {:?}", other),
    };
    match &body[0].kind {
        StmtKind::Return(Some(value)) => {
            assert_eq!(assert_include_value(&value.kind), (false, true));
        }
        other => panic!("Expected Return, got {:?}", other),
    }
}

/// Verifies that `$x = require_once X;` assigns the `IncludeValue` marker so the resolver can
/// expand it, and that the once/required flags are carried through.
#[test]
fn test_assign_require_parses_as_include_value() {
    let stmts = parse_source("<?php $x = require_once 'helper.php';");
    match &stmts[0].kind {
        StmtKind::Assign { name, value } => {
            assert_eq!(name, "x");
            assert_eq!(assert_include_value(&value.kind), (true, true));
        }
        other => panic!("Expected Assign, got {:?}", other),
    }
}

/// Returns the value expression of a single `ExprStmt`/`Return`/`Assign` program.
fn sole_value(src: &str) -> Expr {
    let stmts = parse_source(src);
    match &stmts[0].kind {
        StmtKind::ExprStmt(value) | StmtKind::Return(Some(value)) | StmtKind::Assign { value, .. } => {
            value.clone()
        }
        other => panic!("Expected an expression statement, got {:?}", other),
    }
}

/// Verifies an include is an expression in argument position, the Carbon lang-file shape
/// `array_replace_recursive(require __DIR__.'/en.php', [...])`, with the whole concatenation as
/// its path and the comma ending it.
#[test]
fn test_require_parses_as_call_argument() {
    let value = sole_value("<?php return array_replace_recursive(require __DIR__.'/en.php', ['a' => 1]);");
    let ExprKind::FunctionCall { args, .. } = &value.kind else {
        panic!("Expected FunctionCall, got {:?}", value.kind);
    };
    assert_eq!(args.len(), 2);
    let ExprKind::IncludeValue { path, once, required } = &args[0].kind else {
        panic!("Expected IncludeValue argument, got {:?}", args[0].kind);
    };
    assert!(!once && *required);
    assert!(matches!(path.kind, ExprKind::BinaryOp { op: BinOp::Concat, .. }));
}

/// Verifies PHP's include precedence: the path is a whole expression, so
/// `1 + require 'a' . 'b'` is `1 + (require ('a' . 'b'))`, and a parenthesized path is a grouped
/// expression (`require ('a') . 'b'` includes `'ab'`).
#[test]
fn test_include_operand_takes_the_whole_following_expression() {
    let value = sole_value("<?php $x = 1 + require 'a' . 'b';");
    let ExprKind::BinaryOp { op: BinOp::Add, right, .. } = &value.kind else {
        panic!("Expected Add, got {:?}", value.kind);
    };
    let ExprKind::IncludeValue { path, .. } = &right.kind else {
        panic!("Expected IncludeValue, got {:?}", right.kind);
    };
    assert!(matches!(path.kind, ExprKind::BinaryOp { op: BinOp::Concat, .. }));

    let value = sole_value("<?php $x = require ('a') . 'b';");
    let ExprKind::IncludeValue { path, .. } = &value.kind else {
        panic!("Expected IncludeValue, got {:?}", value.kind);
    };
    assert!(matches!(path.kind, ExprKind::BinaryOp { op: BinOp::Concat, .. }));
}

/// Verifies includes parse in every assignment target and operator position PHP allows,
/// which used to be `Unexpected token: Include` (Carbon `static::$x ??= require ...`).
#[test]
fn test_include_parses_in_assignment_and_conditional_positions() {
    for src in [
        "<?php int $a = include 'f.php';",
        "<?php $a = []; $a[0] = include 'f.php';",
        "<?php $o = new stdClass(); $o->p = include 'f.php';",
        "<?php class K { public static $s; } K::$s ??= require 'f.php';",
        "<?php $x = $c ? require_once 'a.php' : require_once 'b.php';",
        "<?php $x = (fn () => require 'f.php')();",
        "<?php echo 1 + include 'f.php', 2;",
    ] {
        parse_source(src);
    }
}

/// Verifies unary `+` parses as php-src lowers it, `expr * 1`, binding like unary minus.
#[test]
fn test_unary_plus_parses_as_multiplication_by_one() {
    let value = sole_value("<?php return +$a ** 2;");
    let ExprKind::BinaryOp { left, op: BinOp::Mul, right } = &value.kind else {
        panic!("Expected Mul, got {:?}", value.kind);
    };
    assert!(matches!(right.kind, ExprKind::IntLiteral(1)));
    assert!(matches!(left.kind, ExprKind::BinaryOp { op: BinOp::Pow, .. }));
}

/// Verifies `new (expr)` and `new static::$map[...]` parse as a dynamic `new` whose class-name
/// expression is the parenthesized expression or the whole static-property chain.
#[test]
fn test_new_with_expression_class_name_parses_as_dynamic_new() {
    let value = sole_value("<?php $x = new ($this->getPivotClass())('a');");
    let ExprKind::NewDynamic { name_expr, args } = &value.kind else {
        panic!("Expected NewDynamic, got {:?}", value.kind);
    };
    assert!(matches!(name_expr.kind, ExprKind::MethodCall { .. }));
    assert_eq!(args.len(), 1);

    let value = sole_value("<?php $x = new $this->class();");
    let ExprKind::NewDynamic { name_expr, .. } = &value.kind else {
        panic!("Expected NewDynamic, got {:?}", value.kind);
    };
    assert!(matches!(&name_expr.kind, ExprKind::PropertyAccess { property, .. } if property == "class"));

    let value = sole_value("<?php $x = new static::$classes[static::class]($m);");
    let ExprKind::NewDynamic { name_expr, .. } = &value.kind else {
        panic!("Expected NewDynamic, got {:?}", value.kind);
    };
    assert!(matches!(name_expr.kind, ExprKind::ArrayAccess { .. }));
}

/// Verifies a reserved word is a method name after a dynamic `::` receiver (`$factory::new()`).
#[test]
fn test_reserved_word_method_after_dynamic_scope_receiver_parses() {
    for src in ["<?php $factory::new();", "<?php $o::match(1);", "<?php echo $c::DEFAULT;"] {
        parse_source(src);
    }
    assert!(parse_fails("<?php $factory::;"));
}
