//! Purpose:
//! Provides TOKEN_PARSE syntax validation and Zend's semi-reserved-word feedback through
//! elephc's own lexer, parser and AST.
//!
//! Called from:
//! - `elephc-tokenizer-engine` after native tokenization when PHP's TOKEN_PARSE flag is set.
//!
//! Key details:
//! - No separate PHP grammar lives here: the source is normalized into elephc's AST parser view.
//! - Inline HTML, open/close tags and `__halt_compiler` payloads are blanked while line numbers
//!   stay stable, so parser diagnostics keep PHP's source lines.
//! - Accepted keyword identifiers are mapped back to byte offsets for token reclassification.

use std::collections::HashSet;

use crate::errors::CompileError;
use crate::span::Span;


/// Compile-time PHP 8.5 token IDs used by the validation view.
///
/// Every value is derived from the shared contract catalog; a typo produces `-1`, which the
/// tests fail loudly on. Resolving them at compile time removes ~18 catalog hash lookups per
/// source token from `parser_view`.
mod tok {
    use elephc_builtin_contract::token_const_id as id;
    pub const T_ABSTRACT: i64 = id("T_ABSTRACT");
    pub const T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG: i64 = id("T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG");
    pub const T_AMPERSAND_NOT_FOLLOWED_BY_VAR_OR_VARARG: i64 = id("T_AMPERSAND_NOT_FOLLOWED_BY_VAR_OR_VARARG");
    pub const T_ARRAY: i64 = id("T_ARRAY");
    pub const T_AS: i64 = id("T_AS");
    pub const T_CASE: i64 = id("T_CASE");
    pub const T_CLASS: i64 = id("T_CLASS");
    pub const T_CLOSE_TAG: i64 = id("T_CLOSE_TAG");
    pub const T_COMMENT: i64 = id("T_COMMENT");
    pub const T_CONST: i64 = id("T_CONST");
    pub const T_CONSTANT_ENCAPSED_STRING: i64 = id("T_CONSTANT_ENCAPSED_STRING");
    pub const T_DNUMBER: i64 = id("T_DNUMBER");
    pub const T_LNUMBER: i64 = id("T_LNUMBER");
    pub const T_DOC_COMMENT: i64 = id("T_DOC_COMMENT");
    pub const T_DOLLAR_OPEN_CURLY_BRACES: i64 = id("T_DOLLAR_OPEN_CURLY_BRACES");
    pub const T_DOUBLE_ARROW: i64 = id("T_DOUBLE_ARROW");
    pub const T_DO: i64 = id("T_DO");
    pub const T_CURLY_OPEN: i64 = id("T_CURLY_OPEN");
    pub const T_DOUBLE_COLON: i64 = id("T_DOUBLE_COLON");
    pub const T_ATTRIBUTE: i64 = id("T_ATTRIBUTE");
    pub const T_DECLARE: i64 = id("T_DECLARE");
    pub const T_FOR: i64 = id("T_FOR");
    pub const T_FOREACH: i64 = id("T_FOREACH");
    pub const T_IF: i64 = id("T_IF");
    pub const T_MATCH: i64 = id("T_MATCH");
    pub const T_SWITCH: i64 = id("T_SWITCH");
    pub const T_WHILE: i64 = id("T_WHILE");
    pub const T_ELSE: i64 = id("T_ELSE");
    pub const T_ELSEIF: i64 = id("T_ELSEIF");
    pub const T_FINALLY: i64 = id("T_FINALLY");
    pub const T_TRY: i64 = id("T_TRY");
    pub const T_ELLIPSIS: i64 = id("T_ELLIPSIS");
    pub const T_ENUM: i64 = id("T_ENUM");
    pub const T_FN: i64 = id("T_FN");
    pub const T_FUNCTION: i64 = id("T_FUNCTION");
    pub const T_GOTO: i64 = id("T_GOTO");
    pub const T_HALT_COMPILER: i64 = id("T_HALT_COMPILER");
    pub const T_INLINE_HTML: i64 = id("T_INLINE_HTML");
    pub const T_INTERFACE: i64 = id("T_INTERFACE");
    pub const T_LIST: i64 = id("T_LIST");
    pub const T_NEW: i64 = id("T_NEW");
    pub const T_NS_SEPARATOR: i64 = id("T_NS_SEPARATOR");
    pub const T_NULLSAFE_OBJECT_OPERATOR: i64 = id("T_NULLSAFE_OBJECT_OPERATOR");
    pub const T_NUM_STRING: i64 = id("T_NUM_STRING");
    pub const T_OPEN_TAG: i64 = id("T_OPEN_TAG");
    pub const T_OBJECT_OPERATOR: i64 = id("T_OBJECT_OPERATOR");
    pub const T_OPEN_TAG_WITH_ECHO: i64 = id("T_OPEN_TAG_WITH_ECHO");
    pub const T_PRIVATE: i64 = id("T_PRIVATE");
    pub const T_PROTECTED: i64 = id("T_PROTECTED");
    pub const T_PUBLIC: i64 = id("T_PUBLIC");
    pub const T_READONLY: i64 = id("T_READONLY");
    pub const T_RETURN: i64 = id("T_RETURN");
    pub const T_START_HEREDOC: i64 = id("T_START_HEREDOC");
    pub const T_STATIC: i64 = id("T_STATIC");
    pub const T_STRING: i64 = id("T_STRING");
    pub const T_STRING_CAST: i64 = id("T_STRING_CAST");
    pub const T_TRAIT: i64 = id("T_TRAIT");
    pub const T_UNSET_CAST: i64 = id("T_UNSET_CAST");
    pub const T_USE: i64 = id("T_USE");
    pub const T_VARIABLE: i64 = id("T_VARIABLE");
    pub const T_VOID_CAST: i64 = id("T_VOID_CAST");
    pub const T_WHITESPACE: i64 = id("T_WHITESPACE");
    /// PHP's parser does not use this token; preserving the old fallback keeps the comparison.
    pub const T_IDENTIFIER: i64 = -1;
}

