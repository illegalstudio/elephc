//! Purpose:
//! Maps parser and lexer failures to Zend's PHP 8.5 diagnostic text, positions and expectations.
//!
//! Called from:
//! - `crate::syntax::tokenizer_validation::validate()`, the prechecks, and the parser-view lexer path.
//!
//! Key details:
//! - Overrides are keyed on the shared parser's raw messages, never on generated view text.

use super::*;

/// Returns `Unclosed '<delimiter>'` when the parser failed while an opener was still open.
///
/// Zend reports the innermost unmatched `(`/`[`/`{` (and `#[` as `[`) instead of a generic
/// end-of-file or token error. The override only applies when the failure is at EOF or the
/// opener is followed by EOF inside a construct that can legally continue, so a genuinely
/// invalid token inside the opener keeps its own diagnostic.
#[inline(never)]
pub(crate) fn unclosed_delimiter_error<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
    mapped_pos: usize,
    parser_message: &str,
) -> Option<ValidationError> {
    let (index, delimiter) = unmatched_open_delimiter(tokens)?;
    let error_at_eof = mapped_pos >= source.len()
        || parser_message.contains("Eof")
        || parser_message.contains("Unexpected end")
        || parser_message.contains("end of input");
    let next_is_eof = next_significant_index(tokens, index).is_none();
    let continuation_message = [
        "Expected '}'",
        "Expected ']'",
        "Expected ')'",
        "class body",
        "interface body",
        "trait body",
        "enum body",
        "at end of",
    ]
    .iter()
    .any(|fragment| parser_message.contains(fragment));
    if !(error_at_eof
        || (open_ended_construct(tokens, index) && (next_is_eof || continuation_message)))
    {
        return None;
    }
    Some(ValidationError {
        message: format!("Unclosed '{}'", delimiter),
        pos: tokens[index].pos(),
        lexical: false,
    })
}

/// Returns the innermost unmatched opener and its closing delimiter spelling.
#[inline(never)]
pub(crate) fn unmatched_open_delimiter<T: SourceToken>(tokens: &[T]) -> Option<(usize, char)> {
    let mut stack: Vec<(usize, char)> = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        let token_id = token.id();
        if token_id == i64::from(b'(') {
            stack.push((index, '('));
        } else if token_id == i64::from(b'[') || token_id == tok::T_ATTRIBUTE {
            stack.push((index, '['));
        } else if token_id == i64::from(b'{')
            || token_id == tok::T_CURLY_OPEN
            || token_id == tok::T_DOLLAR_OPEN_CURLY_BRACES
        {
            stack.push((index, '{'));
        } else if token_id == i64::from(b')')
            && stack.last().is_some_and(|(_, open)| *open == '(')
        {
            stack.pop();
        } else if token_id == i64::from(b']') && stack.last().is_some_and(|(_, open)| *open == '[')
        {
            stack.pop();
        } else if token_id == i64::from(b'}') && stack.last().is_some_and(|(_, open)| *open == '{')
        {
            stack.pop();
        }
    }
    stack.last().copied()
}

/// Returns whether the tokens before `index` open a construct that may still continue at EOF.
#[inline(never)]
pub(crate) fn open_ended_construct<T: SourceToken>(tokens: &[T], index: usize) -> bool {
    if tokens.get(index).is_some_and(|token| token.id() == tok::T_ATTRIBUTE) {
        return true;
    }
    let mut cursor = index;
    let mut inspected = 0usize;
    while let Some(previous) = previous_significant_index(tokens, cursor) {
        let token_id = tokens[previous].id();
        if [
            tok::T_FUNCTION,
            tok::T_FN,
            tok::T_IF,
            tok::T_ELSEIF,
            tok::T_WHILE,
            tok::T_FOR,
            tok::T_FOREACH,
            tok::T_SWITCH,
            tok::T_MATCH,
            tok::T_TRY,
            tok::T_DO,
            tok::T_ELSE,
            tok::T_FINALLY,
            tok::T_DECLARE,
            tok::T_ARRAY,
            tok::T_LIST,
            tok::T_CLASS,
            tok::T_INTERFACE,
            tok::T_TRAIT,
            tok::T_ENUM,
        ]
        .contains(&token_id)
        {
            return true;
        }
        if [i64::from(b';'), i64::from(b'{'), i64::from(b'}')].contains(&token_id) {
            return false;
        }
        cursor = previous;
        inspected += 1;
        if inspected >= 16 {
            return false;
        }
    }
    false
}

