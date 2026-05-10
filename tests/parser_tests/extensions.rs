//! Purpose:
//! Integration or regression tests for parser AST coverage of extensions, including packed class and typed buffer decl, buffer packed element field access, and ptr cast.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP snippets are parsed and assertions inspect AST shape, precedence, or expected parse failures.

use super::*;

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
