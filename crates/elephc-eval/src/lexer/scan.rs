//! Purpose:
//! Scans UTF-8 eval source fragments into eval parser tokens.
//! This file owns trivia skipping, literal lexing, PHP string escapes, and
//! magic-constant token recognition.
//!
//! Called from:
//! - `crate::lexer::tokenize()` re-exported by `crate::lexer`.
//!
//! Key details:
//! - Comments and whitespace advance line metadata for `__LINE__`.
//! - Unterminated strings or block comments return parse errors before grammar parsing.

use super::TokenKind;
use crate::errors::EvalParseError;
use crate::eval_ir::EvalMagicConst;

/// Tokenizes a complete source fragment and appends an EOF sentinel.
pub(crate) fn tokenize(source: &str) -> Result<Vec<TokenKind>, EvalParseError> {
    Lexer::new(source).tokenize()
}

/// Converts a UTF-8 eval source fragment into parser tokens.
struct Lexer<'a> {
    source: &'a str,
    pos: usize,
    line: i64,
}

impl<'a> Lexer<'a> {
    /// Creates a lexer over a UTF-8 eval fragment.
    fn new(source: &'a str) -> Self {
        Self {
            source,
            pos: 0,
            line: 1,
        }
    }

    /// Tokenizes the complete source and appends an EOF sentinel.
    fn tokenize(mut self) -> Result<Vec<TokenKind>, EvalParseError> {
        let mut tokens = Vec::new();
        loop {
            let token = self.next_token()?;
            let done = token == TokenKind::Eof;
            tokens.push(token);
            if done {
                break;
            }
        }
        Ok(tokens)
    }