const _: () = {
    assert!(tok::T_ABSTRACT != -1, "T_ABSTRACT missing from the tokenizer catalog");
    assert!(tok::T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG != -1, "T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG missing from the tokenizer catalog");
    assert!(tok::T_AMPERSAND_NOT_FOLLOWED_BY_VAR_OR_VARARG != -1, "T_AMPERSAND_NOT_FOLLOWED_BY_VAR_OR_VARARG missing from the tokenizer catalog");
    assert!(tok::T_ARRAY != -1, "T_ARRAY missing from the tokenizer catalog");
    assert!(tok::T_AS != -1, "T_AS missing from the tokenizer catalog");
    assert!(tok::T_CASE != -1, "T_CASE missing from the tokenizer catalog");
    assert!(tok::T_CLASS != -1, "T_CLASS missing from the tokenizer catalog");
    assert!(tok::T_CLOSE_TAG != -1, "T_CLOSE_TAG missing from the tokenizer catalog");
    assert!(tok::T_COMMENT != -1, "T_COMMENT missing from the tokenizer catalog");
    assert!(tok::T_CONST != -1, "T_CONST missing from the tokenizer catalog");
    assert!(tok::T_CONSTANT_ENCAPSED_STRING != -1, "T_CONSTANT_ENCAPSED_STRING missing from the tokenizer catalog");
    assert!(tok::T_DNUMBER != -1, "T_DNUMBER missing from the tokenizer catalog");
    assert!(tok::T_LNUMBER != -1, "T_LNUMBER missing from the tokenizer catalog");
    assert!(tok::T_DOC_COMMENT != -1, "T_DOC_COMMENT missing from the tokenizer catalog");
    assert!(tok::T_DOLLAR_OPEN_CURLY_BRACES != -1, "T_DOLLAR_OPEN_CURLY_BRACES missing from the tokenizer catalog");
    assert!(tok::T_DOUBLE_ARROW != -1, "T_DOUBLE_ARROW missing from the tokenizer catalog");
    assert!(tok::T_DO != -1, "T_DO missing from the tokenizer catalog");
    assert!(tok::T_CURLY_OPEN != -1, "T_CURLY_OPEN missing from the tokenizer catalog");
    assert!(tok::T_DOUBLE_COLON != -1, "T_DOUBLE_COLON missing from the tokenizer catalog");
    assert!(tok::T_ATTRIBUTE != -1, "T_ATTRIBUTE missing from the tokenizer catalog");
    assert!(tok::T_DECLARE != -1, "T_DECLARE missing from the tokenizer catalog");
    assert!(tok::T_FOR != -1, "T_FOR missing from the tokenizer catalog");
    assert!(tok::T_FOREACH != -1, "T_FOREACH missing from the tokenizer catalog");
    assert!(tok::T_IF != -1, "T_IF missing from the tokenizer catalog");
    assert!(tok::T_MATCH != -1, "T_MATCH missing from the tokenizer catalog");
    assert!(tok::T_SWITCH != -1, "T_SWITCH missing from the tokenizer catalog");
    assert!(tok::T_WHILE != -1, "T_WHILE missing from the tokenizer catalog");
    assert!(tok::T_ELSE != -1, "T_ELSE missing from the tokenizer catalog");
    assert!(tok::T_ELSEIF != -1, "T_ELSEIF missing from the tokenizer catalog");
    assert!(tok::T_FINALLY != -1, "T_FINALLY missing from the tokenizer catalog");
    assert!(tok::T_TRY != -1, "T_TRY missing from the tokenizer catalog");
    assert!(tok::T_ELLIPSIS != -1, "T_ELLIPSIS missing from the tokenizer catalog");
    assert!(tok::T_ENUM != -1, "T_ENUM missing from the tokenizer catalog");
    assert!(tok::T_FN != -1, "T_FN missing from the tokenizer catalog");
    assert!(tok::T_FUNCTION != -1, "T_FUNCTION missing from the tokenizer catalog");
    assert!(tok::T_GOTO != -1, "T_GOTO missing from the tokenizer catalog");
    assert!(tok::T_HALT_COMPILER != -1, "T_HALT_COMPILER missing from the tokenizer catalog");
    assert!(tok::T_INLINE_HTML != -1, "T_INLINE_HTML missing from the tokenizer catalog");
    assert!(tok::T_INTERFACE != -1, "T_INTERFACE missing from the tokenizer catalog");
    assert!(tok::T_LIST != -1, "T_LIST missing from the tokenizer catalog");
    assert!(tok::T_NEW != -1, "T_NEW missing from the tokenizer catalog");
    assert!(tok::T_NS_SEPARATOR != -1, "T_NS_SEPARATOR missing from the tokenizer catalog");
    assert!(tok::T_NULLSAFE_OBJECT_OPERATOR != -1, "T_NULLSAFE_OBJECT_OPERATOR missing from the tokenizer catalog");
    assert!(tok::T_NUM_STRING != -1, "T_NUM_STRING missing from the tokenizer catalog");
    assert!(tok::T_OPEN_TAG != -1, "T_OPEN_TAG missing from the tokenizer catalog");
    assert!(tok::T_OBJECT_OPERATOR != -1, "T_OBJECT_OPERATOR missing from the tokenizer catalog");
    assert!(tok::T_OPEN_TAG_WITH_ECHO != -1, "T_OPEN_TAG_WITH_ECHO missing from the tokenizer catalog");
    assert!(tok::T_PRIVATE != -1, "T_PRIVATE missing from the tokenizer catalog");
    assert!(tok::T_PROTECTED != -1, "T_PROTECTED missing from the tokenizer catalog");
    assert!(tok::T_PUBLIC != -1, "T_PUBLIC missing from the tokenizer catalog");
    assert!(tok::T_READONLY != -1, "T_READONLY missing from the tokenizer catalog");
    assert!(tok::T_RETURN != -1, "T_RETURN missing from the tokenizer catalog");
    assert!(tok::T_START_HEREDOC != -1, "T_START_HEREDOC missing from the tokenizer catalog");
    assert!(tok::T_STATIC != -1, "T_STATIC missing from the tokenizer catalog");
    assert!(tok::T_STRING != -1, "T_STRING missing from the tokenizer catalog");
    assert!(tok::T_STRING_CAST != -1, "T_STRING_CAST missing from the tokenizer catalog");
    assert!(tok::T_TRAIT != -1, "T_TRAIT missing from the tokenizer catalog");
    assert!(tok::T_UNSET_CAST != -1, "T_UNSET_CAST missing from the tokenizer catalog");
    assert!(tok::T_USE != -1, "T_USE missing from the tokenizer catalog");
    assert!(tok::T_VARIABLE != -1, "T_VARIABLE missing from the tokenizer catalog");
    assert!(tok::T_VOID_CAST != -1, "T_VOID_CAST missing from the tokenizer catalog");
    assert!(tok::T_WHITESPACE != -1, "T_WHITESPACE missing from the tokenizer catalog");
};

/// The token metadata this validator needs, implemented by each embedder's token type.
///
/// The trait keeps `TOKEN_PARSE` allocation-free for callers that already hold a token
/// vector: validation reads `id`/`pos`/`len` in place instead of copying 24 bytes per token.
pub trait SourceToken {
    /// Returns the PHP token identifier.
    fn id(&self) -> i64;
    /// Returns the zero-based byte offset in the original source.
    fn pos(&self) -> usize;
    /// Returns the byte length in the original source.
    fn len(&self) -> usize;
}

/// One tokenizer token reduced to the metadata this validator needs.
#[derive(Clone, Copy, Debug)]
pub struct SyntaxToken {
    /// PHP token identifier.
    pub id: i64,
    /// Zero-based byte offset in the original source.
    pub pos: usize,
    /// Byte length in the original source.
    pub len: usize,
}

impl SourceToken for SyntaxToken {
    /// Returns the PHP token identifier.
    fn id(&self) -> i64 { self.id }
    /// Returns the zero-based byte offset in the original source.
    fn pos(&self) -> usize { self.pos }
    /// Returns the byte length in the original source.
    fn len(&self) -> usize { self.len }
}

/// One TOKEN_PARSE failure with PHP-compatible text and a source byte offset.
#[derive(Clone, Debug)]
pub struct ValidationError {
    /// PHP-style parser message without a filename suffix.
    pub message: String,
    /// Byte offset used to derive the ParseError line.
    pub pos: usize,
    /// Whether elephc's parser-view lexer produced this diagnostic.
    pub lexical: bool,
}

/// One token's identifier, source offset and byte length for non-generic validation.
#[derive(Clone, Copy, Debug)]
pub struct TokenView {
    /// PHP token identifier.
    pub id: i64,
    /// Zero-based byte offset in the original source.
    pub pos: usize,
    /// Byte length in the original source.
    pub len: usize,
}

