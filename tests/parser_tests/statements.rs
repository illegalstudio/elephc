//! Purpose:
//! Integration or regression tests for parser AST coverage of statements, including echo string literal, echo integer, and variable assignment.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP snippets cover successful AST shapes plus malformed syntax that must fail during parsing.

use super::*;

/// Verifies that `<?php echo "hello";` parses to a single `Echo` stmt containing a `StringLiteral`.
#[test]
fn test_echo_string_literal() {
    let stmts = parse_source("<?php echo \"hello\";");
    assert_eq!(stmts, vec![Stmt::echo(Expr::string_lit("hello"))]);
}

/// Verifies that `<?php echo 42;` parses to a single `Echo` stmt containing an `IntLiteral(42)`.
#[test]
fn test_echo_integer() {
    let stmts = parse_source("<?php echo 42;");
    assert_eq!(stmts, vec![Stmt::echo(Expr::int_lit(42))]);
}

/// Verifies that `<?php $x = 10;` parses to a simple `Assign` stmt with variable name "x"
/// and integer literal value 10.
#[test]
fn test_variable_assignment() {
    let stmts = parse_source("<?php $x = 10;");
    assert_eq!(stmts, vec![Stmt::assign("x", Expr::int_lit(10))]);
}

/// Verifies that `<?php $x = 5; echo $x;` parses to two stmts: assign and echo.
/// Asserts the echoed expression is a `Variable("x")`.
#[test]
fn test_echo_variable() {
    let stmts = parse_source("<?php $x = 5; echo $x;");
    assert_eq!(stmts.len(), 2);
    assert_eq!(stmts[1], Stmt::echo(Expr::var("x")));
}

// --- Unary ---

/// Verifies that `<?php $a = 1; $b = 2; echo $a;` parses to three stmts in order.
#[test]
fn test_multiple_statements() {
    let stmts = parse_source("<?php $a = 1; $b = 2; echo $a;");
    assert_eq!(stmts.len(), 3);
}

// --- Parse errors ---

/// Verifies that `<?php echo "hi"` (missing semicolon) fails during parsing.
#[test]
fn test_missing_semicolon() {
    assert!(parse_fails("<?php echo \"hi\""));
}

/// Verifies that `<?php if (1) { echo "a";` (missing closing brace) fails during parsing.
#[test]
fn test_missing_closing_brace() {
    assert!(parse_fails("<?php if (1) { echo \"a\";"));
}

/// Verifies that `<?php if 1 { echo "a"; }` (missing parentheses around condition) fails parsing.
#[test]
fn test_missing_condition_parens() {
    assert!(parse_fails("<?php if 1 { echo \"a\"; }"));
}

/// Verifies that `<?php print "hello";` parses as an `ExprStmt` wrapping `Expr::print(...)`.
/// PHP's `print` is an expression construct (returns 1), distinct from `echo`.
#[test]
fn test_print_parses_as_expression_statement() {
    let stmts = parse_source("<?php print \"hello\";");
    assert_eq!(
        stmts,
        vec![Stmt::new(
            StmtKind::ExprStmt(Expr::print(Expr::string_lit("hello"))),
            elephc::span::Span::dummy(),
        )]
    );
}

/// Verifies parenthesized expressions are accepted as standalone expression statements.
#[test]
fn test_parenthesized_expression_statement() {
    let stmts = parse_source("<?php (1 + 2);");
    assert_eq!(
        stmts,
        vec![Stmt::new(
            StmtKind::ExprStmt(Expr::binop(Expr::int_lit(1), BinOp::Add, Expr::int_lit(2))),
            elephc::span::Span::dummy(),
        )]
    );
}

