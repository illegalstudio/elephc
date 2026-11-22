//! Purpose:
//! Runs the source-level prechecks that preserve Zend diagnostics for constructs the view erases.
//!
//! Called from:
//! - `crate::syntax::tokenizer_validation::validate()` before the parser view is built.
//!
//! Key details:
//! - Each check returns the first PHP-shaped error; the main parser never sees blanked regions.

use crate::scanner::{newline_count, operator, Operator, HALT_COMPILER_OUTERMOST_SCOPE_MESSAGE};

use super::diagnostics::*;
use super::parser_view::{parser_view, ternary_colon_before_label};
use super::*;

/// Rejects an unterminated backtick shell string with Zend's delimiter expectation.
#[inline(never)]
pub(crate) fn unterminated_backtick_error<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
) -> Option<ValidationError> {
    let mut open = false;
    for token in tokens {
        if token.id() == i64::from(b'`') {
            open = !open;
        }
    }
    open.then(|| ValidationError {
        message: "syntax error, unexpected end of file, expecting \"`\"".into(),
        pos: source.len(),
        lexical: false,
    })
}

/// Reports PHP's diagnostics for malformed `goto` statements.
///
/// Valid `goto label;` is blanked from the parser view before parsing, so only invalid shapes
/// reach this scan. Zend reports the token after `goto` (or EOF), not `goto` itself.
#[inline(never)]
pub(crate) fn goto_statement_error<T: SourceToken>(source: &[u8], tokens: &[T]) -> Option<ValidationError> {
    for index in 0..tokens.len() {
        if tokens[index].id() != tok::T_GOTO || !goto_is_statement(tokens, index) {
            continue;
        }
        let next = next_significant_index(tokens, index);
        let Some(next) = next else {
            return Some(end_of_file_expecting(source.len(), Some("\";\"")));
        };
        let id = tokens[next].id();
        if id == tok::T_STRING {
            let after = next_significant_index(tokens, next);
            let Some(after) = after else {
                return Some(end_of_file_expecting(source.len(), Some("\";\"")));
            };
            let after_id = tokens[after].id();
            if after_id == i64::from(b';') || after_id == tok::T_CLOSE_TAG {
                // Valid `goto label;`; the parser view removes it.
                continue;
            }
            return Some(unexpected_token_at(
                source,
                tokens[after].pos(),
                tokens[after].len(),
                Some("\";\""),
            ));
        }
        let noun = if id == tok::T_LNUMBER {
            "integer"
        } else if id == tok::T_VARIABLE {
            "variable"
        } else {
            "token"
        };
        return Some(unexpected_noun_at(
            source,
            tokens[next].pos(),
            tokens[next].len(),
            noun,
            Some("identifier"),
        ));
    }
    None
}

/// Returns whether a `goto` token is in statement position rather than a member name.
#[inline(never)]
pub(crate) fn goto_is_statement<T: SourceToken>(tokens: &[T], index: usize) -> bool {
    let Some(previous) = previous_significant_index(tokens, index) else {
        return true;
    };
    matches!(
        tokens[previous].id(),
        id if id == i64::from(b';')
            || id == i64::from(b'{')
            || id == i64::from(b'}')
            || id == i64::from(b':')
            || id == i64::from(b')')
            || id == tok::T_OPEN_TAG
            || id == tok::T_CLOSE_TAG
            || id == tok::T_ELSE
            || id == tok::T_DO
            || id == tok::T_TRY
            || id == tok::T_FINALLY
    )
}

/// Maps each paired delimiter to its partner for reference-chain and arrow-header scans.
///
/// The map follows the same generic nesting rule as the reference-target scan: any closing
/// delimiter closes the innermost opener, regardless of its spelling. A target group can then
/// be skipped in O(1) instead of rescanned once per enclosing reference marker.
pub(super) fn delimiter_pairs<T: SourceToken>(tokens: &[T]) -> Vec<Option<usize>> {
    let mut stack: Vec<usize> = Vec::new();
    let mut pairs = vec![None; tokens.len()];
    for (index, token) in tokens.iter().enumerate() {
        let id = token.id();
        if id == i64::from(b'(')
            || id == i64::from(b'[')
            || id == i64::from(b'{')
            || id == tok::T_ATTRIBUTE
            || id == tok::T_CURLY_OPEN
            || id == tok::T_DOLLAR_OPEN_CURLY_BRACES
        {
            stack.push(index);
        } else if id == i64::from(b')') || id == i64::from(b']') || id == i64::from(b'}') {
            if let Some(open) = stack.pop() {
                pairs[open] = Some(index);
                pairs[index] = Some(open);
            }
        }
    }
    pairs
}

/// Rejects a malformed `array(&…)`, `list(&…)` or `[&…]` reference entry.
///
/// Zend consumes the marker and reports the token that follows it, so `array(&1)` is
/// `unexpected integer "1"`. A valid target must then stay a write-context variable chain:
/// `array(&$x + 1)` is `unexpected token "+", expecting "->" or "?->" or "["`. Removing the
/// marker before either check would hand the parser a valid plain array entry.
#[inline(never)]
pub(crate) fn invalid_array_reference_error<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
    reference_delimiters: &OnceCell<Vec<Option<usize>>>,
) -> Option<ValidationError> {
    let has_ampersand = tokens.iter().any(|token| {
        token.id() == tok::T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG
            || token.id() == tok::T_AMPERSAND_NOT_FOLLOWED_BY_VAR_OR_VARARG
    });
    if !has_ampersand {
        return None;
    }
    // Every nested target group is skipped through this map instead of rescanned, keeping
    // `array(&$a[array(&$b[0])])` linear in the number of tokens.
    let close_of = reference_delimiters.get_or_init(|| delimiter_pairs(tokens));
    // Every open delimiter with its closing token and whether it is an array/list entry list.
    let mut delimiters: Vec<(i64, bool)> = Vec::new();
    for index in 0..tokens.len() {
        let id = tokens[index].id();
        if id == i64::from(b'(') {
            let entry = previous_significant_index(tokens, index).is_some_and(|before| {
                matches!(tokens[before].id(), id if id == tok::T_ARRAY || id == tok::T_LIST)
            });
            delimiters.push((i64::from(b')'), entry));
            continue;
        }
        if id == i64::from(b'[') {
            delimiters.push((i64::from(b']'), bracket_opens_array_literal(tokens, index)));
            continue;
        }
        if id == tok::T_ATTRIBUTE {
            delimiters.push((i64::from(b']'), false));
            continue;
        }
        if id == i64::from(b'{')
            || id == tok::T_CURLY_OPEN
            || id == tok::T_DOLLAR_OPEN_CURLY_BRACES
        {
            delimiters.push((i64::from(b'}'), false));
            continue;
        }
        if id == i64::from(b')') || id == i64::from(b']') || id == i64::from(b'}') {
            delimiters.pop();
            continue;
        }
        if id != tok::T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG
            && id != tok::T_AMPERSAND_NOT_FOLLOWED_BY_VAR_OR_VARARG
        {
            continue;
        }
        let Some(previous) = previous_significant_index(tokens, index) else { continue; };
        if tokens[previous].id() != i64::from(b'(')
            && tokens[previous].id() != i64::from(b',')
            && tokens[previous].id() != i64::from(b'[')
            && tokens[previous].id() != tok::T_DOUBLE_ARROW
        {
            continue;
        }
        let Some((terminator, true)) = delimiters.last().copied() else { continue; };
        let Some(target) = next_significant_index(tokens, index) else {
            return Some(end_of_file_expecting(source.len(), None));
        };
        // The parser view checks these targets without hiding syntax errors inside them.
        if tokens[target].id() == i64::from(b'(') || tokens[target].id() == tok::T_READONLY {
            continue;
        }
        if !reference_target_starts_at(tokens, target) {
            return Some(token_error_at(source, tokens, tokens[target].pos()));
        }
        let mut depth = 0usize;
        let mut cursor = target;
        let mut previous_chain_id = 0i64;
        let mut first = true;
        while cursor < tokens.len() {
            let id = tokens[cursor].id();
            if is_trivia_token(id) {
                cursor += 1;
                continue;
            }
            if id == i64::from(b'(')
                || id == i64::from(b'[')
                || id == i64::from(b'{')
                || id == tok::T_ATTRIBUTE
                || id == tok::T_CURLY_OPEN
                || id == tok::T_DOLLAR_OPEN_CURLY_BRACES
            {
                if let Some(close) = close_of[cursor] {
                    previous_chain_id = tokens[close].id();
                    first = false;
                    cursor = close + 1;
                    continue;
                }
                depth += 1;
            } else if id == i64::from(b')')
                || id == i64::from(b']')
                || id == i64::from(b'}')
            {
                if depth == 0 {
                    if id == terminator {
                        break;
                    }
                    return Some(reference_chain_error(source, tokens, cursor));
                }
                depth -= 1;
            } else if depth == 0 {
                if id == i64::from(b',') {
                    break;
                }
                let token = &tokens[cursor];
                let identifier = is_identifier_text(
                    &source[token.pos()..token.pos() + token.len()],
                );
                let member_name = identifier
                    && (first
                        || matches!(
                            previous_chain_id,
                            id if id == tok::T_DOUBLE_COLON
                                || id == tok::T_OBJECT_OPERATOR
                                || id == tok::T_NULLSAFE_OBJECT_OPERATOR
                        ));
                if !reference_chain_base_token(id) && !member_name {
                    return Some(reference_chain_error(source, tokens, cursor));
                }
            }
            previous_chain_id = id;
            first = false;
            cursor += 1;
        }
        if cursor >= tokens.len() {
            return Some(end_of_file_expecting(source.len(), None));
        }
    }
    None
}

