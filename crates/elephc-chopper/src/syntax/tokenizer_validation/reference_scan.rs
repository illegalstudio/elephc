//! Purpose:
//! Owns the shared reference-target and continuation rules used by the parser view,
//! tokenizer prechecks and token view.
//!
//! Called from:
//! - `parser_view::is_reference_ampersand`, the reference prechecks and `token_view`.
//!
//! Key details:
//! - Target and chain scanning must stay identical across the three consumers.
//! - The helpers operate on native tokens and never allocate beyond the caller's maps.

use super::{next_significant_index, tok, SourceToken};

/// Returns whether a token is one of Zend's reference ampersand forms.
pub(super) fn is_reference_marker(id: i64) -> bool {
    matches!(
        id,
        tok::T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG
            | tok::T_AMPERSAND_NOT_FOLLOWED_BY_VAR_OR_VARARG
    )
}

/// Returns whether an ordinary reference target can start at `next`.
pub(super) fn reference_target_starts_at<T: SourceToken>(tokens: &[T], next: usize) -> bool {
    match tokens[next].id() {
        id if id == tok::T_VARIABLE => true,
        // `$$name` / `${$name}`: the name is computed at run time.
        id if id == i64::from(b'$') => true,
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
        // The native lexer folds `\foo`, `Ns\foo` and `namespace\foo` into one name token.
        id if id == tok::T_NAME_FULLY_QUALIFIED
            || id == tok::T_NAME_QUALIFIED
            || id == tok::T_NAME_RELATIVE =>
        {
            next_significant_index(tokens, next).is_some_and(|after| {
                matches!(
                    tokens[after].id(),
                    id if id == i64::from(b'(')
                        || id == i64::from(b'[')
                        || id == tok::T_DOUBLE_COLON
                        || id == tok::T_OBJECT_OPERATOR
                        || id == tok::T_NULLSAFE_OBJECT_OPERATOR
                )
            })
        }
        _ => false,
    }
}

/// Returns the last token of a reference target chain.
pub(super) fn reference_chain_end<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
    target: usize,
    close_of: &[Option<usize>],
) -> Option<usize> {
    let mut cursor = target;
    loop {
        let Some(next) = next_significant_index(tokens, cursor) else {
            return Some(cursor);
        };
        match tokens[next].id() {
            id if id == i64::from(b'[')
                || id == i64::from(b'(')
                || id == i64::from(b'{') =>
            {
                let close = close_of[next]?;
                if contains_reference_marker(tokens, next + 1, close) {
                    return None;
                }
                cursor = close;
            }
            id if id == tok::T_OBJECT_OPERATOR
                || id == tok::T_NULLSAFE_OBJECT_OPERATOR
                || id == tok::T_DOUBLE_COLON =>
            {
                let member = next_significant_index(tokens, next)?;
                if tokens[member].id() == i64::from(b'{') {
                    let close = close_of[member]?;
                    if contains_reference_marker(tokens, member + 1, close) {
                        return None;
                    }
                    cursor = close;
                } else if matches!(
                    tokens[member].id(),
                    tok::T_VARIABLE | tok::T_STRING | tok::T_IDENTIFIER
                ) || token_is_word(source, &tokens[member])
                {
                    cursor = member;
                } else {
                    return None;
                }
            }
            _ => return Some(cursor),
        }
    }
}

/// Returns whether a continuation token is not a write assignment or update.
pub(super) fn reference_continuation_is_valid<T: SourceToken>(source: &[u8], token: &T) -> bool {
    !is_write_continuation(source, token)
}

/// Returns whether a token is a write assignment or update operator.
pub(super) fn is_write_continuation<T: SourceToken>(source: &[u8], token: &T) -> bool {
    let Some(bytes) = source.get(token.pos()..token.pos() + token.len()) else {
        return false;
    };
    let Ok(text) = std::str::from_utf8(bytes) else {
        return false;
    };
    matches!(
        text,
        "=" | "+="
            | "-="
            | "*="
            | "/="
            | ".="
            | "%="
            | "&="
            | "|="
            | "^="
            | "<<="
            | ">>="
            | "**="
            | "??="
            | "++"
            | "--"
    )
}

/// Returns whether a token range contains another reference marker.
pub(super) fn contains_reference_marker<T: SourceToken>(
    tokens: &[T],
    start: usize,
    end: usize,
) -> bool {
    tokens[start..end]
        .iter()
        .any(|token| is_reference_marker(token.id()))
}

/// Returns whether one native token's source text is a word.
fn token_is_word<T: SourceToken>(source: &[u8], token: &T) -> bool {
    source
        .get(token.pos()..token.pos() + token.len())
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
        .is_some_and(is_word)
}

/// Returns whether `text` is a PHP label-shaped word.
fn is_word(text: &str) -> bool {
    let mut bytes = text.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == b'_' || first >= 0x80)
        && bytes.all(|byte| {
            byte.is_ascii_alphanumeric() || byte == b'_' || byte >= 0x80
        })
}
