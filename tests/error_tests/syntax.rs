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
    // `function () { }` is NOT a nameless declaration -- it is a closure EXPRESSION, and PHP
    // parses it as one, then complains about the missing statement terminator
    // ("unexpected end of file" on 8.5.10). elephc now says the same thing.
    expect_error("<?php function () { }", "Expected ';'");
    // A genuinely malformed declaration -- a name that is not an identifier -- still reports
    // the missing name, which is the case this test was reaching for.
    expect_error("<?php function 123() { }", "Expected function name");
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

// --- `<>` (PHP alias for `!=`) errors ---

/// Verifies that `<>` with a missing right operand is reported as an unexpected token
/// rather than silently parsing as `<` followed by `>`.
#[test]
fn test_error_angle_not_equal_missing_right_operand() {
    expect_error("<?php $x = 1 <> ;", "Unexpected token: Semicolon");
}

/// Verifies prefix `++` on `$this` itself (not a member of it) is rejected as an invalid
/// increment target instead of being parsed as a member increment.
#[test]
fn test_error_prefix_increment_on_this_itself() {
    expect_error(
        "<?php class C { function f() { ++$this; } }",
        "Invalid increment target",
    );
}

/// Verifies incrementing a method return value is rejected, matching PHP's
/// "Can't use method return value in write context" fatal.
#[test]
fn test_error_increment_method_return_value() {
    expect_error(
        "<?php class C { function foo() { return 1; } function f() { $this->foo()++; } }",
        "Invalid assignment target",
    );
}

// --- foreach destructuring errors ---

/// Verifies an empty `foreach` destructuring pattern reports PHP's "Cannot use empty list".
#[test]
fn test_error_foreach_empty_destructuring_pattern() {
    expect_error("<?php $m = [[1]]; foreach ($m as []) {}", "Cannot use empty list");
}

/// Verifies a `foreach` destructuring pattern that is never closed is reported as a missing
/// `]`, not as a missing loop variable.
#[test]
fn test_error_foreach_unclosed_destructuring_pattern() {
    expect_error(
        "<?php $m = [[1, 2]]; foreach ($m as [$a, $b) {}",
        "Expected ']' after list pattern",
    );
}

/// Verifies taking a reference to the whole destructuring pattern is rejected.
#[test]
fn test_error_foreach_reference_to_destructuring_pattern() {
    expect_error(
        "<?php $m = [[1, 2]]; foreach ($m as &[$a, $b]) {}",
        "Cannot take a reference to a destructuring pattern in foreach",
    );
}

// --- Alternative control-structure syntax ---

/// Verifies an alternative-syntax block that is never closed names the terminator it wants.
#[test]
fn test_error_alternative_syntax_missing_terminator() {
    expect_error("<?php if (true): echo 1;", "Expected 'endif' to close");
    expect_error("<?php while (true): echo 1;", "Expected 'endwhile' to close");
    expect_error("<?php for ($i=0;$i<1;$i++): echo 1;", "Expected 'endfor' to close");
    expect_error("<?php foreach ([1] as $x): echo 1;", "Expected 'endforeach' to close");
    expect_error("<?php switch (1): case 1: echo 1;", "Expected 'endswitch' to close");
}

/// Verifies closing an alternative block with the wrong keyword reports the stray terminator
/// rather than silently accepting a mismatched pair.
#[test]
fn test_error_alternative_syntax_mismatched_terminator() {
    expect_error(
        "<?php foreach ([1] as $x): echo 1; endwhile;",
        "Unexpected 'endwhile': there is no open alternative-syntax block",
    );
}

/// Verifies a terminator keyword with no open alternative block is reported by name.
#[test]
fn test_error_alternative_syntax_stray_terminator() {
    expect_error(
        "<?php endif;",
        "Unexpected 'endif': there is no open alternative-syntax block",
    );
}

/// Verifies an alternative block terminator without its mandatory `;` is reported.
#[test]
fn test_error_alternative_syntax_terminator_needs_semicolon() {
    expect_error("<?php if (true): echo 1; endif", "Expected ';'");
}

/// Verifies mixing a brace `if` body with an `else:`/`elseif:` branch is rejected, as in PHP.
#[test]
fn test_error_alternative_syntax_cannot_mix_with_braces() {
    expect_error(
        "<?php if (true) { echo 1; } else: echo 2; endif;",
        "Cannot mix brace and alternative syntax in one if statement",
    );
    expect_error(
        "<?php if (true) { echo 1; } elseif (false): echo 2; endif;",
        "Cannot mix brace and alternative syntax in one if statement",
    );
}

/// Verifies an alternative `if` cannot take a brace `else` body, mirroring PHP's requirement
/// that the whole chain uses one style.
#[test]
fn test_error_alternative_if_rejects_brace_else() {
    expect_error(
        "<?php if (true): echo 1; else { echo 2; } endif;",
        "Expected ':' after 'else' in an alternative-syntax if block",
    );
}

