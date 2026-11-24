//! Purpose:
//! Owns Zend's keyword and cast vocabulary for the native tokenizer.
//!
//! Called from:
//! - `super::lex` for every identifier and cast candidate.
//!
//! Key details:
//! - Keyword matching is ASCII case-insensitive, matching PHP's lexer.
//! - Token IDs are resolved once from the shared PHP 8.5 token catalog.

use std::collections::HashMap;
use std::sync::OnceLock;

use super::id;

/// Longest keyword spelling (`__halt_compiler`), used to reject identifiers early.
const MAX_KEYWORD_LEN: usize = 16;
/// Longest cast spelling (`boolean`/`integer`), used to reject candidates early.
const MAX_CAST_LEN: usize = 8;
/// Keyword bytes to token identifier, built once per process.
static KEYWORD_IDS: OnceLock<HashMap<&'static [u8], i64>> = OnceLock::new();
/// Cast bytes to token identifier, built once per process.
static CAST_IDS: OnceLock<HashMap<&'static [u8], i64>> = OnceLock::new();

/// Returns PHP's whitespace set for T_WHITESPACE runs and open/close tag swallowing.
pub(crate) fn is_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r')
}

/// Returns whether a byte may start a PHP label.
#[inline]
pub fn is_label_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte >= 0x80
}

/// Returns whether a byte may continue a PHP label.
pub(crate) fn is_label(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte >= 0x80
}

/// Returns the token ID for a complete identifier, or `None` for T_STRING.
pub(crate) fn keyword_id(word: &[u8]) -> Option<i64> {
    if word.len() > MAX_KEYWORD_LEN {
        return None;
    }
    let mut lower = [0u8; MAX_KEYWORD_LEN];
    let lower = &mut lower[..word.len()];
    for (destination, source) in lower.iter_mut().zip(word) {
        *destination = source.to_ascii_lowercase();
    }
    KEYWORD_IDS
        .get_or_init(|| {
            KEYWORDS
                .iter()
                .map(|(text, name)| (text.as_bytes(), id(name)))
                .collect()
        })
        .get(lower)
        .copied()
}

/// Returns Zend's grammar spelling for a keyword diagnostic, preserving ordinary names.
pub(crate) fn keyword_diagnostic_spelling(token: i64, source: &[u8]) -> Option<String> {
    use elephc_builtin_contract::token_const_id;

    if token == const { token_const_id("T_YIELD_FROM") } {
        return Some("yield from".into());
    }
    let visibility_set = [
        const { token_const_id("T_PUBLIC_SET") },
        const { token_const_id("T_PROTECTED_SET") },
        const { token_const_id("T_PRIVATE_SET") },
    ];
    if !visibility_set.contains(&token) && keyword_id(source) != Some(token) {
        return None;
    }
    let text = std::str::from_utf8(source).ok()?;
    if text.starts_with("__") && text.ends_with("__") {
        Some(text.to_ascii_uppercase())
    } else {
        Some(text.to_ascii_lowercase())
    }
}

/// Returns Zend's canonical cast spelling for a cast token.
///
/// `(integer)`, `( Array )` and `(binary)` share the ids of `(int)`, `(array)` and `(string)`.
/// The source spelling is not the diagnostic text. `(real)` is a removed-cast error and does
/// not use this spelling.
pub(crate) fn cast_diagnostic_text(token_id: i64) -> Option<&'static str> {
    use elephc_builtin_contract::token_const_id;

    Some(if token_id == const { token_const_id("T_ARRAY_CAST") } {
        "(array)"
    } else if token_id == const { token_const_id("T_BOOL_CAST") } {
        "(bool)"
    } else if token_id == const { token_const_id("T_DOUBLE_CAST") } {
        "(float)"
    } else if token_id == const { token_const_id("T_INT_CAST") } {
        "(int)"
    } else if token_id == const { token_const_id("T_OBJECT_CAST") } {
        "(object)"
    } else if token_id == const { token_const_id("T_STRING_CAST") } {
        "(string)"
    } else if token_id == const { token_const_id("T_UNSET_CAST") } {
        "(unset)"
    } else if token_id == const { token_const_id("T_VOID_CAST") } {
        "(void)"
    } else {
        return None;
    })
}

/// Returns the heredoc header Zend prints for a start token.
///
/// The cut is the first `\n` only, so a carriage return stays visible. One trailing `'` or
/// `"` is dropped. A nowdoc header still uses the `heredoc start` noun.
pub(crate) fn heredoc_start_header(text: &[u8]) -> &[u8] {
    let header = text.split(|byte| *byte == b'\n').next().unwrap_or(text);
    match header.last() {
        Some(b'\'') | Some(b'"') => &header[..header.len() - 1],
        _ => header,
    }
}