/// Returns a more Zend-shaped diagnostic for parser messages with known PHP expectations.
#[inline(never)]
pub(crate) fn diagnostic_override<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
    mapped: &ValidationError,
    raw_message: &str,
) -> Option<ValidationError> {
    if let Some(keyword) = raw_message.strip_prefix("Unexpected token at statement position: ") {
        let keyword = keyword.trim().to_ascii_lowercase();
        if keyword == "rbrace" {
            // A closing brace where a statement must start has no matching opener.
            return Some(ValidationError {
                message: "Unmatched '}'".into(),
                pos: mapped.pos,
                lexical: false,
            });
        }
        if matches!(keyword.as_str(), "catch" | "else" | "elseif" | "finally" | "dot") {
            // `if ($a) else` is a missing body, so Zend lists no expected token; after a
            // completed statement (`echo 1; else`) the only continuation is end of file.
            if matches!(keyword.as_str(), "else" | "elseif")
                && statement_slot_after_control(tokens, mapped.pos)
            {
                return Some(mapped.clone());
            }
            return Some(with_expectation(mapped.clone(), "end of file"));
        }
    }
    if raw_message == "Expected ';'" && mapped.pos < source.len() {
        return Some(match expected_statement_terminator(tokens, mapped.pos) {
            Some(expecting) => with_expectation(mapped.clone(), expecting),
            None => mapped.clone(),
        });
    }
    if raw_message == "Expected '{'" && mapped.pos < source.len() {
        return Some(with_expectation(mapped.clone(), "\"{\""));
    }
    if raw_message == "Unexpected token: Question" {
        return Some(with_expectation(mapped.clone(), "end of file"));
    }
    if raw_message == "Expected '=' after constant name" {
        return Some(with_expectation(mapped.clone(), "\"=\""));
    }
    let delimiter = match raw_message {
        "Expected ':' after list argument label" => Some("\":\""),
        "array must be followed by '('" | "list must be followed by '('" => Some("\"(\""),
        _ => None,
    };
    if let Some(delimiter) = delimiter {
        let mut diagnostic = mapped.clone();
        if source.get(mapped.pos..).is_some_and(|rest| rest.starts_with(b"?>")) {
            diagnostic.message = "syntax error, unexpected token \";\"".into();
        }
        return Some(with_expectation(diagnostic, delimiter));
    }
    if raw_message == "Expected ')' before list in unset operands" {
        return Some(with_expectation(mapped.clone(), "\")\""));
    }
    if raw_message == "Expected '::' after 'static'" {
        return Some(match token_after_keyword(tokens, mapped.pos, tok::T_STATIC) {
            Some(next) if tokens[next].id() == tok::T_CLOSE_TAG => ValidationError {
                message: "syntax error, unexpected token \";\", expecting \"::\"".into(),
                pos: tokens[next].pos(),
                lexical: false,
            },
            Some(next) => with_expectation(token_at_index_error(source, tokens, next), "\"::\""),
            None => end_of_file_expecting(source.len(), Some("\"::\"")),
        });
    }
    if raw_message.starts_with("Expected parent class name after 'extends'") {
        return Some(match token_after_keyword(tokens, mapped.pos, tok::T_EXTENDS) {
            Some(next) => token_at_index_error(source, tokens, next),
            None => end_of_file_expecting(source.len(), None),
        });
    }
    if raw_message.starts_with("Expected class name after 'new'") {
        return Some(match token_after_keyword(tokens, mapped.pos, tok::T_NEW) {
            Some(next) => with_expectation(token_at_index_error(source, tokens, next), "\"class\""),
            None => end_of_file_expecting(source.len(), Some("\"class\"")),
        });
    }
    if raw_message.starts_with("Expected property or method name after '->'")
        || raw_message.starts_with("Expected property or method name after '?->'")
    {
        let arrow = tok::T_OBJECT_OPERATOR;
        let next = token_after_keyword(tokens, mapped.pos, arrow)
            .or_else(|| token_after_keyword(tokens, mapped.pos, tok::T_NULLSAFE_OBJECT_OPERATOR));
        return Some(match next {
            Some(next) => with_expectation(
                token_at_index_error(source, tokens, next),
                "identifier or variable or \"{\" or \"$\"",
            ),
            None => end_of_file_expecting(
                source.len(),
                Some("identifier or variable or \"{\" or \"$\""),
            ),
        });
    }
    if raw_message.starts_with("Expected a statement after declare") {
        let Some(declare) = tokens
            .iter()
            .position(|token| token.pos() >= mapped.pos && token.id() == tok::T_DECLARE)
        else {
            return None;
        };
        let Some(open) = next_significant_index(tokens, declare) else { return None; };
        let Some(close) = matching_paren_end(tokens, open) else { return None; };
        if next_significant_index(tokens, close).is_none() {
            return Some(end_of_file_expecting(source.len(), None));
        }
    }
    if raw_message == "Expected 'class' after class modifier at statement position" {
        if let Some(index) = tokens
            .iter()
            .position(|token| token.pos() <= mapped.pos && mapped.pos < token.pos() + token.len())
        {
            if let Some(next) = next_significant_index(tokens, index) {
                return Some(with_expectation(
                    token_at_index_error(source, tokens, next),
                    "\"abstract\" or \"final\" or \"readonly\" or \"class\"",
                ));
            }
        }
    }
    if raw_message.starts_with("Expected member name after '::'")
        || raw_message.starts_with("Expected method name after '::'")
    {
        let next = token_after_keyword(tokens, mapped.pos, tok::T_DOUBLE_COLON);
        return next.map(|next| token_at_index_error(source, tokens, next));
    }
    if raw_message.starts_with("Expected '(' after 'fn'")
        || raw_message.starts_with("Expected '(' after 'match'")
    {
        let keyword = if raw_message.contains("'fn'") { tok::T_FN } else { tok::T_MATCH };
        if let Some(next) = token_after_keyword(tokens, mapped.pos, keyword) {
            return Some(with_expectation(
                token_at_index_error(source, tokens, next),
                "\"(\"",
            ));
        }
    }
    if raw_message.starts_with("Expected '=>' after arrow function parameters") {
        let keyword = tokens
            .iter()
            .position(|token| token.pos() >= mapped.pos && token.id() == tok::T_FN);
        if let Some(close) = keyword
            .and_then(|index| next_significant_index(tokens, index))
            .and_then(|open| matching_paren_end(tokens, open))
        {
            if let Some(next) = next_significant_index(tokens, close) {
                return Some(with_expectation(
                    token_at_index_error(source, tokens, next),
                    "\"=>\"",
                ));
            }
        }
    }
    if raw_message.starts_with("Expected '{' to open anonymous class body")
        && mapped.pos < source.len()
    {
        return Some(with_expectation(mapped.clone(), "\"{\""));
    }
    if raw_message.starts_with("Expected imported name after 'use'") {
        let use_index = tokens
            .iter()
            .position(|token| token.pos() >= mapped.pos && token.id() == tok::T_USE);
        if let Some(mut cursor) = use_index.and_then(|index| next_significant_index(tokens, index)) {
            if matches!(tokens[cursor].id(), id if id == tok::T_FUNCTION || id == tok::T_CONST) {
                let Some(after) = next_significant_index(tokens, cursor) else {
                    return None;
                };
                cursor = after;
            }
            return Some(token_error_at(source, tokens, tokens[cursor].pos()));
        }
    }
    if raw_message == "Unexpected token: Semicolon" {
        if matches!(unmatched_open_delimiter(tokens), Some((_, '['))) {
            return Some(with_expectation(mapped.clone(), "\"]\""));
        }
    }
    if raw_message == "Expected '{' after catch clause" {
        if let Some(pos) = first_token_text_after(source, tokens, mapped.pos, ";") {
            return Some(ValidationError {
                message: "syntax error, unexpected token \";\", expecting \"{\"".into(),
                pos,
                lexical: false,
            });
        }
    }
    if raw_message == "Group use declaration cannot be empty" {
        if let Some(pos) = first_token_text_after(source, tokens, mapped.pos, "}") {
            return Some(ValidationError {
                message: "syntax error, unexpected token \"}\", expecting identifier or namespaced name or \"function\" or \"const\"".into(),
                pos,
                lexical: false,
            });
        }
    }
    if raw_message == "Property declarations require a modifier" {
        return Some(ValidationError {
            ..with_expectation(mapped.clone(), "\"function\"")
        });
    }
    if raw_message == "Expected property hook name" {
        return Some(with_expectation(mapped.clone(), "identifier"));
    }
    if raw_message.contains("Expected trait use, property, or method declaration in class body") {
        if let Some(pos) = set_marker_close_paren(tokens, mapped.pos) {
            return Some(ValidationError {
                message: "syntax error, unexpected token \")\", expecting token \"&\"".into(),
                pos,
                lexical: false,
            });
        }
    }
    if raw_message == "Reserved type name" {
        let token = tokens
            .iter()
            .position(|token| token.pos() <= mapped.pos && mapped.pos < token.pos() + token.len());
        let mut next = token.and_then(|index| next_significant_index(tokens, index));
        if next.is_some_and(|next| {
            matches!(
                tokens[next].id(),
                id if id == tok::T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG
                    || id == tok::T_AMPERSAND_NOT_FOLLOWED_BY_VAR_OR_VARARG
                    || id == tok::T_ELLIPSIS
            )
        }) {
            next = next.and_then(|next| next_significant_index(tokens, next));
        }
        return Some(if next.is_some_and(|next| tokens[next].id() == tok::T_VARIABLE) {
            with_expectation(mapped.clone(), "variable")
        } else {
            mapped.clone()
        });
    }
    if raw_message == "Expected variable after '...'" {
        // Pick the marker that actually lacks its variable; the declaration span makes the
        // first `...` in a multi-parameter list look like the failure.
        let ellipsis = tokens
            .iter()
            .enumerate()
            .skip_while(|(_, token)| token.pos() < mapped.pos)
            .find(|(index, token)| {
                token.id() == tok::T_ELLIPSIS && !ellipsis_has_variable(tokens, *index)
            })
            .map(|(index, _)| index)
            .or_else(|| {
                tokens
                    .iter()
                    .position(|token| token.id() == tok::T_ELLIPSIS && token.pos() >= mapped.pos)
            });
        return Some(match ellipsis.and_then(|index| next_significant_index(tokens, index)) {
            Some(next) => with_expectation(token_at_index_error(source, tokens, next), "variable"),
            None => end_of_file_expecting(source.len(), Some("variable")),
        });
    }
    if raw_message == "Expected parameter variable" {
        // A `(set)` marker separated from its visibility word is a syntax error at its `)`,
        // with PHP's fixed expectation for that position.
        if let Some(pos) = spaced_set_marker_close(source, tokens, mapped.pos) {
            return Some(ValidationError {
                message: "syntax error, unexpected token \")\", expecting token \"&\"".into(),
                pos,
                lexical: false,
            });
        }
        if let Some(pos) = first_token_text_after(source, tokens, mapped.pos, "|") {
            return Some(ValidationError {
                message: "syntax error, unexpected token \"|\", expecting variable".into(),
                pos,
                lexical: false,
            });
        }
        return Some(with_expectation(mapped.clone(), "variable"));
    }
    if raw_message == "Unexpected token: Ampersand" {
        if ampersand_follows_subscript(tokens, mapped.pos) {
            return Some(with_expectation(mapped.clone(), "\"]\""));
        }
        // By-reference returns are declared on the function, never on the statement.
        if ampersand_follows(tokens, mapped.pos, tok::T_RETURN) {
            return Some(with_expectation(mapped.clone(), "\";\""));
        }
        // A later call argument can only be followed by the closing parenthesis.
        if ampersand_follows(tokens, mapped.pos, i64::from(b',')) {
            return Some(with_expectation(mapped.clone(), "\")\""));
        }
    }
    if raw_message == "Expected declaration name" {
        return Some(with_expectation(mapped.clone(), "identifier"));
    }
    if raw_message == "Expected function name" {
        return Some(with_expectation(mapped.clone(), "\"(\""));
    }
    if raw_message == "Expected static property after '::' in new class expression" {
        // PHP reports the offending member name with its fixed expectation.
        return Some(with_expectation(mapped.clone(), "variable or \"$\""));
    }
    if raw_message == "Expected '{' after finally" {
        return Some(with_expectation(mapped.clone(), "\"{\""));
    }
    if raw_message == "Expected 'class' after new attributes" {
        return Some(with_expectation(mapped.clone(), "\"class\""));
    }
    if raw_message == "Expected ',' between arguments" {
        // Inside an argument list the only token that can follow an argument is `)`.
        return Some(with_expectation(mapped.clone(), "\")\""));
    }
    if raw_message == "Expected '=>', '{', or ';' in property hook" {
        if let Some(pos) = first_token_text_after(source, tokens, mapped.pos, "}") {
            return Some(ValidationError {
                message: "syntax error, unexpected token \"}\", expecting \"=>\" or \";\" or \"{\""
                    .into(),
                pos,
                lexical: false,
            });
        }
    }
    if raw_message == "Invalid assignment target" {
        if let Some((pos, text)) = first_assignment_operator(source, tokens, mapped.pos) {
            return Some(unexpected_token_at(source, pos, text.len(), None));
        }
    }
    eof_override(source, tokens, mapped, raw_message)
}