// --- goto (unsupported) ---

/// Verifies `goto` is rejected with a diagnostic that names the construct, not a generic
/// "unexpected token" error.
#[test]
fn test_error_goto_is_not_supported() {
    expect_error("<?php goto done; done: echo 1;", "`goto` is not supported");
}

/// Verifies a `goto` target label on its own is rejected and names the label.
#[test]
fn test_error_goto_label_is_not_supported() {
    expect_error("<?php done: echo 1;", "`goto` labels are not supported");
    expect_error("<?php done: echo 1;", "the label `done:`");
}

// --- References inside array literals ---

/// Verifies `[&$x]` reports the unsupported construct by name instead of "Unexpected token:
/// Ampersand". elephc arrays hold values, so an element cannot alias a variable's storage.
#[test]
fn test_error_reference_element_in_array_literal() {
    expect_error(
        "<?php $first = 1; $r = [&$first];",
        "Reference elements in array literals (`[&$x]`) are not supported",
    );
}

/// Verifies the same diagnostic covers a keyed element and the legacy `array(...)` spelling.
#[test]
fn test_error_reference_element_in_keyed_and_legacy_array_literals() {
    expect_error(
        "<?php $a = 1; $r = [\"k\" => &$a];",
        "Reference elements in array literals (`[&$x]`) are not supported",
    );
    expect_error(
        "<?php $a = 1; $r = array(&$a);",
        "Reference elements in array literals (`[&$x]`) are not supported",
    );
}

// --- Append lvalues (issue #845) ---

/// Verifies `$a[] = &$x` names the unsupported construct in statement position, in value
/// position, and after a mid-chain append, instead of "Unexpected token: Ampersand".
#[test]
fn test_error_reference_append_is_named() {
    expect_error(
        "<?php $b = []; $y = 1; $b[] = &$y;",
        "Appending a reference (`$a[] = &$x`) is not supported",
    );
    expect_error(
        "<?php $b = []; $y = 1; $x = ($b[] = &$y);",
        "Appending a reference (`$a[] = &$x`) is not supported",
    );
    expect_error(
        "<?php $b = []; $y = 1; $b['k'][]['v'] = &$y;",
        "Appending a reference (`$a[] = &$x`) is not supported",
    );
}

/// Verifies an append dimension used as a READ reports PHP's "Cannot use [] for reading".
#[test]
fn test_error_append_dimension_read() {
    expect_error("<?php $a = [1]; echo $a[];", "Cannot use [] for reading");
    expect_error("<?php $a = [1]; $x = $a[][0];", "Cannot use [] for reading");
}

/// Verifies a compound operator on an append target stays rejected, matching PHP's fatal
/// "Cannot use [] for reading", in both statement and expression position.
#[test]
fn test_error_compound_assignment_to_append_target() {
    expect_error("<?php $a = []; $a[]['k'] .= 'x';", "Invalid assignment target");
    expect_error("<?php $a = []; $x = ($a[] += 1);", "Invalid assignment target");
}

/// Verifies a temporary expression cannot receive an append, as in PHP.
#[test]
fn test_error_append_to_temporary_expression() {
    expect_error("<?php $x = ((1 + 2)[] = 3);", "Invalid assignment target");
}

/// Verifies a `(object)` cast with no operand is rejected rather than silently accepted.
///
/// `(object)` is a PREFIX operator over the three-token `( identifier )` window, so a missing
/// operand surfaces at whatever follows the cast.
#[test]
fn test_error_object_cast_without_an_operand() {
    expect_error("<?php $v = (object);", "Unexpected token: Semicolon");
    expect_error("<?php $v = (object) ;", "Unexpected token: Semicolon");
}

/// Verifies an unterminated cast window is reported as the missing parenthesis it is, rather
/// than being mistaken for a cast over `object`.
#[test]
fn test_error_object_cast_without_a_closing_paren() {
    expect_error("<?php $v = (object;", "Expected closing ')'");
}

/// Verifies a program that declares the compiler's object-cast helper is rejected with a named
/// diagnostic, not with the checker's bare `Duplicate function declaration`.
///
/// `ir_lower` lowers every `(object)` cast to a call on this name, so a user definition would
/// not merely shadow the prelude — it would BECOME the cast's semantics.
#[test]
fn test_error_declaring_the_object_cast_helper() {
    expect_error(
        "<?php function __elephc_cast_object(mixed $v): stdClass { return new stdClass(); } $o = (object) [1];",
        "the name is reserved for the compiler's `(object)` cast helper",
    );
    expect_error(
        "<?php function __elephc_cast_object_dynamic(mixed $v): mixed { return $v; } $o = (object) [1];",
        "the name is reserved for the compiler's `(object)` cast helper",
    );
}

