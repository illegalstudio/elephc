//! Purpose:
//! Provides TOKEN_PARSE syntax validation and Zend's semi-reserved-word feedback through
//! elephc's own lexer, parser and AST.
//!
//! Called from:
//! - `crate::syntax::php_tokenizer` after Chopper scans the source when PHP's TOKEN_PARSE flag is set.
//!
//! Key details:
//! - No separate PHP grammar lives here: the source is normalized into elephc's AST parser view.
//! - Inline HTML, open/close tags and `__halt_compiler` payloads are blanked while line numbers
//!   stay stable, so parser diagnostics keep PHP's source lines.
//! - Accepted keyword identifiers are mapped back to byte offsets for token reclassification.

use crate::scanner::HALT_COMPILER_OUTERMOST_SCOPE_MESSAGE;
use crate::syntax::errors::CompileError;
use crate::syntax::span::Span;
use std::cell::OnceCell;
use std::sync::OnceLock;

mod diagnostics;
mod parser_view;
mod prechecks;
mod token_view;

use diagnostics::*;
use parser_view::*;
use prechecks::*;

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
    pub const T_CATCH: i64 = id("T_CATCH");
    pub const T_CLASS: i64 = id("T_CLASS");
    pub const T_CLOSE_TAG: i64 = id("T_CLOSE_TAG");
    pub const T_COMMENT: i64 = id("T_COMMENT");
    pub const T_CALLABLE: i64 = id("T_CALLABLE");
    pub const T_CONST: i64 = id("T_CONST");
    pub const T_CONTINUE: i64 = id("T_CONTINUE");
    pub const T_CONSTANT_ENCAPSED_STRING: i64 = id("T_CONSTANT_ENCAPSED_STRING");
    pub const T_DNUMBER: i64 = id("T_DNUMBER");
    pub const T_LNUMBER: i64 = id("T_LNUMBER");
    pub const T_DOC_COMMENT: i64 = id("T_DOC_COMMENT");
    pub const T_DOLLAR_OPEN_CURLY_BRACES: i64 = id("T_DOLLAR_OPEN_CURLY_BRACES");
    pub const T_DOUBLE_ARROW: i64 = id("T_DOUBLE_ARROW");
    pub const T_DO: i64 = id("T_DO");
    pub const T_ECHO: i64 = id("T_ECHO");
    pub const T_EXIT: i64 = id("T_EXIT");
    pub const T_CURLY_OPEN: i64 = id("T_CURLY_OPEN");
    pub const T_DOUBLE_COLON: i64 = id("T_DOUBLE_COLON");
    pub const T_ATTRIBUTE: i64 = id("T_ATTRIBUTE");
    pub const T_DECLARE: i64 = id("T_DECLARE");
    pub const T_ENDDECLARE: i64 = id("T_ENDDECLARE");
    pub const T_ENDFOR: i64 = id("T_ENDFOR");
    pub const T_ENDFOREACH: i64 = id("T_ENDFOREACH");
    pub const T_ENDIF: i64 = id("T_ENDIF");
    pub const T_ENDSWITCH: i64 = id("T_ENDSWITCH");
    pub const T_ENDWHILE: i64 = id("T_ENDWHILE");
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
    pub const T_ENCAPSED_AND_WHITESPACE: i64 = id("T_ENCAPSED_AND_WHITESPACE");
    pub const T_END_HEREDOC: i64 = id("T_END_HEREDOC");
    pub const T_EXTENDS: i64 = id("T_EXTENDS");
    pub const T_FINAL: i64 = id("T_FINAL");
    pub const T_VAR: i64 = id("T_VAR");
    pub const T_PUBLIC_SET: i64 = id("T_PUBLIC_SET");
    pub const T_PROTECTED_SET: i64 = id("T_PROTECTED_SET");
    pub const T_PRIVATE_SET: i64 = id("T_PRIVATE_SET");
    pub const T_FN: i64 = id("T_FN");
    pub const T_FUNCTION: i64 = id("T_FUNCTION");
    pub const T_GOTO: i64 = id("T_GOTO");
    pub const T_GLOBAL: i64 = id("T_GLOBAL");
    pub const T_HALT_COMPILER: i64 = id("T_HALT_COMPILER");
    pub const T_INLINE_HTML: i64 = id("T_INLINE_HTML");
    pub const T_INTERFACE: i64 = id("T_INTERFACE");
    pub const T_LIST: i64 = id("T_LIST");
    pub const T_NAME_FULLY_QUALIFIED: i64 = id("T_NAME_FULLY_QUALIFIED");
    pub const T_NAME_QUALIFIED: i64 = id("T_NAME_QUALIFIED");
    pub const T_NAME_RELATIVE: i64 = id("T_NAME_RELATIVE");
    pub const T_NEW: i64 = id("T_NEW");
    pub const T_BREAK: i64 = id("T_BREAK");
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
    pub const T_NAMESPACE: i64 = id("T_NAMESPACE");
    pub const T_STRING: i64 = id("T_STRING");
    pub const T_INT_CAST: i64 = id("T_INT_CAST");
    pub const T_DOUBLE_CAST: i64 = id("T_DOUBLE_CAST");
    pub const T_BOOL_CAST: i64 = id("T_BOOL_CAST");
    pub const T_ARRAY_CAST: i64 = id("T_ARRAY_CAST");
    pub const T_OBJECT_CAST: i64 = id("T_OBJECT_CAST");
    pub const T_STRING_CAST: i64 = id("T_STRING_CAST");
    pub const T_STRING_VARNAME: i64 = id("T_STRING_VARNAME");
    pub const T_TRAIT: i64 = id("T_TRAIT");
    pub const T_UNSET_CAST: i64 = id("T_UNSET_CAST");
    pub const T_USE: i64 = id("T_USE");
    pub const T_UNSET: i64 = id("T_UNSET");
    pub const T_YIELD: i64 = id("T_YIELD");
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
    assert!(tok::T_CATCH != -1, "T_CATCH missing from the tokenizer catalog");
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
    assert!(tok::T_EXTENDS != -1, "T_EXTENDS missing from the tokenizer catalog");
    assert!(tok::T_FINAL != -1, "T_FINAL missing from the tokenizer catalog");
    assert!(tok::T_VAR != -1, "T_VAR missing from the tokenizer catalog");
    assert!(tok::T_PUBLIC_SET != -1, "T_PUBLIC_SET missing from the tokenizer catalog");
    assert!(tok::T_PROTECTED_SET != -1, "T_PROTECTED_SET missing from the tokenizer catalog");
    assert!(tok::T_PRIVATE_SET != -1, "T_PRIVATE_SET missing from the tokenizer catalog");
    assert!(tok::T_END_HEREDOC != -1, "T_END_HEREDOC missing from the tokenizer catalog");
    assert!(tok::T_ENDDECLARE != -1, "T_ENDDECLARE missing from the tokenizer catalog");
    assert!(tok::T_ENDFOR != -1, "T_ENDFOR missing from the tokenizer catalog");
    assert!(tok::T_ENDFOREACH != -1, "T_ENDFOREACH missing from the tokenizer catalog");
    assert!(tok::T_ENDIF != -1, "T_ENDIF missing from the tokenizer catalog");
    assert!(tok::T_ENDSWITCH != -1, "T_ENDSWITCH missing from the tokenizer catalog");
    assert!(tok::T_ENDWHILE != -1, "T_ENDWHILE missing from the tokenizer catalog");
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
    assert!(tok::T_NAME_FULLY_QUALIFIED != -1, "T_NAME_FULLY_QUALIFIED missing from the tokenizer catalog");
    assert!(tok::T_NAME_QUALIFIED != -1, "T_NAME_QUALIFIED missing from the tokenizer catalog");
    assert!(tok::T_NAME_RELATIVE != -1, "T_NAME_RELATIVE missing from the tokenizer catalog");
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
    assert!(tok::T_INT_CAST != -1, "T_INT_CAST missing from the tokenizer catalog");
    assert!(tok::T_DOUBLE_CAST != -1, "T_DOUBLE_CAST missing from the tokenizer catalog");
    assert!(tok::T_BOOL_CAST != -1, "T_BOOL_CAST missing from the tokenizer catalog");
    assert!(tok::T_ARRAY_CAST != -1, "T_ARRAY_CAST missing from the tokenizer catalog");
    assert!(tok::T_OBJECT_CAST != -1, "T_OBJECT_CAST missing from the tokenizer catalog");
    assert!(tok::T_STRING_CAST != -1, "T_STRING_CAST missing from the tokenizer catalog");
    assert!(tok::T_STRING_VARNAME != -1, "T_STRING_VARNAME missing from the tokenizer catalog");
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
    /// Returns the one-based source line when the embedder retains it.
    fn line(&self) -> usize {
        0
    }
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
    /// One-based source line.
    pub line: usize,
}