    /// Reads the next token from the source.
    fn next_token(&mut self) -> Result<TokenKind, EvalParseError> {
        self.skip_trivia()?;
        let Some(ch) = self.peek_char() else {
            return Ok(TokenKind::Eof);
        };
        let line = self.line;
        match ch {
            '$' => self.lex_variable(),
            '\'' | '"' => self.lex_string(ch),
            '0'..='9' => self.lex_number(),
            '+' => {
                self.bump_char();
                if self.peek_char() == Some('+') {
                    self.bump_char();
                    Ok(TokenKind::PlusPlus)
                } else if self.peek_char() == Some('=') {
                    self.bump_char();
                    Ok(TokenKind::PlusEqual)
                } else {
                    Ok(TokenKind::Plus)
                }
            }
            '-' => {
                self.bump_char();
                if self.peek_char() == Some('>') {
                    self.bump_char();
                    Ok(TokenKind::Arrow)
                } else if self.peek_char() == Some('-') {
                    self.bump_char();
                    Ok(TokenKind::MinusMinus)
                } else if self.peek_char() == Some('=') {
                    self.bump_char();
                    Ok(TokenKind::MinusEqual)
                } else {
                    Ok(TokenKind::Minus)
                }
            }
            '*' => {
                self.bump_char();
                if self.peek_char() == Some('*') {
                    self.bump_char();
                    if self.peek_char() == Some('=') {
                        self.bump_char();
                        Ok(TokenKind::StarStarEqual)
                    } else {
                        Ok(TokenKind::StarStar)
                    }
                } else if self.peek_char() == Some('=') {
                    self.bump_char();
                    Ok(TokenKind::StarEqual)
                } else {
                    Ok(TokenKind::Star)
                }
            }
            '/' => {
                self.bump_char();
                if self.peek_char() == Some('=') {
                    self.bump_char();
                    Ok(TokenKind::SlashEqual)
                } else {
                    Ok(TokenKind::Slash)
                }
            }
            '%' => {
                self.bump_char();
                if self.peek_char() == Some('=') {
                    self.bump_char();
                    Ok(TokenKind::PercentEqual)
                } else {
                    Ok(TokenKind::Percent)
                }
            }
            '.' => {
                self.bump_char();
                if self.peek_char() == Some('.') && self.peek_next_char() == Some('.') {
                    self.bump_char();
                    self.bump_char();
                    Ok(TokenKind::Ellipsis)
                } else if self.peek_char() == Some('=') {
                    self.bump_char();
                    Ok(TokenKind::DotEqual)
                } else {
                    Ok(TokenKind::Dot)
                }
            }
            '=' => {
                self.bump_char();
                if self.peek_char() == Some('=') {
                    self.bump_char();
                    if self.peek_char() == Some('=') {
                        self.bump_char();
                        Ok(TokenKind::EqualEqualEqual)
                    } else {
                        Ok(TokenKind::EqualEqual)
                    }
                } else if self.peek_char() == Some('>') {
                    self.bump_char();
                    Ok(TokenKind::FatArrow)
                } else {
                    Ok(TokenKind::Equal)
                }
            }
            '!' => {
                self.bump_char();
                if self.peek_char() == Some('=') {
                    self.bump_char();
                    if self.peek_char() == Some('=') {
                        self.bump_char();
                        Ok(TokenKind::NotEqualEqual)
                    } else {
                        Ok(TokenKind::NotEqual)
                    }
                } else {
                    Ok(TokenKind::Bang)
                }
            }
            '&' => {
                self.bump_char();
                if self.peek_char() == Some('&') {
                    self.bump_char();
                    Ok(TokenKind::AndAnd)
                } else if self.peek_char() == Some('=') {
                    self.bump_char();
                    Ok(TokenKind::AmpEqual)
                } else {
                    Ok(TokenKind::Ampersand)
                }
            }
            '|' => {
                self.bump_char();
                if self.peek_char() == Some('|') {
                    self.bump_char();
                    Ok(TokenKind::OrOr)
                } else if self.peek_char() == Some('=') {
                    self.bump_char();
                    Ok(TokenKind::PipeEqual)
                } else {
                    Ok(TokenKind::Pipe)
                }
            }
            '^' => {
                self.bump_char();
                if self.peek_char() == Some('=') {
                    self.bump_char();
                    Ok(TokenKind::CaretEqual)
                } else {
                    Ok(TokenKind::Caret)
                }
            }
            '~' => {
                self.bump_char();
                Ok(TokenKind::Tilde)
            }
            '<' => {
                self.bump_char();
                if self.peek_char() == Some('<') {
                    self.bump_char();
                    if self.peek_char() == Some('=') {
                        self.bump_char();
                        Ok(TokenKind::LessLessEqual)
                    } else {
                        Ok(TokenKind::LessLess)
                    }
                } else if self.peek_char() == Some('=') {
                    self.bump_char();
                    if self.peek_char() == Some('>') {
                        self.bump_char();
                        Ok(TokenKind::Spaceship)
                    } else {
                        Ok(TokenKind::LessEqual)
                    }
                } else {
                    Ok(TokenKind::Less)
                }
            }
            '>' => {
                self.bump_char();
                if self.peek_char() == Some('>') {
                    self.bump_char();
                    if self.peek_char() == Some('=') {
                        self.bump_char();
                        Ok(TokenKind::GreaterGreaterEqual)
                    } else {
                        Ok(TokenKind::GreaterGreater)
                    }
                } else if self.peek_char() == Some('=') {
                    self.bump_char();
                    Ok(TokenKind::GreaterEqual)
                } else {
                    Ok(TokenKind::Greater)
                }
            }
            '?' => {
                self.bump_char();
                if self.peek_char() == Some('?') {
                    self.bump_char();
                    Ok(TokenKind::QuestionQuestion)
                } else {
                    Ok(TokenKind::Question)
                }
            }
            ';' => {
                self.bump_char();
                Ok(TokenKind::Semicolon)
            }
            '(' => {
                self.bump_char();
                Ok(TokenKind::LParen)
            }
            ')' => {
                self.bump_char();
                Ok(TokenKind::RParen)
            }
            '[' => {
                self.bump_char();
                Ok(TokenKind::LBracket)
            }
            ']' => {
                self.bump_char();
                Ok(TokenKind::RBracket)
            }
            '{' => {
                self.bump_char();
                Ok(TokenKind::LBrace)
            }
            '}' => {
                self.bump_char();
                Ok(TokenKind::RBrace)
            }
            ',' => {
                self.bump_char();
                Ok(TokenKind::Comma)
            }
            ':' => {
                self.bump_char();
                if self.peek_char() == Some(':') {
                    self.bump_char();
                    Ok(TokenKind::DoubleColon)
                } else {
                    Ok(TokenKind::Colon)
                }
            }
            '\\' => {
                self.bump_char();
                Ok(TokenKind::Backslash)
            }
            _ if is_ident_start(ch) => {
                let ident = self.lex_ident();
                Ok(magic_const_token(&ident, line).unwrap_or(TokenKind::Ident(ident)))
            }
            _ => Err(EvalParseError::UnexpectedToken),
        }
    }

    /// Reads a `$name` token.
    fn lex_variable(&mut self) -> Result<TokenKind, EvalParseError> {
        self.bump_char();
        let name = self.lex_ident();
        if name.is_empty() {
            return Err(EvalParseError::ExpectedVariable);
        }
        Ok(TokenKind::DollarIdent(name))
    }

    /// Reads a PHP identifier body at the current byte offset.
    fn lex_ident(&mut self) -> String {
        let mut ident = String::new();
        while let Some(ch) = self.peek_char() {
            if !is_ident_continue(ch) {
                break;
            }
            ident.push(ch);
            self.bump_char();
        }
        ident
    }

    /// Reads an integer or float literal.
    fn lex_number(&mut self) -> Result<TokenKind, EvalParseError> {
        let start = self.pos;
        while matches!(self.peek_char(), Some('0'..='9')) {
            self.bump_char();
        }
        let mut is_float = false;
        if self.peek_char() == Some('.') && matches!(self.peek_next_char(), Some('0'..='9')) {
            is_float = true;
            self.bump_char();
            while matches!(self.peek_char(), Some('0'..='9')) {
                self.bump_char();
            }
        }
        let raw = &self.source[start..self.pos];
        if is_float {
            raw.parse::<f64>()
                .map(TokenKind::Float)
                .map_err(|_| EvalParseError::InvalidNumber)
        } else {
            raw.parse::<i64>()
                .map(TokenKind::Int)
                .map_err(|_| EvalParseError::InvalidNumber)
        }
    }

