//! Purpose:
//! Projects Chopper byte scanner events to the token representation used by the compiler.
//!
//! Called from:
//! - The syntax lexer entry points for source files and TOKEN_PARSE validation.
//!
//! Key details:
//! - Source coordinates count Unicode scalars and identify the physical source file.
//! - The scanner determines token boundaries. Literal decoding preserves the
//!   compiler's current representation of interpolated strings.

use super::cursor::Cursor;
use super::literals;
use super::token::{spanned, SpannedToken, Token, TokenMetadata};
use crate::scanner::{scan_compiler, CompilerLexeme, CompilerOutput, Operator};
use crate::syntax::errors::CompileError;
use crate::syntax::span::Span;
use crate::syntax::source::SourceMode;

/// Tokenizes one source while assigning a physical source identity to its spans.
pub(super) fn scan_tokens_in_source(
    source: &str,
    mode: SourceMode,
    source_id: u32,
) -> Result<Vec<SpannedToken>, CompileError> {
    scan_tokens_with_semantics(
        source,
        mode,
        source_id,
        elephc_semantics::SemanticProfile::PERMISSIVE,
    )
}

/// Tokenizes one source with an explicit semantic policy for literal preservation.
pub(super) fn scan_tokens_with_semantics(
    source: &str,
    mode: SourceMode,
    source_id: u32,
    profile: elephc_semantics::SemanticProfile,
) -> Result<Vec<SpannedToken>, CompileError> {
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    let projection = scan_compiler(
        source.as_bytes(),
        mode.requires_open_tag(),
        mode == SourceMode::Lfc,
        crate::syntax::parser::validation_only(),
        Projection {
            cursor: Cursor::new_in_source(source, source_id, profile),
            tokens: Vec::new(),
            error: None,
        },
    );
    let Projection { mut cursor, mut tokens, error } = projection;
    if let Some(error) = error {
        return Err(error);
    }
    cursor.skip_bytes(cursor.remaining().len());
    tokens.push(spanned(Token::Eof, cursor.span()));
    super::qualified_names::identify_reserved_name_segments(&mut tokens);
    Ok(tokens)
}

/// Tracks physical positions and produces one syntax token stream.
struct Projection<'a> {
    cursor: Cursor<'a>,
    tokens: Vec<SpannedToken>,
    error: Option<CompileError>,
}

impl CompilerOutput for Projection<'_> {
    /// Materializes one scanner event and stops at the first compiler diagnostic.
    fn emit(&mut self, lexeme: CompilerLexeme, start: usize, end: usize) -> Option<usize> {
        self.cursor.skip_bytes(start - self.cursor.position());
        match self.project(lexeme, end - start) {
            Ok(()) => Some(self.cursor.position()),
            Err(error) => {
                self.error = Some(error);
                None
            }
        }
    }
}

impl Projection<'_> {
    /// Converts a bounded lexeme or decodes a string body opened by the byte scanner.
    fn project(&mut self, lexeme: CompilerLexeme, len: usize) -> Result<(), CompileError> {
        let span = self.cursor.span();
        let text = &self.cursor.remaining()[..len];
        self.cursor.skip_bytes(len);
        let token = match lexeme {
            CompilerLexeme::OpenTag => {
                self.tokens.push(spanned(Token::OpenTag, span));
                return Ok(());
            }
            CompilerLexeme::MissingOpenTag => {
                return Err(CompileError::new(span, "Expected '<?php' at start of file"));
            }
            CompilerLexeme::ForbiddenTag => {
                return Err(CompileError::new(
                    span,
                    "PHP opening and closing tags are not valid in .lfc source files",
                ));
            }
            CompilerLexeme::DoubleQuoted => {
                self.tokens.extend(literals::decode_double_quoted_body(&mut self.cursor, span)?);
                return Ok(());
            }
            CompilerLexeme::Heredoc => {
                self.tokens.extend(literals::scan_heredoc(&mut self.cursor)?);
                return Ok(());
            }
            CompilerLexeme::Attribute => Token::AttrOpen,
            CompilerLexeme::Variable => literals::variable_token(&text[1..], &self.cursor)?,
            CompilerLexeme::Word => literals::word_token(text),
            CompilerLexeme::Number(kind) => literals::number_token(text, kind, span, &self.cursor)?,
            CompilerLexeme::SingleQuoted { terminated } => literals::single_quoted_token(text, terminated, span)?,
            CompilerLexeme::Operator(operator) => operator_token(operator),
            CompilerLexeme::Byte(byte) => byte_token(byte, span)?,
        };
        let span = Span::with_end_from(span, self.cursor.span());
        let metadata = if matches!(lexeme, CompilerLexeme::Word)
            && !matches!(token, Token::Identifier(_))
            && token.canonical_word_spelling() != Some(text)
        {
            TokenMetadata::with_source_spelling(span, text)
        } else {
            TokenMetadata::new(span)
        };
        self.tokens.push((token, metadata));
        Ok(())
    }
}

/// Converts a lexical operator into the corresponding compiler token.
pub(crate) fn operator_token(operator: Operator) -> Token {
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

/// Converts punctuation or reports an unexpected source byte at its original position.
pub(crate) fn byte_token(byte: u8, span: Span) -> Result<Token, CompileError> {
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
            span,
            &format!("Unexpected character: '{}'", char::from(byte)),
        )),
    };
    Ok(token)
}