/// Returns an EOF-shaped diagnostic when the parser failed at the end of the source.
#[inline(never)]
pub(crate) fn eof_override<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
    mapped: &ValidationError,
    raw_message: &str,
) -> Option<ValidationError> {
    let error_index = tokens
        .iter()
        .position(|token| token.pos() <= mapped.pos && mapped.pos < token.pos() + token.len());
    let at_eof = mapped.pos >= source.len()
        || error_index.is_some_and(|index| next_significant_index(tokens, index).is_none());
    if !at_eof {
        return None;
    }
    let expecting = if raw_message == "Unexpected token: Eof" {
        expected_statement_terminator(tokens, mapped.pos)
    } else if raw_message.starts_with("Expected class name after 'new'") {
        Some("\"class\"")
    } else if raw_message.starts_with("Expected namespace name after 'namespace'")
        || raw_message.starts_with("Expected ';' or '{' after namespace name")
    {
        Some("\"{\"")
    } else if raw_message.starts_with("Expected imported name after 'use'") {
        None
    } else if raw_message == "Expected ';'" {
        expected_statement_terminator(tokens, mapped.pos)
    } else if raw_message.starts_with("Expected property or method name after '->'") {
        Some("identifier or variable or \"{\" or \"$\"")
    } else if raw_message.starts_with("Expected function name")
        || raw_message.starts_with("Expected interface name")
        || raw_message.starts_with("Expected trait name")
        || raw_message.starts_with("Expected constant name")
        || raw_message.starts_with("Expected class name after 'class'")
        || raw_message.starts_with("Expected enum name")
        || raw_message.starts_with("Expected a directive name")
    {
        Some("identifier")
    } else if let Some(literal) = expected_literal(raw_message) {
        // Reuse the caller-provided literal in quotes without re-escaping.
        return Some(end_of_file_literal(source.len(), literal));
    } else if raw_message.contains("Expected variable") {
        Some("variable")
    } else if raw_message.contains("Expected") {
        None
    } else {
        return None;
    };
    Some(end_of_file_expecting(source.len(), expecting))
}

