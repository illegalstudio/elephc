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

/// Verifies `declare(strict_types=1);` (statement form) parses to an empty
/// `Synthetic` block — a no-op, since elephc is always strict.
#[test]
fn test_declare_strict_types_parses_to_empty_synthetic() {
    let stmts = parse_source("<?php declare(strict_types=1);");
    assert_eq!(stmts.len(), 1);
    match &stmts[0].kind {
        StmtKind::Synthetic(body) => assert!(body.is_empty()),
        other => panic!("Expected empty Synthetic, got {:?}", other),
    }
}

/// Verifies `declare(ticks=1) { ... }` (block form) parses to a `Synthetic`
/// wrapper around its body statements.
#[test]
fn test_declare_block_parses_to_synthetic_body() {
    let stmts = parse_source("<?php declare(ticks=1) { echo 1; }");
    assert_eq!(stmts.len(), 1);
    match &stmts[0].kind {
        StmtKind::Synthetic(body) => assert_eq!(body.len(), 1),
        other => panic!("Expected Synthetic body, got {:?}", other),
    }
}

/// Verifies PHP's alternative `declare: ... enddeclare;` syntax preserves its body.
#[test]
fn test_declare_alternative_syntax_parses_to_synthetic_body() {
    let stmts = parse_source("<?php declare(ticks=1): echo 1; enddeclare;");
    assert_eq!(stmts.len(), 1);
    match &stmts[0].kind {
        StmtKind::Synthetic(body) => assert_eq!(body.len(), 1),
        other => panic!("Expected Synthetic body, got {:?}", other),
    }
}

/// Verifies PHP's single-statement `declare(...) statement` form wraps that statement.
#[test]
fn test_declare_single_statement_parses_to_synthetic_body() {
    let stmts = parse_source("<?php declare(ticks=1) echo 1;");
    assert_eq!(stmts.len(), 1);
    match &stmts[0].kind {
        StmtKind::Synthetic(body) => assert_eq!(body.len(), 1),
        other => panic!("Expected Synthetic body, got {:?}", other),
    }
}

/// Verifies declare values accept PHP literal float and string forms.
#[test]
fn test_declare_accepts_php_literal_value_kinds() {
    let stmts = parse_source("<?php declare(ticks=1.5, encoding=\"UTF-8\"); echo 1;");
    assert_eq!(stmts.len(), 2);
    assert!(matches!(stmts[0].kind, StmtKind::Synthetic(ref body) if body.is_empty()));
}