impl SourceToken for TokenView {
    /// Returns the PHP token identifier.
    fn id(&self) -> i64 { self.id }
    /// Returns the zero-based byte offset in the original source.
    fn pos(&self) -> usize { self.pos }
    /// Returns the byte length in the original source.
    fn len(&self) -> usize { self.len }
}

/// Validates a token view without monomorphizing the parser in the engine's opt-level crate.
pub fn validate_tokens(source: &[u8], tokens: &[TokenView]) -> ValidationFeedback {
    validate(source, tokens)
}

/// One non-fatal TOKEN_PARSE diagnostic produced by validation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationWarning {
    /// PHP's exact parser-facing message.
    pub message: String,
    /// Byte offset of the construct that raised it.
    pub pos: usize,
}

/// The result of validating one source for TOKEN_PARSE.
#[derive(Clone, Debug, Default)]
pub struct ValidationFeedback {
    /// Byte offsets of keyword tokens Zend must rewrite to T_STRING.
    pub names: Vec<usize>,
    /// The first parser error, if any.
    pub error: Option<ValidationError>,
    /// Non-fatal diagnostics raised during validation.
    pub warnings: Vec<ValidationWarning>,
}

/// Validates PHP source and returns identifier feedback or the first PHP-style parse error.
pub fn validate<T: SourceToken>(source: &[u8], tokens: &[T]) -> ValidationFeedback {
    if let Some(error) = close_tag_else_error(source, tokens)
        .or_else(|| halt_compiler_scope_error(source, tokens))
        .or_else(|| new_dynamic_static_error(source, tokens))
        .or_else(|| goto_statement_error(source, tokens))
        .or_else(|| unterminated_backtick_error(source, tokens))
    {
        return ValidationFeedback {
            names: Vec::new(),
            error: Some(error),
            warnings: Vec::new(),
        };
    }
    let (view, map) = parser_view(source, tokens);
    let view_tokens = match crate::lexer::tokenize(&view) {
        Ok(tokens) => tokens,
        Err(error) => {
            let view_pos = view_offset(&view, &line_starts(&view), error.span.line, error.span.col);
            let source_pos = map.get(view_pos).copied().map(|pos| pos as usize).unwrap_or(source.len()).min(source.len());
            return ValidationFeedback {
                names: Vec::new(),
                error: Some(ValidationError { message: error.message, pos: source_pos, lexical: true }),
                warnings: Vec::new(),
            };
        }
    };
    let (result, names) = crate::parser::parse_with_tokenizer_feedback(&view_tokens);
    match result {
        Ok(_) => {
            let error = reserved_list_error(source, tokens);
            let names = if error.is_some() || names.is_empty() {
                Vec::new()
            } else {
                mapped_names(&view, &map, &line_starts(&view), names, source)
            };
            ValidationFeedback {
                names,
                error,
                warnings: declare_encoding_warnings(source, tokens),
            }
        },
        Err(error) => {
            let mapped = parser_error(&view, &map, &line_starts(&view), source, tokens, &error);
            let error = unclosed_delimiter_error(source, tokens, mapped.pos, &error.message)
                .or_else(|| diagnostic_override(source, tokens, &mapped, &error.message))
                .unwrap_or(mapped);
            ValidationFeedback {
                names: Vec::new(),
                error: Some(error),
                warnings: Vec::new(),
            }
        },
    }
}

