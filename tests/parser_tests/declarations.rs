//! Purpose:
//! Integration or regression tests for parser AST coverage of declarations, including const decl integer, const decl string, and const ref in echo.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP snippets are parsed and assertions inspect AST shape, precedence, or expected parse failures.

use super::*;

/// Verifies that `<?php const MAX = 100;` parses to a `ConstDecl` with name "MAX" and an `IntLiteral(100)` value.
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

/// Verifies that `<?php const NAME = "hello";` parses to a `ConstDecl` with name "NAME" and a `StringLiteral` value.
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

/// Verifies that `<?php echo MAX;` parses to an `Echo` of a `ConstRef("MAX")` expression.
/// Constant references are resolved at parse time to this AST node.
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

/// Verifies that `<?php enum Color: int { case Red = 1; case Green = 2; }` parses to an
/// `EnumDecl` with backing type `Some(Int)`, two cases with integer values.
#[test]
fn test_parse_backed_enum_decl() {
    let stmts = parse_source("<?php enum Color: int { case Red = 1; case Green = 2; }");
    assert_eq!(stmts.len(), 1);
    match &stmts[0].kind {
        StmtKind::EnumDecl {
            name,
            backing_type,
            cases,
            ..
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

/// Verifies that `<?php echo Color::Red;` parses to an `Echo` containing a `ScopedConstantAccess`
/// with receiver "Color" and member "Red". The parser emits `ScopedConstantAccess`; the type
/// checker disambiguates between enum cases and class constants.
#[test]
fn test_parse_enum_case_expr() {
    // The parser now emits `ScopedConstantAccess` for `Foo::BAR`; the type
    // checker disambiguates between enum cases and class constants. Either
    // form is acceptable here — we just verify the receiver and member.
    let stmts = parse_source("<?php echo Color::Red;");
    assert_eq!(stmts.len(), 1);
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::ScopedConstantAccess { receiver, name } => {
                assert!(matches!(receiver, StaticReceiver::Named(n) if n.as_str() == "Color"));
                assert_eq!(name, "Red");
            }
            other => panic!("Expected ScopedConstantAccess, got {:?}", other),
        },
        other => panic!("Expected Echo, got {:?}", other),
    }
}

/// Verifies that `<?php [$a, $b] = [1, 2];` parses to a `ListUnpack` with vars `["a", "b"]`.
/// Destructuring via list literal unpacks the source array into named variables.
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

/// Verifies that `<?php [$x, $y, $z] = [10, 20, 30];` parses to a `ListUnpack` with three vars.
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

/// Verifies that `<?php [$a, , $c] = [10, 20, 30];` (skipped list entry) lowers to a `Synthetic`
/// node containing three `Assign` stmts. The middle entry is skipped — the array index is not
/// bound to any variable.
#[test]
fn test_list_unpack_skipped_entries_lowers_to_synthetic_assignments() {
    let stmts = parse_source("<?php [$a, , $c] = [10, 20, 30];");
    assert_eq!(stmts.len(), 1);
    let StmtKind::Synthetic(stmts) = &stmts[0].kind else {
        panic!("Expected Synthetic list unpack");
    };
    assert_eq!(stmts.len(), 3);
    match &stmts[1].kind {
        StmtKind::Assign { name, value } => {
            assert_eq!(name, "a");
            match &value.kind {
                ExprKind::ArrayAccess { index, .. } => {
                    assert_eq!(index.kind, ExprKind::IntLiteral(0));
                }
                other => panic!("Expected array access, got {:?}", other),
            }
        }
        other => panic!("Expected Assign, got {:?}", other),
    }
    match &stmts[2].kind {
        StmtKind::Assign { name, value } => {
            assert_eq!(name, "c");
            match &value.kind {
                ExprKind::ArrayAccess { index, .. } => {
                    assert_eq!(index.kind, ExprKind::IntLiteral(2));
                }
                other => panic!("Expected array access, got {:?}", other),
            }
        }
        other => panic!("Expected Assign, got {:?}", other),
    }
}

/// Verifies that `<?php ["id" => $id, "name" => $name] = $row;` (keyed list entries) lowers to a
/// `Synthetic` node with three `Assign` stmts using string-keyed array accesses.
#[test]
fn test_list_unpack_keyed_entries_lowers_with_key_accesses() {
    let stmts = parse_source("<?php [\"id\" => $id, \"name\" => $name] = $row;");
    assert_eq!(stmts.len(), 1);
    let StmtKind::Synthetic(stmts) = &stmts[0].kind else {
        panic!("Expected Synthetic list unpack");
    };
    assert_eq!(stmts.len(), 3);
    match &stmts[1].kind {
        StmtKind::Assign { name, value } => {
            assert_eq!(name, "id");
            match &value.kind {
                ExprKind::ArrayAccess { index, .. } => {
                    assert_eq!(index.kind, ExprKind::StringLiteral("id".to_string()));
                }
                other => panic!("Expected array access, got {:?}", other),
            }
        }
        other => panic!("Expected Assign, got {:?}", other),
    }
}

/// Verifies that `<?php [[$a, $b], $c] = [[1, 2], [3, 4]];` (nested list pattern) lowers to a
/// `Synthetic` node with 5 stmts: a temp assignment, then three variable assignments.
#[test]
fn test_list_unpack_nested_pattern_lowers_to_nested_temp() {
    let stmts = parse_source("<?php [[$a, $b], $c] = [[1, 2], [3, 4]];");
    assert_eq!(stmts.len(), 1);
    let StmtKind::Synthetic(stmts) = &stmts[0].kind else {
        panic!("Expected Synthetic list unpack");
    };
    assert_eq!(stmts.len(), 5);
    assert!(matches!(&stmts[1].kind, StmtKind::Assign { .. }));
    assert!(matches!(&stmts[2].kind, StmtKind::Assign { name, .. } if name == "a"));
    assert!(matches!(&stmts[3].kind, StmtKind::Assign { name, .. } if name == "b"));
    assert!(matches!(&stmts[4].kind, StmtKind::Assign { name, .. } if name == "c"));
}

/// Verifies that `<?php list($a, $b) = [1, 2];` (legacy `list()` construct) parses to a
/// `ListUnpack` with vars `["a", "b"]`. The `list()` keyword form is an alias for `[]`.
#[test]
fn test_list_construct_unpack_is_supported() {
    let stmts = parse_source("<?php list($a, $b) = [1, 2];");
    assert_eq!(stmts.len(), 1);
    match &stmts[0].kind {
        StmtKind::ListUnpack { vars, .. } => {
            assert_eq!(vars, &["a".to_string(), "b".to_string()]);
        }
        other => panic!("Expected ListUnpack, got {:?}", other),
    }
}

/// Verifies that `<?php [$items[], $box->x] = [1, 2];` (non-local list targets) lowers to a
/// `Synthetic` node with three stmts: a temp assignment, an `ArrayPush` for `$items[]`, and a
/// `PropertyAssign` for `$box->x`. Non-local targets are not simple variable assignments.
#[test]
fn test_list_unpack_non_local_targets_lowers_to_target_assignments() {
    let stmts = parse_source("<?php [$items[], $box->x] = [1, 2];");
    assert_eq!(stmts.len(), 1);
    let StmtKind::Synthetic(stmts) = &stmts[0].kind else {
        panic!("Expected Synthetic list unpack");
    };
    assert_eq!(stmts.len(), 3);
    assert!(matches!(&stmts[1].kind, StmtKind::ArrayPush { array, .. } if array == "items"));
    assert!(matches!(&stmts[2].kind, StmtKind::PropertyAssign { property, .. } if property == "x"));
}

// --- Global ---

/// Verifies that `<?php global $x;` parses to a `Global` stmt with a single variable "x".
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

/// Verifies that `<?php global $a, $b, $c;` parses to a `Global` stmt with three variables.
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

/// Verifies that an enum may declare methods, constants, and an `implements` clause alongside its
/// cases, all parsed into the `EnumDecl`.
#[test]
fn test_parse_enum_with_methods_implements_constants() {
    let stmts = parse_source(
        "<?php interface HasLabel {} enum Suit implements HasLabel { case Hearts; case Spades; const COUNT = 2; public function label(): string { return \"x\"; } public static function make(): self { return Suit::Hearts; } }",
    );
    let enum_decl = stmts
        .iter()
        .find(|s| matches!(s.kind, StmtKind::EnumDecl { .. }))
        .expect("enum declared");
    match &enum_decl.kind {
        StmtKind::EnumDecl {
            cases,
            implements,
            methods,
            constants,
            ..
        } => {
            assert_eq!(cases.len(), 2);
            assert_eq!(implements.len(), 1);
            assert_eq!(implements[0].as_str(), "HasLabel");
            assert_eq!(methods.len(), 2);
            assert!(methods.iter().any(|m| m.name == "label" && !m.is_static));
            assert!(methods.iter().any(|m| m.name == "make" && m.is_static));
            assert_eq!(constants.len(), 1);
            assert_eq!(constants[0].name, "COUNT");
        }
        other => panic!("Expected EnumDecl, got {:?}", other),
    }
}
