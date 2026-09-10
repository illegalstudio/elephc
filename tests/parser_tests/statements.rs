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

/// Verifies prefix `++`/`--` on complex targets parses to the same statement as the
/// equivalent compound assignment, since statement position discards the result.
#[test]
fn test_prefix_increment_on_complex_targets_parses_as_compound_assignment() {
    assert_eq!(
        parse_source("<?php ++$this->n;"),
        parse_source("<?php $this->n += 1;")
    );
    assert_eq!(
        parse_source("<?php ++$obj->n;"),
        parse_source("<?php $obj->n += 1;")
    );
    assert_eq!(
        parse_source("<?php --$a[0];"),
        parse_source("<?php $a[0] -= 1;")
    );
}

/// Verifies `$this->arr[0]++;` parses to the same statement as the compound assignment,
/// so the array element under a `$this` property is reached as well.
#[test]
fn test_this_property_element_increment_parses_as_compound_assignment() {
    assert_eq!(
        parse_source("<?php $this->arr[0]++;"),
        parse_source("<?php $this->arr[0] += 1;")
    );
}
/// Verifies a terminal closing tag terminates a statement without an explicit semicolon.
#[test]
fn test_terminal_closing_tag_finishes_program() {
    let stmts = parse_source("<?php echo 1 ?>\n");
    assert_eq!(stmts.len(), 1);
    assert!(matches!(stmts[0].kind, StmtKind::Echo(_)));
}

/// Verifies terminal inline HTML becomes a literal `Echo` AST statement with its source span.
#[test]
fn test_terminal_inline_html_becomes_echo_statement() {
    let stmts = parse_source("<?php echo 'A'; ?>\nDone");
    assert_eq!(stmts.len(), 2);
    assert!(matches!(
        &stmts[1].kind,
        StmtKind::Echo(Expr {
            kind: ExprKind::StringLiteral(value),
            ..
        }) if value == "Done"
    ));
    assert_eq!((stmts[1].span.line, stmts[1].span.col), (2, 1));
}

/// Verifies the outermost halt directive becomes file-finalization metadata, not a runtime call.
#[test]
fn test_halt_compiler_parses_as_terminal_source_metadata() {
    let stmts = parse_source("<?php echo 'before'; __HALT_COMPILER();opaque");
    assert_eq!(stmts.len(), 2);
    assert!(matches!(stmts[0].kind, StmtKind::Echo(_)));
    assert!(matches!(
        &stmts[1].kind,
        StmtKind::ConstDecl { name, value }
            if name == "\0elephc.compiler_halt_offset\0"
                && value.kind == ExprKind::IntLiteral(39)
    ));
}

/// Verifies PHP's outermost-scope restriction is diagnosed for nested halt directives.
#[test]
fn test_halt_compiler_rejects_inner_scope() {
    let tokens = tokenize("<?php function f() { __HALT_COMPILER(); payload").unwrap();
    let error = parse(&tokens).expect_err("nested halt must be rejected");
    assert!(
        error
            .to_string()
            .contains("__HALT_COMPILER() can only be used from the outermost scope")
    );
}

/// Verifies malformed direct halt syntax remains a reserved construct, not a function call.
#[test]
fn test_halt_compiler_rejects_arguments() {
    let tokens = tokenize("<?php __HALT_COMPILER(1); echo 'after';").unwrap();
    assert!(matches!(
        &tokens[1].0,
        elephc::lexer::Token::HaltCompiler(0)
    ));
    let error = parse(&tokens).expect_err("halt compiler accepts no arguments");
    assert!(error.to_string().contains("Expected ')' after __HALT_COMPILER("));
}

/// Verifies PHP reserves the halt token in static calls and method declarations.
#[test]
fn test_halt_compiler_rejects_reserved_static_and_declaration_contexts() {
    for source in [
        "<?php HaltFacade::__HALT_COMPILER();",
        "<?php class HaltFacade { public function __HALT_COMPILER() {} }",
    ] {
        let tokens = tokenize(source).expect("reserved halt syntax still tokenizes");
        parse(&tokens).expect_err("PHP rejects reserved static and declaration contexts");
    }
}

/// Verifies PHP still permits the halt spelling after an instance member operator.
#[test]
fn test_halt_compiler_spelling_remains_valid_for_instance_calls() {
    let tokens = tokenize("<?php $object->__HALT_COMPILER();")
        .expect("instance member halt spelling tokenizes as an ordinary method name");
    parse(&tokens).expect("instance member halt spelling remains valid PHP syntax");
}