impl SourceToken for TokenView {
    /// Returns the PHP token identifier.
    fn id(&self) -> i64 { self.id }
    /// Returns the zero-based byte offset in the original source.
    fn pos(&self) -> usize { self.pos }
    /// Returns the byte length in the original source.
    fn len(&self) -> usize { self.len }
    /// Returns the one-based source line.
    fn line(&self) -> usize { self.line }
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
    /// PHP error level Zend raises for this diagnostic (`E_WARNING` = 2 or
    /// `E_DEPRECATED` = 8192).
    pub severity: i64,
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

/// A reference diagnostic found before its marker is removed from the parser view.
struct ReferenceError {
    diagnostic: ValidationError,
    target_pos: usize,
    /// A non-call `readonly` keyword can cause an artificial parser error here.
    readonly_without_call: bool,
}

impl ReferenceError {
    /// Prefers the reference location unless the parser found an earlier syntax error.
    fn precedes(&self, error: &ValidationError) -> bool {
        self.diagnostic.pos <= error.pos
            || (self.readonly_without_call && !error.lexical && self.target_pos == error.pos)
    }
}

/// Validates PHP source and returns identifier feedback or the first PHP-style parse error.
pub fn validate<T: SourceToken>(source: &[u8], tokens: &[T]) -> ValidationFeedback {
    crate::syntax::parser::with_tokenizer_validation(|| validate_in_scope(source, tokens))
}

/// Runs the prechecks and validates the normalized view under the tokenizer grammar.
fn validate_in_scope<T: SourceToken>(source: &[u8], tokens: &[T]) -> ValidationFeedback {
    let presence = TokenPresence::scan(tokens);
    // `declare(encoding=...)` is emitted while that statement is compiled, so it survives a
    // later parse failure. The caller drops the warnings that follow the failure position.
    let encoding_warnings = if presence.declare {
        declare_encoding_warnings(source, tokens)
    } else {
        Vec::new()
    };
    // These pre-checks parse regions the parser view blanks, so the identifier feedback they
    // record has to be merged with the main parse's feedback below.
    let mut precheck_names = Vec::new();
    let reference_delimiters = OnceCell::new();
    let precheck = run_prechecks(
        source,
        tokens,
        &presence,
        &reference_delimiters,
        &mut precheck_names,
    );
    // Precheck errors are only authoritative when they happen before the main parser's
    // failure: `echo ;` followed by `${1+}` must report the earlier `echo` error.
    if token_view_enabled() {
        if let Some(view) = token_view::build(source, tokens) {
            return validate_token_view(
                source,
                tokens,
                view,
                presence.list,
                precheck,
                precheck_names,
                encoding_warnings,
            );
        }
    }
    let (view, map, reference_error) =
        parser_view(source, tokens, source.len(), &reference_delimiters);
    let view_tokens = match crate::syntax::lexer::tokenize(&view) {
        Ok(tokens) => tokens,
        Err(error) => {
            let view_pos = view_offset(&view, &line_starts(&view), error.span.line, error.span.col);
            let source_pos = map.get(view_pos).copied().map(|pos| pos as usize).unwrap_or(source.len()).min(source.len());
            let view_error = ValidationError {
                message: error.message,
                pos: source_pos,
                lexical: true,
            };
            return ValidationFeedback {
                names: Vec::new(),
                error: Some(earlier_error(
                    source,
                    tokens,
                    precheck,
                    view_error,
                    reference_error,
                )),
                warnings: encoding_warnings,
            };
        }
    };
    let (result, names) = crate::syntax::parser::parse_with_tokenizer_feedback(&view_tokens);
    match result {
        Ok(_) => {
            if let Some(precheck) = prefer_reference_error(precheck, reference_error) {
                return ValidationFeedback {
                    names: Vec::new(),
                    error: Some(precheck),
                    warnings: encoding_warnings,
                };
            }
            let error = if presence.list {
                reserved_list_error(source, tokens)
            } else {
                None
            };
            let mut names = if error.is_some() || names.is_empty() {
                Vec::new()
            } else {
                mapped_names(&view, &map, &line_starts(&view), names, source)
            };
            if error.is_none() {
                names.extend(precheck_names);
                names.sort_unstable();
                names.dedup();
            }
            ValidationFeedback {
                names,
                error,
                warnings: encoding_warnings,
            }
        },
        Err(error) => {
            let mapped = parser_error(&view, &map, &line_starts(&view), source, tokens, &error);
            let error = unclosed_delimiter_error(source, tokens, mapped.pos, &error.message)
                .or_else(|| diagnostic_override(source, tokens, &mapped, &error.message))
                .unwrap_or(mapped);
            ValidationFeedback {
                names: Vec::new(),
                error: Some(earlier_error(
                    source,
                    tokens,
                    precheck,
                    error,
                    reference_error,
                )),
                warnings: encoding_warnings,
            }
        },
    }
}

/// Returns whether the source-spanned token view is enabled.
///
/// The token view is the default for supported sources; `ELEPHC_TOKEN_VIEW=text`
/// keeps the legacy text view as an escape hatch during rollout.
fn token_view_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        !std::env::var_os("ELEPHC_TOKEN_VIEW")
            .and_then(|value| value.to_str().map(str::to_owned))
            .is_some_and(|value| matches!(value.as_str(), "0" | "off" | "text"))
    })
}