/// A program that declares the helper name but spells NO object cast is left alone: the prelude
/// is pay-for-use, so there is nothing to collide with and nothing to reject.
#[test]
fn test_declaring_the_object_cast_helper_without_a_cast_is_accepted() {
    expect_no_error(
        "<?php function __elephc_cast_object(mixed $v): int { return 1; } echo __elephc_cast_object(2);",
    );
}

/// Issue #476: the `for` clause parser now delegates to the general statement parser, which
/// buys every assignment form for free — and would also accept a DECLARATION, which PHP's
/// clause grammar does not.
///
/// Measured on PHP 8.5.10: `for (function f() {}; false; ) {}` is
/// `syntax error, unexpected identifier "f", expecting "("` — php-src reads `function` as the
/// start of a CLOSURE there, so a named declaration cannot appear. It was rejected here
/// before the delegation too, and has to stay rejected.
#[test]
fn test_error_for_clause_rejects_non_expression_statements() {
    // A named declaration. php-src reads `function` in an expression position as the start of
    // a CLOSURE, so the name is what makes this invalid there:
    // `syntax error, unexpected identifier "f", expecting "("` on 8.5.10.
    expect_error(
        "<?php for (function f() {}; false; ) {} echo 1;",
        "Only expressions are allowed in a for clause",
    );
    // `echo` is a statement, not an expression, and PHP rejects it in a clause too:
    // `syntax error, unexpected token "echo", expecting ";"`. A deny-list of declarations
    // alone let this through.
    expect_error(
        "<?php for (echo \"x\"; false; ) {} echo 1;",
        "Only expressions are allowed in a for clause",
    );
}

/// Issue #476 review follow-up: `include` / `require` in a clause is named, not crashed on.
///
/// They ARE expressions in PHP and PHP runs them there — both `for (include "f.php"; …)` and
/// `for ($v = include "f.php"; …)` execute on 8.5.10. elephc cannot yet, because
/// `resolver::engine`'s `StmtKind::For` arm resolves only the loop BODY, so an include left
/// in a clause survives into the checker as a transient node every consumer treats as
/// `unreachable!()`.
///
/// The assignment spelling is the one that mattered: once the clause started going through
/// the real statement parser it PARSED, and the compiler then panicked with
/// `ExprKind::IncludeValue must be expanded by the resolver`. Before that it was a plain
/// parse error. This pins the diagnostic that replaced the panic.
#[test]
fn test_error_for_clause_rejects_include() {
    expect_error(
        "<?php for (include \"f.php\"; false; ) {} echo 1;",
        "include/require is not supported in a for clause",
    );
    expect_error(
        "<?php for ($v = include \"f.php\"; false; ) {} echo 1;",
        "include/require is not supported in a for clause",
    );
    expect_error(
        "<?php for ($i = 0; $i < 1; require \"f.php\") {} echo 1;",
        "include/require is not supported in a for clause",
    );
}

/// Pins the parser invariant the `for`-clause include guard rests on: `include` is an
/// expression in exactly ONE position, the whole right-hand side of a plain assignment or a
/// `return`.
///
/// `ExprKind::IncludeValue` has two construction sites, both in `try_parse_value_include`, and
/// the assignment one builds a plain `StmtKind::Assign` BEFORE `parse_assignment_value_expr`
/// runs. So no typed, indexed, property or static-property assignment can carry one, and the
/// guard in `parse_for_clause` does not need to walk for them.
///
/// Raised in review on issue #476 as a possible hole. It is not one today — but it is an
/// invariant nobody was testing, so if any of these ever starts parsing, this fails and points
/// at that guard.
#[test]
fn test_error_include_is_only_an_expression_in_an_assignment_rhs() {
    expect_error(
        r#"<?php int $a = include "f.php";"#,
        "Unexpected token: Include",
    );
    expect_error(
        r#"<?php $a = []; $a[0] = include "f.php";"#,
        "Unexpected token: Include",
    );
    expect_error(
        r#"<?php $o = new stdClass(); $o->p = include "f.php";"#,
        "Unexpected token: Include",
    );
    expect_error(
        r#"<?php class K { public static $s; } K::$s = include "f.php";"#,
        "Unexpected token: Include",
    );
    // The same rejection inside a clause, which is the case the review asked about.
    expect_error(
        r#"<?php for (int $v = include "f.php"; false; ) {} echo 1;"#,
        "Unexpected token: Include",
    );
}

/// Issue #476 review follow-up: PHP allows a comma list in the `for` CONDITION as well, but
/// elephc has no sequence expression to hold one and the condition re-runs every iteration,
/// so the leading expressions cannot be hoisted into the init clause.
///
/// The diagnostic says so rather than letting the comma fall through to a bare `Expected ';'`
/// that names neither the construct nor the limitation.
#[test]
fn test_error_for_condition_comma_list_is_named() {
    expect_error(
        "<?php for ($i = 0; $i++, $i < 3; $i++) {}",
        "not supported in a for CONDITION",
    );
}
