//! Purpose:
//! Adapts Chopper's source-preserving scan for the eval parser.
//!
//! Called from:
//! - `crate::parser::parse_fragment()` and runtime file loading.
//!
//! Key details:
//! - Original spelling is retained for keywords and decoded string values match Magician.
//! - Runtime lexical errors retain their EvalParseError category.

use elephc_chopper::scanner::{decode_single_quoted, scan_runtime, token_name};
use elephc_builtin_contract::string_literal::{push_escaped_byte, push_literal_char};

use crate::errors::EvalParseError;

use super::{Token, TokenKind, decode::{decode_double_quoted, decode_number, eval_operator, magic_const_token}};

/// Tokenizes an eval fragment that starts inside PHP mode.
pub(super) fn tokenize_fragment(source: &str) -> Result<Vec<Token>, EvalParseError> {
    tokenize(source.as_bytes(), false)
}

/// Scans a PHP file beginning in HTML mode with its original source line numbering.
pub(crate) fn tokenize_script(source: &[u8]) -> Result<Vec<Token>, EvalParseError> {
    tokenize(source, true)
}

/// Adapts Chopper code and HTML tokens to the input required by the runtime grammar.
fn tokenize(source: &[u8], file: bool) -> Result<Vec<Token>, EvalParseError> {
    let scan = scan_runtime(source, file).map_err(|_| EvalParseError::UnsupportedConstruct)?;
    let mut tokens = Vec::new();
    let mut html_origin = None;
    let mut text_cursor = TextCursor::new(source);
    let mut raw = scan.tokens.into_iter().peekable();
    while let Some(token) = raw.next() {
        let line = token.line as i64;
        let name = token_name(token.id);
        if name == Some("T_INLINE_HTML") {
            let line = if file { line } else { html_origin.ok_or(EvalParseError::UnexpectedToken)? };
            push_inline_html(&mut tokens, &source[token.pos..token.pos + token.len], line);
            continue;
        }
        let rest = text_cursor.at(token.pos).ok_or(EvalParseError::InvalidUtf8)?;
        let text = rest.get(..token.len).ok_or(EvalParseError::UnexpectedToken)?;
        let kind = match name {
            Some("T_WHITESPACE") => continue,
            Some("T_COMMENT" | "T_DOC_COMMENT")
                if text.starts_with("/*") && !text[2..].ends_with("*/") =>
            {
                return Err(EvalParseError::UnterminatedComment);
            }
            Some("T_COMMENT") if !text.starts_with("/**") => continue,
            Some("T_COMMENT" | "T_DOC_COMMENT") => TokenKind::DocComment(text.to_owned()),
            Some("T_CLOSE_TAG") => {
                html_origin = Some(line);
                TokenKind::Semicolon
            }
            Some("T_OPEN_TAG") => {
                if !file {
                    html_origin.take().ok_or(EvalParseError::UnexpectedToken)?;
                }
                continue;
            }
            Some("T_OPEN_TAG_WITH_ECHO") => {
                let line = if file { line } else { html_origin.take().ok_or(EvalParseError::UnexpectedToken)? };
                tokens.push(Token::new(TokenKind::Ident("echo".to_owned()), line));
                continue;
            }
            Some("T_VARIABLE") if text.is_ascii() => {
                TokenKind::DollarIdent(text[1..].to_owned())
            }
            None if text == "$" && raw.peek().is_some_and(|next| {
                next.pos == token.pos + 1 && next.len == 1 && source[next.pos] == b'{'
            }) => {
                raw.next();
                TokenKind::DollarLBrace
            }
            None if text == "$" => return Err(EvalParseError::ExpectedVariable),
            Some("T_NAME_QUALIFIED" | "T_NAME_FULLY_QUALIFIED" | "T_NAME_RELATIVE") if text.is_ascii() => {
                push_qualified_name(&mut tokens, text, line);
                continue;
            }
            Some("T_FILE" | "T_DIR" | "T_LINE" | "T_FUNC_C" | "T_CLASS_C" | "T_METHOD_C" | "T_NS_C" | "T_TRAIT_C") => {
                magic_const_token(text, line).ok_or(EvalParseError::UnexpectedToken)?
            }
            Some("T_LNUMBER" | "T_DNUMBER") => {
                decode_number(text, &rest[token.len..]).ok_or(EvalParseError::InvalidNumber)?
            }
            Some("T_ATTRIBUTE") => TokenKind::AttributeStart,
            Some(
                "T_ARRAY_CAST" | "T_BOOL_CAST" | "T_DOUBLE_CAST" | "T_INT_CAST" | "T_STRING_CAST" | "T_OBJECT_CAST" | "T_UNSET_CAST"
                | "T_PUBLIC_SET" | "T_PROTECTED_SET" | "T_PRIVATE_SET"
            ) => {
                let (visibility, inner) = text.strip_suffix(')').and_then(|body| body.split_once('('))
                    .ok_or(EvalParseError::UnexpectedToken)?;
                if !visibility.is_empty() {
                    tokens.push(Token::new(TokenKind::Ident(visibility.to_owned()), line));
                }
                tokens.push(Token::new(TokenKind::LParen, line));
                tokens.push(Token::new(TokenKind::Ident(inner.trim().to_owned()), line));
                tokens.push(Token::new(TokenKind::RParen, line));
                continue;
            }
            Some("T_COALESCE_EQUAL" | "T_IS_NOT_EQUAL" | "T_PIPE") if matches!(text, "??=" | "<>" | "|>") => {
                let (first, second) = match text {
                    "??=" => (TokenKind::QuestionQuestion, TokenKind::Equal),
                    "<>" => (TokenKind::Less, TokenKind::Greater),
                    _ => (TokenKind::Pipe, TokenKind::Greater),
                };
                tokens.push(Token::new(first, line));
                tokens.push(Token::new(second, line));
                continue;
            }
            Some("T_CONSTANT_ENCAPSED_STRING") if text.starts_with('"') => {
                let (literal_tokens, length) = decode_double_quoted(text, line)?;
                if length != token.len {
                    return Err(EvalParseError::UnexpectedToken);
                }
                tokens.extend(literal_tokens);
                continue;
            }
            None if text == "\"" => {
                let (literal_tokens, length) = decode_double_quoted(rest, line)?;
                let end = token.pos + length;
                loop {
                    let part = raw.next().ok_or(EvalParseError::UnterminatedString)?;
                    let part_end = part.pos + part.len;
                    if part_end >= end {
                        if part_end != end {
                            return Err(EvalParseError::UnexpectedToken);
                        }
                        break;
                    }
                }
                tokens.extend(literal_tokens);
                continue;
            }
            Some("T_ENCAPSED_AND_WHITESPACE") if text.starts_with('\'') => {
                return Err(EvalParseError::UnterminatedString);
            }
            Some("T_CONSTANT_ENCAPSED_STRING") => {
                let body = text.strip_prefix('\'').and_then(|body| body.strip_suffix('\''))
                    .ok_or(EvalParseError::UnexpectedToken)?;
                TokenKind::String(decode_single_quoted(body))
            }
            Some(_) if ascii_identifier(text) => {
                magic_const_token(text, line).unwrap_or_else(|| TokenKind::Ident(text.to_owned()))
            }
            _ => operator_or_punctuation(text).ok_or(EvalParseError::UnexpectedToken)?,
        };
        tokens.push(Token::new(kind, line));
    }
    let eof_line = source.iter().filter(|&&byte| byte == b'\n').count() as i64 + 1;
    tokens.push(Token::new(TokenKind::Eof, eof_line));
    Ok(tokens)
}

