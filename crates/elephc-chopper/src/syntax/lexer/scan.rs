//! Purpose:
//! Owns the main PHP token scanning loop and dispatches literal-specific scanners.
//! Skips whitespace/comments and emits structural, operator, keyword, and literal tokens.
//!
//! Called from:
//! - `crate::syntax::lexer::tokenize()`.
//!
//! Key details:
//! - Multi-character operators and PHP opening tags must be recognized before shorter prefixes.

use super::cursor::Cursor;
use super::literals;
use super::token::{spanned, SpannedToken, Token, TokenMetadata};
use crate::syntax::errors::CompileError;
use crate::syntax::source::SourceMode;
use crate::scanner::{compiler_trivia_len, identifier_start, operator, Operator};

/// Scans the full PHP source into a stream of syntax tokens with source metadata.
///
/// Requires `<?php` as the first five characters. Dispatches to `literals` for
/// strings (which may contain interpolation), heredoc/nowdoc, numbers, variables,
/// and keywords. Returns `Token::Eof` at end-of-input.
///
/// # Errors
/// Returns `CompileError` when PHP mode lacks its opening tag, LFC contains a
/// physical PHP tag at a code boundary, or either mode contains invalid syntax.
pub fn scan_tokens(
    source: &str,
    mode: SourceMode,
) -> Result<Vec<SpannedToken>, CompileError> {
    scan_tokens_in_source(source, mode, 0)
}

/// Tokenizes one physical source while assigning a disambiguating source identity to its spans.
pub(super) fn scan_tokens_in_source(
    source: &str,
    mode: SourceMode,
    source_id: u32,
) -> Result<Vec<SpannedToken>, CompileError> {
    // A leading UTF-8 byte-order mark (U+FEFF) is ignored, matching editors that save PHP
    // files as BOM-prefixed UTF-8; stripping it keeps the `<?php` open tag at the start.
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    let mut cursor = Cursor::new_in_source(source, source_id);
    let mut tokens = Vec::new();

    let span = cursor.span();
    if mode.requires_open_tag() {
        cursor.skip_bytes(compiler_trivia_len(cursor.remaining().as_bytes()));
        let span = cursor.span();
        if cursor.remaining().starts_with("<?php") {
            cursor.skip_ascii(5);
            tokens.push(spanned(Token::OpenTag, span));
        } else {
            return Err(CompileError::new(span, "Expected '<?php' at start of file"));
        }
    } else {
        tokens.push(spanned(Token::OpenTag, span));
    }

    loop {
        cursor.skip_bytes(compiler_trivia_len(cursor.remaining().as_bytes()));

        if cursor.is_eof() {
            tokens.push(spanned(Token::Eof, cursor.span()));
            break;
        }

        let span = cursor.span();
        let remaining = cursor.remaining();
        if matches!(mode, SourceMode::Lfc)
            && (remaining.starts_with("<?php") || remaining.starts_with("?>"))
        {
            return Err(CompileError::new(
                span,
                "PHP opening and closing tags are not valid in .lfc source files",
            ));
        } else if remaining.starts_with('"') {
            // Double-quoted strings may contain interpolation ($var)
            let string_tokens = literals::scan_double_string_interpolated(&mut cursor)?;
            tokens.extend(string_tokens);
        } else if remaining.starts_with("<<<") {
            // Heredoc/nowdoc — may contain interpolation ($var) for heredoc
            cursor.skip_ascii(3);
            let heredoc_tokens = literals::scan_heredoc(&mut cursor)?;
            tokens.extend(heredoc_tokens);
        } else {
            let starts_word = remaining.as_bytes().first().copied().is_some_and(identifier_start);
            let token = scan_token(&mut cursor, remaining.as_bytes())?;
            let end = cursor.span();
            let span = crate::syntax::span::Span::with_end_from(span, end);
            let metadata = if starts_word && !matches!(token, Token::Identifier(_)) {
                let consumed_len = remaining.len() - cursor.remaining().len();
                let source_spelling = &remaining[..consumed_len];
                if token.canonical_word_spelling() == Some(source_spelling) {
                    TokenMetadata::new(span)
                } else {
                    TokenMetadata::with_source_spelling(span, source_spelling)
                }
            } else {
                TokenMetadata::new(span)
            };
            tokens.push((token, metadata));
        }
    }

    super::qualified_names::identify_reserved_name_segments(&mut tokens);
    Ok(tokens)
}