    /// Reads a single- or double-quoted string literal.
    fn lex_string(&mut self, quote: char) -> Result<TokenKind, EvalParseError> {
        self.bump_char();
        let mut out = String::new();
        while let Some(ch) = self.peek_char() {
            self.bump_char();
            if ch == quote {
                return Ok(TokenKind::String(out));
            }
            if ch == '\\' {
                let Some(escaped) = self.peek_char() else {
                    return Err(EvalParseError::UnterminatedString);
                };
                self.bump_char();
                if quote == '\'' {
                    match escaped {
                        '\\' => out.push('\\'),
                        '\'' => out.push('\''),
                        other => {
                            out.push('\\');
                            out.push(other);
                        }
                    }
                } else {
                    match escaped {
                        'n' => out.push('\n'),
                        'r' => out.push('\r'),
                        't' => out.push('\t'),
                        'v' => out.push('\x0b'),
                        'e' => out.push('\x1b'),
                        'f' => out.push('\x0c'),
                        '\\' => out.push('\\'),
                        '"' => out.push('"'),
                        '$' => out.push('$'),
                        other => {
                            out.push('\\');
                            out.push(other);
                        }
                    }
                }
            } else {
                out.push(ch);
            }
        }
        Err(EvalParseError::UnterminatedString)
    }

    /// Advances past ASCII/Unicode whitespace and PHP comments.
    fn skip_trivia(&mut self) -> Result<(), EvalParseError> {
        loop {
            while self.peek_char().is_some_and(char::is_whitespace) {
                self.bump_char();
            }
            match (self.peek_char(), self.peek_next_char()) {
                (Some('/'), Some('/')) => self.skip_line_comment(),
                (Some('#'), _) => self.skip_line_comment(),
                (Some('/'), Some('*')) => self.skip_block_comment()?,
                _ => return Ok(()),
            }
        }
    }

    /// Advances past a `//` or `#` comment, including its trailing newline when present.
    fn skip_line_comment(&mut self) {
        while let Some(ch) = self.peek_char() {
            self.bump_char();
            if ch == '\n' {
                break;
            }
        }
    }

    /// Advances past a `/* ... */` comment while preserving fragment line metadata.
    fn skip_block_comment(&mut self) -> Result<(), EvalParseError> {
        self.bump_char();
        self.bump_char();
        while let Some(ch) = self.peek_char() {
            if ch == '*' && self.peek_next_char() == Some('/') {
                self.bump_char();
                self.bump_char();
                return Ok(());
            }
            self.bump_char();
        }
        Err(EvalParseError::UnterminatedComment)
    }

    /// Returns the current char without advancing.
    fn peek_char(&self) -> Option<char> {
        self.source[self.pos..].chars().next()
    }

    /// Returns the char after the current char without advancing.
    fn peek_next_char(&self) -> Option<char> {
        let mut chars = self.source[self.pos..].chars();
        chars.next()?;
        chars.next()
    }

    /// Advances by one UTF-8 char.
    fn bump_char(&mut self) {
        if let Some(ch) = self.peek_char() {
            self.pos += ch.len_utf8();
            if ch == '\n' {
                self.line += 1;
            }
        }
    }
}

/// Returns true for the first character of a PHP variable/function identifier.
fn is_ident_start(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphabetic()
}

/// Returns true for subsequent characters in a PHP variable/function identifier.
fn is_ident_continue(ch: char) -> bool {
    is_ident_start(ch) || ch.is_ascii_digit()
}

/// Converts a PHP magic-constant identifier into a parser token when recognized.
fn magic_const_token(name: &str, line: i64) -> Option<TokenKind> {
    let magic = if ident_eq(name, "__FILE__") {
        EvalMagicConst::File
    } else if ident_eq(name, "__DIR__") {
        EvalMagicConst::Dir
    } else if ident_eq(name, "__LINE__") {
        EvalMagicConst::Line(line)
    } else if ident_eq(name, "__FUNCTION__") {
        EvalMagicConst::Function
    } else if ident_eq(name, "__CLASS__") {
        EvalMagicConst::Class
    } else if ident_eq(name, "__METHOD__") {
        EvalMagicConst::Method
    } else if ident_eq(name, "__NAMESPACE__") {
        EvalMagicConst::Namespace
    } else if ident_eq(name, "__TRAIT__") {
        EvalMagicConst::Trait
    } else {
        return None;
    };
    Some(TokenKind::Magic(magic))
}

/// Compares a source identifier to a PHP keyword using ASCII case-insensitive rules.
fn ident_eq(actual: &str, expected: &str) -> bool {
    actual.eq_ignore_ascii_case(expected)
}
