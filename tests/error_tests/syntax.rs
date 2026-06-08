//! Purpose:
//! Integration or regression tests for diagnostic coverage of syntax, including missing open tag, unterminated string, and empty variable.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Invalid PHP snippets are checked through shared diagnostic helpers for messages, spans, and recovery behavior.

use super::*;

/// Verifies the error diagnostic for missing open tag.
#[test]
fn test_error_missing_open_tag() {
    // PHP code starting outside an open tag produces a "missing open tag" error.
    expect_error("echo \"hi\";", "<?php");
}

/// Verifies the error diagnostic for unterminated string.
#[test]
fn test_error_unterminated_string() {
    // A double-quoted string that is never closed produces an "Unterminated string" error.
    expect_error("<?php \"no end", "Unterminated string");
}

/// Verifies a complex `{$...}` interpolation without a closing brace is reported, rather
/// than running past the end of the string.
#[test]
fn test_error_unterminated_complex_interpolation() {
    expect_error("<?php $x = 1; echo \"a{$x\";", "complex interpolation");
}

/// Verifies a simple `$arr[offset` interpolation without a closing bracket is reported.
#[test]
fn test_error_unterminated_interpolation_offset() {
    expect_error("<?php $a = [1]; echo \"$a[0\";", "Unterminated array offset");
}

/// Verifies a flexible heredoc whose body line is indented less than the closing marker
/// is reported as an invalid body indentation level (PHP 7.3+).
#[test]
fn test_error_heredoc_invalid_indentation() {
    expect_error(
        "<?php echo <<<EOT\n    indented\n  under\n    EOT;\n",
        "Invalid heredoc body indentation level",
    );
}

/// Verifies the error diagnostic for invalid unicode string escape.
#[test]
fn test_error_invalid_unicode_string_escape() {
    // A UTF-8 codepoint escape (`\u{NNNNN}`) outside the valid Unicode range (0x10FFFF) produces
    // "Invalid UTF-8 codepoint escape sequence". Regression test for \u{110000} specifically.
    expect_error(
        r#"<?php echo "\u{110000}";"#,
        "Invalid UTF-8 codepoint escape sequence",
    );
}

/// Verifies the error diagnostic for empty variable.
#[test]
fn test_error_empty_variable() {
    // A bare `$` followed by a semicolon (no variable name) produces "Expected variable name".
    expect_error("<?php $;", "Expected variable name");
}

/// Verifies the error diagnostic for bare identifier.
#[test]
fn test_error_bare_identifier() {
    // An unquoted identifier with no matching constant definition produces
    // "Undefined constant: foo". The lexer treats `foo` as a name token, not a variable.
    expect_error("<?php foo;", "Undefined constant: foo");
}

/// Verifies the error diagnostic for unexpected character.
#[test]
fn test_error_unexpected_character() {
    // A backtick outside any expression context is an unexpected character error.
    expect_error("<?php `", "Unexpected character");
}

/// Verifies the error diagnostic for empty list destructuring pattern.
#[test]
fn test_error_empty_list_destructuring_pattern() {
    // `list()` with no entries (`[]`) on the left side of an assignment is forbidden.
    expect_error("<?php [] = [1];", "Cannot use empty list");
}

/// Verifies the error diagnostic for list destructuring all skipped.
#[test]
fn test_error_list_destructuring_all_skipped() {
    // `list()` with only skip placeholders (`[, ,]`) is not allowed.
    expect_error("<?php [, ,] = [1, 2];", "Cannot use empty list");
}

/// Verifies the error diagnostic for list destructuring mixes keyed and unkeyed entries.
#[test]
fn test_error_list_destructuring_mixes_keyed_and_unkeyed_entries() {
    // `list()` cannot mix keyed (`"id" => $id`) and unkeyed (`$a`) entries in the same destructuring.
    expect_error(
        "<?php [$a, \"id\" => $id] = [1, \"id\" => 2];",
        "Cannot mix keyed and unkeyed list entries",
    );
}

/// Verifies the error diagnostic for list destructuring requires writable target.
#[test]
fn test_error_list_destructuring_requires_writable_target() {
    // The list pattern left-hand side must be writable; an expression like `1 + 2` is invalid.
    expect_error("<?php [1 + 2] = [3];", "Invalid list destructuring target");
}

// --- Attribute syntax errors ---

/// Verifies the error diagnostic for unterminated attribute group.
#[test]
fn test_error_unterminated_attribute_group() {
    // An attribute group opened with `#[` but missing the closing `]` produces an error.
    expect_error(
        "<?php #[Foo class C {}",
        "Expected ',' or ']' between attributes",
    );
}

/// Verifies the error diagnostic for empty attribute group.
#[test]
fn test_error_empty_attribute_group() {
    // An empty attribute group `#[]` before a class declaration is rejected.
    expect_error("<?php #[] class C {}", "Empty attribute group");
}