/// Returns `Unclosed '<delimiter>'` when the parser failed while an opener was still open.
///
/// Zend reports the innermost unmatched `(`/`[`/`{` (and `#[` as `[`) instead of a generic
/// end-of-file or token error. The override only applies when the failure is at EOF or the
/// opener is followed by EOF inside a construct that can legally continue, so a genuinely
/// invalid token inside the opener keeps its own diagnostic.
#[inline(never)]
fn unclosed_delimiter_error<T: SourceToken>(
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
fn unmatched_open_delimiter<T: SourceToken>(tokens: &[T]) -> Option<(usize, char)> {
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
fn open_ended_construct<T: SourceToken>(tokens: &[T], index: usize) -> bool {
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
fn diagnostic_override<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
    mapped: &ValidationError,
    raw_message: &str,
) -> Option<ValidationError> {
    if let Some(keyword) = raw_message.strip_prefix("Unexpected token at statement position: ") {
        let keyword = keyword.trim().to_ascii_lowercase();
        if matches!(keyword.as_str(), "catch" | "else" | "elseif" | "finally" | "dot") {
            return Some(with_expectation(mapped.clone(), "end of file"));
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
    if raw_message.contains("Expected trait use, property, or method declaration in class body") {
        if let Some(pos) = set_marker_close_paren(tokens, mapped.pos) {
            return Some(ValidationError {
                message: "syntax error, unexpected token \")\", expecting token \"&\"".into(),
                pos,
                lexical: false,
            });
        }
    }
    if raw_message == "array must be followed by '('" {
        return Some(ValidationError {
            ..with_expectation(mapped.clone(), "\"(\"")
        });
    }
    if raw_message == "Expected parameter variable" {
        if let Some(pos) = first_token_text_after(source, tokens, mapped.pos, "|") {
            return Some(ValidationError {
                message: "syntax error, unexpected token \"|\", expecting variable".into(),
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
fn eof_override<T: SourceToken>(
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
        let last = tokens
            .iter()
            .rfind(|token| token.pos() < mapped.pos.min(source.len()));
        match last.map(|token| token.id()) {
            Some(id) if id == tok::T_RETURN => Some("\";\""),
            _ => None,
        }
    } else if raw_message.starts_with("Expected class name after 'new'") {
        Some("\"class\"")
    } else if raw_message.starts_with("Expected namespace name after 'namespace'") {
        Some("\"{\"")
    } else if raw_message.starts_with("Expected imported name after 'use'") {
        None
    } else if raw_message == "Expected ';'"
        && tokens
            .iter()
            .rev()
            .find(|token| {
                !is_trivia(token.id()) && token.pos() <= mapped.pos.min(source.len())
            })
            .is_some_and(|token| {
                token.id() == tok::T_ENUM
                    || &source[token.pos()..token.pos() + token.len()] == b"enum"
            })
    {
        None
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
fn expected_literal(message: &str) -> Option<&str> {
    let rest = message.strip_prefix("Expected '")?;
    let end = rest.find('\'')?;
    Some(&rest[..end])
}

/// Returns the byte offset of the first token with exactly `text` at or after `from`.
#[inline(never)]
fn first_token_text_after<T: SourceToken>(
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
fn first_assignment_operator<T: SourceToken>(
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
fn set_marker_close_paren<T: SourceToken>(tokens: &[T], from: usize) -> Option<usize> {
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

/// Rejects an unterminated backtick shell string with Zend's delimiter expectation.
#[inline(never)]
fn unterminated_backtick_error<T: SourceToken>(
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
fn goto_statement_error<T: SourceToken>(source: &[u8], tokens: &[T]) -> Option<ValidationError> {
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
fn goto_is_statement<T: SourceToken>(tokens: &[T], index: usize) -> bool {
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

/// Builds an end-of-file diagnostic with an optional expected-token suffix.
#[inline(never)]
fn end_of_file_expecting(pos: usize, expecting: Option<&str>) -> ValidationError {
    let message = match expecting {
        Some(expecting) => format!("syntax error, unexpected end of file, expecting {expecting}"),
        None => "syntax error, unexpected end of file".into(),
    };
    ValidationError { message, pos, lexical: false }
}

/// Builds an end-of-file diagnostic whose expected token is a quoted literal.
#[inline(never)]
fn end_of_file_literal(pos: usize, literal: &str) -> ValidationError {
    end_of_file_expecting(pos, Some(&format!("\"{literal}\"")))
}

/// Builds `syntax error, unexpected token "<text>"` with an optional expectation.
#[inline(never)]
fn unexpected_token_at(
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
fn unexpected_noun_at(
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

/// Appends `, expecting ...` to an existing diagnostic.
#[inline(never)]
fn with_expectation(mut error: ValidationError, expecting: &str) -> ValidationError {
    error.message = format!("{}, expecting {expecting}", error.message);
    error
}

/// Returns whether a `T_HALT_COMPILER` token is a member/namespace name segment.
#[inline(never)]
fn halt_compiler_is_name<T: SourceToken>(tokens: &[T], index: usize) -> bool {
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

/// Rejects `new $class::Name` and `new $class::{...}` class-name chains.
///
/// PHP only accepts `new $class::$property(...)`; the parser view rewrites other dynamic
/// `::` receivers to `->`, which would hide the distinction.
#[inline(never)]
fn new_dynamic_static_error<T: SourceToken>(source: &[u8], tokens: &[T]) -> Option<ValidationError> {
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
fn declare_encoding_warnings<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
) -> Vec<ValidationWarning> {
    let mut warnings = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        if token.id() != tok::T_DECLARE {
            continue;
        }
        let Some(open) = next_significant_index(tokens, index) else { continue; };
        if tokens[open].id() != i64::from(b'(') {
            continue;
        }
        let Some(name) = next_significant_index(tokens, open) else { continue; };
        if !source[tokens[name].pos()..tokens[name].pos() + tokens[name].len()]
            .eq_ignore_ascii_case(b"encoding")
        {
            continue;
        }
        let Some(equals) = next_significant_index(tokens, name) else { continue; };
        if tokens[equals].id() != i64::from(b'=') {
            continue;
        }
        warnings.push(ValidationWarning {
            message: "declare(encoding=...) ignored because Zend multibyte feature is turned off by settings".into(),
            pos: token.pos(),
        });
    }
    warnings
}

/// Rejects an `else`/`elseif` that follows a close tag outside a braced or alternative body.
///
/// PHP ends the whole `if` statement at `?>`, so `if (1) ?>x<?php else ...` is a parse error,
/// while `if (1): ?>x<?php else:` and `if (1) { ?>x<?php } else` are valid.
#[inline(never)]
fn close_tag_else_error<T: SourceToken>(source: &[u8], tokens: &[T]) -> Option<ValidationError> {
    for index in 0..tokens.len() {
        if tokens[index].id() != tok::T_CLOSE_TAG {
            continue;
        }
        let Some(next) = next_non_inline_html_index(tokens, index) else { continue; };
        let next_id = tokens[next].id();
        if next_id != tok::T_ELSE && next_id != tok::T_ELSEIF {
            continue;
        }
        let previous = previous_significant_index(tokens, index);
        if previous.is_some_and(|previous| tokens[previous].id() == i64::from(b':')) {
            continue;
        }
        return Some(unexpected_token_at(
            source,
            tokens[next].pos(),
            tokens[next].len(),
            Some("end of file"),
        ));
    }
    None
}

/// Returns the next token after `index`, skipping inline HTML, open tags and trivia.
#[inline(never)]
fn next_non_inline_html_index<T: SourceToken>(tokens: &[T], index: usize) -> Option<usize> {
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

/// Rejects `__halt_compiler()` outside the outermost file scope.
///
/// Zend's parser only accepts it as a top-level statement; inside a block, function, class or
/// unbraced control body it is `syntax error, unexpected token "__halt_compiler"`.
#[inline(never)]
fn halt_compiler_scope_error<T: SourceToken>(source: &[u8], tokens: &[T]) -> Option<ValidationError> {
    let mut depth = 0usize;
    for index in 0..tokens.len() {
        let id = tokens[index].id();
        if id == i64::from(b'{')
            || id == tok::T_CURLY_OPEN
            || id == tok::T_DOLLAR_OPEN_CURLY_BRACES
        {
            depth += 1;
        } else if id == i64::from(b'}') {
            depth = depth.saturating_sub(1);
        } else if id == tok::T_HALT_COMPILER {
            let previous = previous_significant_index(tokens, index);
            // Only member access and namespace separators treat the word as a name.
            if previous.is_some_and(|previous| {
                matches!(
                    tokens[previous].id(),
                    token_id if token_id == tok::T_OBJECT_OPERATOR
                        || token_id == tok::T_NULLSAFE_OBJECT_OPERATOR
                        || token_id == i64::from(b'\\')
                        || token_id == tok::T_NS_SEPARATOR
                )
            }) {
                continue;
            }
            let top_level_previous = previous.is_none_or(|previous| {
                matches!(
                    tokens[previous].id(),
                    id if id == i64::from(b';')
                        || id == i64::from(b'{')
                        || id == i64::from(b'}')
                        || id == tok::T_OPEN_TAG
                )
            });
            if depth == 0 && top_level_previous {
                continue;
            }
            return Some(unexpected_token_at(
                source,
                tokens[index].pos(),
                tokens[index].len(),
                None,
            ));
        }
    }
    None
}

/// Builds the PHP-only parser view and a view-byte to source-byte map.
fn parser_view<T: SourceToken>(source: &[u8], tokens: &[T]) -> (String, Vec<u32>) {
    let mut view = Vec::with_capacity(source.len() + 8);
    let mut map = Vec::with_capacity(source.len() + 8);
    append_mapped(&mut view, &mut map, b"<?php ", 0);
    let mut index = 0;
    let mut previous_significant = 0i64;
    let mut blank_indices = HashSet::new();
    mark_parser_view_adjustments(tokens, &mut blank_indices);
    while index < tokens.len() {
        let token = &tokens[index];
        let text = &source[token.pos()..token.pos() + token.len()];
        if blank_indices.remove(&index) {
            append_blank(&mut view, &mut map, text, token.pos());
            index += 1;
            continue;
        }
        if is_reference_ampersand(tokens, index, previous_significant) {
            append_mapped(&mut view, &mut map, b" ", token.pos());
            index += 1;
            continue;
        }
        // Zend emits T_GOTO for the keyword, but PHP also allows `goto` as a member,
        // constant, enum-case and named-argument name. A real goto statement is the
        // shape `goto <label> ;` (a close tag also terminates it).
        if token.id() == tok::T_GOTO
            && next_significant_index(tokens, index).is_some_and(|label| {
                tokens[label].id() == tok::T_STRING
                    && next_significant_index(tokens, label).is_some_and(|end| {
                        tokens[end].id() == i64::from(b';') || tokens[end].id() == tok::T_CLOSE_TAG
                    })
            })
        {
            let mut end = index;
            while end < tokens.len() && tokens[end].id() != i64::from(b';') {
                end += 1;
            }
            if end < tokens.len() {
                end += 1;
            }
            for skipped in &tokens[index..end] {
                let text = &source[skipped.pos()..skipped.pos() + skipped.len()];
                append_blank(&mut view, &mut map, text, skipped.pos());
            }
            index = end;
            previous_significant = i64::from(b';');
            continue;
        }
        if is_label_declaration(tokens, index, previous_significant) {
            let text = &source[token.pos()..token.pos() + token.len()];
            append_blank(&mut view, &mut map, text, token.pos());
            let colon = next_significant_index(tokens, index).expect("label colon follows label");
            append_blank(&mut view, &mut map, b":", tokens[colon].pos());
            index = colon + 1;
            continue;
        }
        if token.id() == i64::from(b'$')
            && next_significant_index(tokens, index).is_some_and(|next| {
                let id = tokens[next].id();
                id == i64::from(b'$')
                    || id == tok::T_VARIABLE
                    || id == tok::T_DOLLAR_OPEN_CURLY_BRACES
            })
        {
            append_blank(&mut view, &mut map, b"$", token.pos());
            index += 1;
            continue;
        }
        if token.id() == i64::from(b'$')
            && next_significant_index(tokens, index).is_some_and(|next| tokens[next].id() == i64::from(b'{'))
        {
            append_mapped(&mut view, &mut map, b"$x", token.pos());
            let brace = next_significant_index(tokens, index).unwrap();
            append_blank(&mut view, &mut map, b"{", tokens[brace].pos());
            let mut end = brace + 1;
            let mut depth = 1usize;
            while end < tokens.len() {
                if tokens[end].id() == i64::from(b'{') {
                    depth += 1;
                } else if tokens[end].id() == i64::from(b'}') {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                end += 1;
            }
            for skipped in &tokens[brace + 1..end.min(tokens.len())] {
                let text = &source[skipped.pos()..skipped.pos() + skipped.len()];
                append_blank(&mut view, &mut map, text, skipped.pos());
            }
            if end < tokens.len() {
                append_blank(&mut view, &mut map, b"}", tokens[end].pos());
            }
            index = end.saturating_add(1);
            continue;
        }
        if token.id() == tok::T_DOLLAR_OPEN_CURLY_BRACES {
            append_mapped(&mut view, &mut map, b"$x", token.pos());
            let mut end = index + 1;
            let mut depth = 1usize;
            while end < tokens.len() && depth > 0 {
                if tokens[end].id() == i64::from(b'{') {
                    depth += 1;
                } else if tokens[end].id() == i64::from(b'}') {
                    depth -= 1;
                }
                if depth == 0 {
                    break;
                }
                end += 1;
            }
            for skipped in &tokens[index + 1..end.min(tokens.len())] {
                let text = &source[skipped.pos()..skipped.pos() + skipped.len()];
                append_blank(&mut view, &mut map, text, skipped.pos());
            }
            if end < tokens.len() {
                append_blank(&mut view, &mut map, b"}", tokens[end].pos());
            }
            index = end.saturating_add(1);
            continue;
        }
        if token.id() == tok::T_ELLIPSIS
            && matches!(
                previous_significant,
                t if t == i64::from(b')')
                    || t == i64::from(b']')
                    || t == i64::from(b'(')
                    || t == tok::T_VARIABLE
            )
            && next_significant_index(tokens, index)
                .is_some_and(|next| tokens[next].id() == i64::from(b')'))
        {
            append_blank(&mut view, &mut map, b"...", token.pos());
            index += 1;
            continue;
        }
        if token.id() == tok::T_DOUBLE_COLON {
            let Some(previous) = previous_significant_index(tokens, index) else {
                append_mapped(&mut view, &mut map, b"::", token.pos());
                index += 1;
                continue;
            };
            let before_previous = previous_significant_index(tokens, previous);
            let chained = before_previous.is_some_and(|before| {
                let id = tokens[before].id();
                id == tok::T_DOUBLE_COLON
                    || id == tok::T_OBJECT_OPERATOR
                    || id == tok::T_NULLSAFE_OBJECT_OPERATOR
            });
            if !chained
                && tokens[previous].id() == tok::T_VARIABLE
                && next_significant_index(tokens, index)
                    .is_some_and(|next| tokens[next].id() != tok::T_CLASS)
            {
                append_mapped(&mut view, &mut map, b"->", token.pos());
                index += 1;
                continue;
            }
            if chained && tokens[previous].id() == tok::T_VARIABLE {
                append_blank(&mut view, &mut map, b"::", token.pos());
                if let Some(next) = next_significant_index(tokens, index) {
                    if tokens[next].id() == tok::T_VARIABLE || tokens[next].id() == tok::T_STRING {
                        let text = &source[tokens[next].pos()..tokens[next].pos() + tokens[next].len()];
                        append_blank(&mut view, &mut map, text, tokens[next].pos());
                        index = next + 1;
                        continue;
                    }
                }
                index += 1;
                continue;
            }
            if chained {
                append_mapped(&mut view, &mut map, b"->", token.pos());
                index += 1;
                continue;
            }
        }
        if token.id() == i64::from(b'{')
            && previous_significant == tok::T_DOUBLE_COLON
        {
            append_blank(&mut view, &mut map, b"{", token.pos());
            let mut end = index + 1;
            let mut depth = 1usize;
            while end < tokens.len() && depth > 0 {
                if tokens[end].id() == i64::from(b'{') {
                    depth += 1;
                } else if tokens[end].id() == i64::from(b'}') {
                    depth -= 1;
                }
                if depth == 0 {
                    break;
                }
                let text = &source[tokens[end].pos()..tokens[end].pos() + tokens[end].len()];
                if tokens[end].id() == tok::T_CONSTANT_ENCAPSED_STRING
                    && text.len() >= 2
                {
                    let mut normalized = text.to_vec();
                    normalized[0] = b' ';
                    let last = normalized.len() - 1;
                    normalized[last] = b' ';
                    append_mapped(&mut view, &mut map, &normalized, tokens[end].pos());
                } else {
                    append_mapped(&mut view, &mut map, text, tokens[end].pos());
                }
                end += 1;
            }
            if end < tokens.len() {
                append_blank(&mut view, &mut map, b"}", tokens[end].pos());
            }
            index = end.saturating_add(1);
            continue;
        }
        if token.id() == i64::from(b'`') {
            index = append_backtick_string(&mut view, &mut map, source, tokens, index);
            previous_significant = token.id();
            index += 1;
            continue;
        }
        if token.id() == tok::T_OPEN_TAG || token.id() == tok::T_INLINE_HTML {
            append_blank(&mut view, &mut map, text, token.pos());
        } else if token.id() == tok::T_OPEN_TAG_WITH_ECHO {
            append_mapped(&mut view, &mut map, b"echo ", token.pos());
        } else if token.id() == tok::T_CLOSE_TAG {
            append_mapped(&mut view, &mut map, b";", token.pos());
            append_blank(&mut view, &mut map, text, token.pos());
        } else if token.id() == tok::T_HALT_COMPILER
            && !halt_compiler_is_name(tokens, index)
        {
            index = skip_halt_compiler(&mut view, &mut map, source, tokens, index);
        } else if matches!(token.id(), tok::T_UNSET_CAST | tok::T_VOID_CAST | tok::T_STRING_CAST) {
            append_safe_cast(&mut view, &mut map, text, token.pos());
        } else if token.id() == tok::T_CONSTANT_ENCAPSED_STRING {
            append_string(&mut view, &mut map, text, token.pos());
        } else if token.id() == tok::T_DNUMBER && text.ends_with(b".") {
            let mut normalized = text.to_vec();
            if let Some(last) = normalized.last_mut() {
                *last = b' ';
            }
            append_mapped(&mut view, &mut map, &normalized, token.pos());
        } else if token.id() == tok::T_START_HEREDOC
            && text.len() >= 4
            && matches!(text[0], b'b' | b'B')
            && &text[1..4] == b"<<<"
        {
            let mut normalized = text.to_vec();
            normalized[0] = b' ';
            append_mapped(&mut view, &mut map, &normalized, token.pos());
        } else if token.id() == tok::T_NUM_STRING && !text.iter().all(u8::is_ascii_digit) {
            let normalized = vec![b'0'; text.len()];
            append_mapped(&mut view, &mut map, &normalized, token.pos());
        } else if token.id() == tok::T_STRING && text.eq_ignore_ascii_case(b"enum") {
            append_mapped(&mut view, &mut map, b"zenum", token.pos());
        } else if text.len() >= 2
            && matches!(text[0], b'b' | b'B')
            && matches!(text[1], b'\'' | b'"')
        {
            let mut normalized = text.to_vec();
            normalized[0] = b' ';
            append_mapped(&mut view, &mut map, &normalized, token.pos());
        } else {
            append_mapped(&mut view, &mut map, text, token.pos());
        }
        if !is_trivia_token(token.id()) {
            previous_significant = token.id();
        }
        index += 1;
    }
    normalize_parse_newlines(&mut view);
    (sanitize_view(view), map)
}

/// Replaces each invalid UTF-8 byte with one ASCII placeholder, preserving byte offsets.
fn sanitize_view(view: Vec<u8>) -> String {
    match String::from_utf8(view) {
        Ok(text) => return text,
        Err(error) => return sanitize_invalid_utf8(error.into_bytes()),
    }
}

/// Replaces invalid UTF-8 bytes with ASCII placeholders while preserving byte offsets.
fn sanitize_invalid_utf8(view: Vec<u8>) -> String {
    let mut out = String::with_capacity(view.len());
    let mut offset = 0;
    while offset < view.len() {
        match std::str::from_utf8(&view[offset..]) {
            Ok(valid) => {
                out.push_str(valid);
                break;
            }
            Err(error) => {
                let valid = error.valid_up_to();
                out.push_str(std::str::from_utf8(&view[offset..offset + valid])
                    .expect("valid_up_to always ends on a UTF-8 boundary"));
                let invalid = error.error_len().unwrap_or(view.len() - offset - valid);
                for _ in 0..invalid {
                    out.push('x');
                }
                offset += valid + invalid;
            }
        }
    }
    out
}

/// Marks every parser-view normalization that needs surrounding context in one token pass.
///
/// Marks the static multi-declaration and abstract-property normalizations in one pass over
/// the source tokens.
fn mark_parser_view_adjustments<T: SourceToken>(tokens: &[T], blank: &mut HashSet<usize>) {
    for (index, token) in tokens.iter().enumerate() {
        let id = token.id();
        if id == tok::T_STATIC {
            mark_static_multi_declaration(tokens, index, blank);
        } else if id == tok::T_ABSTRACT {
            mark_abstract_property(tokens, index, blank);
        }
    }
}

/// Marks the declarations after the first variable in one `static $a = ..., $b;` statement.
fn mark_static_multi_declaration<T: SourceToken>(
    tokens: &[T],
    index: usize,
    blank: &mut HashSet<usize>,
) {
    let Some(next) = next_significant_index(tokens, index) else { return; };
    if tokens[next].id() != tok::T_VARIABLE {
        return;
    }
    let mut depth = 0usize;
    let mut comma = None;
    let mut cursor = next + 1;
    while cursor < tokens.len() {
        match tokens[cursor].id() {
            id if id == i64::from(b'(') || id == i64::from(b'[') || id == i64::from(b'{') => depth += 1,
            id if id == i64::from(b')') || id == i64::from(b']') || id == i64::from(b'}') => depth = depth.saturating_sub(1),
            id if id == i64::from(b',') && depth == 0 => { comma = Some(cursor); break; }
            id if id == i64::from(b';') && depth == 0 => break,
            _ => {}
        }
        cursor += 1;
    }
    if comma.is_none() {
        return;
    }
    while cursor < tokens.len() && tokens[cursor].id() != i64::from(b';') {
        blank.insert(cursor);
        cursor += 1;
    }
}

/// Marks one abstract property-hook declaration the parser does not model.
fn mark_abstract_property<T: SourceToken>(
    tokens: &[T],
    index: usize,
    blank: &mut HashSet<usize>,
) {
    // `abstract class` / `abstract interface` is a class-like modifier, not a property.
    if next_significant_index(tokens, index).is_some_and(|next| {
        matches!(
            tokens[next].id(),
            id if id == tok::T_CLASS || id == tok::T_INTERFACE || id == tok::T_TRAIT || id == tok::T_ENUM
        )
    }) {
        return;
    }
    let mut cursor = index + 1;
    while cursor < tokens.len() {
        let id = tokens[cursor].id();
        if id == tok::T_VARIABLE {
            break;
        }
        if [tok::T_FUNCTION, tok::T_FN, i64::from(b';')].contains(&id) {
            cursor = tokens.len();
            break;
        }
        cursor += 1;
    }
    if cursor >= tokens.len() || tokens[cursor].id() != tok::T_VARIABLE {
        return;
    }
    while cursor < tokens.len() && tokens[cursor].id() != i64::from(b'{') {
        if tokens[cursor].id() == i64::from(b';') {
            break;
        }
        cursor += 1;
    }
    if cursor >= tokens.len() || tokens[cursor].id() != i64::from(b'{') {
        return;
    }
    let Some(close) = matching_brace(tokens, cursor) else { return; };
    let start = previous_significant_index(tokens, index).filter(|previous| {
        matches!(
            tokens[*previous].id(),
            id if id == tok::T_PUBLIC
                || id == tok::T_PROTECTED
                || id == tok::T_PRIVATE
                || id == tok::T_STATIC
                || id == tok::T_READONLY
        )
    }).unwrap_or(index);
    for skipped in start..=close {
        blank.insert(skipped);
    }
}

/// Finds the closing brace matching the opening brace at `open`.
fn matching_brace<T: SourceToken>(tokens: &[T], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for index in open..tokens.len() {
        if tokens[index].id() == i64::from(b'{') {
            depth += 1;
        } else if tokens[index].id() == i64::from(b'}') {
            depth -= 1;
            if depth == 0 {
                return Some(index);
            }
        }
    }
    None
}

/// Returns the next non-trivia token index after `index`.
fn next_significant_index<T: SourceToken>(tokens: &[T], index: usize) -> Option<usize> {
    let mut cursor = index + 1;
    while cursor < tokens.len() {
        if !is_trivia_token(tokens[cursor].id()) {
            return Some(cursor);
        }
        cursor += 1;
    }
    None
}

/// Returns whether the token after a reference `&` can begin a referenceable expression.
///
/// PHP allows `$a = &$b`, `$a = &$b[0]`, and `$a = &foo()`, but not `$a = &1` or
/// `$a = &new C`. Keeping the ampersand in the view for the latter lets the parser reject it.
fn reference_target_starts_at<T: SourceToken>(tokens: &[T], index: usize) -> bool {
    let Some(next) = next_significant_index(tokens, index) else { return false; };
    match tokens[next].id() {
        id if id == tok::T_VARIABLE => true,
        id if id == tok::T_STATIC => next_significant_index(tokens, next)
            .is_some_and(|after| tokens[after].id() == tok::T_DOUBLE_COLON),
        id if id == tok::T_STRING || id == tok::T_IDENTIFIER => {
            next_significant_index(tokens, next).is_some_and(|after| {
                matches!(
                    tokens[after].id(),
                    id if id == i64::from(b'(')
                        || id == i64::from(b'[')
                        || id == i64::from(b'\\')
                        || id == tok::T_DOUBLE_COLON
                        || id == tok::T_OBJECT_OPERATOR
                        || id == tok::T_NULLSAFE_OBJECT_OPERATOR
                )
            })
        }
        _ => false,
    }
}

/// Returns whether the token at `index` is a statement label (`name:`).
fn is_label_declaration<T: SourceToken>(tokens: &[T], index: usize, previous_significant: i64) -> bool {
    let Some(next) = next_significant_index(tokens, index) else {
        return false;
    };
    if tokens[next].id() != i64::from(b':') {
        return false;
    }
    if previous_significant != 0
        && previous_significant != i64::from(b';')
        && previous_significant != i64::from(b'{')
        && previous_significant != i64::from(b'}')
        && previous_significant != i64::from(b')')
        && previous_significant != i64::from(b':')
        && previous_significant != tok::T_OPEN_TAG
        && previous_significant != tok::T_CLOSE_TAG
        && previous_significant != tok::T_INLINE_HTML
        && previous_significant != tok::T_ELSE
        && previous_significant != tok::T_DO
        && previous_significant != tok::T_TRY
        && previous_significant != tok::T_FINALLY
    {
        return false;
    }
    tokens[index].id() == tok::T_STRING
}

/// Rejects `list` where PHP's grammar reserves it, while keeping member/declaration uses.
///
/// elephc's lexer keeps `list` as an identifier, so the parser accepts it in expression and
/// declaration positions where reference PHP reports a ParseError. The allowed contexts below
/// are exactly the semi-reserved ones: member names, method/class-constant names, enum cases,
/// named arguments, trait adaptations, and the `list(...)` construct.
fn reserved_list_error<T: SourceToken>(source: &[u8], tokens: &[T]) -> Option<ValidationError> {
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
            return Some(unexpected_token_at(source, token.pos(), token.len(), None));
        }
    }
    None
}

/// Precomputes the nearest open brace for every token in one forward pass.
///
/// `u32::MAX` means "no enclosing brace". The engine caps source length at `u32::MAX`, so a
/// token index always fits.
fn enclosing_braces<T: SourceToken>(tokens: &[T]) -> Vec<u32> {
    let mut stack: Vec<u32> = Vec::new();
    let mut enclosing = Vec::with_capacity(tokens.len());
    for (index, token) in tokens.iter().enumerate() {
        enclosing.push(stack.last().copied().unwrap_or(u32::MAX));
        if token.id() == i64::from(b'{') {
            stack.push(u32::try_from(index).unwrap_or(u32::MAX));
        } else if token.id() == i64::from(b'}') {
            stack.pop();
        }
    }
    enclosing
}

/// Returns whether one precomputed enclosing brace opens a class-like body.
fn enclosing_brace_is_class<T: SourceToken>(tokens: &[T], open: usize) -> bool {
    let mut cursor = open;
    while cursor > 0 {
        cursor -= 1;
        let id = tokens[cursor].id();
        if [
            tok::T_CLASS,
            tok::T_INTERFACE,
            tok::T_TRAIT,
            tok::T_ENUM,
        ]
        .contains(&id)
        {
            return true;
        }
        if [tok::T_FUNCTION, tok::T_FN].contains(&id)
            || [i64::from(b';'), i64::from(b'{'), i64::from(b'}')].contains(&id)
        {
            return false;
        }
    }
    false
}

/// Converts a lone CR into LF so elephc's line-based parser sees Zend's line breaks.
fn normalize_parse_newlines(view: &mut [u8]) {
    for index in 0..view.len() {
        if view[index] == b'\r' && view.get(index + 1) != Some(&b'\n') {
            view[index] = b'\n';
        }
    }
}

/// Returns whether one ampersand is a PHP reference marker the parser view should remove.
///
/// Only declaration, capture, foreach and literal contexts accept a reference marker. A call
/// argument such as `foo(&$a)` is a PHP ParseError, so its `&` must stay in the view and let the
/// parser reject it. The same rule keeps `($a & $b)` as the bitwise operator.
fn is_reference_ampersand<T: SourceToken>(tokens: &[T], index: usize, previous_significant: i64) -> bool {
    let ampersand = tokens[index].id() == tok::T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG
        || tokens[index].id() == tok::T_AMPERSAND_NOT_FOLLOWED_BY_VAR_OR_VARARG;
    if !ampersand {
        return false;
    }
    if previous_significant == i64::from(b'=') {
        return reference_target_starts_at(tokens, index);
    }
    if [
        tok::T_DOUBLE_ARROW,
        tok::T_FUNCTION,
        tok::T_AS,
        tok::T_RETURN,
    ]
    .contains(&previous_significant)
        || previous_significant == i64::from(b'[')
    {
        return true;
    }
    if previous_significant != i64::from(b'(') && previous_significant != i64::from(b',')
        && previous_significant != i64::from(b'[')
    {
        return false;
    }
    if let Some(open) = enclosing_bracket(tokens, index) {
        if let Some(close) = matching_bracket(tokens, open) {
            let after = next_significant_index(tokens, close);
            let assigns = after.is_some_and(|after| tokens[after].id() == i64::from(b'='));
            let foreach_target = previous_significant_index(tokens, open)
                .is_some_and(|before| tokens[before].id() == tok::T_AS);
            if assigns || foreach_target {
                return true;
            }
        }
    }
    let Some(open) = enclosing_paren(tokens, index) else {
        return false;
    };
    let before = previous_significant_index(tokens, open);
    let Some(before) = before else {
        return false;
    };
    if [
        tok::T_FUNCTION,
        tok::T_FN,
        tok::T_USE,
        tok::T_ARRAY,
        tok::T_LIST,
    ]
    .contains(&tokens[before].id())
    {
        return true;
    }
    // A named function or method declaration: `function foo(&$x)`.
    if tokens[before].id() == tok::T_STRING
        || tokens[before].id() == tok::T_IDENTIFIER
    {
        return previous_significant_index(tokens, before)
            .is_some_and(|prior| tokens[prior].id() == tok::T_FUNCTION);
    }
    false
}

/// Returns the nearest enclosing bracket for one token, if any.
fn enclosing_bracket<T: SourceToken>(tokens: &[T], index: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut cursor = index;
    while cursor > 0 {
        cursor -= 1;
        if tokens[cursor].id() == i64::from(b']') {
            depth += 1;
        } else if tokens[cursor].id() == i64::from(b'[') {
            if depth == 0 {
                return Some(cursor);
            }
            depth -= 1;
        }
    }
    None
}

/// Finds the closing bracket matching the opening bracket at `open`.
fn matching_bracket<T: SourceToken>(tokens: &[T], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for index in open..tokens.len() {
        if tokens[index].id() == i64::from(b'[') {
            depth += 1;
        } else if tokens[index].id() == i64::from(b']') {
            depth -= 1;
            if depth == 0 {
                return Some(index);
            }
        }
    }
    None
}

/// Finds the opening parenthesis that encloses the token at `index`.
fn enclosing_paren<T: SourceToken>(tokens: &[T], index: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut cursor = index;
    while cursor > 0 {
        cursor -= 1;
        let id = tokens[cursor].id();
        if id == i64::from(b')') {
            depth += 1;
        } else if id == i64::from(b'(') {
            if depth == 0 {
                return Some(cursor);
            }
            depth -= 1;
        }
    }
    None
}

/// Returns the index of the previous non-trivia token before `index`.
fn previous_significant_index<T: SourceToken>(tokens: &[T], index: usize) -> Option<usize> {
    let mut cursor = index;
    while cursor > 0 {
        cursor -= 1;
        if !is_trivia_token(tokens[cursor].id()) {
            return Some(cursor);
        }
    }
    None
}

/// Returns whether a token is trivia for previous-significant-token tracking.
fn is_trivia_token(id: i64) -> bool {
    id == tok::T_WHITESPACE || id == tok::T_COMMENT || id == tok::T_DOC_COMMENT
}

/// Appends a backtick expression as a plain double-quoted string for parser validation.
fn append_backtick_string<T: SourceToken>(
    view: &mut Vec<u8>,
    map: &mut Vec<u32>,
    source: &[u8],
    tokens: &[T],
    open: usize,
) -> usize {
    let close = tokens[open + 1..]
        .iter()
        .position(|token| token.id() == i64::from(b'`'))
        .map(|offset| open + 1 + offset);
    append_mapped(view, map, b"\"", tokens[open].pos());
    let end = close.unwrap_or(tokens.len());
    for token in &tokens[open + 1..end] {
        let text = &source[token.pos()..token.pos() + token.len()];
        for (offset, byte) in text.iter().copied().enumerate() {
            view.push(if matches!(byte, b'\n' | b'\r') {
                byte
            } else if matches!(byte, b'"' | b'\\') {
                b' '
            } else {
                byte
            });
            map.push(u32::try_from(token.pos() + offset).unwrap_or(u32::MAX));
        }
    }
    if let Some(close) = close {
        append_mapped(view, map, b"\"", tokens[close].pos());
        return close;
    }
    open
}

/// Appends a valid cast in the same number of view bytes, preserving offsets.
fn append_safe_cast(view: &mut Vec<u8>, map: &mut Vec<u32>, text: &[u8], source_pos: usize) {
    let safe = b"(int)";
    for (offset, byte) in text.iter().copied().enumerate() {
        view.push(if offset < safe.len() {
            safe[offset]
        } else if matches!(byte, b'\n' | b'\r') {
            byte
        } else {
            b' '
        });
        map.push(u32::try_from(source_pos + offset).unwrap_or(u32::MAX));
    }
}

/// Appends a string literal with any binary prefix blanked for parser validation.
fn append_string(view: &mut Vec<u8>, map: &mut Vec<u32>, text: &[u8], source_pos: usize) {
    if text.len() >= 2 && matches!(text[0], b'b' | b'B') && matches!(text[1], b'\'' | b'"') {
        let mut normalized = text.to_vec();
        normalized[0] = b' ';
        append_mapped(view, map, &normalized, source_pos);
    } else {
        append_mapped(view, map, text, source_pos);
    }
}

/// Blanks one token while preserving CR/LF bytes for line numbering.
fn append_blank(view: &mut Vec<u8>, map: &mut Vec<u32>, text: &[u8], source_pos: usize) {
    for (offset, byte) in text.iter().copied().enumerate() {
        view.push(if matches!(byte, b'\n' | b'\r') { byte } else { b' ' });
        map.push(u32::try_from(source_pos + offset).unwrap_or(u32::MAX));
    }
}

/// Appends bytes mapped to one source position.
fn append_mapped(view: &mut Vec<u8>, map: &mut Vec<u32>, bytes: &[u8], source_pos: usize) {
    view.extend_from_slice(bytes);
    map.resize(map.len() + bytes.len(), u32::try_from(source_pos).unwrap_or(u32::MAX));
}

/// Blanks a valid `__halt_compiler ( ) ;` header and returns the last consumed token index.
fn skip_halt_compiler<T: SourceToken>(
    view: &mut Vec<u8>,
    map: &mut Vec<u32>,
    source: &[u8],
    tokens: &[T],
    start: usize,
) -> usize {
    let mut index = start;
    let mut significant = 0;
    while index < tokens.len() {
        let token = &tokens[index];
        let text = &source[token.pos()..token.pos() + token.len()];
        append_blank(view, map, text, token.pos());
        if !is_trivia(token.id()) {
            significant += 1;
            if significant == 3 {
                return index;
            }
        }
        index += 1;
    }
    tokens.len().saturating_sub(1)
}

/// Returns identifier feedback mapped from parser view spans to original source offsets.
fn mapped_names(
    view: &str, map: &[u32], lines: &[usize], names: Vec<(Span, String)>, source: &[u8],
) -> Vec<usize> {
    let mut result: Vec<usize> = names
        .into_iter()
        .filter_map(|(span, spelling)| {
            let view_pos = view_offset(view, lines, span.line, span.col);
            let source_pos = map.get(view_pos).copied().map(|pos| pos as usize).unwrap_or(source.len()).min(source.len());
            let text = source.get(source_pos..)?;
            text.get(..spelling.len())
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case(spelling.as_bytes()))
                .then_some(source_pos)
        })
        .collect();
    result.sort_unstable();
    result.dedup();
    result
}

/// Builds a PHP-style parse error from the AST parser's first diagnostic.
fn parser_error<T: SourceToken>(
    view: &str,
    map: &[u32],
    lines: &[usize],
    source: &[u8],
    tokens: &[T],
    error: &CompileError,
) -> ValidationError {
    let view_pos = view_offset(view, lines, error.span.line, error.span.col);
    let source_pos = map.get(view_pos).copied().map(|pos| pos as usize).unwrap_or(source.len());
    if source_pos >= source.len() {
        return ValidationError { message: "syntax error, unexpected end of file".into(), pos: source.len(), lexical: false };
    }
    let token = tokens.iter().find(|token| token.pos() <= source_pos && source_pos < token.pos() + token.len());
    match token {
        None => end_of_file_expecting(source.len(), None),
        Some(token) => {
            let pos = token.pos();
            let len = token.len();
            match token.id() {
                id if id == tok::T_STRING || id == tok::T_IDENTIFIER => {
                    unexpected_noun_at(source, pos, len, "identifier", None)
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
                // Keywords, operators and punctuation all report their source spelling in
                // Zend's diagnostics; only variables, numbers and identifiers get the
                // dedicated noun.
                _ => unexpected_token_at(source, pos, len, None),
            }
        }
    }
}

/// Returns the byte offset of one one-based line/column in a UTF-8 parser view.
fn view_offset(view: &str, lines: &[usize], line: u32, col: u32) -> usize {
    let Some(&start) = lines.get(line.saturating_sub(1) as usize) else {
        return view.len();
    };
    if col <= 1 {
        return start;
    }
    view[start..]
        .char_indices()
        .nth((col - 1) as usize)
        .map_or(view.len(), |(offset, _)| start + offset)
}

/// Precomputes the byte offset of every one-based parser-view line.
fn line_starts(view: &str) -> Vec<usize> {
    let mut starts = vec![0];
    starts.extend(view.bytes().enumerate().filter_map(|(index, byte)| {
        (byte == b'\n').then_some(index + 1)
    }));
    starts
}

/// Returns whether a token is trivia for halt-compiler header accounting.
fn is_trivia(id: i64) -> bool {
    id == tok::T_WHITESPACE || id == tok::T_COMMENT || id == tok::T_DOC_COMMENT
}