/// Tracks a UTF-8 run so decoding PHP tokens never rescans the remaining file.
struct TextCursor<'a> {
    chunks: std::str::Utf8Chunks<'a>,
    valid: &'a str,
    valid_start: usize,
    next_start: usize,
}

impl<'a> TextCursor<'a> {
    /// Starts before the first UTF-8 run.
    fn new(source: &'a [u8]) -> Self {
        Self { chunks: source.utf8_chunks(), valid: "", valid_start: 0, next_start: 0 }
    }

    /// Returns the valid source tail containing the next PHP token.
    fn at(&mut self, position: usize) -> Option<&'a str> {
        while position >= self.next_start {
            let chunk = self.chunks.next()?;
            self.valid = chunk.valid();
            self.valid_start = self.next_start;
            self.next_start += self.valid.len() + chunk.invalid().len();
        }
        let offset = position.checked_sub(self.valid_start)?;
        if offset >= self.valid.len() {
            return None;
        }
        self.valid.get(offset..)
    }
}

/// Emits inline HTML without changing any bytes, including invalid UTF-8.
fn push_inline_html(tokens: &mut Vec<Token>, html: &[u8], line: i64) {
    let mut text = String::with_capacity(html.len());
    for chunk in html.utf8_chunks() {
        for ch in chunk.valid().chars() {
            push_literal_char(ch, &mut text);
        }
        for &byte in chunk.invalid() {
            push_escaped_byte(byte, &mut text);
        }
    }
    tokens.push(Token::new(TokenKind::Ident("echo".to_owned()), line));
    tokens.push(Token::new(TokenKind::String(text), line));
    tokens.push(Token::new(TokenKind::Semicolon, line));
}