/// Verifies the error diagnostic for attribute missing identifier.
#[test]
fn test_error_attribute_missing_identifier() {
    // An attribute whose first entry is a numeric literal (not an identifier) is rejected.
    expect_error(
        "<?php #[123] class C {}",
        "Expected attribute name (identifier)",
    );
}

/// Verifies the error diagnostic for attribute starts with comma.
#[test]
fn test_error_attribute_starts_with_comma() {
    // An attribute group whose first entry is a comma (not an identifier) is rejected.
    expect_error(
        "<?php #[, A] class C {}",
        "Expected attribute name (identifier)",
    );
}

/// Verifies the error diagnostic for attribute qualifier dangling backslash.
#[test]
fn test_error_attribute_qualifier_dangling_backslash() {
    // An attribute name that is a lone backslash is rejected as an invalid identifier.
    expect_error(
        "<?php #[\\] class C {}",
        "Expected attribute name (identifier)",
    );
}

/// Verifies the error diagnostic for attribute unterminated arguments.
#[test]
fn test_error_attribute_unterminated_arguments() {
    // An attribute opened with `(` but never closed (missing `)`) produces an error.
    expect_error(
        "<?php #[Foo(1, 2 class C {}",
        "Expected ',' between arguments",
    );
}

/// Verifies the error diagnostic for attribute on echo statement is rejected.
#[test]
fn test_error_attribute_on_echo_statement_is_rejected() {
    // PHP only allows attributes on declarations; an `echo` statement is not a valid target.
    expect_error(
        "<?php #[Foo] echo 1;",
        "Attributes are only allowed before declarations",
    );
}

/// Verifies the error diagnostic for attribute on assignment is rejected.
#[test]
fn test_error_attribute_on_assignment_is_rejected() {
    // Attributes are only permitted before declaration statements; an assignment is rejected.
    expect_error(
        "<?php #[Foo] $x = 1;",
        "Attributes are only allowed before declarations",
    );
}

/// Verifies the error diagnostic for attribute on if is rejected.
#[test]
fn test_error_attribute_on_if_is_rejected() {
    // Attributes are only permitted before declaration statements; an `if` control flow is rejected.
    expect_error(
        "<?php #[Foo] if (true) { echo 1; }",
        "Attributes are only allowed before declarations",
    );
}

// --- Numeric literal errors ---

/// Verifies the error diagnostic for explicit octal invalid digit.
#[test]
fn test_error_explicit_octal_invalid_digit() {
    // Explicit octal literals (`0o`) using a digit outside 0-7 (e.g., `0o78`) produces an error.
    expect_error("<?php $x = 0o78;", "after octal literal");
}

/// Verifies the error diagnostic for explicit octal empty.
#[test]
fn test_error_explicit_octal_empty() {
    // An explicit octal literal with no digits (`0o`) produces "Expected octal digits".
    expect_error("<?php $x = 0o;", "Expected octal digits");
}

/// Verifies the error diagnostic for explicit octal separator after prefix.
#[test]
fn test_error_explicit_octal_separator_after_prefix() {
    // An underscore immediately after the `0o` prefix (e.g., `0o_77`) is rejected.
    expect_error("<?php $x = 0o_77;", "Expected octal digits");
}

/// Verifies the error diagnostic for legacy octal invalid digit.
#[test]
fn test_error_legacy_octal_invalid_digit() {
    // Legacy octal literals (starting with `0` followed by digits) that contain 8 or 9 produce
    // "Invalid octal literal". E.g., `078` contains the digit 8.
    expect_error("<?php $x = 078;", "Invalid octal literal");
}

/// Verifies the error diagnostic for legacy octal separator invalid digit.
#[test]
fn test_error_legacy_octal_separator_invalid_digit() {
    // A legacy octal literal with a digit 8 or 9 after the leading zero and separator (e.g.,
    // `0_778`) is rejected as an invalid octal digit, not as a separator placement error.
    expect_error("<?php $x = 0_778;", "Invalid octal literal");
}

/// Verifies the error diagnostic for hex empty.
#[test]
fn test_error_hex_empty() {
    // A hex literal with no digits (`0x`) produces "Expected hex digits".
    expect_error("<?php $x = 0x;", "Expected hex digits");
}

/// Verifies the error diagnostic for hex invalid trailing.
#[test]
fn test_error_hex_invalid_trailing() {
    // A hex literal with a non-hex character after valid digits (e.g., `0xfg`) produces an error.
    expect_error("<?php $x = 0xfg;", "after hex literal");
}

/// Verifies the error diagnostic for hex separator after prefix.
#[test]
fn test_error_hex_separator_after_prefix() {
    // An underscore immediately after the `0x` prefix (e.g., `0x_FF`) is rejected.
    expect_error("<?php $x = 0x_FF;", "Expected hex digits");
}

