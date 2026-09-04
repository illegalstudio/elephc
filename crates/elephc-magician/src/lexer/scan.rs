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
//! - Double-quoted literals are expanded by `super::strings` into concatenation token
//!   streams, so one source character can yield more than one token.

use super::{Token, TokenKind};
use crate::errors::EvalParseError;
use crate::eval_ir::EvalMagicConst;
use std::num::IntErrorKind;

/// Every word PHP's grammar reserves. A numeric literal butted straight against one of
/// them ends at the keyword, because PHP's own lexer stops a number at the first
/// character that cannot continue it and hands the rest to the parser:
///
/// ```text
/// $ php -n -r 'var_dump(1and 2);'  => bool(true)
/// $ php -n -r 'var_dump(1xor 2);'  => bool(false)
/// ```
///
/// Mirrors `elephc::lexer::literals::numbers::RESERVED_WORDS_AFTER_NUMBER` so a literal
/// means the same thing compiled ahead of time and evaluated inside `eval()`.
const RESERVED_WORDS_AFTER_NUMBER: &[&str] = &[
    "abstract", "and", "array", "as", "break", "callable", "case", "catch", "class", "clone",
    "const", "continue", "declare", "default", "die", "do", "echo", "else", "elseif", "empty",
    "enddeclare", "endfor", "endforeach", "endif", "endswitch", "endwhile", "enum", "eval",
    "exit", "extends", "final", "finally", "fn", "for", "foreach", "function", "global", "goto",
    "if", "implements", "include", "include_once", "instanceof", "insteadof", "interface",
    "isset", "list", "match", "namespace", "new", "or", "print", "private", "protected",
    "public", "readonly", "require", "require_once", "return", "static", "switch", "throw",
    "trait", "try", "unset", "use", "var", "while", "xor", "yield",
];

/// Tokenizes a complete source fragment and appends an EOF sentinel.
pub(crate) fn tokenize(source: &str) -> Result<Vec<Token>, EvalParseError> {
    Lexer::new(source).tokenize()
}