/// Returns whether `id` is a base token in a `&` reference target's variable chain.
fn reference_chain_base_token(id: i64) -> bool {
    id == tok::T_VARIABLE
        || id == tok::T_STRING
        || id == tok::T_IDENTIFIER
        || id == tok::T_NAME_FULLY_QUALIFIED
        || id == tok::T_NAME_QUALIFIED
        || id == tok::T_NAME_RELATIVE
        || id == tok::T_STATIC
        || id == i64::from(b'\\')
        || id == i64::from(b'$')
        || id == tok::T_DOUBLE_COLON
        || id == tok::T_OBJECT_OPERATOR
        || id == tok::T_NULLSAFE_OBJECT_OPERATOR
}

/// Zend's expected tokens when a reference target needs a variable dereference.
const REFERENCE_CONTINUATION: &str = "\"->\" or \"?->\" or \"[\"";

/// Builds Zend's reference-chain continuation diagnostic for one token.
fn reference_chain_error<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
    index: usize,
) -> ValidationError {
    unexpected_token_at(
        source,
        tokens[index].pos(),
        tokens[index].len(),
        Some(REFERENCE_CONTINUATION),
    )
}

/// Checks the postfix chain of a grouped reference or a reference to a `readonly()` call.
///
/// A group or class constant needs another dereference. A call, index, object member or
/// static variable can finish the chain. The shared parser still validates the expressions
/// inside the skipped delimiters and the expression following a valid reference.
pub(super) fn normalized_reference_error<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
    target: usize,
    close_of: &[Option<usize>],
    assignment: bool,
    end_pos: usize,
) -> Option<ReferenceError> {
    let target_pos = tokens[target].pos();
    let mut referenceable = tokens[target].id() == tok::T_READONLY;
    let open = if referenceable {
        let after = next_significant_index(tokens, target);
        let Some(open) = after.filter(|&open| tokens[open].id() == i64::from(b'(')) else {
            return Some(ReferenceError {
                diagnostic: reference_expectation_error(source, tokens, after, end_pos, "\"(\""),
                target_pos,
                readonly_without_call: true,
            });
        };
        open
    } else {
        target
    };
    let close = close_of[open].filter(|&close| tokens[close].id() == i64::from(b')'))?;
    let mut next = next_significant_index(tokens, close);
    while let Some(index) = next {
        let operator = tokens[index].id();
        let end = match operator {
            id if id == i64::from(b'(') || id == i64::from(b'[') => {
                referenceable = true;
                let expected = if id == i64::from(b'(') { b')' } else { b']' };
                close_of[index].filter(|&close| tokens[close].id() == i64::from(expected))?
            }
            tok::T_OBJECT_OPERATOR | tok::T_NULLSAFE_OBJECT_OPERATOR | tok::T_DOUBLE_COLON => {
                let mut member = next_significant_index(tokens, index)?;
                let computed_variable = tokens[member].id() == i64::from(b'$');
                while tokens[member].id() == i64::from(b'$') {
                    member = next_significant_index(tokens, member)?;
                }
                let token = &tokens[member];
                referenceable = operator != tok::T_DOUBLE_COLON
                    || computed_variable
                    || token.id() == tok::T_VARIABLE;
                if token.id() == i64::from(b'{') {
                    close_of[member].filter(|&close| tokens[close].id() == i64::from(b'}'))?
                } else if token.id() == tok::T_VARIABLE
                    || is_identifier_text(&source[token.pos()..token.pos() + token.len()])
                {
                    member
                } else {
                    return None;
                }
            }
            _ => break,
        };
        next = next_significant_index(tokens, end);
    }
    if !referenceable
        || (!assignment && next.is_some_and(|next| {
            let id = tokens[next].id();
            id != i64::from(b',') && id != i64::from(b']') && id != i64::from(b')')
        }))
    {
        return Some(ReferenceError {
            diagnostic: reference_expectation_error(
                source,
                tokens,
                next,
                end_pos,
                REFERENCE_CONTINUATION,
            ),
            target_pos,
            readonly_without_call: false,
        });
    }
    let next = next?;
    let token = &tokens[next];
    if assignment
        && invalid_reference_assignment_continuation(&source[token.pos()..token.pos() + token.len()])
    {
        return Some(ReferenceError {
            diagnostic: unexpected_token_at(source, token.pos(), token.len(), None),
            target_pos,
            readonly_without_call: false,
        });
    }
    None
}

/// Returns whether the token would consume the reference source as an assignment or update.
fn invalid_reference_assignment_continuation(source: &[u8]) -> bool {
    source == b"="
        || matches!(
            operator(source),
            Some((
                Operator::PlusAssign
                | Operator::MinusAssign
                | Operator::StarAssign
                | Operator::SlashAssign
                | Operator::DotAssign
                | Operator::PercentAssign
                | Operator::AmpAssign
                | Operator::PipeAssign
                | Operator::CaretAssign
                | Operator::LessLessAssign
                | Operator::GreaterGreaterAssign
                | Operator::QuestionQuestionAssign
                | Operator::StarStarAssign
                | Operator::PlusPlus
                | Operator::MinusMinus,
                _,
            ))
        )
}

/// Reports a missing reference continuation at the offending token or the end of the view.
fn reference_expectation_error<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
    next: Option<usize>,
    end_pos: usize,
    expecting: &str,
) -> ValidationError {
    let Some(token) = next.map(|index| &tokens[index]) else {
        return end_of_file_expecting(end_pos, Some(expecting));
    };
    let error = if token.id() == tok::T_CLOSE_TAG {
        ValidationError {
            message: "syntax error, unexpected token \";\"".into(),
            pos: token.pos(),
            lexical: false,
        }
    } else {
        token_error_at(source, std::slice::from_ref(token), token.pos())
    };
    with_expectation(error, expecting)
}

