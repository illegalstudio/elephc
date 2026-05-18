//! Purpose:
//! Integration or regression tests for diagnostic coverage of syntax, including missing open tag, unterminated string, and empty variable.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Invalid PHP snippets are checked through shared diagnostic helpers for messages, spans, and recovery behavior.

use super::*;

#[test]
fn test_error_missing_open_tag() {
    expect_error("echo \"hi\";", "<?php");
}

#[test]
fn test_error_unterminated_string() {
    expect_error("<?php \"no end", "Unterminated string");
}

#[test]
fn test_error_invalid_unicode_string_escape() {
    expect_error(
        r#"<?php echo "\u{110000}";"#,
        "Invalid UTF-8 codepoint escape sequence",
    );
}

#[test]
fn test_error_empty_variable() {
    expect_error("<?php $;", "Expected variable name");
}

#[test]
fn test_error_bare_identifier() {
    expect_error("<?php foo;", "Undefined constant: foo");
}

#[test]
fn test_error_unexpected_character() {
    expect_error("<?php `", "Unexpected character");
}

#[test]
fn test_error_empty_list_destructuring_pattern() {
    expect_error("<?php [] = [1];", "Cannot use empty list");
}

#[test]
fn test_error_list_destructuring_all_skipped() {
    expect_error("<?php [, ,] = [1, 2];", "Cannot use empty list");
}

#[test]
fn test_error_list_destructuring_mixes_keyed_and_unkeyed_entries() {
    expect_error(
        "<?php [$a, \"id\" => $id] = [1, \"id\" => 2];",
        "Cannot mix keyed and unkeyed list entries",
    );
}

#[test]
fn test_error_list_destructuring_requires_writable_target() {
    expect_error("<?php [1 + 2] = [3];", "Invalid list destructuring target");
}

// --- Attribute syntax errors ---

#[test]
fn test_error_unterminated_attribute_group() {
    expect_error(
        "<?php #[Foo class C {}",
        "Expected ',' or ']' between attributes",
    );
}

#[test]
fn test_error_empty_attribute_group() {
    expect_error("<?php #[] class C {}", "Empty attribute group");
}

#[test]
fn test_error_attribute_missing_identifier() {
    expect_error(
        "<?php #[123] class C {}",
        "Expected attribute name (identifier)",
    );
}

#[test]
fn test_error_attribute_starts_with_comma() {
    expect_error(
        "<?php #[, A] class C {}",
        "Expected attribute name (identifier)",
    );
}

#[test]
fn test_error_attribute_qualifier_dangling_backslash() {
    expect_error(
        "<?php #[\\] class C {}",
        "Expected attribute name (identifier)",
    );
}

#[test]
fn test_error_attribute_unterminated_arguments() {
    // An attribute argument list opened with `(` but never closed must fail.
    expect_error(
        "<?php #[Foo(1, 2 class C {}",
        "Expected ',' between arguments",
    );
}

#[test]
fn test_error_attribute_on_echo_statement_is_rejected() {
    // PHP only allows attributes on declarations; an `echo` statement must
    // be rejected when preceded by `#[Foo]`.
    expect_error(
        "<?php #[Foo] echo 1;",
        "Attributes are only allowed before declarations",
    );
}

#[test]
fn test_error_attribute_on_assignment_is_rejected() {
    expect_error(
        "<?php #[Foo] $x = 1;",
        "Attributes are only allowed before declarations",
    );
}

#[test]
fn test_error_attribute_on_if_is_rejected() {
    expect_error(
        "<?php #[Foo] if (true) { echo 1; }",
        "Attributes are only allowed before declarations",
    );
}

// --- Numeric literal errors ---

#[test]
fn test_error_explicit_octal_invalid_digit() {
    expect_error("<?php $x = 0o78;", "after octal literal");
}

#[test]
fn test_error_explicit_octal_empty() {
    expect_error("<?php $x = 0o;", "Expected octal digits");
}

#[test]
fn test_error_explicit_octal_separator_after_prefix() {
    expect_error("<?php $x = 0o_77;", "Expected octal digits");
}

#[test]
fn test_error_legacy_octal_invalid_digit() {
    expect_error("<?php $x = 078;", "Invalid octal literal");
}

