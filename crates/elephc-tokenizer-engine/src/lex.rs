//! Purpose:
//! Adapts Mago's lossless byte lexer to Zend's token IDs and exact token boundaries.
//!
//! Called from:
//! - `crate::tokenize` before optional parser feedback.
//!
//! Key details:
//! - Opening/closing tag newlines, yield-from, enum lookahead, property names and
//!   ampersand lookahead follow PHP rather than the parser library's token vocabulary.
//! - Malformed input remains tokenizable without TOKEN_PARSE.

use mago_database::file::FileId;
use mago_syntax::{lexer::Lexer, settings::LexerSettings, token::TokenKind};
use mago_syntax_core::input::Input;
use crate::{id, newline_count, Token};

/// A string interpolation frame, with the delimiter and nested expression depth.
#[derive(Default)]
struct StringFrame { delimiter: u8, depth: usize }

/// Normalizes PHP tag, cast and whitespace boundaries while preserving source offsets.
pub(crate) fn normalize_tags(source: &[u8]) -> Vec<u8> {
    let mut input = source.to_vec();
    let mut offset = 0;
    loop {
        let mut lexer = Lexer::new(Input::anchored_at(FileId::zero(), &input[offset..], mago_span::Position::new(offset as u32)),
            LexerSettings { enable_short_tags: false });
        let mut invalid = None;
        let mut edits = Vec::new();
        while let Some(next) = lexer.advance() {
            if let Ok(t) = next {
                let at = t.start.offset as usize;
                if t.kind == TokenKind::Whitespace {
                    edits.extend(t.value.iter().enumerate().filter(|(_, b)| matches!(b, 0x0b | 0x0c)).map(|(i, _)| (at + i, 0)));
                }
                if is_cast(t.kind) {
                    let body = &t.value[1..t.value.len() - 1];
                    if body.iter().any(|b| b.is_ascii_whitespace() && !matches!(b, b' ' | b'\t'))
                        || !body.trim_ascii().iter().all(u8::is_ascii_alphabetic) {
                        if let Some(letter) = t.value.iter().position(u8::is_ascii_alphabetic) { edits.push((at + letter, b'z')); }
                    }
                }
                if t.kind == TokenKind::OpenTag && source.get(at + 5).is_some_and(|b| !matches!(b, b' ' | b'\t' | b'\r' | b'\n')) {
                    invalid = Some(at);
                    break;
                }
            }
        }
        drop(lexer);
        for (at, byte) in edits { input[at] = byte; }
        match invalid {
            Some(at) => { input[at] = b'x'; offset = at + 5; }
            None => return input,
        }
    }
}