/// Rejects `new $class::Name` and `new $class::{...}` class-name chains.
///
/// PHP only accepts `new $class::$property(...)`; the general expression parser is more
/// permissive, so the dynamic-class shapes are rejected explicitly.
#[inline(never)]
pub(crate) fn new_dynamic_static_error<T: SourceToken>(source: &[u8], tokens: &[T]) -> Option<ValidationError> {
    for index in 0..tokens.len() {
        if tokens[index].id() != tok::T_NEW {
            continue;
        }
        let Some(name) = next_significant_index(tokens, index) else { continue; };
        if !matches!(
            tokens[name].id(),
            id if id == tok::T_VARIABLE || id == tok::T_STRING || id == tok::T_IDENTIFIER
        ) {
            continue;
        }
        let Some(colon) = next_significant_index(tokens, name) else { continue; };
        if tokens[colon].id() != tok::T_DOUBLE_COLON {
            continue;
        }
        let Some(member) = next_significant_index(tokens, colon) else { continue; };
        if tokens[member].id() == tok::T_VARIABLE {
            continue;
        }
        let noun = if tokens[member].id() == tok::T_STRING {
            "identifier"
        } else {
            "token"
        };
        return Some(unexpected_noun_at(
            source,
            tokens[member].pos(),
            tokens[member].len(),
            noun,
            Some("variable or \"$\""),
        ));
    }
    None
}

/// Returns Zend's warning for every `declare(encoding=...)` directive.
#[inline(never)]
pub(crate) fn declare_encoding_warnings<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
) -> Vec<ValidationWarning> {
    let mut warnings = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        if token.id() != tok::T_DECLARE {
            continue;
        }
        let Some(close) = complete_declare_encoding(source, tokens, index) else { continue; };
        warnings.push(ValidationWarning {
            message: "declare(encoding=...) ignored because Zend multibyte feature is turned off by settings".into(),
            // Zend reports this at the closing `)`: a multiline directive list prints the
            // line of that token, not the `declare` keyword.
            pos: tokens[close].pos(),
            // Zend reports this through php_error_docref(NULL, E_WARNING, ...), not as a
            // deprecation.
            severity: 2,
        });
    }
    warnings
}

/// Returns the closing `)` of a complete `declare(...)` list when it sets `encoding`.
///
/// Zend compiles a `declare` only after its whole directive list parses. A syntax error
/// inside the list therefore never reports the encoding warning, so the token scan has to
/// prove the list complete before claiming one.
pub(crate) fn complete_declare_encoding<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
    declare: usize,
) -> Option<usize> {
    let open = next_significant_index(tokens, declare)?;
    if tokens[open].id() != i64::from(b'(') {
        return None;
    }
    let mut encoding = false;
    let mut openers: Vec<u8> = Vec::new();
    let mut expect_name = true;
    let mut has_equals = false;
    let mut has_value = false;
    let mut index = next_significant_index(tokens, open)?;
    loop {
        let id = tokens[index].id();
        if expect_name {
            if id != tok::T_STRING {
                return None;
            }
            let text = &source[tokens[index].pos()..tokens[index].pos() + tokens[index].len()];
            encoding |= text.eq_ignore_ascii_case(b"encoding");
            expect_name = false;
            has_equals = false;
            has_value = false;
        } else if !has_equals {
            if id != i64::from(b'=') {
                return None;
            }
            has_equals = true;
        } else if id == i64::from(b')') && openers.is_empty() {
            return (has_value && encoding).then_some(index);
        } else if id == i64::from(b',') && openers.is_empty() {
            if !has_value {
                return None;
            }
            expect_name = true;
        } else {
            if let Some(opener) = [b'(', b'[', b'{']
                .into_iter()
                .find(|opener| i64::from(*opener) == id)
            {
                openers.push(opener);
            } else if let Some(expected) = [b')', b']', b'}']
                .into_iter()
                .find(|closer| i64::from(*closer) == id)
            {
                let opener = match expected {
                    b')' => b'(',
                    b']' => b'[',
                    _ => b'{',
                };
                if openers.pop() != Some(opener) {
                    return None;
                }
            }
            has_value = true;
        }
        index = next_significant_index(tokens, index)?;
    }
}

/// Validates the expression inside `${...}` before the parser view erases it.
///
/// Zend parses the contents as an expression (variable variables included), so `${1+}` and
/// `${}` are syntax errors even though the view rewrites the whole interpolation to `$x`.
pub(crate) fn dollar_brace_expression_error<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
    names: &mut Vec<usize>,
) -> Option<ValidationError> {
    // Matching braces are computed once: a nested `${${...}}` chain otherwise rescans the
    // remaining source from every `$` and turns a linear parse into quadratic work.
    let close_of = brace_matches(tokens);
    // Only nested `${...}` bodies count toward the view cap: ordinary `{...}` blocks must
    // still have their interpolation checked (`256 × {` + `${1+}` is a syntax error).
    let mut dollar_closes: Vec<usize> = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        while dollar_closes.last() == Some(&index) {
            dollar_closes.pop();
        }
        let opener_depth = dollar_closes.len();
        // Inside a string the lexer emits `${` as one T_DOLLAR_OPEN_CURLY_BRACES token; in
        // code it is a plain `$` followed by `{`, so the body starts after the brace.
        let (inner_start, first_inner, opener) = if token.id() == tok::T_DOLLAR_OPEN_CURLY_BRACES {
            (token.pos() + token.len(), index + 1, index)
        } else if token.id() == i64::from(b'$') {
            let Some(brace) = next_significant_index(tokens, index) else {
                continue;
            };
            if tokens[brace].id() != i64::from(b'{') {
                continue;
            }
            (
                tokens[brace].pos() + tokens[brace].len(),
                brace + 1,
                brace,
            )
        } else {
            continue;
        };
        let cursor = close_of[opener];
        if cursor == NO_BRACE {
            return Some(ValidationError {
                message: "syntax error, unexpected end of file, expecting \"}\"".into(),
                pos: source.len(),
                lexical: false,
            });
        }
        let cursor = cursor as usize;
        // Bound the nested-view construction on pathological chains; the outermost 256
        // bodies are still validated, which covers every real source.
        if opener_depth >= MAX_DOLLAR_BRACE_NESTING {
            continue;
        }
        dollar_closes.push(cursor);
        let close_pos = tokens[cursor].pos();
        let close_len = tokens[cursor].len();
        let inner = &source[inner_start..close_pos];
        if inner.iter().all(u8::is_ascii_whitespace) {
            return Some(unexpected_token_at(source, close_pos, close_len, None));
        }
        // Zend's lexer emits T_STRING_VARNAME only for the simple `${name}` and
        // `${name[expr]}` forms inside strings. Re-parsing that text as an expression would
        // turn a keyword name such as `class` into the keyword token, so validate the shape
        // directly and parse only the offset expression.
        if tokens[first_inner].id() == tok::T_STRING_VARNAME {
            let name = &tokens[first_inner];
            let Some(after_name) = next_significant_index(tokens, first_inner) else {
                continue;
            };
            if after_name == cursor {
                continue;
            }
            if tokens[after_name].id() != i64::from(b'[') {
                return Some(unexpected_token_at(source, name.pos(), name.len(), None));
            }
            let Some(close_bracket) = matching_bracket_before(tokens, after_name, cursor) else {
                return Some(ValidationError {
                    message: "Unclosed '[' does not match '}'".into(),
                    pos: tokens[after_name].pos(),
                    lexical: false,
                });
            };
            if let Some(error) = expression_slice_error(
                source,
                tokens,
                after_name + 1,
                close_bracket,
                tokens[close_bracket].pos(),
                names,
            ) {
                return Some(error);
            }
            let after_bracket = next_significant_index(tokens, close_bracket);
            if after_bracket != Some(cursor) {
                let (pos, len) = after_bracket
                    .map(|index| (tokens[index].pos(), tokens[index].len()))
                    .unwrap_or((close_pos, close_len));
                return Some(unexpected_token_at(source, pos, len, Some("\"}\"")));
            }
            continue;
        }
        if let Some(error) =
            expression_slice_error(source, tokens, first_inner, cursor, close_pos, names)
        {
            return Some(error);
        }
    }
    None
}

