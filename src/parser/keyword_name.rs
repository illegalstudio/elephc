//! Purpose:
//! Maps a token that may legally appear as a bareword name to its source-text spelling.
//! Centralizes the PHP "semi-reserved" rule: identifiers and (nearly) all keywords are
//! valid as named-argument labels, `->`/`?->`/`::` member names, and method/constant names.
//!
//! Called from:
//! - `crate::parser::expr` (named arguments, `->`/`?->` members, `::` scoped access)
//! - `crate::parser::stmt::oop` (method and class-constant declaration names)
//!
//! Key details:
//! - Returns the exact source lexeme so the resulting name matches what PHP would record.
//! - Callers that need PHP's narrow exceptions (e.g. `class` is reserved as a constant name,
//!   and `Foo::class` is the special class-name fetch) must special-case those tokens first.

use crate::lexer::{Token, TokenMetadata};

/// Returns the source-text spelling of a token that can be used as a bareword name, or `None`
/// for tokens (operators, punctuation, literals) that cannot.
///
/// Accepts plain identifiers plus every reserved keyword and magic-constant token, matching
/// PHP 8's semi-reserved-word rule where keywords are permitted as member, method, constant,
/// and named-argument names. Callers requiring PHP's exceptions (`class` as a constant name,
/// the `Foo::class` fetch) must handle those tokens before consulting this helper.
pub(crate) fn bareword_name_from_token(
    token: &Token,
    metadata: &TokenMetadata,
) -> Option<String> {
    token.word_spelling(metadata).map(str::to_string)
}