/// Converts a lexical operator into the corresponding compiler token.
fn operator_token(operator: Operator) -> Token {
    match operator {
        Operator::StarStarAssign => Token::StarStarAssign,
        Operator::Ellipsis => Token::Ellipsis,
        Operator::Spaceship => Token::Spaceship,
        Operator::EqualEqualEqual => Token::EqualEqualEqual,
        Operator::NotEqualEqual => Token::NotEqualEqual,
        Operator::LessLessAssign => Token::LessLessAssign,
        Operator::GreaterGreaterAssign => Token::GreaterGreaterAssign,
        Operator::QuestionQuestionAssign => Token::QuestionQuestionAssign,
        Operator::QuestionArrow => Token::QuestionArrow,
        Operator::StarStar => Token::StarStar,
        Operator::PlusPlus => Token::PlusPlus,
        Operator::MinusMinus => Token::MinusMinus,
        Operator::DoubleArrow => Token::DoubleArrow,
        Operator::LessEqual => Token::LessEqual,
        Operator::GreaterEqual => Token::GreaterEqual,
        Operator::EqualEqual => Token::EqualEqual,
        Operator::NotEqual => Token::NotEqual,
        Operator::LessGreater => Token::LessGreater,
        Operator::PlusAssign => Token::PlusAssign,
        Operator::MinusAssign => Token::MinusAssign,
        Operator::StarAssign => Token::StarAssign,
        Operator::SlashAssign => Token::SlashAssign,
        Operator::DotAssign => Token::DotAssign,
        Operator::PercentAssign => Token::PercentAssign,
        Operator::AmpAssign => Token::AmpAssign,
        Operator::PipeAssign => Token::PipeAssign,
        Operator::CaretAssign => Token::CaretAssign,
        Operator::AndAnd => Token::AndAnd,
        Operator::OrOr => Token::OrOr,
        Operator::QuestionQuestion => Token::QuestionQuestion,
        Operator::PipeArrow => Token::PipeArrow,
        Operator::DoubleColon => Token::DoubleColon,
        Operator::Arrow => Token::Arrow,
        Operator::LessLess => Token::LessLess,
        Operator::GreaterGreater => Token::GreaterGreater,
    }
}

/// Dispatches one token to Chopper's operator scanner or a compiler literal adapter.
fn scan_token(cursor: &mut Cursor, remaining: &[u8]) -> Result<Token, CompileError> {
    let Some(&byte) = remaining.first() else { return Ok(Token::Eof); };
    if identifier_start(byte) {
        return literals::scan_keyword(cursor);
    }
    match byte {
        b'$' => return literals::scan_variable(cursor),
        b'\'' => return literals::scan_single_string(cursor),
        b'0'..=b'9' => return literals::scan_number(cursor),
        b'.' if remaining.get(1).is_some_and(u8::is_ascii_digit) => {
            return literals::scan_dot_float(cursor);
        }
        _ => {}
    }
    if let Some((op, len)) = operator(remaining) {
        cursor.skip_ascii(len);
        return Ok(operator_token(op));
    }
    if remaining.starts_with(b"#[") {
        cursor.skip_ascii(2);
        return Ok(Token::AttrOpen);
    }

    let token = match byte {
        b';' => Token::Semicolon,
        b',' => Token::Comma,
        b'\\' => Token::Backslash,
        b'?' => Token::Question,
        b':' => Token::Colon,
        b'(' => Token::LParen,
        b')' => Token::RParen,
        b'{' => Token::LBrace,
        b'}' => Token::RBrace,
        b'[' => Token::LBracket,
        b']' => Token::RBracket,
        b'=' => Token::Assign,
        b'!' => Token::Bang,
        b'&' => Token::Ampersand,
        b'|' => Token::Pipe,
        b'^' => Token::Caret,
        b'~' => Token::Tilde,
        b'<' => Token::Less,
        b'>' => Token::Greater,
        b'+' => Token::Plus,
        b'-' => Token::Minus,
        b'*' => Token::Star,
        b'/' => Token::Slash,
        b'%' => Token::Percent,
        b'.' => Token::Dot,
        b'@' => Token::At,
        _ => return Err(CompileError::new(
            cursor.span(),
            &format!("Unexpected character: '{}'", char::from(byte)),
        )),
    };
    cursor.skip_ascii(1);
    Ok(token)
}