/// Standalone arrays, including spreads and nested arrays, are expressions rather than patterns.
#[test]
fn test_array_expression_statements_are_not_destructuring() {
    for source in ["<?php [];", "<?php [...$items];", "<?php [[1], [...$items]];"] {
        let statements = parse_source(source);
        assert_eq!(statements.len(), 1, "{source}");
        assert!(matches!(&statements[0].kind, StmtKind::ExprStmt(Expr { kind: ExprKind::ArrayLiteral(_), .. })), "{source}: {statements:?}");
    }
    let statements = parse_source("<?php [1, 2][0];");
    assert!(matches!(&statements[0].kind, StmtKind::ExprStmt(Expr { kind: ExprKind::ArrayAccess { .. }, .. })));
}

/// Matching the outer bracket retains positional, nested and keyed destructuring assignments.
#[test]
fn test_bracket_assignment_still_selects_destructuring() {
    for source in ["<?php [$a] = $items;", "<?php [[$a], $b] = $items;", "<?php ['key' => $a] = $items;"] {
        let statements = parse_source(source);
        assert!(!statements.is_empty(), "{source}");
        assert!(!matches!(&statements[0].kind, StmtKind::ExprStmt(_)), "{source}");
    }
    for source in ["<?php [...$a] = $items;", "<?php [1] = $items;", "<?php [...$items;"] {
        assert!(parse_fails(source), "{source}");
    }
}

/// Verifies `$this->n++;` parses to the same read-modify-write statement as `$this->n += 1;`.
/// Regression: the `$this` statement parser used to reject the trailing `++`.
#[test]
fn test_this_property_postfix_increment_parses_as_compound_assignment() {
    assert_eq!(
        parse_source("<?php $this->n++;"),
        parse_source("<?php $this->n += 1;")
    );
    assert_eq!(
        parse_source("<?php $this->n--;"),
        parse_source("<?php $this->n -= 1;")
    );
}

/// Property updates retain their lowering while array updates preserve PHP incdec semantics.
#[test]
fn test_prefix_increment_on_complex_targets_retains_array_incdec() {
    assert_eq!(
        parse_source("<?php ++$this->n;"),
        parse_source("<?php $this->n += 1;")
    );
    assert_eq!(
        parse_source("<?php ++$obj->n;"),
        parse_source("<?php $obj->n += 1;")
    );
    let statements = parse_source("<?php --$a[0];");
    let StmtKind::ArrayAssign { array, index, value } = &statements[0].kind else {
        panic!("Expected array writeback, got {:?}", statements[0].kind);
    };
    assert_eq!(array, "a");
    assert!(matches!(index.kind, ExprKind::IntLiteral(0)));
    let ExprKind::Assignment { value, prelude, .. } = &value.kind else {
        panic!("Expected captured decrement, got {:?}", value.kind);
    };
    let ExprKind::PreDecrement(old_name) = &value.kind else {
        panic!("Expected PHP decrement, got {:?}", value.kind);
    };
    assert_eq!(prelude.len(), 1);
    assert!(matches!(&prelude[0].kind, StmtKind::Assign { name, value }
        if name == old_name && matches!(value.kind, ExprKind::ArrayAccess { .. })));
}

/// Property array elements capture their value and retain PHP increment semantics.
#[test]
fn test_this_property_element_increment_retains_php_incdec() {
    let statements = parse_source("<?php $this->arr[0]++;");
    let StmtKind::PropertyArrayAssign { object, property, index, value } = &statements[0].kind else {
        panic!("Expected property array writeback, got {:?}", statements[0].kind);
    };
    assert!(matches!(object.kind, ExprKind::This));
    assert_eq!(property, "arr");
    assert!(matches!(index.kind, ExprKind::IntLiteral(0)));
    let ExprKind::Assignment { value, prelude, .. } = &value.kind else {
        panic!("Expected captured increment, got {:?}", value.kind);
    };
    let ExprKind::PreIncrement(old_name) = &value.kind else {
        panic!("Expected PHP increment, got {:?}", value.kind);
    };
    assert_eq!(prelude.len(), 1);
    let StmtKind::Assign { name, value } = &prelude[0].kind else {
        panic!("Expected captured property element, got {:?}", prelude[0].kind);
    };
    assert_eq!(name, old_name);
    let ExprKind::ArrayAccess { array, index } = &value.kind else {
        panic!("Expected property element read, got {:?}", value.kind);
    };
    assert!(matches!(index.kind, ExprKind::IntLiteral(0)));
    assert!(matches!(&array.kind, ExprKind::PropertyAccess { object, property }
        if matches!(object.kind, ExprKind::This) && property == "arr"));
}