/// Returns the literal inside the first `Expected '...'` fragment.
pub(crate) fn expected_literal(message: &str) -> Option<&str> {
    let rest = message.strip_prefix("Expected '")?;
    let end = rest.find('\'')?;
    Some(&rest[..end])
}

/// Returns the byte offset of the first token with exactly `text` at or after `from`.
#[inline(never)]
pub(crate) fn first_token_text_after<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
    from: usize,
    text: &str,
) -> Option<usize> {
    tokens
        .iter()
        .filter(|token| token.pos() >= from)
        .find(|token| &source[token.pos()..token.pos() + token.len()] == text.as_bytes())
        .map(|token| token.pos())
}

/// Returns the assignment operator following the mapped position.
pub(crate) fn first_assignment_operator<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
    from: usize,
) -> Option<(usize, String)> {
    const OPERATORS: &[&str] = &[
        "=", "+=", "-=", "*=", "**=", "/=", ".=", "%=", "&=", "|=", "^=", "<<=",
        ">>=", "??=",
    ];
    tokens
        .iter()
        .filter(|token| token.pos() >= from)
        .find_map(|token| {
            let text = std::str::from_utf8(&source[token.pos()..token.pos() + token.len()]).ok()?;
            OPERATORS.contains(&text).then(|| (token.pos(), text.to_string()))
        })
}