/// Validates one source through the source-spanned token-view prototype.
fn validate_token_view<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
    view: token_view::TokenProjection,
    has_list: bool,
    precheck: Option<ValidationError>,
    precheck_names: Vec<usize>,
    encoding_warnings: Vec<ValidationWarning>,
) -> ValidationFeedback {
    let token_view::TokenProjection {
        tokens: view_tokens,
        reference_error,
    } = view;
    let (result, names) = crate::syntax::parser::parse_with_tokenizer_feedback(&view_tokens);
    match result {
        Ok(_) => {
            if let Some(precheck) = prefer_reference_error(precheck, reference_error) {
                return ValidationFeedback {
                    names: Vec::new(),
                    error: Some(precheck),
                    warnings: encoding_warnings,
                };
            }
            let error = if has_list {
                reserved_list_error(source, tokens)
            } else {
                None
            };
            let mut names = if error.is_some() || names.is_empty() {
                Vec::new()
            } else {
                mapped_names_source(source, names)
            };
            if error.is_none() {
                names.extend(precheck_names);
                names.sort_unstable();
                names.dedup();
            }
            ValidationFeedback {
                names,
                error,
                warnings: encoding_warnings,
            }
        }
        Err(error) => {
            let mapped = parser_error_source(source, tokens, &error);
            let error = unclosed_delimiter_error(source, tokens, mapped.pos, &error.message)
                .or_else(|| diagnostic_override(source, tokens, &mapped, &error.message))
                .unwrap_or(mapped);
            ValidationFeedback {
                names: Vec::new(),
                error: Some(earlier_error(source, precheck, error, reference_error)),
                warnings: encoding_warnings,
            }
        }
    }
}