/// Validates the expression inside a computed static member name, `C::{expr}`.
///
/// PHP accepts any expression there (`C::{'FO' . 'O'}`, `C::{strtoupper('foo')}`), but the
/// parser view only needs a member name, so the body is validated here and then rewritten to
/// an identifier. Returning the body to the view would leave `C:: FO . O`, which is not a
/// member expression.
pub(crate) fn computed_member_expression_error<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
    names: &mut Vec<usize>,
) -> Option<ValidationError> {
    let close_of = brace_matches(tokens);
    let mut member_closes: Vec<usize> = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        while member_closes.last() == Some(&index) {
            member_closes.pop();
        }
        let opener_depth = member_closes.len();
        if token.id() != tok::T_DOUBLE_COLON
            && token.id() != tok::T_OBJECT_OPERATOR
            && token.id() != tok::T_NULLSAFE_OBJECT_OPERATOR
        {
            continue;
        }
        let Some(brace) = next_significant_index(tokens, index) else {
            continue;
        };
        if tokens[brace].id() != i64::from(b'{') {
            continue;
        }
        let close = close_of[brace];
        if close == NO_BRACE {
            return Some(ValidationError {
                message: "syntax error, unexpected end of file, expecting \"}\"".into(),
                pos: source.len(),
                lexical: false,
            });
        }
        let close = close as usize;
        if opener_depth >= MAX_COMPUTED_MEMBER_NESTING {
            continue;
        }
        member_closes.push(close);
        let inner_start = tokens[brace].pos() + tokens[brace].len();
        let inner = &source[inner_start..tokens[close].pos()];
        if inner.iter().all(u8::is_ascii_whitespace) {
            return Some(unexpected_token_at(
                source,
                tokens[close].pos(),
                tokens[close].len(),
                None,
            ));
        }
        if let Some(error) = expression_slice_error(
            source,
            tokens,
            brace + 1,
            close,
            tokens[close].pos(),
            names,
        ) {
            return Some(error);
        }
    }
    None
}

/// Rejects variable variables in declaration name slots before the shared lexer sees them.
///
/// PHP reports `function f(int $$a)` at the first `$` with `expecting variable`. The parser
/// view keeps that `$` so the parser can reject it, but the view lexer has no variable-name
/// table and would fail first with an internal message. This scan produces Zend's diagnostic
/// from the native tokens, where the declaration context is still visible.
pub(crate) fn declaration_variable_variable_error<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
) -> Option<ValidationError> {
    if !tokens.iter().any(|token| token.id() == i64::from(b'$')) {
        return None;
    }
    /// One open delimiter. `declaration_default` is `Some` only for a declaration parameter
    /// list; `previous_declaration` restores the enclosing declaration scope when it pops.
    struct Scope {
        declaration_default: Option<bool>,
        previous_declaration: Option<usize>,
    }
    let mut scopes: Vec<Scope> = Vec::new();
    let mut parens: Vec<usize> = Vec::new();
    let mut declaration_index: Option<usize> = None;
    let mut previous_significant = 0i64;
    for (index, token) in tokens.iter().enumerate() {
        let id = token.id();
        if id == i64::from(b'(') {
            let declaration = paren_is_declaration_parameters(tokens, index);
            let previous_declaration = declaration.then_some(declaration_index).flatten();
            scopes.push(Scope {
                declaration_default: declaration.then_some(false),
                previous_declaration,
            });
            if declaration {
                declaration_index = Some(scopes.len() - 1);
            }
            parens.push(index);
        } else if id == i64::from(b'[') || id == tok::T_ATTRIBUTE {
            scopes.push(Scope {
                declaration_default: None,
                previous_declaration: None,
            });
        } else if id == i64::from(b'{')
            || id == tok::T_CURLY_OPEN
            || id == tok::T_DOLLAR_OPEN_CURLY_BRACES
        {
            scopes.push(Scope {
                declaration_default: None,
                previous_declaration: None,
            });
        } else if id == i64::from(b')') || id == i64::from(b']') || id == i64::from(b'}') {
            if id == i64::from(b')') {
                parens.pop();
            }
            if let Some(scope) = scopes.pop() {
                if scope.declaration_default.is_some() {
                    declaration_index = scope.previous_declaration;
                }
            }
        } else if id == i64::from(b'=') || id == i64::from(b',') {
            // Only the top level of the declaration list starts a parameter or its default;
            // a comma inside an array/call belongs to the default expression.
            if declaration_index.is_some_and(|declaration| declaration + 1 == scopes.len()) {
                if let Some(scope) = declaration_index.and_then(|declaration| scopes.get_mut(declaration)) {
                    scope.declaration_default = Some(id == i64::from(b'='));
                }
            }
        }
        if id == i64::from(b'$') {
            let default_started = declaration_index
                .and_then(|declaration| scopes.get(declaration))
                .is_some_and(|scope| scope.declaration_default == Some(true));
            let starts_variable = next_significant_index(tokens, index).is_some_and(|next| {
                let next = tokens[next].id();
                next == i64::from(b'$')
                    || next == i64::from(b'{')
                    || next == tok::T_VARIABLE
                    || next == tok::T_DOLLAR_OPEN_CURLY_BRACES
            });
            if !default_started
                && starts_variable
                && dollar_starts_declaration(
                    tokens,
                    index,
                    previous_significant,
                    parens.last().copied(),
                )
            {
                return Some(unexpected_token_at(
                    source,
                    token.pos(),
                    token.len(),
                    Some("variable"),
                ));
            }
        }
        if !is_trivia(id) && id != 0 {
            previous_significant = id;
        }
    }
    None
}

/// Validates the first token of every `{$...}` interpolation before the parser view copies it.
///
/// The shared lexer rejects `{$ b}` (and siblings) with an internal "Expected variable name"
/// message and a synthetic fragment position, so Zend's first-token diagnostic is built from the
/// native token stream here, where the real source position is known.
pub(crate) fn complex_interpolation_error<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
) -> Option<ValidationError> {
    if !tokens.iter().any(|token| token.id() == tok::T_CURLY_OPEN) {
        return None;
    }
    let close_of = brace_matches(tokens);
    let nearest = enclosing_delimiters(tokens);
    for (index, token) in tokens.iter().enumerate() {
        if token.id() != tok::T_CURLY_OPEN {
            continue;
        }
        let first = next_significant_index(tokens, index);
        let close = close_of[index];
        if close == NO_BRACE {
            if let Some(error) = unterminated_interpolation_error(source, tokens, index, &nearest) {
                return Some(error);
            }
            continue;
        }
        let close = close as usize;
        let Some(first) = first else { continue; };
        let expectation = "variable or \"{\" or \"$\"";
        if first == close {
            return Some(unexpected_token_at(
                source,
                tokens[close].pos(),
                tokens[close].len(),
                Some(expectation),
            ));
        }
        let mut cursor = first;
        while tokens[cursor].id() == i64::from(b'$') {
            let Some(next) = next_significant_index(tokens, cursor) else { break; };
            if next >= close || tokens[next].id() != i64::from(b'$') {
                break;
            }
            cursor = next;
        }
        if tokens[cursor].id() == i64::from(b'$') {
            let next = next_significant_index(tokens, cursor);
            let valid = next.is_some_and(|next| {
                next != close
                    && matches!(
                        tokens[next].id(),
                        id if id == tok::T_VARIABLE
                            || id == i64::from(b'{')
                            || id == tok::T_DOLLAR_OPEN_CURLY_BRACES
                    )
            });
            if !valid {
                return Some(match next {
                    Some(next) if next != close => token_noun_error(
                        source,
                        tokens,
                        next,
                        expectation,
                    ),
                    _ => unexpected_token_at(
                        source,
                        tokens[close].pos(),
                        tokens[close].len(),
                        Some(expectation),
                    ),
                });
            }
        } else if !matches!(
            tokens[cursor].id(),
            id if id == tok::T_VARIABLE
                || id == i64::from(b'{')
                || id == tok::T_DOLLAR_OPEN_CURLY_BRACES
        ) {
            return Some(token_noun_error(source, tokens, cursor, expectation));
        }
        let open = nearest[close];
        if open != u32::MAX && open as usize >= first {
            let text = if tokens[open as usize].id() == i64::from(b'(') { "(" } else { "[" };
            return Some(ValidationError {
                message: format!("Unclosed '{text}' does not match '}}'"),
                pos: tokens[open as usize].pos(),
                lexical: false,
            });
        }
    }
    None
}