/// Returns the `)` that closes a whitespace-separated `( set )` marker.
#[inline(never)]
pub(crate) fn set_marker_close_paren<T: SourceToken>(tokens: &[T], from: usize) -> Option<usize> {
    for (index, token) in tokens.iter().enumerate() {
        if token.pos() < from || token.id() != i64::from(b')') {
            continue;
        }
        let Some(set) = previous_significant_index(tokens, index) else { continue; };
        if tokens[set].id() != tok::T_STRING {
            continue;
        }
        let Some(open) = previous_significant_index(tokens, set) else { continue; };
        if tokens[open].id() == i64::from(b'(') {
            return Some(token.pos());
        }
    }
    None
}

/// Builds an end-of-file diagnostic with an optional expected-token suffix.
#[inline(never)]
pub(crate) fn end_of_file_expecting(pos: usize, expecting: Option<&str>) -> ValidationError {
    let message = match expecting {
        Some(expecting) => format!("syntax error, unexpected end of file, expecting {expecting}"),
        None => "syntax error, unexpected end of file".into(),
    };
    ValidationError { message, pos, lexical: false }
}

/// Builds an end-of-file diagnostic whose expected token is a quoted literal.
#[inline(never)]
pub(crate) fn end_of_file_literal(pos: usize, literal: &str) -> ValidationError {
    end_of_file_expecting(pos, Some(&format!("\"{literal}\"")))
}

/// Builds `syntax error, unexpected token "<text>"` with an optional expectation.
#[inline(never)]
pub(crate) fn unexpected_token_at(
    source: &[u8],
    pos: usize,
    len: usize,
    expecting: Option<&str>,
) -> ValidationError {
    let text = String::from_utf8_lossy(&source[pos..pos + len]);
    let message = match expecting {
        Some(expecting) => {
            format!("syntax error, unexpected token \"{text}\", expecting {expecting}")
        }
        None => format!("syntax error, unexpected token \"{text}\""),
    };
    ValidationError { message, pos, lexical: false }
}

/// Builds `syntax error, unexpected <noun> "<text>"` with an optional expectation.
#[inline(never)]
pub(crate) fn unexpected_noun_at(
    source: &[u8],
    pos: usize,
    len: usize,
    noun: &str,
    expecting: Option<&str>,
) -> ValidationError {
    let text = String::from_utf8_lossy(&source[pos..pos + len]);
    let message = match expecting {
        Some(expecting) => {
            format!("syntax error, unexpected {noun} \"{text}\", expecting {expecting}")
        }
        None => format!("syntax error, unexpected {noun} \"{text}\""),
    };
    ValidationError { message, pos, lexical: false }
}