/// Tokenizes without syntax validation, including incomplete source and bad bytes.
pub(super) fn tokenize(source: &[u8]) -> Vec<Token> {
    let mut input = normalize_tags(source);
    // Zend's non-parser tokenizer stops after any three significant tokens after halt.
    // Keep Mago in scripting mode so its stricter halt state cannot reject that input.
    for (i, word) in source.windows(15).enumerate() {
        if word.eq_ignore_ascii_case(b"__halt_compiler") { input[i] = b'z'; }
    }
    let mut lexer = Lexer::new(Input::new(FileId::zero(), &input), LexerSettings { enable_short_tags: false });
    let mut result: Vec<Token> = Vec::new();
    let mut cursor = 0;
    let mut line = 1;
    let mut strings: Vec<StringFrame> = Vec::new();
    let mut property = false;
    let mut halt: Option<usize> = None;
    while let Some(next) = lexer.advance() {
        let before = cursor;
        let (kind, mut start, mut end) = match next {
            Ok(t) => (Some(t.kind), t.start.offset as usize, t.start.offset as usize + t.value.len()),
            Err(_) => (None, cursor, lexer.current_position().offset as usize),
        };
        start = start.max(cursor);
        end = end.max(start).min(source.len());
        if end == start { continue; }
        if start > cursor {
            // A lexer error can consume bytes before reporting its position.
            push(&mut result, id("T_ENCAPSED_AND_WHITESPACE"), cursor, start, &mut line, source);
        }
        let raw = &source[start..end];
        let mut token_id = match kind {
            Some(k) => token_id(k, raw, &source[end..]),
            None if raw.starts_with(b"/*") => comment_id(raw),
            None if raw.first().is_some_and(u8::is_ascii_digit) => id("T_LNUMBER"),
            None if raw.len() == 1 => id("T_BAD_CHARACTER"),
            None => id("T_ENCAPSED_AND_WHITESPACE"),
        };
        if matches!(kind, Some(TokenKind::SingleLineComment | TokenKind::HashComment)) {
            while source.get(end).is_some_and(|b| !matches!(b, b'\r' | b'\n')) && source.get(end..end + 2) != Some(b"?>") { end += 1; }
        }
        if matches!(kind, Some(TokenKind::OpenTag)) {
            end += match source.get(end..) {
                Some([b'\r', b'\n', ..]) => 2,
                Some([b' ' | b'\t' | b'\n' | b'\r', ..]) => 1,
                _ => 0,
            };
        } else if matches!(kind, Some(TokenKind::CloseTag)) {
            end += match source.get(end..) {
                Some([b'\r', b'\n', ..]) => 2,
                Some([b'\n' | b'\r', ..]) => 1,
                _ => 0,
            };
        }
        if property && kind == Some(TokenKind::QualifiedIdentifier) {
            if let Some(separator) = raw.iter().position(|b| *b == b'\\') {
                push(&mut result, id("T_STRING"), start, start + separator, &mut line, source);
                push(&mut result, id("T_NAME_FULLY_QUALIFIED"), start + separator, end, &mut line, source);
                cursor = end;
                property = false;
                continue;
            }
        }
        if property && is_identifier(raw) {
            token_id = id("T_STRING");
            property = false;
        } else if token_id != id("T_WHITESPACE") && token_id != id("T_COMMENT") && token_id != id("T_DOC_COMMENT") {
            property = matches!(kind, Some(TokenKind::MinusGreaterThan | TokenKind::QuestionMinusGreaterThan));
        }
        // Mago exposes `yield`, trivia and `from` separately; Zend exposes one token.
        if token_id == id("T_YIELD") {
            let from = skip_trivia(source, end);
            if from > end && source.get(from..from + 4).is_some_and(|s| s.eq_ignore_ascii_case(b"from"))
                && source.get(from + 4).is_some_and(|b| !identifier_part(*b)) {
                token_id = id("T_YIELD_FROM");
                end = from + 4;
            }
        }
        match kind {
            Some(TokenKind::DoubleQuote | TokenKind::Backtick) => {
                let delimiter = if kind == Some(TokenKind::Backtick) { b'`' } else { b'"' };
                if strings.last().is_some_and(|f| f.delimiter == delimiter && f.depth == 0) { strings.pop(); }
                else { strings.push(StringFrame { delimiter, depth: 0 }); }
            }
            Some(TokenKind::DocumentStart(_)) => strings.push(StringFrame { delimiter: 0, depth: 0 }),
            Some(TokenKind::DocumentEnd) => { strings.pop(); }
            Some(TokenKind::LeftBrace | TokenKind::DollarLeftBrace) => {
                if let Some(frame) = strings.last_mut() {
                    if frame.depth == 0 && kind == Some(TokenKind::LeftBrace) { token_id = id("T_CURLY_OPEN"); }
                    frame.depth += 1;
                }
            }
            Some(TokenKind::RightBrace) => {
                if let Some(frame) = strings.last_mut() { frame.depth = frame.depth.saturating_sub(1); }
            }
            _ => {}
        }
        if kind == Some(TokenKind::Whitespace) && strings.last().is_some_and(|f| f.delimiter == 0 && f.depth == 0) {
            token_id = id("T_ENCAPSED_AND_WHITESPACE");
        }
        if kind == Some(TokenKind::PartialLiteralString) {
            let prefix = usize::from(matches!(raw.first(), Some(b'b' | b'B')));
            if raw.get(prefix) == Some(&b'"') {
                push(&mut result, i64::from(b'"'), start, start + prefix + 1, &mut line, source);
                start += prefix + 1;
            }
        }
        push(&mut result, token_id, start, end, &mut line, source);
        cursor = end;
        if let Some(remaining) = halt.as_mut() {
            if ![id("T_WHITESPACE"), id("T_COMMENT"), id("T_DOC_COMMENT"), id("T_OPEN_TAG")].contains(&token_id) {
                *remaining -= 1;
                if *remaining == 0 {
                    if end < source.len() { push(&mut result, id("T_INLINE_HTML"), end, source.len(), &mut line, source); }
                    cursor = source.len();
                    break;
                }
            }
        } else if token_id == id("T_HALT_COMPILER") { halt = Some(3); }
        // A malformed lexer must always make progress, even in incomplete interpolation.
        if cursor <= before { break; }
    }
    if cursor < source.len() { push(&mut result, id("T_ENCAPSED_AND_WHITESPACE"), cursor, source.len(), &mut line, source); }
    result
}

