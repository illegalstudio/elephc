//! Purpose:
//! Owns Zend's keyword and cast vocabulary for the native tokenizer.
//!
//! Called from:
//! - `super::lex` for every identifier and cast candidate.
//!
//! Key details:
//! - Keyword matching is ASCII case-insensitive, matching PHP's lexer.
//! - Token IDs are resolved once from the shared PHP 8.5 token catalog.

use std::sync::OnceLock;

use super::id;

/// Longest keyword spelling (`__halt_compiler`), used to reject identifiers early.
const MAX_KEYWORD_LEN: usize = 16;
/// Longest cast spelling (`boolean`/`integer`), used to reject candidates early.
const MAX_CAST_LEN: usize = 8;
/// First-byte classes used by the vocabulary: `a`-`z`, then the `_` names.
const FIRST_CLASSES: usize = 27;

/// Returns the vocabulary bucket class for one lowercased first byte.
fn first_class(byte: u8) -> Option<usize> {
    if byte.is_ascii_lowercase() {
        Some(usize::from(byte - b'a'))
    } else if byte == b'_' {
        Some(FIRST_CLASSES - 1)
    } else {
        None
    }
}

/// A fixed identifier vocabulary indexed by lowercased first byte and length.
///
/// Keyword and cast lookups dominate identifier scanning. Bucketing by the first byte and
/// length leaves at most a couple of candidates per bucket, so a lookup compares a few short
/// slices instead of hashing every identifier.
struct Vocabulary {
    /// Buckets addressed as `first_class(first) * (max_len + 1) + word.len()`.
    buckets: Vec<Vec<(&'static [u8], i64)>>,
    /// Longest spelling the vocabulary can contain.
    max_len: usize,
}

impl Vocabulary {
    /// Builds the buckets once from a static spelling-to-token table.
    fn new(entries: &[(&'static str, &'static str)], max_len: usize) -> Self {
        let stride = max_len + 1;
        let mut buckets = vec![Vec::new(); FIRST_CLASSES * stride];
        for (text, name) in entries {
            let bytes = text.as_bytes();
            let first = bytes[0].to_ascii_lowercase();
            let Some(class) = first_class(first) else { continue };
            buckets[class * stride + bytes.len()].push((bytes, id(name)));
        }
        Self { buckets, max_len }
    }

    /// Returns the token ID for one case-insensitive spelling.
    fn get(&self, word: &[u8]) -> Option<i64> {
        let first = word.first().map(u8::to_ascii_lowercase)?;
        if word.len() > self.max_len {
            return None;
        }
        let class = first_class(first)?;
        let index = class * (self.max_len + 1) + word.len();
        self.buckets[index]
            .iter()
            .find_map(|(candidate, token)| word.eq_ignore_ascii_case(candidate).then_some(*token))
    }
}

/// Keyword bytes to token identifier, built once per process.
static KEYWORD_IDS: OnceLock<Vocabulary> = OnceLock::new();
/// Cast bytes to token identifier, built once per process.
static CAST_IDS: OnceLock<Vocabulary> = OnceLock::new();

/// Returns PHP's whitespace set for T_WHITESPACE runs and open/close tag swallowing.
pub(crate) fn is_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r')
}

/// Returns whether a byte may start a PHP label.
#[inline]
pub fn is_label_start(byte: u8) -> bool {
    matches!(byte, b'a'..=b'z' | b'A'..=b'Z' | b'_') || byte >= 0x80
}

/// Returns whether a byte may continue a PHP label.
#[inline]
pub(crate) fn is_label(byte: u8) -> bool {
    matches!(byte, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_') || byte >= 0x80
}

/// Returns the token ID for a complete identifier, or `None` for T_STRING.
pub(crate) fn keyword_id(word: &[u8]) -> Option<i64> {
    KEYWORD_IDS
        .get_or_init(|| Vocabulary::new(KEYWORDS, MAX_KEYWORD_LEN))
        .get(word)
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
    CAST_IDS
        .get_or_init(|| Vocabulary::new(CASTS, MAX_CAST_LEN))
        .get(word)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the bucketed vocabulary keeps PHP's case-insensitive matching, including the
    /// underscore-prefixed magic keywords, and rejects ordinary identifiers.
    #[test]
    fn vocabulary_keeps_case_insensitive_lookup() {
        assert_eq!(keyword_id(b"CLASS"), keyword_id(b"class"));
        assert!(keyword_id(b"class").is_some());
        assert!(keyword_id(b"__halt_compiler").is_some());
        assert!(keyword_id(b"__CLASS__").is_some());
        assert!(keyword_id(b"ClassName").is_none());
        assert!(keyword_id(b"").is_none());
        assert_eq!(cast_id(b"INTEGER"), cast_id(b"integer"));
        assert!(cast_id(b"notacast").is_none());
    }
}