/// Maps parser feedback spans from source coordinates to byte offsets.
fn mapped_names_source(source: &[u8], names: Vec<(Span, String)>) -> Vec<usize> {
    let Ok(text) = std::str::from_utf8(source) else {
        return Vec::new();
    };
    let lines = line_starts(text);
    let positions: Vec<(u32, u32)> = names.iter().map(|(span, _)| (span.line, span.col)).collect();
    let offsets = view_offsets(text, &lines, &positions);
    let mut result: Vec<usize> = names
        .into_iter()
        .zip(offsets)
        .filter_map(|((_, spelling), source_pos)| {
            let source_pos = source_pos.min(source.len());
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

/// Maps one parser error span from source coordinates to a source-token diagnostic.
fn parser_error_source<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
    error: &CompileError,
) -> ValidationError {
    let Ok(text) = std::str::from_utf8(source) else {
        return token_error_at(source, tokens, source.len());
    };
    let lines = line_starts(text);
    let source_pos = view_offset(text, &lines, error.span.line, error.span.col);
    token_error_at(source, tokens, source_pos)
}

/// Preserves parser errors inside a reference base while refining a rejected `readonly` name.
fn prefer_reference_error(
    error: Option<ValidationError>,
    reference: Option<ReferenceError>,
) -> Option<ValidationError> {
    match reference {
        Some(reference) if error.as_ref().is_none_or(|error| reference.precedes(error)) => {
            Some(reference.diagnostic)
        }
        _ => error,
    }
}

/// Returns the first diagnostic in source order, preferring the precheck on a tie.
///
/// A parser failure on an earlier token can be a same-statement refinement (the precheck
/// reports the real member after `::`), so only a separator between the two positions proves
/// the parser reached an earlier statement. That separator does not apply when the parser's
/// `Unclosed '{'` is the block `__halt_compiler()` already ended: the scanner stops there, the
/// closing brace becomes inline HTML, and the halt diagnostic stays authoritative. The same
/// applies to `Unclosed '('` when the halt diagnostic is a `for` header clause.
/// Unterminated backticks report at EOF and stay authoritative. A reference diagnostic may
/// displace a later precheck only after comparing it with the original parser position. Later
/// prechecks also yield to earlier parser errors inside the reference.
fn earlier_error<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
    precheck: Option<ValidationError>,
    main: ValidationError,
    reference: Option<ReferenceError>,
) -> ValidationError {
    let precheck = precheck.filter(|precheck| {
        precheck.pos <= main.pos
            || precheck.message == "syntax error, unexpected end of file, expecting \"`\""
            || !statement_boundary_between(source, main.pos, precheck.pos)
            || halt_diagnostic_inside_unclosed_brace(tokens, precheck, &main)
            || halt_for_header_inside_unclosed_paren(tokens, precheck, &main)
    });
    if let Some(reference) = reference {
        let preferred = precheck.as_ref().unwrap_or(&main);
        let reference_precedes_preferred = reference.precedes(preferred);
        if !reference.precedes(&main)
            && (reference_precedes_preferred
                || (main.pos >= reference.target_pos && main.pos < preferred.pos))
        {
            return main;
        }
        if reference_precedes_preferred {
            return reference.diagnostic;
        }
    }
    precheck.unwrap_or(main)
}

/// Returns whether a halt diagnostic sits inside the brace the parser reported as unclosed.
///
/// `__halt_compiler();` ends tokenization, so a `}` after it is not a brace token. A real `}`
/// token before the halt means this opener already closed and the parser error is an earlier
/// block. A `}` inside a string is not a token, so it does not close the block. An argument
/// inside `__halt_compiler(…)`, and a cast token written where `(` should be, stay
/// authoritative across a newline in that same block.
fn halt_diagnostic_inside_unclosed_brace<T: SourceToken>(
    tokens: &[T],
    precheck: &ValidationError,
    main: &ValidationError,
) -> bool {
    if main.message != "Unclosed '{'"
        || precheck.pos <= main.pos
        || !(halt_scope_diagnostic(precheck) || halt_argument_diagnostic(tokens, precheck))
    {
        return false;
    }
    let Some(open) = tokens.iter().position(|token| {
        token.pos() == main.pos && is_halt_brace_open(token.id())
    }) else {
        return false;
    };
    let mut depth = 0i32;
    for token in &tokens[open..] {
        let id = token.id();
        if is_halt_brace_open(id) {
            depth += 1;
        } else if id == i64::from(b'}') {
            depth -= 1;
            if depth <= 0 {
                return false;
            }
        }
        if token.pos() >= precheck.pos {
            return depth > 0;
        }
    }
    depth > 0
}

/// Returns whether `error` belongs to a malformed `__halt_compiler` call.
///
/// The parser can report the enclosing `Unclosed '{'` first, including across a newline.
/// An argument after `(` expects `)`. A cast such as `(array)` is one token, so that
/// diagnostic expects `(` and has no parenthesis to look behind. A heredoc argument is
/// reported on its body line. When the header newline ends the start token, that byte is
/// the next token, so a miss there still checks the heredoc span.
fn halt_argument_diagnostic<T: SourceToken>(tokens: &[T], error: &ValidationError) -> bool {
    if let Some(index) = tokens.iter().position(|token| token.pos() == error.pos) {
        if error.message.ends_with("expecting \")\"") && halt_parenthesized_argument(tokens, index)
        {
            return true;
        }
        if error.message.ends_with("expecting \"(\"")
            && previous_significant_index(tokens, index).is_some_and(|halt| {
                tokens[halt].id() == tok::T_HALT_COMPILER && !halt_used_as_name_token(tokens, halt)
            })
        {
            return true;
        }
    }
    heredoc_halt_argument(tokens, error)
}

/// Returns whether `index` is the first significant token inside `__halt_compiler(`.
fn halt_parenthesized_argument<T: SourceToken>(tokens: &[T], index: usize) -> bool {
    let Some(open) = previous_significant_index(tokens, index) else {
        return false;
    };
    if tokens[open].id() != i64::from(b'(') {
        return false;
    }
    previous_significant_index(tokens, open).is_some_and(|halt| {
        tokens[halt].id() == tok::T_HALT_COMPILER && !halt_used_as_name_token(tokens, halt)
    })
}

/// Returns whether `error` is the body-line diagnostic of a halt heredoc argument.
///
/// The reported offset is the byte after the header newline, so it can sit on the following
/// token. Inside `__halt_compiler(` the expectation is `)`. A heredoc written where `(` should
/// be expects `(`, and the previous significant token is the halt itself.
fn heredoc_halt_argument<T: SourceToken>(tokens: &[T], error: &ValidationError) -> bool {
    if !error.message.starts_with("syntax error, unexpected heredoc start ") {
        return false;
    }
    let inside = error.message.ends_with("expecting \")\"");
    let opens = error.message.ends_with("expecting \"(\"");
    if !inside && !opens {
        return false;
    }
    tokens.iter().enumerate().any(|(index, token)| {
        if token.id() != tok::T_START_HEREDOC
            || token.pos() >= error.pos
            || error.pos > token.pos() + token.len()
        {
            return false;
        }
        if inside {
            return halt_parenthesized_argument(tokens, index);
        }
        previous_significant_index(tokens, index).is_some_and(|halt| {
            tokens[halt].id() == tok::T_HALT_COMPILER && !halt_used_as_name_token(tokens, halt)
        })
    })
}

/// Returns whether the halt at `index` is a member or namespace name.
fn halt_used_as_name_token<T: SourceToken>(tokens: &[T], index: usize) -> bool {
    previous_significant_index(tokens, index).is_some_and(|previous| {
        let id = tokens[previous].id();
        id == tok::T_OBJECT_OPERATOR
            || id == tok::T_NULLSAFE_OBJECT_OPERATOR
            || id == i64::from(b'\\')
            || id == tok::T_NS_SEPARATOR
    })
}

/// Returns whether the diagnostic is one of the halt-scope messages.
///
/// A parse error that names `__halt_compiler` counts too. The scanner has already stopped,
/// so an enclosing `Unclosed '{'` is the missing closer after that halt, not an earlier fault.
fn halt_scope_diagnostic(error: &ValidationError) -> bool {
    error.message == HALT_COMPILER_OUTERMOST_SCOPE_MESSAGE
        || error.message == "Unclosed '{'"
        || error.message.starts_with("Unclosed '{' on line ")
        || error.message.starts_with("syntax error, unexpected token \"__halt_compiler\"")
}

/// Returns whether a `for`-header halt diagnostic sits inside the `(` the parser left open.
///
/// `__halt_compiler();` ends tokenization, so the header's closing `)` becomes inline HTML.
/// A semicolon that separates `for` clauses is inside that header, not an earlier statement.
fn halt_for_header_inside_unclosed_paren<T: SourceToken>(
    tokens: &[T],
    precheck: &ValidationError,
    main: &ValidationError,
) -> bool {
    if main.message != "Unclosed '('"
        || precheck.pos <= main.pos
        || !for_header_halt_message(precheck)
    {
        return false;
    }
    let Some(open) = tokens.iter().position(|token| {
        token.pos() == main.pos && token.id() == i64::from(b'(')
    }) else {
        return false;
    };
    let mut depth = 0i32;
    for token in &tokens[open..] {
        let id = token.id();
        if id == i64::from(b'(') {
            depth += 1;
        } else if id == i64::from(b')') {
            depth -= 1;
            if depth <= 0 {
                return false;
            }
        }
        if token.pos() >= precheck.pos {
            return depth > 0;
        }
    }
    depth > 0
}

/// Returns whether `error` is the halt diagnostic for a `for` init, condition or update clause.
fn for_header_halt_message(error: &ValidationError) -> bool {
    error.message == "syntax error, unexpected token \"__halt_compiler\", expecting \";\""
        || error.message == "syntax error, unexpected token \"__halt_compiler\", expecting \")\""
}

/// Returns whether `id` opens a brace the halt-scope walk treats as a real block.
fn is_halt_brace_open(id: i64) -> bool {
    id == i64::from(b'{')
        || id == tok::T_CURLY_OPEN
        || id == tok::T_DOLLAR_OPEN_CURLY_BRACES
}

/// Returns whether a statement boundary separates two source positions.
fn statement_boundary_between(source: &[u8], start: usize, end: usize) -> bool {
    source
        .get(start.min(source.len())..end.min(source.len()))
        .is_some_and(|between| {
            between
                .iter()
                .any(|byte| matches!(*byte, b';' | b'\n' | b'\r'))
        })
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












/// Returns identifier feedback mapped from parser view spans to original source offsets.
fn mapped_names(
    view: &str, map: &[u32], lines: &[usize], names: Vec<(Span, String)>, source: &[u8],
) -> Vec<usize> {
    let positions: Vec<(u32, u32)> = names.iter().map(|(span, _)| (span.line, span.col)).collect();
    let offsets = view_offsets(view, lines, &positions);
    let mut result: Vec<usize> = names
        .into_iter()
        .zip(offsets)
        .filter_map(|((_, spelling), view_pos)| {
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

/// Maps multiple parser-view line/column pairs to byte offsets in one forward pass per line.
///
/// `view_offset` restarts at the line beginning for every lookup, which turns a minified
/// single-line file into quadratic work. Spans are sorted once and each requested line is
/// walked once.
fn view_offsets(view: &str, lines: &[usize], positions: &[(u32, u32)]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..positions.len()).collect();
    order.sort_by_key(|&index| positions[index]);
    let mut offsets = vec![view.len(); positions.len()];
    let mut cursor = 0;
    while cursor < order.len() {
        let line = positions[order[cursor]].0;
        let Some(&start) = lines.get(line.saturating_sub(1) as usize) else {
            break;
        };
        let end = lines.get(line as usize).copied().unwrap_or(view.len());
        let mut chars = view[start..end].char_indices();
        let mut current = chars.next().map(|(index, _)| (1u32, start + index));
        while cursor < order.len() && positions[order[cursor]].0 == line {
            let column = positions[order[cursor]].1;
            let offset = if column <= 1 {
                start
            } else {
                while let Some((index, _)) = current {
                    if index == column {
                        break;
                    }
                    current = chars.next().map(|(next, _)| (index + 1, start + next));
                }
                current.map_or(view.len(), |(_, byte)| byte)
            };
            offsets[order[cursor]] = offset;
            cursor += 1;
        }
    }
    offsets
}

/// Precomputes the byte offset of every one-based parser-view line.
pub(super) fn line_starts(view: &str) -> Vec<usize> {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Scans one source into the reduced token view used by validation.
    fn scan_tokens(source: &[u8]) -> Vec<TokenView> {
        crate::scanner::scan(source)
            .expect("scan")
            .tokens
            .into_iter()
            .map(|token| TokenView {
                id: token.id,
                pos: token.pos,
                len: token.len,
                line: token.line,
            })
            .collect()
    }

    /// Verifies the source-spanned token view matches the text parser view for supported code.
    #[test]
    fn token_view_matches_text_view_on_code_sources() {
        let sources: &[&[u8]] = &[
            b"<?php $a = 1; $b = $a + 2;",
            b"<?php class C { public int $x = 0; }",
            b"<?php #[Attr] class C { public int $x = 0 { get => $this->x; set => $value; } }",
            b"<?php echo 1; ?>html",
            b"<?= 1 ?>",
            b"<?php // comment\n$a = 1;",
            b"<?php /** doc */ $a = 1;",
            b"<?php\n$a = 1;\n",
            b"<?php new \\Foo\\Bar();",
            b"<?php Foo\\Bar::baz();",
            b"<?php namespace\\Foo;",
            b"<?php $a = 'hello';",
            b"<?php $a = \"hello\";",
            b"<?php $a = \"hello $name\";",
            b"<?php $a = <<<TXT\nhello\nTXT;",
            b"<?php $a = `echo hi`;",
            b"<?php $a = 'it\\'s';",
            b"<?php $a = true; $b = false; $c = null;",
            b"<?php $a = 1.5; $b = 1e3;",
            b"<?php $a = \"h\xc3\xa9llo\";",
            b"<?php $a = (int) $b; $c = (string) $d; $e = (array) $f;",
            b"<?php $a = (integer) $b; $c = (boolean) $d; $e = (double) $f;",
            b"<?php $a = (binary) $b; $c = (unset) $d;",
            b"<?php (void) $a;",
            b"<?php echo C::{foo()}::BAR;",
            b"<?php $a::{ $b + 1 };",
            b"<?php function f(&$x) {}",
            b"<?php function &f() {}",
            b"<?php foreach ($a as &$v) {}",
            b"<?php $a = &$b;",
            b"<?php $a = [&$b];",
            b"<?php $a = array(&$b);",
            b"<?php $f = function () use (&$x) {};",
            b"<?php $f = fn&($x) => $x;",
            b"<?php foreach ($a as $k => &$v) {}",
        ];
        for source in [
            b"<?php done: echo 1;".as_slice(),
            b"<?php if (1) done: echo 1;",
            b"<?php\r\n$a = 1;\r\n",
            b"<?php $a = (void) $b;",
            b"<?php $a = &$b[0];",
            b"<?php $a = &foo();",
            b"<?php $a = &1;",
            b"<?php $a = [&$b + 1];",
            b"<?php foo(&$x);",
            b"<?php function f($a = &$b) {}",
        ] {
            let tokens = scan_tokens(source);
            assert!(
                token_view::build(source, &tokens).is_none(),
                "label source must fall back: {}",
                String::from_utf8_lossy(source)
            );
        }
        for source in sources {
            let tokens = scan_tokens(source);
            let (view, map, _) =
                parser_view(source, &tokens, source.len(), &OnceCell::new());
            let old = crate::syntax::lexer::tokenize(&view).expect("old view");
            let old_lines = line_starts(&view);
            let positions: Vec<(u32, u32)> = old
                .iter()
                .map(|(_, metadata)| (metadata.span.line, metadata.span.col))
                .collect();
            let offsets = view_offsets(&view, &old_lines, &positions);
            let old_offsets: Vec<usize> = offsets
                .iter()
                .map(|position| {
                    map.get(*position)
                        .copied()
                        .unwrap_or(source.len() as u32) as usize
                })
                .collect();
            let new = token_view::build(source, &tokens)
                .unwrap_or_else(|| panic!("code source supported: {}", String::from_utf8_lossy(source)));
            assert_eq!(
                old.len(),
                new.tokens.len(),
                "token count for {}",
                String::from_utf8_lossy(source)
            );
            let source_text = std::str::from_utf8(source).expect("valid fixture");
            let new_lines = line_starts(source_text);
            for (index, (pairs, old_offset)) in old
                .iter()
                .zip(new.tokens.iter())
                .zip(old_offsets.iter())
                .enumerate()
            {
                let ((old_token, _), (new_token, new_metadata)) = pairs;
                let same_kind = match (old_token, new_token) {
                    (crate::syntax::lexer::Token::Identifier(_), crate::syntax::lexer::Token::Identifier(_)) => true,
                    _ => old_token == new_token,
                };
                assert!(
                    same_kind,
                    "kind at {index} for {}",
                    String::from_utf8_lossy(source)
                );
                let new_offset = view_offset(
                    source_text,
                    &new_lines,
                    new_metadata.span.line,
                    new_metadata.span.col,
                );
                // The text view maps every byte of a multi-byte native token to its start;
                // the token view keeps the exact segment offset. Accept either the old
                // start mapping or an offset inside the native token that starts there.
                let same_native_token = tokens.iter().any(|native| {
                    native.pos() == *old_offset
                        && new_offset >= native.pos()
                        && new_offset < native.pos() + native.len()
                });
                assert!(
                    *old_offset == new_offset || same_native_token,
                    "offset at {index} for {}: old={} new={}",
                    String::from_utf8_lossy(source),
                    old_offset,
                    new_offset
                );
            }
        }
    }
}
