use super::*;

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