/// Reports the opening delimiter left unmatched before the closing quote of an interpolation.
pub(crate) fn unterminated_interpolation_error<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
    open: usize,
    nearest: &[u32],
) -> Option<ValidationError> {
    let delimiter = tokens.iter().enumerate().skip(open + 1).find_map(|(index, token)| {
        let id = token.id();
        (id == i64::from(b'"') || id == i64::from(b'`') || id == tok::T_END_HEREDOC)
            .then_some(index)
    })?;
    let pair = nearest[delimiter];
    if pair != u32::MAX && pair as usize >= open + 1 {
        let text = if tokens[pair as usize].id() == i64::from(b'(') { "(" } else { "[" };
        return Some(ValidationError {
            message: format!("Unclosed '{text}'"),
            pos: tokens[pair as usize].pos(),
            lexical: false,
        });
    }
    let last = previous_significant_index(tokens, delimiter)?;
    if matches!(
        tokens[last].id(),
        id if id == tok::T_OBJECT_OPERATOR || id == tok::T_NULLSAFE_OBJECT_OPERATOR
    ) {
        return Some(delimiter_error(
            source,
            tokens,
            delimiter,
            "identifier or variable or \"{\" or \"$\"",
        ));
    }
    None
}

/// Rejects a simple `"$a[...]"` offset that the closing quote interrupts.
pub(crate) fn simple_interpolation_offset_error<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
) -> Option<ValidationError> {
    for (index, token) in tokens.iter().enumerate() {
        if token.id() != tok::T_VARIABLE {
            continue;
        }
        let previous = previous_significant_index(tokens, index);
        let in_string = previous.is_some_and(|previous| {
            matches!(
                tokens[previous].id(),
                id if id == i64::from(b'"')
                    || id == i64::from(b'`')
                    || id == tok::T_ENCAPSED_AND_WHITESPACE
                    || id == tok::T_START_HEREDOC
            )
        });
        if !in_string {
            continue;
        }
        let Some(open) = next_significant_index(tokens, index) else { continue; };
        if tokens[open].id() != i64::from(b'[') {
            continue;
        }
        // Stop at the first `]` (the simple offset cannot nest) or at the closing string
        // delimiter. Scanning to the delimiter first made a long string of `$a[0]` entries
        // quadratic.
        let mut close = None;
        let mut delimiter = None;
        for (index, token) in tokens.iter().enumerate().skip(open + 1) {
            let id = token.id();
            if id == i64::from(b']') {
                close = Some(index);
                break;
            }
            if id == i64::from(b'"') || id == i64::from(b'`') || id == tok::T_END_HEREDOC {
                delimiter = Some(index);
                break;
            }
        }
        if let Some(close) = close {
            // The offset is closed, but the simple syntax accepts exactly one key: a number,
            // a bareword, a variable, or a negated number.
            let Some(mut key) = next_significant_index(tokens, open) else { continue; };
            if key >= close {
                continue;
            }
            if tokens[key].id() == i64::from(b'-') {
                let Some(next) = next_significant_index(tokens, key) else { continue; };
                key = next;
            }
            let is_key = matches!(
                tokens[key].id(),
                id if id == tok::T_NUM_STRING
                    || id == tok::T_LNUMBER
                    || id == tok::T_DNUMBER
                    || id == tok::T_STRING
                    || id == tok::T_IDENTIFIER
                    || id == tok::T_VARIABLE
            );
            if !is_key {
                return Some(token_noun_error(source, tokens, key, "\"]\""));
            }
            if let Some(after) = next_significant_index(tokens, key) {
                if after != close {
                    return Some(token_noun_error(source, tokens, after, "\"]\""));
                }
            }
            continue;
        }
        let Some(delimiter) = delimiter else { continue; };
        let Some(first) = next_significant_index(tokens, open) else { continue; };
        if first >= delimiter {
            return Some(delimiter_error(
                source,
                tokens,
                delimiter,
                "\"-\" or identifier or variable or number",
            ));
        }
        let (after_key, negated) = if tokens[first].id() == i64::from(b'-') {
            match next_significant_index(tokens, first) {
                Some(next) if next < delimiter => (Some(next), true),
                _ => (None, true),
            }
        } else {
            (Some(first), false)
        };
        let Some(after_key) = after_key else {
            return Some(delimiter_error(source, tokens, delimiter, "number"));
        };
        let is_key = matches!(
            tokens[after_key].id(),
            id if id == tok::T_NUM_STRING
                || id == tok::T_LNUMBER
                || id == tok::T_DNUMBER
                || id == tok::T_STRING
                || id == tok::T_IDENTIFIER
                || id == tok::T_VARIABLE
        );
        if !is_key {
            return Some(token_noun_error(source, tokens, after_key, "\"]\""));
        }
        let after = next_significant_index(tokens, after_key);
        return Some(match after {
            Some(after) if after >= delimiter => {
                delimiter_error(source, tokens, delimiter, "\"]\"")
            }
            Some(after) => token_noun_error(source, tokens, after, "\"]\""),
            None if negated => delimiter_error(source, tokens, delimiter, "number"),
            None => delimiter_error(source, tokens, delimiter, "\"]\""),
        });
    }
    None
}

/// Precomputes the nearest open `(` or `[` before every token in one forward pass.
pub(crate) fn enclosing_delimiters<T: SourceToken>(tokens: &[T]) -> Vec<u32> {
    let mut stack: Vec<(usize, i64)> = Vec::new();
    let mut nearest = vec![u32::MAX; tokens.len()];
    for (index, token) in tokens.iter().enumerate() {
        nearest[index] = stack.last().map_or(u32::MAX, |(open, _)| *open as u32);
        let id = token.id();
        if id == i64::from(b'(') || id == i64::from(b'[') {
            stack.push((index, id));
        } else if id == i64::from(b')') || id == i64::from(b']') {
            let expected = if id == i64::from(b')') { i64::from(b'(') } else { i64::from(b'[') };
            if stack.last().is_some_and(|(_, open)| *open == expected) {
                stack.pop();
            }
        }
    }
    nearest
}

/// Deepest nested `${...}` body the validator rebuilds before skipping the inner view.
pub(crate) const MAX_DOLLAR_BRACE_NESTING: usize = 256;

/// Deepest nested computed-member body the validator rebuilds before skipping the inner view.
pub(crate) const MAX_COMPUTED_MEMBER_NESTING: usize = 256;

/// Sentinel for a `{`-style opener with no matching `}`.
pub(crate) const NO_BRACE: u32 = u32::MAX;

/// Precomputes the matching `}` for every `{`-style opener in one pass.
pub(crate) fn brace_matches<T: SourceToken>(tokens: &[T]) -> Vec<u32> {
    let mut stack: Vec<usize> = Vec::new();
    let mut close = vec![NO_BRACE; tokens.len()];
    for (index, token) in tokens.iter().enumerate() {
        let id = token.id();
        if id == i64::from(b'{')
            || id == tok::T_CURLY_OPEN
            || id == tok::T_DOLLAR_OPEN_CURLY_BRACES
        {
            stack.push(index);
        } else if id == i64::from(b'}') {
            if let Some(open) = stack.pop() {
                close[open] = u32::try_from(index).unwrap_or(NO_BRACE);
            }
        }
    }
    close
}