/// Expands an already scanned qualified name into Magician's compatibility token format.
fn push_qualified_name(tokens: &mut Vec<Token>, name: &str, line: i64) {
    let mut segments = name.split('\\').peekable();
    while let Some(segment) = segments.next() {
        if !segment.is_empty() {
            let kind = magic_const_token(segment, line).unwrap_or_else(|| {
                if segments.peek().is_some() && !segment.eq_ignore_ascii_case("namespace")
                    && crate::parser::is_reserved_word(segment)
                {
                    TokenKind::ReservedNameHead(segment.to_owned())
                } else {
                    TokenKind::Ident(segment.to_owned())
                }
            });
            tokens.push(Token::new(kind, line));
        }
        if segments.peek().is_some() {
            tokens.push(Token::new(TokenKind::Backslash, line));
        }
    }
}

/// Keeps the runtime lexer's ASCII identifier vocabulary for PHP words.
fn ascii_identifier(text: &str) -> bool {
    let mut bytes = text.bytes();
    bytes.next().is_some_and(|byte| byte == b'_' || byte.is_ascii_alphabetic())
        && bytes.all(|byte| byte == b'_' || byte.is_ascii_alphanumeric())
}

/// Adapts the original spelling of operators and punctuation in the shared subset.
fn operator_or_punctuation(text: &str) -> Option<TokenKind> {
    if let Some((op, len)) = eval_operator(text.as_bytes()) {
        return (len == text.len()).then_some(op);
    }
    Some(match text {
        "+" => TokenKind::Plus,
        "-" => TokenKind::Minus,
        "*" => TokenKind::Star,
        "/" => TokenKind::Slash,
        "%" => TokenKind::Percent,
        "!" => TokenKind::Bang,
        "~" => TokenKind::Tilde,
        "@" => TokenKind::At,
        "." => TokenKind::Dot,
        "=" => TokenKind::Equal,
        "<" => TokenKind::Less,
        ">" => TokenKind::Greater,
        "&" => TokenKind::Ampersand,
        "|" => TokenKind::Pipe,
        "^" => TokenKind::Caret,
        "?" => TokenKind::Question,
        ":" => TokenKind::Colon,
        "\\" => TokenKind::Backslash,
        "(" => TokenKind::LParen,
        ")" => TokenKind::RParen,
        "[" => TokenKind::LBracket,
        "]" => TokenKind::RBracket,
        "{" => TokenKind::LBrace,
        "}" => TokenKind::RBrace,
        "," => TokenKind::Comma,
        ";" => TokenKind::Semicolon,
        _ => return None,
    })
}