/// Verifies directive matching is case-insensitive like PHP keyword handling.
#[test]
fn test_declare_strict_types_name_is_case_insensitive() {
    let stmts = parse_source("<?php declare(STRICT_TYPES=1); echo 1;");
    assert_eq!(stmts.len(), 2);
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

/// Verifies keyword-named enum cases retain source spelling, including two names that differ
/// only by case; PHP enum-case identity and `->name` use these exact case-sensitive names.
#[test]
fn test_parse_keyword_named_enum_cases_preserve_spelling() {
    let stmts = parse_source(
        "<?php enum KeywordCase { case Default; case DEFAULT; case Match; case MATCH; case Print; }",
    );
    let StmtKind::EnumDecl { cases, .. } = &stmts[0].kind else {
        panic!("Expected EnumDecl");
    };
    assert_eq!(
        cases
            .iter()
            .map(|case| case.name.as_str())
            .collect::<Vec<_>>(),
        ["Default", "DEFAULT", "Match", "MATCH", "Print"]
    );
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

/// Verifies `enum` remains a soft keyword in declarations, type hints, and scoped expressions.
#[test]
fn test_parse_enum_named_enum_across_name_contexts() {
    let stmts = parse_source(
        "<?php enum Enum { case X; } function pick(Enum $value): Enum { return Enum::X; } Enum::X;",
    );
    assert!(matches!(
        &stmts[0].kind,
        StmtKind::EnumDecl { name, .. } if name == "Enum"
    ));
    assert!(matches!(&stmts[1].kind, StmtKind::FunctionDecl { .. }));
    assert!(matches!(&stmts[2].kind, StmtKind::ExprStmt(_)));
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

/// Extracts `(name, int value)` from every top-level `ConstDecl` in `stmts`.
fn const_decl_names_and_ints(stmts: &[Stmt]) -> Vec<(String, Option<i64>)> {
    stmts
        .iter()
        .filter_map(|stmt| match &stmt.kind {
            StmtKind::ConstDecl { name, value } => Some((
                name.clone(),
                match value.kind {
                    ExprKind::IntLiteral(value) => Some(value),
                    _ => None,
                },
            )),
            _ => None,
        })
        .collect()
}

/// Verifies a file-scope `const` declarator list parses to one flat `ConstDecl` per name
/// (issue #1142).
///
/// `const A = 1, B = 2;` used to report `Expected ';'`. The declarations are spliced into the
/// program list itself rather than nested in a wrapper, so every pass that collects top-level
/// declarations sees exactly what two `const` statements produce. The first keeps the statement
/// span, as a lone `const` always did; the second carries its own declarator span.
#[test]
fn test_file_scope_const_declarator_list_parses_one_decl_per_name() {
    let stmts = parse_source("<?php const A = 1, BB = 22;");
    assert_eq!(stmts.len(), 2, "{:?}", stmts);
    assert_eq!(
        const_decl_names_and_ints(&stmts),
        vec![("A".to_string(), Some(1)), ("BB".to_string(), Some(22))]
    );
    assert_eq!((stmts[0].span.line, stmts[0].span.col), (1, 7));
    assert_eq!(
        (stmts[1].span.line, stmts[1].span.col, stmts[1].span.end_column()),
        (1, 20, 27)
    );
}

/// Verifies the list form inside both namespace spellings, and that a later value may name an
/// earlier constant.
#[test]
fn test_namespaced_const_declarator_lists_parse_one_decl_per_name() {
    let stmts = parse_source("<?php namespace Shop; const MIN = 1, MAX = MIN + 9;");
    assert_eq!(stmts.len(), 3, "{:?}", stmts);
    let names: Vec<_> = const_decl_names_and_ints(&stmts)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert_eq!(names, vec!["MIN", "MAX"]);

    let stmts = parse_source("<?php namespace Shop { const MIN = 1, MAX = 10; }");
    let StmtKind::NamespaceBlock { body, .. } = &stmts[0].kind else {
        panic!("Expected NamespaceBlock, got {:?}", stmts[0].kind);
    };
    assert_eq!(
        const_decl_names_and_ints(body),
        vec![("MIN".to_string(), Some(1)), ("MAX".to_string(), Some(10))]
    );
}

/// Verifies a malformed file-scope list is rejected by its own cause: a trailing comma and a
/// declarator without a value, both of which PHP reports as syntax errors.
#[test]
fn test_malformed_file_scope_const_declarator_lists_are_rejected() {
    assert!(parse_fails("<?php const A = 1, ;"));
    assert!(parse_fails("<?php const A = 1, B;"));
}


/// Every complete leading declare form can precede the strict directive.
#[test]
fn test_declare_strict_types_after_leading_declare_forms() {
    for prefix in [
        "declare(ticks=1);", "declare(ticks=1) { echo 0; }",
        "declare(ticks=1): echo 0; enddeclare;", "declare(ticks=1) echo 0;",
        "declare(ticks=1) declare(ticks=1) echo 0;",
        "declare(ticks=1) declare(ticks=1);",
        "declare(ticks=1) if (true) { echo 0; } else { echo 2; }",
    ] {
        let stmts = parse_source(&format!("<?php {prefix} declare(strict_types=1); echo 1;"));
        assert_eq!(stmts.len(), 3, "{prefix}");
        assert!(stmts[0].strict_types, "main script prefix: {prefix}");
        assert!(stmts[2].strict_types, "main script suffix: {prefix}");
        assert!(matches!(stmts[2].kind, StmtKind::Echo(_)));
    }
}

/// Prefix tracking consumes each declare once, even in a long flat sequence.
#[test]
fn test_declare_strict_types_after_many_leading_declares() {
    let source = format!("<?php {} declare(strict_types=1); echo 1;", "declare(ticks=1); ".repeat(128));
    let stmts = parse_source(&source);
    assert_eq!(stmts.len(), 130);
    assert!(stmts.iter().all(|stmt| stmt.strict_types));
}


/// An ordinary empty statement is parsed but is not a leading declare directive.
#[test]
fn test_declare_related_empty_statements_parse_as_noops() {
    let stmts = parse_source("<?php ; if (false); echo 1;");
    assert!(matches!(&stmts[0].kind, StmtKind::Synthetic(body) if body.is_empty()));
    assert!(matches!(&stmts[1].kind, StmtKind::If { then_body, .. }
        if matches!(&then_body[0].kind, StmtKind::Synthetic(body) if body.is_empty())));
}