/// Appends a token and advances the PHP line counter, coalescing adjacent inline text.
fn push(out: &mut Vec<Token>, token_id: i64, start: usize, end: usize, line: &mut usize, source: &[u8]) {
    if start == end { return; }
    if [id("T_INLINE_HTML"), id("T_ENCAPSED_AND_WHITESPACE")].contains(&token_id) && out.last().is_some_and(|t| t.id == token_id && t.pos + t.len == start) {
        out.last_mut().unwrap().len += end - start;
    } else { out.push(Token { id: token_id, pos: start, len: end - start, line: *line }); }
    *line += newline_count(&source[start..end]);
}

/// Identifies PHP doc comments by the required whitespace after the third asterisk.
fn comment_id(raw: &[u8]) -> i64 {
    id(if raw.starts_with(b"/**") && raw.get(3).is_some_and(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r')) {
        "T_DOC_COMMENT"
    } else { "T_COMMENT" })
}

/// Returns whether a byte can occur in a PHP identifier.
pub(super) fn identifier_part(b: u8) -> bool { b.is_ascii_alphanumeric() || b == b'_' || b >= 0x80 }

/// Returns whether a complete token is an unqualified identifier.
fn is_identifier(raw: &[u8]) -> bool {
    raw.first().is_some_and(|b| identifier_part(*b) && !b.is_ascii_digit()) && raw.iter().all(|b| identifier_part(*b))
}