/// Builds Zend's diagnostic for a quoted string literal token.
///
/// The message uses a dedicated noun and prints the raw source between the quotes, so escapes
/// stay visible (`unexpected double-quoted string "a\"b"`).
pub(crate) fn string_token_error(source: &[u8], pos: usize, len: usize) -> ValidationError {
    let text = &source[pos..pos + len];
    let (noun, body) = match text.first() {
        Some(b'\'') => ("single-quoted string", &text[1..text.len().saturating_sub(1)]),
        Some(b'"') => ("double-quoted string", &text[1..text.len().saturating_sub(1)]),
        Some(b'b') | Some(b'B') if text.len() >= 3 => (
            "quoted string",
            // Zend prints the binary token without its trailing quote.
            &text[..text.len().saturating_sub(1)],
        ),
        _ => ("quoted string", text),
    };
    ValidationError {
        message: format!(
            "syntax error, unexpected {noun} \"{}\"",
            String::from_utf8_lossy(body)
        ),
        pos,
        lexical: false,
    }
}

/// Builds Zend's diagnostic for a heredoc/nowdoc start token.
///
/// The token text includes the newline and the body, but PHP names only the header.
pub(crate) fn heredoc_token_error(source: &[u8], pos: usize, len: usize) -> ValidationError {
    let text = &source[pos..pos + len];
    let header = text.split(|byte| *byte == b'\n' || *byte == b'\r').next().unwrap_or(text);
    let noun = if header.contains(&b'\'') { "nowdoc start" } else { "heredoc start" };
    // Zend reports the diagnostic after consuming the start line, so a multiline heredoc
    // names its first body line rather than the `<<<` line.
    let mut error_pos = pos + header.len();
    if text.get(header.len()) == Some(&b'\r') && text.get(header.len() + 1) == Some(&b'\n') {
        error_pos += 2;
    } else if text.get(header.len()).is_some() {
        error_pos += 1;
    }
    ValidationError {
        message: format!(
            "syntax error, unexpected {noun} \"{}\"",
            String::from_utf8_lossy(header)
        ),
        pos: error_pos.min(pos + len),
        lexical: false,
    }
}

/// Appends `, expecting ...` to an existing diagnostic.
#[inline(never)]
pub(crate) fn with_expectation(mut error: ValidationError, expecting: &str) -> ValidationError {
    error.message = format!("{}, expecting {expecting}", error.message);
    error
}

/// Returns the expectation Zend lists for a missing statement semicolon at end of file.
///
/// Zend's Bison tables report the tokens the failing state accepts: keyword-led statements
/// with a single `;` continuation, list statements with `,` or `;`, namespace declarations
/// with `{`, and expression statements with no expectation at all.
pub(crate) fn expected_statement_terminator<T: SourceToken>(
    tokens: &[T],
    pos: usize,
) -> Option<&'static str> {
    let mut index = tokens
        .iter()
        .position(|token| token.pos() <= pos && pos < token.pos() + token.len())
        .or_else(|| tokens.iter().rposition(|token| token.pos() < pos))?;
    let last = index;
    let mut depth = 0i32;
    loop {
        let id = tokens[index].id();
        if id == i64::from(b')') || id == i64::from(b']') || id == i64::from(b'}') {
            depth += 1;
        } else if id == i64::from(b'(') || id == i64::from(b'[') || id == i64::from(b'{') {
            if depth > 0 {
                depth -= 1;
            }
        } else if depth == 0 {
            match id {
                id if id == tok::T_RETURN
                    || id == tok::T_UNSET
                    || id == tok::T_BREAK
                    || id == tok::T_CONTINUE
                    || id == tok::T_GOTO
                    || id == tok::T_DO
                    || id == tok::T_WHILE =>
                {
                    return Some("\";\"");
                }
                id if id == tok::T_USE
                    || id == tok::T_CONST
                    || id == tok::T_GLOBAL
                    || id == tok::T_STATIC =>
                {
                    return Some("\",\" or \";\"");
                }
                // A bare `echo` at end of file is only the keyword, and Zend lists nothing.
                id if id == tok::T_ECHO && index != last => {
                    return Some("\",\" or \";\"");
                }
                id if id == tok::T_NAMESPACE => return Some("\"{\""),
                id if id == i64::from(b';') => return None,
                _ => {}
            }
        }
        index = previous_significant_index(tokens, index)?;
    }
}

/// Returns whether an ellipsis is immediately followed by its variadic variable.
fn ellipsis_has_variable<T: SourceToken>(tokens: &[T], index: usize) -> bool {
    let mut next = next_significant_index(tokens, index);
    if next.is_some_and(|next| {
        matches!(
            tokens[next].id(),
            id if id == tok::T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG
                || id == tok::T_AMPERSAND_NOT_FOLLOWED_BY_VAR_OR_VARARG
        )
    }) {
        next = next.and_then(|next| next_significant_index(tokens, next));
    }
    next.is_some_and(|next| tokens[next].id() == tok::T_VARIABLE)
}