/// Returns the cast token for the word between parentheses, or `None`.
pub(crate) fn cast_id(word: &[u8]) -> Option<i64> {
    if word.len() > MAX_CAST_LEN {
        return None;
    }
    let mut lower = [0u8; MAX_CAST_LEN];
    let lower = &mut lower[..word.len()];
    for (destination, source) in lower.iter_mut().zip(word) {
        *destination = source.to_ascii_lowercase();
    }
    CAST_IDS
        .get_or_init(|| {
            CASTS
                .iter()
                .map(|(text, name)| (text.as_bytes(), id(name)))
                .collect()
        })
        .get(lower)
        .copied()
}

/// Keywords PHP's lexer answers with a dedicated token instead of T_STRING.
const KEYWORDS: &[(&str, &str)] = &[
    ("abstract", "T_ABSTRACT"),
    ("and", "T_LOGICAL_AND"),
    ("array", "T_ARRAY"),
    ("as", "T_AS"),
    ("break", "T_BREAK"),
    ("callable", "T_CALLABLE"),
    ("case", "T_CASE"),
    ("catch", "T_CATCH"),
    ("class", "T_CLASS"),
    ("clone", "T_CLONE"),
    ("const", "T_CONST"),
    ("continue", "T_CONTINUE"),
    ("declare", "T_DECLARE"),
    ("default", "T_DEFAULT"),
    ("die", "T_EXIT"),
    ("do", "T_DO"),
    ("echo", "T_ECHO"),
    ("else", "T_ELSE"),
    ("elseif", "T_ELSEIF"),
    ("empty", "T_EMPTY"),
    ("enddeclare", "T_ENDDECLARE"),
    ("endfor", "T_ENDFOR"),
    ("endforeach", "T_ENDFOREACH"),
    ("endif", "T_ENDIF"),
    ("endswitch", "T_ENDSWITCH"),
    ("endwhile", "T_ENDWHILE"),
    ("enum", "T_ENUM"),
    ("eval", "T_EVAL"),
    ("exit", "T_EXIT"),
    ("extends", "T_EXTENDS"),
    ("final", "T_FINAL"),
    ("finally", "T_FINALLY"),
    ("fn", "T_FN"),
    ("for", "T_FOR"),
    ("foreach", "T_FOREACH"),
    ("function", "T_FUNCTION"),
    ("global", "T_GLOBAL"),
    ("goto", "T_GOTO"),
    ("if", "T_IF"),
    ("implements", "T_IMPLEMENTS"),
    ("include", "T_INCLUDE"),
    ("include_once", "T_INCLUDE_ONCE"),
    ("instanceof", "T_INSTANCEOF"),
    ("insteadof", "T_INSTEADOF"),
    ("interface", "T_INTERFACE"),
    ("isset", "T_ISSET"),
    ("list", "T_LIST"),
    ("match", "T_MATCH"),
    ("namespace", "T_NAMESPACE"),
    ("new", "T_NEW"),
    ("or", "T_LOGICAL_OR"),
    ("print", "T_PRINT"),
    ("private", "T_PRIVATE"),
    ("protected", "T_PROTECTED"),
    ("public", "T_PUBLIC"),
    ("readonly", "T_READONLY"),
    ("require", "T_REQUIRE"),
    ("require_once", "T_REQUIRE_ONCE"),
    ("return", "T_RETURN"),
    ("static", "T_STATIC"),
    ("switch", "T_SWITCH"),
    ("throw", "T_THROW"),
    ("trait", "T_TRAIT"),
    ("try", "T_TRY"),
    ("unset", "T_UNSET"),
    ("use", "T_USE"),
    ("var", "T_VAR"),
    ("while", "T_WHILE"),
    ("xor", "T_LOGICAL_XOR"),
    ("yield", "T_YIELD"),
    ("__class__", "T_CLASS_C"),
    ("__dir__", "T_DIR"),
    ("__file__", "T_FILE"),
    ("__function__", "T_FUNC_C"),
    ("__halt_compiler", "T_HALT_COMPILER"),
    ("__line__", "T_LINE"),
    ("__method__", "T_METHOD_C"),
    ("__namespace__", "T_NS_C"),
    ("__property__", "T_PROPERTY_C"),
    ("__trait__", "T_TRAIT_C"),
];

/// Words that make `( word )` a cast token rather than a parenthesized expression.
const CASTS: &[(&str, &str)] = &[
    ("array", "T_ARRAY_CAST"),
    ("binary", "T_STRING_CAST"),
    ("bool", "T_BOOL_CAST"),
    ("boolean", "T_BOOL_CAST"),
    ("double", "T_DOUBLE_CAST"),
    ("float", "T_DOUBLE_CAST"),
    ("int", "T_INT_CAST"),
    ("integer", "T_INT_CAST"),
    ("object", "T_OBJECT_CAST"),
    ("real", "T_DOUBLE_CAST"),
    ("string", "T_STRING_CAST"),
    ("unset", "T_UNSET_CAST"),
    ("void", "T_VOID_CAST"),
];