/// Finds the `}` matching an interpolation or computed-member body, counting nested scopes.
///
/// An interpolated string inside the body opens its own `{$...}`/`${...}` scope, which the
/// lexer emits as `T_CURLY_OPEN` / `T_DOLLAR_OPEN_CURLY_BRACES`, so those count as openers too.
pub(crate) fn matching_brace_end<T: SourceToken>(
    tokens: &[T],
    first_inner: usize,
    limit: usize,
) -> Option<usize> {
    let mut depth = 1usize;
    for index in first_inner..limit {
        let id = tokens[index].id();
        if id == i64::from(b'{')
            || id == tok::T_CURLY_OPEN
            || id == tok::T_DOLLAR_OPEN_CURLY_BRACES
        {
            depth += 1;
        } else if id == i64::from(b'}') {
            depth -= 1;
            if depth == 0 {
                return Some(index);
            }
        }
    }
    None
}

/// Validates that the tokens in `[first, end)` form exactly one expression.
///
/// The slice is normalized exactly like the enclosing parser view, so PHP-only variable
/// variables (`${$$a}`, nested `${...}`) stay valid while real syntax errors survive. The view
/// maps every byte back to its real source position; `fallback_pos` is where end-of-input
/// diagnostics land.
pub(crate) fn expression_slice_error<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
    first: usize,
    end: usize,
    fallback_pos: usize,
    names: &mut Vec<usize>,
) -> Option<ValidationError> {
    let floor = tokens
        .get(first)
        .map_or(fallback_pos, |token| token.pos())
        .min(fallback_pos);
    let (view, map, reference_error) =
        parser_view(source, &tokens[first..end], fallback_pos, &OnceCell::new());
    let lines = line_starts(&view);
    let map_position = |view_pos: usize| {
        map.get(view_pos)
            .copied()
            .map(|pos| pos as usize)
            .unwrap_or(fallback_pos)
            .clamp(floor, fallback_pos)
    };
    let view_tokens = match crate::syntax::lexer::tokenize(&view) {
        Ok(view_tokens) => view_tokens,
        Err(error) => {
            let view_pos = view_offset(&view, &lines, error.span.line, error.span.col);
            let error = token_error_at(source, tokens, map_position(view_pos));
            return prefer_reference_error(Some(error), reference_error);
        }
    };
    let mut position = usize::from(
        view_tokens
            .first()
            .is_some_and(|(token, _)| *token == crate::syntax::lexer::Token::OpenTag),
    );
    let feedback = crate::syntax::parser::tokenizer_feedback::FeedbackGuard::start();
    let parsed = crate::syntax::parser::expr::parse_expr(&view_tokens, &mut position);
    let recorded = feedback.names();
    drop(feedback);
    let positions: Vec<(u32, u32)> = recorded
        .iter()
        .map(|(span, _)| (span.line, span.col))
        .collect();
    for offset in view_offsets(&view, &lines, &positions) {
        names.push(map_position(offset));
    }
    // Zend reads exactly one expression, so a leftover token is the syntax error: `${1; 2}`
    // reports the `;` and `${1 2}` reports the `2`.
    let error = match parsed {
        Ok(_) => match view_tokens.get(position) {
            Some((crate::syntax::lexer::Token::Eof, _)) | None => {
                return prefer_reference_error(None, reference_error);
            }
            Some((_, metadata)) => metadata.span,
        },
        Err(error) => error.span,
    };
    let view_pos = view_offset(&view, &lines, error.line, error.col);
    let error = token_error_at(source, tokens, map_position(view_pos));
    prefer_reference_error(Some(error), reference_error)
}

/// Finds the `)` matching the `(` at `open`, counting nested parentheses.
pub(crate) fn matching_paren_end<T: SourceToken>(tokens: &[T], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for index in open..tokens.len() {
        let id = tokens[index].id();
        if id == i64::from(b'(') {
            depth += 1;
        } else if id == i64::from(b')') {
            depth -= 1;
            if depth == 0 {
                return Some(index);
            }
        }
    }
    None
}

/// Finds the `]` matching the `[` at `open`, scanning no further than `limit`.
pub(crate) fn matching_bracket_before<T: SourceToken>(tokens: &[T], open: usize, limit: usize) -> Option<usize> {
    let mut depth = 0usize;
    for index in open..limit {
        let id = tokens[index].id();
        if id == i64::from(b'[') {
            depth += 1;
        } else if id == i64::from(b']') {
            depth -= 1;
            if depth == 0 {
                return Some(index);
            }
        }
    }
    None
}

/// Rejects an `else`/`elseif` that follows a close tag outside a braced or alternative body.
///
/// PHP ends the whole `if` statement at `?>`, so `if (1) ?>x<?php else ...` is a parse error,
/// while `if (1): ?>x<?php else:` and `if (1) { ?>x<?php } else` are valid.
#[inline(never)]
pub(crate) fn close_tag_else_error<T: SourceToken>(source: &[u8], tokens: &[T]) -> Option<ValidationError> {
    // One entry per open alternative-syntax construct: true for `if`, false for the others.
    let mut alternatives: Vec<bool> = Vec::new();
    for index in 0..tokens.len() {
        let id = tokens[index].id();
        let control_keyword = matches!(
            id,
            id if id == tok::T_IF
                || id == tok::T_WHILE
                || id == tok::T_FOR
                || id == tok::T_FOREACH
                || id == tok::T_SWITCH
                || id == tok::T_DECLARE
                || id == tok::T_ENDIF
                || id == tok::T_ENDWHILE
                || id == tok::T_ENDFOR
                || id == tok::T_ENDFOREACH
                || id == tok::T_ENDSWITCH
                || id == tok::T_ENDDECLARE
        );
        if control_keyword && control_keyword_is_name(tokens, index) {
            continue;
        }
        if id == tok::T_IF {
            if alternative_header_colon(tokens, index) {
                alternatives.push(true);
            }
            continue;
        }
        if matches!(
            id,
            id if id == tok::T_WHILE
                || id == tok::T_FOR
                || id == tok::T_FOREACH
                || id == tok::T_SWITCH
                || id == tok::T_DECLARE
        ) {
            if alternative_header_colon(tokens, index) {
                alternatives.push(false);
            }
            continue;
        }
        if matches!(
            id,
            id if id == tok::T_ENDIF
                || id == tok::T_ENDWHILE
                || id == tok::T_ENDFOR
                || id == tok::T_ENDFOREACH
                || id == tok::T_ENDSWITCH
                || id == tok::T_ENDDECLARE
        ) {
            alternatives.pop();
            continue;
        }
        if id != tok::T_CLOSE_TAG {
            continue;
        }
        let Some(next) = next_non_inline_html_index(tokens, index) else { continue; };
        let next_id = tokens[next].id();
        if next_id != tok::T_ELSE && next_id != tok::T_ELSEIF {
            continue;
        }
        if !alternatives.iter().any(|is_if| *is_if)
            || close_tag_after_empty_control_header(tokens, index)
        {
            return Some(unexpected_token_at(
                source,
                tokens[next].pos(),
                tokens[next].len(),
                Some("end of file"),
            ));
        }
        // A normal `else`/`elseif` cannot follow a close tag: the ordinary `if` it would bind
        // to ended at the tag. Only the alternative forms (`else:`, `elseif (...):`) are valid.
        if next_id == tok::T_ELSE {
            let after = next_significant_index(tokens, next).unwrap_or(next);
            if tokens[after].id() != i64::from(b':') {
                return Some(unexpected_token_at(
                    source,
                    tokens[after].pos(),
                    tokens[after].len(),
                    Some("\":\""),
                ));
            }
        } else if !alternative_header_colon(tokens, next) {
            return Some(unexpected_token_at(
                source,
                tokens[next].pos(),
                tokens[next].len(),
                Some("\":\""),
            ));
        }
    }
    None
}

/// Returns whether an alternative-syntax header (`if (...)`, `while (...)`, ...) ends in `:`.
pub(crate) fn alternative_header_colon<T: SourceToken>(tokens: &[T], keyword: usize) -> bool {
    let Some(open) = next_significant_index(tokens, keyword)
        .filter(|&index| tokens[index].id() == i64::from(b'('))
    else {
        return false;
    };
    let mut depth = 0usize;
    let mut cursor = open;
    while cursor < tokens.len() {
        let id = tokens[cursor].id();
        if id == i64::from(b'(') {
            depth += 1;
        } else if id == i64::from(b')') {
            depth -= 1;
            if depth == 0 {
                break;
            }
        }
        cursor += 1;
    }
    next_significant_index(tokens, cursor).is_some_and(|next| tokens[next].id() == i64::from(b':'))
}