/// Verifies the error diagnostic for binary empty.
#[test]
fn test_error_binary_empty() {
    // A binary literal with no digits (`0b`) produces "Expected binary digits".
    expect_error("<?php $x = 0b;", "Expected binary digits");
}

/// Verifies the error diagnostic for binary invalid digit.
#[test]
fn test_error_binary_invalid_digit() {
    // A binary literal using a digit outside 0-1 (e.g., `0b12`) produces an error.
    expect_error("<?php $x = 0b12;", "after binary literal");
}

/// Verifies the error diagnostic for binary separator after prefix.
#[test]
fn test_error_binary_separator_after_prefix() {
    // An underscore immediately after the `0b` prefix (e.g., `0b_10`) is rejected.
    expect_error("<?php $x = 0b_10;", "Expected binary digits");
}

/// Verifies the error diagnostic for decimal trailing underscore.
#[test]
fn test_error_decimal_trailing_underscore() {
    // A decimal literal with a trailing underscore (e.g., `1_`) produces an error.
    expect_error("<?php $x = 1_;", "after decimal literal");
}

/// Verifies the error diagnostic for decimal double underscore.
#[test]
fn test_error_decimal_double_underscore() {
    // A decimal literal with consecutive underscores (e.g., `1__0`) produces an error.
    expect_error("<?php $x = 1__0;", "after decimal literal");
}

/// Verifies the error diagnostic for control requires operand.
#[test]
fn test_error_control_requires_operand() {
    // The error-suppression operator `@` requires an expression operand; bare `@;` is rejected.
    expect_error(
        "<?php @;",
        "Unexpected token",
    );
}

/// Verifies the error diagnostic for print requires operand.
#[test]
fn test_error_print_requires_operand() {
    // The `print` keyword requires an expression operand; bare `print;` is rejected.
    expect_error("<?php print;", "Unexpected token");
}

/// Verifies the error diagnostic for echo trailing comma requires argument.
#[test]
fn test_error_echo_trailing_comma_requires_argument() {
    // `echo` with a trailing comma but no following expression (e.g., `echo "A",;`) is rejected.
    expect_error("<?php echo \"A\",;", "Unexpected token");
}

/// Verifies that a lone comma inside an otherwise-empty call argument list is rejected
/// (a trailing comma after a real argument is allowed, but `foo(,)` is not, matching PHP).
#[test]
fn test_error_leading_comma_in_call_args() {
    expect_error("<?php foo(,);", "Unexpected token");
}

/// Verifies that a doubled trailing comma in a call argument list is rejected (`foo(1,,)`).
#[test]
fn test_error_double_trailing_comma_in_call_args() {
    expect_error("<?php foo(1,,);", "Unexpected token");
}

/// Verifies that a lone comma inside an otherwise-empty parameter list is rejected (`f(,)`).
#[test]
fn test_error_leading_comma_in_param_list() {
    expect_error("<?php function f(,) {}", "Expected parameter variable");
}

/// Verifies the error diagnostic for break level must be positive.
#[test]
fn test_error_break_level_must_be_positive() {
    // The `break` level argument must be a positive integer; `break 0;` is rejected.
    expect_error("<?php while (1) { break 0; }", "accepts only positive integers");
}

/// Verifies the error diagnostic for continue level must be integer literal.
#[test]
fn test_error_continue_level_must_be_integer_literal() {
    // The `continue` level must be an integer literal (not a variable); `continue $n;` is rejected.
    expect_error(
        "<?php $n = 1; while (1) { continue $n; }",
        "requires an integer literal level",
    );
}

/// Verifies the error diagnostic for single ampersand.
#[test]
fn test_error_single_ampersand() {
    // A standalone `&` token (not part of a binop, ref param, or `include`) is rejected.
    expect_error("<?php &;", "Unexpected token");
}

/// Verifies the error diagnostic for single pipe.
#[test]
fn test_error_single_pipe() {
    // A standalone `|` token (not part of a binop) is rejected.
    expect_error("<?php |;", "Unexpected token");
}

// --- Parser errors ---

/// Verifies the error diagnostic for missing semicolon.
#[test]
fn test_error_missing_semicolon() {
    // An `echo` statement without a terminating semicolon produces "Expected ';'".
    expect_error("<?php echo \"hi\"", "Expected ';'");
}

/// Verifies the error diagnostic for missing equals.
#[test]
fn test_error_missing_equals() {
    // An assignment without an `=` between variable and expression produces "Expected '='".
    expect_error("<?php $x \"hi\";", "Expected '='");
}