/// Converts a UTF-8 eval source fragment into parser tokens.
pub(super) struct Lexer<'a> {
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
    fn tokenize(mut self) -> Result<Vec<Token>, EvalParseError> {
        let mut tokens = Vec::new();
        loop {
            let batch = self.next_tokens()?;
            let done = batch
                .last()
                .is_some_and(|token| *token.kind() == TokenKind::Eof);
            tokens.extend(batch);
            if done {
                break;
            }
        }
        Ok(tokens)
    }

    /// Reads the next token batch from the source.
    ///
    /// Every source construct except a double-quoted literal yields exactly one token.
    /// A double-quoted literal yields one `TokenKind::String` when it contains no
    /// interpolation, and a parenthesized concatenation token stream when it does.
    fn next_tokens(&mut self) -> Result<Vec<Token>, EvalParseError> {
        self.skip_trivia()?;
        let Some(ch) = self.peek_char() else {
            return Ok(vec![Token::new(TokenKind::Eof, self.line)]);
        };
        let line = self.line;
        if ch == '"' {
            return self.lex_double_quoted(line);
        }
        let kind = match ch {
            '$' => self.lex_variable(),
            '\'' => self.lex_single_quoted(),
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
                if self.peek_char() == Some('-') && self.peek_next_char() == Some('>') {
                    self.bump_char();
                    self.bump_char();
                    Ok(TokenKind::QuestionArrow)
                } else if self.peek_char() == Some('?') {
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
            '@' => {
                self.bump_char();
                Ok(TokenKind::At)
            }
            '#' if self.peek_next_char() == Some('[') => {
                self.bump_char();
                self.bump_char();
                Ok(TokenKind::AttributeStart)
            }
            _ if is_ident_start(ch) => {
                let ident = self.lex_ident();
                Ok(magic_const_token(&ident, line).unwrap_or(TokenKind::Ident(ident)))
            }
            _ => Err(EvalParseError::UnexpectedToken),
        }?;
        Ok(vec![Token::new(kind, line)])
    }

    /// Reads a `$name` token.
    fn lex_variable(&mut self) -> Result<TokenKind, EvalParseError> {
        self.bump_char();
        if self.peek_char() == Some('{') {
            self.bump_char();
            return Ok(TokenKind::DollarLBrace);
        }
        let name = self.lex_ident();
        if name.is_empty() {
            return Err(EvalParseError::ExpectedVariable);
        }
        Ok(TokenKind::DollarIdent(name))
    }

    /// Reads a PHP identifier body at the current byte offset.
    pub(super) fn lex_ident(&mut self) -> String {
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

    /// Collects digits accepted by `is_digit`, allowing a single `_` BETWEEN two of them
    /// (PHP 7.4+ numeric separator). A leading, trailing or doubled `_` is left on the
    /// cursor so [`Self::reject_trailing_alnum`] can refuse it. Returns the digits with
    /// the separators stripped.
    fn scan_radix_digits<F: Fn(char) -> bool>(&mut self, is_digit: F) -> String {
        let mut digits = String::new();
        while let Some(ch) = self.peek_char() {
            if is_digit(ch) {
                digits.push(ch);
                self.bump_char();
            } else if ch == '_'
                && !digits.is_empty()
                && self.peek_next_char().is_some_and(&is_digit)
            {
                self.bump_char();
            } else {
                break;
            }
        }
        digits
    }

    /// Refuses a numeric literal followed immediately by an alphanumeric or `_`.
    ///
    /// This is what turns `0o78`, `0xfg`, `0b12`, `1_` and `1__0` into refusals instead
    /// of silently truncating the literal at the first out-of-range character and lexing
    /// the remainder as a separate token.
    ///
    /// A following PHP reserved word is the one case that is NOT malformed: PHP's lexer
    /// stops a number at the first character that cannot continue it, so `1and 2` is the
    /// expression `1 and 2` and `php -n -r 'var_dump(1and 2);'` prints `bool(true)`.
    fn reject_trailing_alnum(&self) -> Result<(), EvalParseError> {
        match self.peek_char() {
            Some(ch) if (ch.is_ascii_alphanumeric() || ch == '_') && !self.next_word_is_reserved() => {
                Err(EvalParseError::InvalidNumber)
            }
            _ => Ok(()),
        }
    }

    /// Returns true when the identifier starting at the cursor is a PHP reserved word, so
    /// the numeric literal just scanned ends here rather than being refused.
    fn next_word_is_reserved(&self) -> bool {
        let word: String = self.source[self.pos..]
            .chars()
            .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
            .collect();
        RESERVED_WORDS_AFTER_NUMBER
            .iter()
            .any(|keyword| word.eq_ignore_ascii_case(keyword))
    }

    /// Converts `digits` to an `i64`, promoting to a float on positive overflow the way
    /// PHP does (`echo 9223372036854775808;` prints `9.2233720368548E+18`).
    fn radix_int_or_float(digits: &str, radix: u32) -> Result<TokenKind, EvalParseError> {
        match i64::from_str_radix(digits, radix) {
            Ok(value) => Ok(TokenKind::Int(value)),
            Err(err) if *err.kind() == IntErrorKind::PosOverflow => {
                let radix_float = f64::from(radix);
                let value = digits.chars().fold(0.0_f64, |acc, ch| {
                    let digit = ch
                        .to_digit(radix)
                        .expect("scanner only collects valid radix digits");
                    acc * radix_float + f64::from(digit)
                });
                Ok(TokenKind::Float(value))
            }
            Err(_) => Err(EvalParseError::InvalidNumber),
        }
    }

    /// Reads an integer or float literal.
    ///
    /// Mirrors `crate::lexer::literals::numbers::scan_number` in the AOT compiler so a
    /// literal means the same thing whether it is compiled ahead of time or evaluated in
    /// an `eval()` fragment: hex `0x`/`0X`, explicit octal `0o`/`0O`, binary `0b`/`0B`,
    /// legacy leading-`0` octal, `_` separators, scientific notation, and
    /// overflow-promotes-to-float.
    fn lex_number(&mut self) -> Result<TokenKind, EvalParseError> {
        if self.peek_char() == Some('0') {
            if let Some(prefix) = self.peek_next_char() {
                let radix_scan = match prefix {
                    'x' | 'X' => Some((16_u32, (|c: char| c.is_ascii_hexdigit()) as fn(char) -> bool)),
                    'o' | 'O' => Some((8, (|c: char| c.is_ascii_digit() && c < '8') as fn(char) -> bool)),
                    'b' | 'B' => Some((2, (|c: char| c == '0' || c == '1') as fn(char) -> bool)),
                    _ => None,
                };
                if let Some((radix, is_digit)) = radix_scan {
                    self.bump_char();
                    self.bump_char();
                    let digits = self.scan_radix_digits(is_digit);
                    if digits.is_empty() {
                        return Err(EvalParseError::InvalidNumber);
                    }
                    self.reject_trailing_alnum()?;
                    return Self::radix_int_or_float(&digits, radix);
                }
            }
        }

        let mut raw = self.scan_radix_digits(|c| c.is_ascii_digit());

        let has_fraction = self.peek_char() == Some('.')
            && self.peek_next_char().is_some_and(|c| c.is_ascii_digit());
        let has_exponent = matches!(self.peek_char(), Some('e' | 'E'));

        if has_fraction || has_exponent {
            if has_fraction {
                raw.push('.');
                self.bump_char();
                raw.push_str(&self.scan_radix_digits(|c| c.is_ascii_digit()));
            }
            if matches!(self.peek_char(), Some('e' | 'E')) {
                raw.push('e');
                self.bump_char();
                if let Some(sign @ ('+' | '-')) = self.peek_char() {
                    raw.push(sign);
                    self.bump_char();
                }
                raw.push_str(&self.scan_radix_digits(|c| c.is_ascii_digit()));
            }
            self.reject_trailing_alnum()?;
            return raw
                .parse::<f64>()
                .map(TokenKind::Float)
                .map_err(|_| EvalParseError::InvalidNumber);
        }

        self.reject_trailing_alnum()?;

        // A leading `0` on a multi-digit literal is PHP's legacy octal form, so `0700`
        // is 448 and `08` is refused rather than read as decimal.
        if raw.len() > 1 && raw.starts_with('0') {
            return Self::radix_int_or_float(&raw, 8);
        }

        match raw.parse::<i64>() {
            Ok(value) => Ok(TokenKind::Int(value)),
            Err(err) if *err.kind() == IntErrorKind::PosOverflow => raw
                .parse::<f64>()
                .map(TokenKind::Float)
                .map_err(|_| EvalParseError::InvalidNumber),
            Err(_) => Err(EvalParseError::InvalidNumber),
        }
    }

    /// Reads a single-quoted string literal, which never interpolates.
    fn lex_single_quoted(&mut self) -> Result<TokenKind, EvalParseError> {
        self.bump_char();
        let mut out = String::new();
        while let Some(ch) = self.peek_char() {
            self.bump_char();
            if ch == '\'' {
                return Ok(TokenKind::String(out));
            }
            if ch == '\\' {
                let Some(escaped) = self.peek_char() else {
                    return Err(EvalParseError::UnterminatedString);
                };
                self.bump_char();
                match escaped {
                    '\\' => out.push('\\'),
                    '\'' => out.push('\''),
                    other => {
                        out.push('\\');
                        out.push(other);
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
                (Some('#'), Some('[')) => return Ok(()),
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
    pub(super) fn peek_char(&self) -> Option<char> {
        self.source[self.pos..].chars().next()
    }

    /// Returns the char after the current char without advancing.
    pub(super) fn peek_next_char(&self) -> Option<char> {
        let mut chars = self.source[self.pos..].chars();
        chars.next()?;
        chars.next()
    }

    /// Returns the char `offset` positions ahead of the cursor without advancing.
    ///
    /// `offset` 0 is the current char, so this generalizes `peek_char`/`peek_next_char`
    /// for the three-character lookahead that `"$obj->prop"` interpolation needs.
    pub(super) fn peek_nth_char(&self, offset: usize) -> Option<char> {
        self.source[self.pos..].chars().nth(offset)
    }

    /// Advances by one UTF-8 char.
    pub(super) fn bump_char(&mut self) {
        if let Some(ch) = self.peek_char() {
            self.pos += ch.len_utf8();
            if ch == '\n' {
                self.line += 1;
            }
        }
    }
}

/// Returns true for the first character of a PHP variable/function identifier.
pub(super) fn is_ident_start(ch: char) -> bool {
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