#[test]
fn test_error_legacy_octal_separator_invalid_digit() {
    expect_error("<?php $x = 0_778;", "Invalid octal literal");
}

#[test]
fn test_error_hex_empty() {
    expect_error("<?php $x = 0x;", "Expected hex digits");
}

#[test]
fn test_error_hex_invalid_trailing() {
    expect_error("<?php $x = 0xfg;", "after hex literal");
}

#[test]
fn test_error_hex_separator_after_prefix() {
    expect_error("<?php $x = 0x_FF;", "Expected hex digits");
}

#[test]
fn test_error_binary_empty() {
    expect_error("<?php $x = 0b;", "Expected binary digits");
}

#[test]
fn test_error_binary_invalid_digit() {
    expect_error("<?php $x = 0b12;", "after binary literal");
}

#[test]
fn test_error_binary_separator_after_prefix() {
    expect_error("<?php $x = 0b_10;", "Expected binary digits");
}

#[test]
fn test_error_decimal_trailing_underscore() {
    expect_error("<?php $x = 1_;", "after decimal literal");
}

#[test]
fn test_error_decimal_double_underscore() {
    expect_error("<?php $x = 1__0;", "after decimal literal");
}

#[test]
fn test_error_control_requires_operand() {
    expect_error(
        "<?php @;",
        "Unexpected token",
    );
}

#[test]
fn test_error_print_requires_operand() {
    expect_error("<?php print;", "Unexpected token");
}

#[test]
fn test_error_echo_trailing_comma_requires_argument() {
    expect_error("<?php echo \"A\",;", "Unexpected token");
}

#[test]
fn test_error_break_level_must_be_positive() {
    expect_error("<?php while (1) { break 0; }", "accepts only positive integers");
}

#[test]
fn test_error_continue_level_must_be_integer_literal() {
    expect_error(
        "<?php $n = 1; while (1) { continue $n; }",
        "requires an integer literal level",
    );
}

#[test]
fn test_error_single_ampersand() {
    expect_error("<?php &;", "Unexpected token");
}

#[test]
fn test_error_single_pipe() {
    expect_error("<?php |;", "Unexpected token");
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

#[test]
fn test_error_missing_function_name() {
    expect_error("<?php function () { }", "Expected function name");
}

#[test]
fn test_error_missing_function_paren() {
    expect_error("<?php function foo { }", "Expected '(' after function name");
}

#[test]
fn test_error_missing_if_paren() {
    expect_error("<?php if 1 { }", "Expected '(' after 'if'");
}

#[test]
fn test_error_ifdef_requires_symbol_name() {
    expect_error(
        "<?php ifdef { echo 1; }",
        "Expected symbol name after 'ifdef'",
    );
}

#[test]
fn test_error_ifdef_requires_braced_body() {
    expect_error("<?php ifdef DEBUG echo 1;", "Expected '{'");
}

#[test]
fn test_error_missing_while_paren() {
    expect_error("<?php while 1 { }", "Expected '(' after 'while'");
}

// --- Type errors ---

#[test]
fn test_error_switch_missing_paren() {
    expect_error("<?php switch $x {}", "Expected '(' after 'switch'");
}

#[test]
fn test_error_match_missing_paren() {
    expect_error("<?php $x = match $x {};", "Expected '(' after 'match'");
}

#[test]
fn test_error_arrow_function_missing_arrow() {
    expect_error(r#"<?php $f = fn($x) $x * 2;"#, "Expected '=>'");
}

#[test]
fn test_error_arrow_function_missing_lparen() {
    expect_error(r#"<?php $f = fn $x => $x * 2;"#, "Expected '(' after 'fn'");
}

// --- v0.7: Default parameter, bitwise, spaceship errors ---

#[test]
fn test_error_heredoc_unterminated() {
    expect_error("<?php echo <<<EOT\nHello", "Unterminated heredoc");
}

// --- Constants errors ---

#[test]
fn test_error_extern_missing_function() {
    expect_error(
        "<?php extern badkw;",
        "Expected 'function', string literal, 'class', or 'global' after 'extern'",
    );
}