/// Builds the diagnostic for the token at `index`.
pub(crate) fn token_at_index_error<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
    index: usize,
) -> ValidationError {
    token_error_at(source, tokens, tokens[index].pos())
}

/// Returns the significant token after the first `id` token at or after `from`.
pub(crate) fn token_after_keyword<T: SourceToken>(tokens: &[T], from: usize, id: i64) -> Option<usize> {
    let keyword = tokens
        .iter()
        .position(|token| token.pos() >= from && token.id() == id)?;
    next_significant_index(tokens, keyword)
}

/// Returns whether `pos` sits where a control structure needs its body statement.
///
/// Zend omits its expected-token list in that grammar state, but adds `expecting end of file`
/// once a complete statement has been parsed.
pub(crate) fn statement_slot_after_control<T: SourceToken>(tokens: &[T], pos: usize) -> bool {
    let Some(index) = tokens
        .iter()
        .position(|token| token.pos() <= pos && pos < token.pos() + token.len())
    else {
        return false;
    };
    let Some(previous) = previous_significant_index(tokens, index) else { return false; };
    if tokens[previous].id() == tok::T_ELSE {
        return true;
    }
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
                return previous_significant_index(tokens, cursor).is_some_and(|before| {
                    matches!(
                        tokens[before].id(),
                        id if id == tok::T_IF
                            || id == tok::T_ELSEIF
                            || id == tok::T_WHILE
                            || id == tok::T_FOR
                            || id == tok::T_FOREACH
                            || id == tok::T_DECLARE
                            || id == tok::T_SWITCH
                    )
                });
            }
        }
    }
    false
}

/// Builds Zend's `unexpected double-quote mark` diagnostic at a string delimiter.
pub(crate) fn delimiter_error<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
    delimiter: usize,
    expecting: &str,
) -> ValidationError {
    let text = &source[tokens[delimiter].pos()..tokens[delimiter].pos() + tokens[delimiter].len()];
    let noun = match text {
        b"\"" => "double-quote mark",
        b"`" => "backtick",
        _ => "end of heredoc",
    };
    ValidationError {
        message: format!("syntax error, unexpected {noun}, expecting {expecting}"),
        pos: tokens[delimiter].pos(),
        lexical: false,
    }
}

/// Builds the diagnostic for one token with Zend's noun and an expected-token suffix.
pub(crate) fn token_noun_error<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
    index: usize,
    expecting: &str,
) -> ValidationError {
    let token = &tokens[index];
    let pos = token.pos();
    let len = token.len();
    match token.id() {
        id if id == tok::T_STRING || id == tok::T_IDENTIFIER => {
            unexpected_noun_at(source, pos, len, "identifier", Some(expecting))
        }
        id if id == tok::T_VARIABLE => {
            unexpected_noun_at(source, pos, len, "variable", Some(expecting))
        }
        id if id == tok::T_LNUMBER => {
            unexpected_noun_at(source, pos, len, "integer", Some(expecting))
        }
        id if id == tok::T_DNUMBER => {
            unexpected_noun_at(source, pos, len, "floating-point number", Some(expecting))
        }
        id if id == tok::T_CONSTANT_ENCAPSED_STRING => with_expectation(
            string_token_error(source, pos, len),
            expecting,
        ),
        id if id == tok::T_ENCAPSED_AND_WHITESPACE => ValidationError {
            message: format!("syntax error, unexpected string content \"\", expecting {expecting}"),
            pos,
            lexical: false,
        },
        id if id == tok::T_NAME_FULLY_QUALIFIED => {
            unexpected_noun_at(source, pos, len, "fully qualified name", Some(expecting))
        }
        id if id == tok::T_NAME_QUALIFIED => {
            unexpected_noun_at(source, pos, len, "namespaced name", Some(expecting))
        }
        id if id == tok::T_NAME_RELATIVE => {
            unexpected_noun_at(source, pos, len, "namespace-relative name", Some(expecting))
        }
        _ => {
            let text = &source[pos..pos + len];
            if text == b"\"" {
                ValidationError {
                    message: format!("syntax error, unexpected double-quote mark, expecting {expecting}"),
                    pos,
                    lexical: false,
                }
            } else if let Some(cast) = crate::scanner::cast_diagnostic_text(token.id()) {
                ValidationError {
                    message: format!("syntax error, unexpected token \"{cast}\", expecting {expecting}"),
                    pos,
                    lexical: false,
                }
            } else if token.id() == tok::T_CLOSE_TAG {
                ValidationError {
                    message: format!("syntax error, unexpected token \";\", expecting {expecting}"),
                    pos,
                    lexical: false,
                }
            } else if let Some(keyword) = crate::scanner::keyword_diagnostic_spelling(token.id(), text) {
                ValidationError {
                    message: format!("syntax error, unexpected token \"{keyword}\", expecting {expecting}"),
                    pos,
                    lexical: false,
                }
            } else {
                unexpected_token_at(source, pos, len, Some(expecting))
            }
        }
    }
}