/// Skips PHP whitespace and comments for Zend's enum/yield/reference lookahead.
fn skip_trivia(source: &[u8], mut at: usize) -> usize {
    loop {
        let before = at;
        while source.get(at).is_some_and(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n')) { at += 1; }
        if source.get(at..at + 2) == Some(b"/*") {
            at += 2;
            while at + 1 < source.len() && &source[at..at + 2] != b"*/" { at += 1; }
            at = (at + 2).min(source.len());
        } else if source.get(at..at + 2) == Some(b"//") || source.get(at) == Some(&b'#') && source.get(at + 1) != Some(&b'[') {
            while source.get(at).is_some_and(|b| !matches!(b, b'\r' | b'\n')) && source.get(at..at + 2) != Some(b"?>") { at += 1; }
        }
        if at == before { return at; }
    }
}

/// Maps the lexer vocabulary into PHP's named and single-character token vocabulary.
fn token_id(kind: TokenKind, raw: &[u8], tail: &[u8]) -> i64 {
    use TokenKind::*;
    let name = match kind {
        Whitespace => "T_WHITESPACE",
        InlineText | InlineShebang => "T_INLINE_HTML",
        OpenTag | ShortOpenTag => "T_OPEN_TAG", EchoTag => "T_OPEN_TAG_WITH_ECHO", CloseTag => "T_CLOSE_TAG",
        SingleLineComment | HashComment | MultiLineComment | DocBlockComment => return comment_id(raw),
        LiteralString => "T_CONSTANT_ENCAPSED_STRING", PartialLiteralString | StringPart => "T_ENCAPSED_AND_WHITESPACE",
        Variable => "T_VARIABLE", StringVariableName => "T_STRING_VARNAME", OffsetNumber => "T_NUM_STRING",
        Identifier if raw.eq_ignore_ascii_case(b"__halt_compiler") => "T_HALT_COMPILER",
        Identifier | OffsetString | Self_ | Parent | True | False | Null | From => "T_STRING",
        QualifiedIdentifier if raw.get(..10).is_some_and(|v| v.eq_ignore_ascii_case(b"namespace\\")) => "T_NAME_RELATIVE",
        QualifiedIdentifier => "T_NAME_QUALIFIED", FullyQualifiedIdentifier => "T_NAME_FULLY_QUALIFIED",
        LiteralInteger => return integer_id(raw), LiteralFloat => "T_DNUMBER",
        DocumentStart(_) => "T_START_HEREDOC", DocumentEnd => "T_END_HEREDOC",
        DollarLeftBrace => "T_DOLLAR_OPEN_CURLY_BRACES",
        Ampersand => {
            let at = skip_trivia(tail, 0);
            if tail.get(at) == Some(&b'$') || tail.get(at..at + 3) == Some(b"...") { "T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG" }
            else { "T_AMPERSAND_NOT_FOLLOWED_BY_VAR_OR_VARARG" }
        }
        Enum => {
            let at = skip_trivia(tail, 0);
            let rest = &tail[at..];
            if at > 0 && rest.first().is_some_and(|b| identifier_part(*b) && !b.is_ascii_digit())
                && !rest.get(..7).is_some_and(|s| s.eq_ignore_ascii_case(b"extends"))
                && !rest.get(..10).is_some_and(|s| s.eq_ignore_ascii_case(b"implements")) { "T_ENUM" } else { "T_STRING" }
        }
        IntCast | IntegerCast => "T_INT_CAST", FloatCast | DoubleCast | RealCast => "T_DOUBLE_CAST",
        StringCast | BinaryCast => "T_STRING_CAST", BoolCast | BooleanCast => "T_BOOL_CAST",
        ArrayCast => "T_ARRAY_CAST", ObjectCast => "T_OBJECT_CAST", UnsetCast => "T_UNSET_CAST", VoidCast => "T_VOID_CAST",
        ClassConstant => "T_CLASS_C", TraitConstant => "T_TRAIT_C", FunctionConstant => "T_FUNC_C",
        MethodConstant => "T_METHOD_C", NamespaceConstant => "T_NS_C", PropertyConstant => "T_PROPERTY_C",
        FileConstant => "T_FILE", DirConstant => "T_DIR", LineConstant => "T_LINE",
        Die | Exit => "T_EXIT", HaltCompiler => "T_HALT_COMPILER",
        And => "T_LOGICAL_AND", Or => "T_LOGICAL_OR", Xor => "T_LOGICAL_XOR",
        MinusGreaterThan => "T_OBJECT_OPERATOR", QuestionMinusGreaterThan => "T_NULLSAFE_OBJECT_OPERATOR",
        ColonColon => "T_DOUBLE_COLON", HashLeftBracket => "T_ATTRIBUTE", DotDotDot => "T_ELLIPSIS",
        EqualGreaterThan => "T_DOUBLE_ARROW", QuestionQuestion => "T_COALESCE", QuestionQuestionEqual => "T_COALESCE_EQUAL",
        PlusEqual => "T_PLUS_EQUAL", MinusEqual => "T_MINUS_EQUAL", AsteriskEqual => "T_MUL_EQUAL", SlashEqual => "T_DIV_EQUAL",
        DotEqual => "T_CONCAT_EQUAL", PercentEqual => "T_MOD_EQUAL", AmpersandEqual => "T_AND_EQUAL",
        PipeEqual => "T_OR_EQUAL", CaretEqual => "T_XOR_EQUAL", LeftShiftEqual => "T_SL_EQUAL", RightShiftEqual => "T_SR_EQUAL",
        PipePipe => "T_BOOLEAN_OR", AmpersandAmpersand => "T_BOOLEAN_AND", EqualEqual => "T_IS_EQUAL",
        BangEqual | LessThanGreaterThan => "T_IS_NOT_EQUAL", EqualEqualEqual => "T_IS_IDENTICAL", BangEqualEqual => "T_IS_NOT_IDENTICAL",
        LessThanEqual => "T_IS_SMALLER_OR_EQUAL", GreaterThanEqual => "T_IS_GREATER_OR_EQUAL", LessThanEqualGreaterThan => "T_SPACESHIP",
        LeftShift => "T_SL", RightShift => "T_SR", PlusPlus => "T_INC", MinusMinus => "T_DEC",
        AsteriskAsterisk => "T_POW", AsteriskAsteriskEqual => "T_POW_EQUAL", PipeGreaterThan => "T_PIPE",
        NamespaceSeparator => "T_NS_SEPARATOR", PrivateSet => "T_PRIVATE_SET", ProtectedSet => "T_PROTECTED_SET", PublicSet => "T_PUBLIC_SET",
        _ if raw.len() == 1 => return i64::from(raw[0]),
        _ => {
            let name = format!("T_{}", String::from_utf8_lossy(raw).to_ascii_uppercase());
            return elephc_builtin_contract::lookup_constant(&name).and_then(|c| match c.value {
                elephc_builtin_contract::ConstValue::Int(v) => Some(v), _ => None,
            }).unwrap_or_else(|| id("T_STRING"));
        }
    };
    id(name)
}

/// Distinguishes integer literals from integer-shaped values that overflow a PHP word.
fn integer_id(raw: &[u8]) -> i64 {
    let digits: Vec<u8> = raw.iter().copied().filter(|b| *b != b'_').collect();
    let (base, skip) = match digits.as_slice() {
        [b'0', b'x' | b'X', ..] => (16, 2), [b'0', b'b' | b'B', ..] => (2, 2),
        [b'0', b'o' | b'O', ..] => (8, 2), [b'0', _, ..] => (8, 1), _ => (10, 0),
    };
    let number = std::str::from_utf8(&digits[skip..]).ok().and_then(|s| i64::from_str_radix(s, base).ok());
    id(if number.is_some() || digits == b"0" { "T_LNUMBER" } else { "T_DNUMBER" })
}

/// Identifies the cast tokens whose PHP spelling permits spaces and tabs only.
fn is_cast(kind: TokenKind) -> bool {
    use TokenKind::*;
    matches!(kind, IntCast | IntegerCast | BoolCast | BooleanCast | FloatCast | DoubleCast | RealCast
        | StringCast | BinaryCast | ArrayCast | ObjectCast | UnsetCast | VoidCast)
}