/// Returns whether a control keyword is actually a member or declaration name.
pub(crate) fn control_keyword_is_name<T: SourceToken>(tokens: &[T], index: usize) -> bool {
    previous_significant_index(tokens, index).is_some_and(|previous| {
        matches!(
            tokens[previous].id(),
            id if id == tok::T_DOUBLE_COLON
                || id == tok::T_OBJECT_OPERATOR
                || id == tok::T_NULLSAFE_OBJECT_OPERATOR
                || id == tok::T_NS_SEPARATOR
                || id == i64::from(b'\\')
                || id == tok::T_FUNCTION
                || id == tok::T_CONST
                || id == tok::T_CASE
        )
    })
}

/// Returns whether the close tag immediately follows a control header that has no body yet.
///
/// `if ($b) ?>...<?php else` cannot attach the `else` to that ordinary `if`: PHP ends the
/// unbraced statement at the close tag. An outer alternative `if` must not rescue it.
pub(crate) fn close_tag_after_empty_control_header<T: SourceToken>(tokens: &[T], close: usize) -> bool {
    let Some(previous) = previous_significant_index(tokens, close) else {
        return false;
    };
    if tokens[previous].id() != i64::from(b')') {
        return false;
    }
    let mut depth = 1usize;
    let mut cursor = previous;
    while cursor > 0 {
        cursor -= 1;
        let id = tokens[cursor].id();
        if id == i64::from(b')') {
            depth += 1;
        } else if id == i64::from(b'(') {
            depth -= 1;
            if depth == 0 {
                break;
            }
        }
    }
    if tokens.get(cursor).map(|token| token.id()) != Some(i64::from(b'(')) {
        return false;
    }
    // Only an `if`/`elseif` header can own a following `else`; loops and `declare` accept the
    // close tag as their empty body and leave the `else` to the enclosing alternative `if`.
    previous_significant_index(tokens, cursor)
        .is_some_and(|before| matches!(tokens[before].id(), id if id == tok::T_IF || id == tok::T_ELSEIF))
}

/// Returns the next token after `index`, skipping inline HTML, open tags and trivia.
#[inline(never)]
pub(crate) fn next_non_inline_html_index<T: SourceToken>(tokens: &[T], index: usize) -> Option<usize> {
    let mut cursor = index + 1;
    while cursor < tokens.len() {
        let id = tokens[cursor].id();
        if id == tok::T_INLINE_HTML || id == tok::T_OPEN_TAG || is_trivia_token(id) {
            cursor += 1;
            continue;
        }
        return Some(cursor);
    }
    None
}

/// One open scope that changes how `__halt_compiler()` is diagnosed.
enum HaltScope {
    /// A function, method, control, hook or bare block body.
    Statement,
    /// A class-like body, trait adaptation block or property-hook container.
    ClassLike,
    /// A braced namespace. `line` is the one-based line of its `{`.
    Namespace { line: usize },
    /// `{$...}` or `${...}` interpolation. Its `}` must not close a real block.
    Interpolation,
    /// `if/while/for/foreach/switch/declare (...):` until the matching `end*`.
    Alternative,
}

/// Returns whether `{` opens a trait adaptation block (`use Trait {`, `use A, B {`).
///
/// A closure's `use ($a) {` stops at the parameter list and stays a statement body.
fn brace_is_trait_use<T: SourceToken>(tokens: &[T], open: usize) -> bool {
    let mut cursor = open;
    let mut saw_name = false;
    while let Some(previous) = previous_significant_index(tokens, cursor) {
        let id = tokens[previous].id();
        if id == tok::T_USE {
            return saw_name;
        }
        if id == tok::T_STRING
            || id == tok::T_NAME_FULLY_QUALIFIED
            || id == tok::T_NAME_QUALIFIED
            || id == tok::T_NAME_RELATIVE
            || id == tok::T_NS_SEPARATOR
            || id == i64::from(b'\\')
        {
            saw_name = true;
            cursor = previous;
            continue;
        }
        if id == i64::from(b',') {
            cursor = previous;
            continue;
        }
        return false;
    }
    false
}

/// Classifies a real `{` as a statement block, a class-like body or a namespace body.
///
/// The namespace line is filled by the caller from the brace token. A property-hook container
/// (`public $x {`) is not a statement list; the `get {` body inside it is.
fn halt_brace_kind<T: SourceToken>(tokens: &[T], open: usize) -> HaltScope {
    if brace_is_trait_use(tokens, open)
        || previous_significant_index(tokens, open)
            .is_some_and(|previous| tokens[previous].id() == tok::T_VARIABLE)
    {
        return HaltScope::ClassLike;
    }
    let mut cursor = open;
    while let Some(previous) = previous_significant_index(tokens, cursor) {
        let id = tokens[previous].id();
        if id == tok::T_NAMESPACE {
            return HaltScope::Namespace { line: 0 };
        }
        if id == tok::T_CLASS || id == tok::T_INTERFACE || id == tok::T_TRAIT || id == tok::T_ENUM {
            return HaltScope::ClassLike;
        }
        if id == tok::T_FUNCTION
            || id == tok::T_FN
            || id == i64::from(b';')
            || id == i64::from(b'{')
            || id == i64::from(b'}')
        {
            return HaltScope::Statement;
        }
        cursor = previous;
    }
    HaltScope::Statement
}

/// Returns whether `index` opens or closes one alternative-syntax construct.
///
/// `1` pushes a scope, `-1` pops the nearest one, and `0` leaves the stack alone. `else` and
/// `elseif` stay inside the `if` they continue, so only the outer header is pushed.
fn alternative_scope_delta<T: SourceToken>(tokens: &[T], index: usize) -> i8 {
    let id = tokens[index].id();
    let header = id == tok::T_IF
        || id == tok::T_WHILE
        || id == tok::T_FOR
        || id == tok::T_FOREACH
        || id == tok::T_SWITCH
        || id == tok::T_DECLARE;
    let end = id == tok::T_ENDIF
        || id == tok::T_ENDWHILE
        || id == tok::T_ENDFOR
        || id == tok::T_ENDFOREACH
        || id == tok::T_ENDSWITCH
        || id == tok::T_ENDDECLARE;
    if (!header && !end) || control_keyword_is_name(tokens, index) {
        return 0;
    }
    if header && alternative_header_colon(tokens, index) {
        return 1;
    }
    if end {
        return -1;
    }
    0
}

/// Returns whether `__halt_compiler` sits where PHP's grammar accepts a statement.
///
/// A ternary colon is an expression boundary, so `$a ? 1 : __halt_compiler()` stays a parse
/// error even inside a function. A case, label or alternative-syntax colon starts a statement.
fn halt_is_statement_start<T: SourceToken>(tokens: &[T], index: usize) -> bool {
    let Some(previous) = previous_significant_index(tokens, index) else {
        return true;
    };
    let id = tokens[previous].id();
    if id == i64::from(b';')
        || id == i64::from(b'{')
        || id == i64::from(b'}')
        || id == tok::T_OPEN_TAG
    {
        return true;
    }
    id == i64::from(b':') && !ternary_colon_before_label(tokens, index)
}

/// Returns whether the halt word is a member or namespace name rather than the construct.
fn halt_used_as_name<T: SourceToken>(tokens: &[T], index: usize) -> bool {
    previous_significant_index(tokens, index).is_some_and(|previous| {
        matches!(
            tokens[previous].id(),
            token_id if token_id == tok::T_OBJECT_OPERATOR
                || token_id == tok::T_NULLSAFE_OBJECT_OPERATOR
                || token_id == i64::from(b'\\')
                || token_id == tok::T_NS_SEPARATOR
        )
    })
}