/// Returns the closing `)` of a `(set)` marker the parser stopped on.
///
/// That shape only reaches the parser when the marker does not touch its visibility word
/// (`public (set)`), because a properly attached `public(set)` is consumed as one modifier.
pub(crate) fn spaced_set_marker_close<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
    pos: usize,
) -> Option<usize> {
    let index = tokens.iter().position(|token| token.pos() == pos)?;
    let text = |offset: usize| {
        let token = tokens.get(index + offset)?;
        source.get(token.pos()..token.pos() + token.len())
    };
    if text(0)? != b"(" {
        return None;
    }
    if !text(1).is_some_and(|word| word.eq_ignore_ascii_case(b"set")) {
        return None;
    }
    if text(2)? != b")" {
        return None;
    }
    Some(tokens[index + 2].pos())
}

/// Returns whether the token at `pos` is a `&` whose previous significant token is `id`.
pub(crate) fn ampersand_follows<T: SourceToken>(tokens: &[T], pos: usize, id: i64) -> bool {
    let Some(index) = tokens
        .iter()
        .position(|token| token.pos() == pos && token.id() != 0)
    else {
        return false;
    };
    previous_significant_index(tokens, index)
        .is_some_and(|before| tokens[before].id() == id)
}

/// Returns whether the token at `pos` is a `&` opening a subscript expression (`$a[&$b]`).
pub(crate) fn ampersand_follows_subscript<T: SourceToken>(tokens: &[T], pos: usize) -> bool {
    let Some(index) = tokens
        .iter()
        .position(|token| token.pos() == pos && token.id() != 0)
    else {
        return false;
    };
    previous_significant_index(tokens, index).is_some_and(|before| {
        tokens[before].id() == i64::from(b'[') && !bracket_opens_array_literal(tokens, before)
    })
}

/// Builds a PHP-style parse error from the AST parser's first diagnostic.
pub(crate) fn parser_error<T: SourceToken>(
    view: &str,
    map: &[u32],
    lines: &[usize],
    source: &[u8],
    tokens: &[T],
    error: &CompileError,
) -> ValidationError {
    let view_pos = view_offset(view, lines, error.span.line, error.span.col);
    let source_pos = map.get(view_pos).copied().map(|pos| pos as usize).unwrap_or(source.len());
    token_error_at(source, tokens, source_pos)
}

/// Builds PHP's diagnostic for the source token that contains `source_pos`.
pub(crate) fn token_error_at<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
    source_pos: usize,
) -> ValidationError {
    if source_pos >= source.len() {
        return ValidationError {
            message: "syntax error, unexpected end of file".into(),
            pos: source.len(),
            lexical: false,
        };
    }
    let token = tokens
        .iter()
        .find(|token| token.pos() <= source_pos && source_pos < token.pos() + token.len());
    match token {
        None => end_of_file_expecting(source.len(), None),
        Some(token) => {
            let pos = token.pos();
            let len = token.len();
            match token.id() {
                id if id == tok::T_STRING || id == tok::T_IDENTIFIER => {
                    unexpected_noun_at(source, pos, len, "identifier", None)
                }
                tok::T_NAME_FULLY_QUALIFIED => {
                    unexpected_noun_at(source, pos, len, "fully qualified name", None)
                }
                tok::T_NAME_QUALIFIED => {
                    unexpected_noun_at(source, pos, len, "namespaced name", None)
                }
                tok::T_NAME_RELATIVE => {
                    unexpected_noun_at(source, pos, len, "namespace-relative name", None)
                }
                id if id == tok::T_VARIABLE => {
                    unexpected_noun_at(source, pos, len, "variable", None)
                }
                id if id == tok::T_LNUMBER => {
                    unexpected_noun_at(source, pos, len, "integer", None)
                }
                id if id == tok::T_DNUMBER => {
                    unexpected_noun_at(source, pos, len, "floating-point number", None)
                }
                id if id == tok::T_CONSTANT_ENCAPSED_STRING => {
                    string_token_error(source, pos, len)
                }
                id if id == tok::T_EXIT => ValidationError {
                    // Zend names T_EXIT `exit` even when the source spells it `die`.
                    message: "syntax error, unexpected token \"exit\"".into(),
                    pos,
                    lexical: false,
                },
                id if id == tok::T_START_HEREDOC => heredoc_token_error(source, pos, len),
                _ => match crate::scanner::keyword_diagnostic_spelling(token.id(), &source[pos..pos + len]) {
                    Some(keyword) => ValidationError {
                        message: format!("syntax error, unexpected token \"{keyword}\""),
                        pos,
                        lexical: false,
                    },
                    None => unexpected_token_at(source, pos, len, None),
                },
            }
        }
    }
}