/// Verifies the error diagnostic for unclosed paren.
#[test]
fn test_error_unclosed_paren() {
    // An unclosed parenthesis in an expression (e.g., missing `)`) produces "Expected closing ')'".
    expect_error("<?php echo (1 + 2;", "Expected closing ')'");
}

/// Verifies the error diagnostic for unexpected token in expr.
#[test]
fn test_error_unexpected_token_in_expr() {
    // A bare semicolon in expression position (e.g., `echo ;`) produces "Unexpected token".
    expect_error("<?php echo ;", "Unexpected token");
}

/// Verifies the error diagnostic for unexpected token in stmt.
#[test]
fn test_error_unexpected_token_in_stmt() {
    // A bare expression statement (e.g., `42;`) in statement position produces "Unexpected token".
    expect_error("<?php 42;", "Unexpected token");
}

/// Verifies the error diagnostic for missing function name.
#[test]
fn test_error_missing_function_name() {
    // A `function` keyword without a following name produces "Expected function name".
    expect_error("<?php function () { }", "Expected function name");
}

/// Verifies the error diagnostic for missing function paren.
#[test]
fn test_error_missing_function_paren() {
    // A function declaration missing the opening `(` after the name produces "Expected '(' after function name".
    expect_error("<?php function foo { }", "Expected '(' after function name");
}

/// Verifies the error diagnostic for missing if paren.
#[test]
fn test_error_missing_if_paren() {
    // An `if` statement missing the opening `(` after `if` produces "Expected '(' after 'if'".
    expect_error("<?php if 1 { }", "Expected '(' after 'if'");
}

/// Verifies the error diagnostic for ifdef requires symbol name.
#[test]
fn test_error_ifdef_requires_symbol_name() {
    // `ifdef` without a symbol name after it produces "Expected symbol name after 'ifdef'".
    expect_error(
        "<?php ifdef { echo 1; }",
        "Expected symbol name after 'ifdef'",
    );
}

/// Verifies the error diagnostic for ifdef requires braced body.
#[test]
fn test_error_ifdef_requires_braced_body() {
    // `ifdef` with a symbol but no braced body produces "Expected '{'".
    expect_error("<?php ifdef DEBUG echo 1;", "Expected '{'");
}

/// Verifies the error diagnostic for missing while paren.
#[test]
fn test_error_missing_while_paren() {
    // A `while` statement missing the opening `(` after `while` produces "Expected '(' after 'while'".
    expect_error("<?php while 1 { }", "Expected '(' after 'while'");
}

// --- Type errors ---

/// Verifies the error diagnostic for switch missing paren.
#[test]
fn test_error_switch_missing_paren() {
    // A `switch` statement missing the opening `(` after `switch` produces "Expected '(' after 'switch'".
    expect_error("<?php switch $x {}", "Expected '(' after 'switch'");
}

/// Verifies the error diagnostic for foreach key by reference.
#[test]
fn test_error_foreach_key_by_reference() {
    // In `foreach`, the key element cannot be by-reference (`&$k`); this produces
    // "Key element cannot be a reference in foreach".
    expect_error(
        "<?php foreach ($a as &$k => $v) {}",
        "Key element cannot be a reference in foreach",
    );
}

/// Verifies the error diagnostic for match missing paren.
#[test]
fn test_error_match_missing_paren() {
    // A `match` expression missing the opening `(` after `match` produces "Expected '(' after 'match'".
    expect_error("<?php $x = match $x {};", "Expected '(' after 'match'");
}

/// Verifies the error diagnostic for arrow function missing arrow.
#[test]
fn test_error_arrow_function_missing_arrow() {
    // An arrow function (`fn`) without `=>` after the parameter list produces "Expected '=>'".
    expect_error(r#"<?php $f = fn($x) $x * 2;"#, "Expected '=>'");
}

/// Verifies the error diagnostic for arrow function missing lparen.
#[test]
fn test_error_arrow_function_missing_lparen() {
    // An arrow function (`fn`) without `(` before parameters produces "Expected '(' after 'fn'".
    expect_error(r#"<?php $f = fn $x => $x * 2;"#, "Expected '(' after 'fn'");
}

// --- v0.7: Default parameter, bitwise, spaceship errors ---

/// Verifies the error diagnostic for heredoc unterminated.
#[test]
fn test_error_heredoc_unterminated() {
    // A heredoc opened with `<<<EOT` but never closed produces "Unterminated heredoc".
    expect_error("<?php echo <<<EOT\nHello", "Unterminated heredoc");
}

// --- Constants errors ---

/// Verifies the error diagnostic for extern missing function.
#[test]
fn test_error_extern_missing_function() {
    // `extern` without a valid keyword after it (`badkw`) produces an error describing valid forms:
    // 'function', string literal, 'class', or 'global'.
    expect_error(
        "<?php extern badkw;",
        "Expected 'function', string literal, 'class', or 'global' after 'extern'",
    );
}