/// Builds PHP's `Unclosed '{'` diagnostic for a halt that ends a braced namespace early.
fn halt_namespace_unclosed(source: &[u8], pos: usize, brace_line: usize) -> ValidationError {
    let halt_line = 1 + newline_count(&source[..pos.min(source.len())]);
    let message = if halt_line == brace_line {
        "Unclosed '{'".to_string()
    } else {
        format!("Unclosed '{{' on line {brace_line}")
    };
    ValidationError { message, pos, lexical: false }
}

/// Rejects `__halt_compiler()` outside the outermost file scope.
///
/// A statement inside a function, block or alternative-syntax body is PHP's `CompileError`.
/// A braced namespace halts before its `}` and reports `Unclosed '{'`. A class body, an
/// unbraced `if`, a ternary operand and a property initializer stay ordinary parse errors.
#[inline(never)]
pub(crate) fn halt_compiler_scope_error<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
) -> Option<ValidationError> {
    if !tokens.iter().any(|token| token.id() == tok::T_HALT_COMPILER) {
        return None;
    }
    let mut scopes: Vec<HaltScope> = Vec::new();
    for index in 0..tokens.len() {
        let id = tokens[index].id();
        if id == i64::from(b'{') {
            let mut scope = halt_brace_kind(tokens, index);
            if let HaltScope::Namespace { line } = &mut scope {
                *line = 1 + newline_count(&source[..tokens[index].pos().min(source.len())]);
            }
            scopes.push(scope);
            continue;
        }
        if id == tok::T_CURLY_OPEN || id == tok::T_DOLLAR_OPEN_CURLY_BRACES {
            scopes.push(HaltScope::Interpolation);
            continue;
        }
        if id == i64::from(b'}') {
            scopes.pop();
            continue;
        }
        match alternative_scope_delta(tokens, index) {
            1 => {
                scopes.push(HaltScope::Alternative);
                continue;
            }
            -1 => {
                if let Some(position) = scopes
                    .iter()
                    .rposition(|scope| matches!(scope, HaltScope::Alternative))
                {
                    scopes.remove(position);
                }
                continue;
            }
            _ => {}
        }
        if id != tok::T_HALT_COMPILER || halt_used_as_name(tokens, index) {
            continue;
        }
        let pos = tokens[index].pos();
        let statement = halt_is_statement_start(tokens, index);
        let in_statement = scopes.iter().any(|scope| {
            matches!(scope, HaltScope::Statement | HaltScope::Alternative)
        });
        if statement && in_statement {
            return Some(ValidationError {
                message: HALT_COMPILER_OUTERMOST_SCOPE_MESSAGE.into(),
                pos,
                lexical: false,
            });
        }
        if statement && !scopes.iter().any(|scope| matches!(scope, HaltScope::ClassLike)) {
            if let Some(line) = scopes.iter().rev().find_map(|scope| match scope {
                HaltScope::Namespace { line } => Some(*line),
                _ => None,
            }) {
                return Some(halt_namespace_unclosed(source, pos, line));
            }
        }
        let blocking = scopes.iter().any(|scope| !matches!(scope, HaltScope::Interpolation));
        if statement && !blocking {
            continue;
        }
        return Some(token_at_index_error(source, tokens, index));
    }
    None
}

/// Rejects `list` where PHP's grammar reserves it, while keeping member/declaration uses.
///
/// elephc's lexer keeps `list` as an identifier, so the parser accepts it in expression and
/// declaration positions where reference PHP reports a ParseError. The allowed contexts below
/// are exactly the semi-reserved ones: member names, method/class-constant names, enum cases,
/// named arguments, trait adaptations, and the `list(...)` construct.
pub(crate) fn reserved_list_error<T: SourceToken>(source: &[u8], tokens: &[T]) -> Option<ValidationError> {
    let list = tok::T_LIST;
    if !tokens.iter().any(|token| token.id() == list) {
        return None;
    }
    let enclosing = enclosing_braces(tokens);
    for (index, token) in tokens.iter().enumerate() {
        if token.id() != list {
            continue;
        }
        let previous = previous_significant_index(tokens, index);
        let next = next_significant_index(tokens, index);
        let previous_id = previous.map_or(0, |index| tokens[index].id());
        let next_id = next.map_or(0, |index| tokens[index].id());
        let in_class = enclosing[index] != u32::MAX
            && enclosing_brace_is_class(tokens, enclosing[index] as usize);
        let allowed = if next_id == i64::from(b'(') {
            (previous_id != tok::T_FUNCTION || in_class) && previous_id != tok::T_NEW
        } else if previous_id == tok::T_FUNCTION {
            in_class
        } else if previous_id == tok::T_CONST {
            in_class
        } else if previous_id == tok::T_CASE {
            in_class
        } else if next_id == i64::from(b':') {
            true
        } else if [
            tok::T_OBJECT_OPERATOR,
            tok::T_NULLSAFE_OBJECT_OPERATOR,
            tok::T_DOUBLE_COLON,
        ]
        .contains(&previous_id)
        {
            true
        } else {
            next_id == tok::T_AS
        };
        if !allowed {
            return Some(token_at_index_error(source, tokens, index));
        }
    }
    None
}

/// Rejects a `(void)` cast outside a statement position, with PHP's own wording.
pub(crate) fn void_cast_expression_error<T: SourceToken>(tokens: &[T]) -> Option<ValidationError> {
    for index in 0..tokens.len() {
        if tokens[index].id() != tok::T_VOID_CAST {
            continue;
        }
        let previous = previous_significant_index(tokens, index)
            .map_or(0, |before| tokens[before].id());
        if void_cast_starts_statement(tokens, index, previous) {
            continue;
        }
        let expecting = if previous == tok::T_RETURN { ", expecting \";\"" } else { "" };
        return Some(ValidationError {
            message: format!("syntax error, unexpected token \"(void)\"{expecting}"),
            pos: tokens[index].pos(),
            lexical: false,
        });
    }
    None
}

/// Rejects inline HTML between a `switch` opener and its first case label.
///
/// The parser view blanks inline HTML, so `switch ($a): ?>\n    <?php case 1:` would otherwise
/// look like a close tag directly before `case`. PHP reports the HTML token itself.
pub(crate) fn switch_case_html_error<T: SourceToken>(source: &[u8], tokens: &[T]) -> Option<ValidationError> {
    for (index, token) in tokens.iter().enumerate() {
        if token.id() != tok::T_SWITCH {
            continue;
        }
        let Some(open) = next_significant_index(tokens, index)
            .filter(|&next| tokens[next].id() == i64::from(b'('))
        else {
            continue;
        };
        let mut depth = 0usize;
        let mut cursor = open;
        while cursor < tokens.len() {
            match tokens[cursor].id() {
                id if id == i64::from(b'(') => depth += 1,
                id if id == i64::from(b')') => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            cursor += 1;
        }
        let Some(opener) = next_significant_index(tokens, cursor) else {
            continue;
        };
        let alternative = tokens[opener].id() == i64::from(b':');
        if !alternative && tokens[opener].id() != i64::from(b'{') {
            continue;
        }
        let Some(close) = next_significant_index(tokens, opener) else {
            continue;
        };
        if tokens[close].id() != tok::T_CLOSE_TAG {
            continue;
        }
        let Some(html) = next_significant_index(tokens, close) else {
            continue;
        };
        if tokens[html].id() != tok::T_INLINE_HTML {
            continue;
        }
        let text = &source[tokens[html].pos()..tokens[html].pos() + tokens[html].len()];
        let expecting = if alternative {
            "\"endswitch\" or \"case\" or \"default\""
        } else {
            "\"case\" or \"default\" or \"}\""
        };
        return Some(ValidationError {
            message: format!(
                "syntax error, unexpected T_INLINE_HTML \"{}\", expecting {expecting}",
                String::from_utf8_lossy(text).replace('\n', " ").replace('\r', "")
            ),
            pos: tokens[html].pos(),
            lexical: false,
        });
    }
    None
}