/// Computed write hops are captured independently instead of reading missing receiver storage.
#[test]
fn test_computed_property_write_statements_preserve_writable_traversal() {
    for source in [
        "<?php $x->a[k()]->c .= f();", "<?php $x->a[k()]->c++;",
        "<?php $x->{g()}->b->c += 1;",
    ] {
        let statements = parse_source(source);
        let StmtKind::ExprStmt(expression) = &statements[0].kind else {
            panic!("computed property statement must retain expression write semantics: {source}");
        };
        let ExprKind::Assignment { target, prelude, .. } = &expression.kind else {
            panic!("expected assignment: {source}");
        };
        assert!(matches!(target.kind, ExprKind::PropertyAccess { .. }), "{source}");
        assert!(prelude.iter().any(|statement| matches!(&statement.kind,
            StmtKind::Assign { value, .. } if matches!(value.kind, ExprKind::FunctionCall { .. }))), "{source}");
    }
}

/// Append preparation evaluates only the computed name before selecting its property container.
#[test]
fn test_computed_property_append_keeps_receiver_chain() {
    let statements = parse_source("<?php $x->{g()}->list[] = f();");
    let StmtKind::Synthetic(prelude) = &statements[0].kind else { panic!("expected captured name"); };
    assert!(matches!(&prelude[0].kind, StmtKind::Assign { value, .. }
        if matches!(value.kind, ExprKind::FunctionCall { .. })));
    assert!(matches!(&prelude.last().unwrap().kind, StmtKind::PropertyArrayPush { object, .. }
        if matches!(object.kind, ExprKind::DynamicPropertyAccess { .. })));
}

/// Plain computed storage writes keep their RHS inline rather than in a frame-lived capture.
#[test]
fn test_computed_plain_property_statements_keep_inline_rhs() {
    for source in ["<?php $x->{g()}->c = f();", "<?php $x->a[k()]->c = f();"] {
        let statements = parse_source(source);
        let StmtKind::Synthetic(body) = &statements[0].kind else { panic!("expected captured key"); };
        assert!(matches!(&body.last().unwrap().kind, StmtKind::PropertyAssign { value, .. }
            if matches!(value.kind, ExprKind::FunctionCall { .. })), "{source}");
    }
}

/// Call roots run once before the RHS while property and dimension fetches remain writable.
#[test]
fn test_call_rooted_property_statements_capture_only_the_root() {
    for source in [
        "<?php make()->children()[key()]->mark = rhs();",
        "<?php make()->children()[key()]->item[0] = rhs();",
        "<?php make()->children()[key()]->item[] = rhs();",
    ] {
        let statements = parse_source(source);
        let StmtKind::Synthetic(body) = &statements[0].kind else { panic!("expected root capture: {source}"); };
        let StmtKind::Assign { name, value } = &body[0].kind else { panic!("expected root value: {source}"); };
        assert!(name.starts_with(elephc::parser::ast::SCOPED_WRITE_TEMP_PREFIX), "{source}");
        assert!(matches!(value.kind, ExprKind::MethodCall { .. }), "{source}");
        let (object, value) = match &body.last().unwrap().kind {
            StmtKind::PropertyAssign { object, value, .. }
            | StmtKind::PropertyArrayAssign { object, value, .. }
            | StmtKind::PropertyArrayPush { object, value, .. } => (object, value),
            other => panic!("expected property write, got {other:?}: {source}"),
        };
        assert!(matches!(value.kind, ExprKind::FunctionCall { .. }), "{source}");
        assert!(matches!(&object.kind, ExprKind::ArrayAccess { array, .. }
            if matches!(&array.kind, ExprKind::Variable(root) if root == name)), "{source}");
    }
}

