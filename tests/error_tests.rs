use elephc::lexer::tokenize;
use elephc::parser::parse;
use elephc::types;

fn check_source(src: &str) -> Result<(), String> {
    let tokens = tokenize(src).map_err(|e| e.message.clone())?;
    let ast = parse(&tokens).map_err(|e| e.message.clone())?;
    types::check(&ast).map_err(|e| e.message.clone())?;
    Ok(())
}

fn expect_error(src: &str, expected_substr: &str) {
    match check_source(src) {
        Ok(_) => panic!("Expected error containing '{}', but got Ok", expected_substr),
        Err(msg) => {
            assert!(
                msg.contains(expected_substr),
                "Error '{}' doesn't contain '{}'",
                msg,
                expected_substr,
            );
        }
    }
}

// --- Lexer errors ---

#[test]
fn test_error_missing_open_tag() {
    expect_error("echo \"hi\";", "<?php");
}

#[test]
fn test_error_unterminated_string() {
    expect_error("<?php \"no end", "Unterminated string");
}

#[test]
fn test_error_empty_variable() {
    expect_error("<?php $;", "Expected variable name");
}

#[test]
fn test_error_unknown_keyword() {
    expect_error("<?php function", "Unknown keyword");
}

#[test]
fn test_error_unexpected_character() {
    expect_error("<?php @", "Unexpected character");
}

// --- Parser errors ---

#[test]
fn test_error_missing_semicolon() {
    expect_error("<?php echo \"hi\"", "Expected ';'");
}

#[test]
fn test_error_missing_equals() {
    expect_error("<?php $x \"hi\";", "Expected '='");
}

#[test]
fn test_error_unclosed_paren() {
    expect_error("<?php echo (1 + 2;", "Expected closing ')'");
}

#[test]
fn test_error_unexpected_token_in_expr() {
    expect_error("<?php echo ;", "Unexpected token");
}

#[test]
fn test_error_unexpected_token_in_stmt() {
    expect_error("<?php 42;", "Unexpected token");
}

// --- Type errors ---

#[test]
fn test_error_undefined_variable() {
    expect_error("<?php echo $x;", "Undefined variable: $x");
}

#[test]
fn test_error_type_mismatch_reassign() {
    expect_error(
        "<?php $x = 42; $x = \"hello\";",
        "cannot reassign $x",
    );
}

#[test]
fn test_error_arithmetic_on_string() {
    expect_error(
        "<?php $x = \"hi\"; echo $x + 1;",
        "Arithmetic operators require integer operands",
    );
}

#[test]
fn test_error_negate_string() {
    expect_error(
        "<?php $x = \"hi\"; echo -$x;",
        "Cannot negate a non-integer",
    );
}

// --- Error positions ---

#[test]
fn test_error_has_line_number() {
    let result = tokenize("<?php\n\n\"unterminated");
    let err = result.unwrap_err();
    assert_eq!(err.span.line, 3, "Error should be on line 3");
}

#[test]
fn test_error_has_column() {
    let result = tokenize("<?php @");
    let err = result.unwrap_err();
    assert!(err.span.col > 0, "Error should have a column number");
}