/// Compound statement captures preserve the call root without fetching its indexed receiver.
#[test]
fn test_call_rooted_property_updates_keep_delayed_traversal() {
    let statements = parse_source("<?php make()->children()[key()]->mark .= rhs();");
    let StmtKind::ExprStmt(expression) = &statements[0].kind else { panic!("expected update expression"); };
    let ExprKind::Assignment { target, prelude, .. } = &expression.kind else { panic!("expected assignment"); };
    let StmtKind::Assign { name, value } = &prelude[0].kind else { panic!("expected root capture"); };
    assert!(name.starts_with(elephc::parser::ast::SCOPED_WRITE_TEMP_PREFIX));
    assert!(matches!(value.kind, ExprKind::MethodCall { .. }));
    assert!(matches!(&target.kind, ExprKind::PropertyAccess { object, .. }
        if matches!(&object.kind, ExprKind::ArrayAccess { array, .. }
            if matches!(&array.kind, ExprKind::Variable(root) if root == name))));
}

/// Computed element increments keep the PHP incdec kernel instead of numeric arithmetic.
#[test]
fn test_computed_property_element_increment_retains_php_incdec() {
    let statements = parse_source("<?php $obj->arr[k()]++;");
    let StmtKind::Synthetic(body) = &statements[0].kind else { panic!("expected captured key"); };
    let StmtKind::PropertyArrayAssign { value, .. } = &body.last().unwrap().kind else { panic!("expected element writeback"); };
    let ExprKind::Assignment { value, .. } = &value.kind else { panic!("expected captured increment"); };
    assert!(matches!(value.kind, ExprKind::PreIncrement(_)));
}

/// Statement write captures carry a dedicated scope tag while expression captures keep theirs.
#[test]
fn test_computed_statement_capture_scope_does_not_escape_to_expressions() {
    let statements = parse_source("<?php $root->{g()}->value += f();");
    let StmtKind::ExprStmt(expression) = &statements[0].kind else { panic!("expected update"); };
    let ExprKind::Assignment { prelude, .. } = &expression.kind else { panic!("expected assignment"); };
    assert!(prelude.iter().any(|stmt| matches!(&stmt.kind, StmtKind::Assign { name, .. }
        if name.starts_with(elephc::parser::ast::SCOPED_WRITE_TEMP_PREFIX))));
    let statements = parse_source("<?php echo ($root->{g()}->value += f());");
    let StmtKind::Echo(expression) = &statements[0].kind else { panic!("expected expression context"); };
    let ExprKind::Assignment { prelude, .. } = &expression.kind else { panic!("expected assignment"); };
    assert!(!prelude.iter().any(|stmt| matches!(&stmt.kind, StmtKind::Assign { name, .. }
        if name.starts_with(elephc::parser::ast::SCOPED_WRITE_TEMP_PREFIX))));
}

/// Low-precedence logic keeps expression captures outside a top-level statement scope.
#[test]
fn test_computed_assignment_logic_does_not_leave_scoped_captures() {
    for operator in ["or", "and", "xor"] {
        for assignment in ["+=", "??="] {
            let source = format!("<?php $root->rows[k()]->value {assignment} 1 {operator} f();");
            let statements = parse_source(&source);
            let StmtKind::ExprStmt(expression) = &statements[0].kind else { panic!("expected logic expression"); };
            let ExprKind::BinaryOp { left, .. } = &expression.kind else { panic!("expected wrapping logic"); };
            let ExprKind::Assignment { prelude, .. } = &left.kind else { panic!("expected nested assignment"); };
            assert!(!prelude.iter().any(|stmt| matches!(&stmt.kind, StmtKind::Assign { name, .. }
                if name.starts_with(elephc::parser::ast::SCOPED_WRITE_TEMP_PREFIX))), "{source}");
        }
    }
}
