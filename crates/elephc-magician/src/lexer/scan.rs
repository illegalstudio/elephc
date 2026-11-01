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
//! - Double-quoted literals and heredoc bodies are expanded by `super::strings` into
//!   concatenation token streams, so one source character can yield more than one token.
//! - Identifier characters follow PHP's byte rule, which admits every byte from `0x80` up.

use super::{Token, TokenKind};
use crate::errors::EvalParseError;
use crate::eval_ir::EvalMagicConst;

/// Tokenizes a complete source fragment and appends an EOF sentinel.
pub(crate) fn tokenize(source: &str) -> Result<Vec<Token>, EvalParseError> {
    Lexer::new(source).tokenize()
}

/// Tokenizes one code block of a larger file whose first line is `line`.
///
/// An included file is lexed block by block (`crate::script_cache::segments`), and every
/// block has to count lines from where it sits in the file, or `__LINE__` restarts at 1 after
/// each `?>`.
pub(crate) fn tokenize_at_line(source: &str, line: i64) -> Result<Vec<Token>, EvalParseError> {
    Lexer { line, ..Lexer::new(source) }.tokenize()
}

/// Converts a UTF-8 eval source fragment into parser tokens.
pub(super) struct Lexer<'a> {
    source: &'a str,
    pos: usize,
    line: i64,
}

impl<'a> Lexer<'a> {
    /// Creates a lexer over a UTF-8 eval fragment.
    ///
    /// `super::strings` builds a second lexer over an extracted heredoc body so the body
    /// runs through the same interpolation scanner a `"…"` literal uses.
    pub(super) fn new(source: &'a str) -> Self {
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
        if self.at_doc_comment() {
            return self.lex_doc_comment(line);
        }
        if ch == '"' {
            return self.lex_double_quoted(line);
        }
        if ch == '<'
            && self.peek_next_char() == Some('<')
            && self.peek_nth_char(2) == Some('<')
        {
            return self.lex_heredoc(line);
        }
        if ch == '?' && self.peek_next_char() == Some('>') {
            return Ok(self.lex_close_tag(line));
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
                    if self.peek_char() == Some('=') {
                        self.bump_char();
                        Ok(TokenKind::QuestionQuestionEqual)
                    } else {
                        Ok(TokenKind::QuestionQuestion)
                    }
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
                if let Some(magic) = magic_const_token(&ident, line) {
                    Ok(magic)
                } else if self.reserved_word_leads_qualified_name(&ident) {
                    Ok(TokenKind::ReservedNameHead(ident))
                } else {
                    Ok(TokenKind::Ident(ident))
                }
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
        // `$$name`: the name of the variable is the value of `$name`. The inner `$` is lexed on
        // the next call, so `$$$x` nests the same way.
        if self.peek_char() == Some('$') {
            return Ok(TokenKind::Dollar);
        }
        let name = self.lex_ident();
        if name.is_empty() {
            return Err(EvalParseError::ExpectedVariable);
        }
        Ok(TokenKind::DollarIdent(name))
    }

    /// Returns whether the reserved word just read is glued to a `\` and a further segment, so
    /// that PHP 8 reads it as the first segment of a qualified name (`Function\Lib\g()`,
    /// `use Default\Theme\Palette;`). `namespace` never qualifies: a leading `namespace\` is
    /// the relative-name prefix. Ordinary identifiers stay `Ident` either way.
    fn reserved_word_leads_qualified_name(&self, word: &str) -> bool {
        let mut rest = self.source[self.pos..].chars();
        !word.eq_ignore_ascii_case("namespace")
            && crate::parser::is_reserved_word(word)
            && rest.next() == Some('\\')
            && rest.next().is_some_and(is_ident_start)
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

    /// Reads an integer or float literal, in every base PHP writes one in.
    ///
    /// This used to read decimal digits only, which made `0700` the integer SEVEN HUNDRED
    /// instead of 448, and made `0x1F`, `0b101`, `0o17` and `1_000` reject the whole fragment
    /// as invalid. Both are reachable from a plain `eval('return mkdir($p, 0700, true);')`.
    ///
    /// The grammar mirrors the compiler's own `lexer::literals::numbers::scan_number`, which
    /// is the authority: `0x`/`0X` hex, `0b`/`0B` binary, `0o`/`0O` octal, legacy leading-zero
    /// octal, `_` separators anywhere between digits, and decimal with an optional fraction
    /// and exponent. A literal too large for `i64` becomes a float, as it does in PHP.
    fn lex_number(&mut self) -> Result<TokenKind, EvalParseError> {
        // A radix prefix needs a digit after it: php lexes `0x` and `0b2` as `0` followed by the
        // identifier `x` / `b2`, and the PARSER then refuses them in token order.
        if self.peek_char() == Some('0') {
            let first = self.peek_nth_char(2);
            match self.peek_next_char() {
                Some('x' | 'X') if first.is_some_and(|c| c.is_ascii_hexdigit()) => {
                    return self.lex_radix_number(16, |c| c.is_ascii_hexdigit())
                }
                Some('o' | 'O') if first.is_some_and(|c| c.is_ascii_digit() && c < '8') => {
                    return self.lex_radix_number(8, |c| c.is_ascii_digit() && c < '8')
                }
                Some('b' | 'B') if first.is_some_and(|c| c == '0' || c == '1') => {
                    return self.lex_radix_number(2, |c| c == '0' || c == '1')
                }
                _ => {}
            }
        }

        let mut digits = self.lex_digits(|c| c.is_ascii_digit());
        if digits.is_empty() {
            return Err(EvalParseError::InvalidNumber);
        }

        let has_fraction =
            self.peek_char() == Some('.') && matches!(self.peek_next_char(), Some('0'..='9'));
        let has_exponent = self.exponent_follows();
        if has_fraction || has_exponent {
            if has_fraction {
                digits.push('.');
                self.bump_char();
                digits.push_str(&self.lex_digits(|c| c.is_ascii_digit()));
            }
            if self.exponent_follows() {
                digits.push('e');
                self.bump_char();
                if let Some(sign @ ('+' | '-')) = self.peek_char() {
                    digits.push(sign);
                    self.bump_char();
                }
                digits.push_str(&self.lex_digits(|c| c.is_ascii_digit()));
            }
            return digits
                .parse::<f64>()
                .map(TokenKind::Float)
                .map_err(|_| EvalParseError::InvalidNumber);
        }

        // A leading zero followed by more digits is PHP's legacy octal spelling. `8` and `9`
        // are not octal digits, so `078` is a PHP parse error ("Invalid numeric literal") and
        // has to be REFUSED here — the conversion below would otherwise reach a `to_digit(8)`
        // that cannot answer, and panic the interpreter.
        if digits.len() > 1 && digits.starts_with('0') {
            let octal = &digits[1..];
            if !octal.chars().all(|ch| ('0'..'8').contains(&ch)) {
                return Err(EvalParseError::InvalidNumber);
            }
            return Ok(eval_radix_int_or_float(octal, 8));
        }
        Ok(match digits.parse::<i64>() {
            Ok(value) => TokenKind::Int(value),
            // PHP promotes an integer literal past `PHP_INT_MAX` to a float rather than
            // rejecting it.
            Err(_) => TokenKind::Float(digits.parse::<f64>().map_err(|_| {
                EvalParseError::InvalidNumber
            })?),
        })
    }

    /// Reads the digits of a `0x` / `0o` / `0b` literal, prefix included.
    fn lex_radix_number(
        &mut self,
        radix: u32,
        is_digit: impl Fn(char) -> bool,
    ) -> Result<TokenKind, EvalParseError> {
        self.bump_char();
        self.bump_char();
        let digits = self.lex_digits(is_digit);
        if digits.is_empty() {
            return Err(EvalParseError::InvalidNumber);
        }
        Ok(eval_radix_int_or_float(&digits, radix))
    }

    /// Returns whether an `e`/`E` at the cursor starts an exponent: a digit follows it, after an
    /// optional sign. `1e` is `1` followed by the identifier `e` in php.
    ///
    /// A literal running into letters is NOT refused here. php lexes `123abc`, `0x1G` and `0o78`
    /// as two tokens and its PARSER reports the second ("unexpected identifier"), in token order:
    /// refusing in the lexer reported a literal deep in an eval'd fragment before an earlier
    /// syntax error php names first. Only `078`, an invalid legacy octal, is a lexical error.
    fn exponent_follows(&self) -> bool {
        matches!(self.peek_char(), Some('e' | 'E'))
            && match self.peek_next_char() {
                Some('0'..='9') => true,
                Some('+' | '-') => matches!(self.peek_nth_char(2), Some('0'..='9')),
                _ => false,
            }
    }

    /// Reads a run of digits accepted by `is_digit`, dropping PHP's `_` separators.
    fn lex_digits(&mut self, is_digit: impl Fn(char) -> bool) -> String {
        let mut digits = String::new();
        while let Some(ch) = self.peek_char() {
            if ch == '_' {
                // A separator is only legal BETWEEN digits, which is what this checks.
                if digits.is_empty() || !self.peek_next_char().is_some_and(&is_digit) {
                    break;
                }
                self.bump_char();
                continue;
            }
            if !is_digit(ch) {
                break;
            }
            digits.push(ch);
            self.bump_char();
        }
        digits
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
                        elephc_builtin_contract::string_literal::push_literal_char(other, &mut out);
                    }
                }
            } else {
                elephc_builtin_contract::string_literal::push_literal_char(ch, &mut out);
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
                (Some('/'), Some('*')) if self.at_doc_comment() => return Ok(()),
                (Some('/'), Some('*')) => self.skip_block_comment()?,
                _ => return Ok(()),
            }
        }
    }

    /// Reports whether the cursor opens a doc comment rather than an ordinary block comment.
    ///
    /// PHP's scanner rule is `"/**"{WHITESPACE}`: the third star must be followed by a space,
    /// tab or newline. `token_get_all()` under PHP 8.5 names `/**/`, `/***/` and `/**x*/`
    /// `T_COMMENT` and only `/** x */` `T_DOC_COMMENT`, so the shorter forms stay trivia.
    fn at_doc_comment(&self) -> bool {
        self.peek_char() == Some('/')
            && self.peek_next_char() == Some('*')
            && self.peek_nth_char(2) == Some('*')
            && matches!(self.peek_nth_char(3), Some(' ' | '\t' | '\n' | '\r'))
    }

    /// Returns one PHP doc comment as a parser-visible metadata token.
    ///
    /// Ordinary comments remain trivia, but `/** ... */` is retained verbatim so reflection
    /// can report the declaration metadata that PHP exposes through `getDocComment()`.
    fn lex_doc_comment(&mut self, line: i64) -> Result<Vec<Token>, EvalParseError> {
        let start = self.pos;
        self.skip_block_comment()?;
        Ok(vec![Token::new(
            TokenKind::DocComment(self.source[start..self.pos].to_owned()),
            line,
        )])
    }

    /// Advances past a `//` or `#` comment, including its trailing newline when present.
    ///
    /// A `?>` ends the comment too, and is left for `next_tokens` to lex as a close tag:
    /// `eval('// c ?>X')` prints `X` in reference PHP 8.5.10.
    fn skip_line_comment(&mut self) {
        while let Some(ch) = self.peek_char() {
            if ch == '?' && self.peek_next_char() == Some('>') {
                return;
            }
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

    /// Returns the unconsumed fragment suffix for multi-line literal scanners.
    pub(super) fn remaining(&self) -> &str {
        &self.source[self.pos..]
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
///
/// PHP's identifier rule is a byte rule, not a Unicode one: every byte from `0x80` up is
/// accepted, so `class ©` and `class Café` are both legal declarations. `php -n` 8.5.6 runs
/// `class \xA9 { public const V = 5; } echo \xA9::V;` and prints `5`.
///
/// Source bytes that are not valid UTF-8 never reach this scanner as themselves: the parser
/// entry point rewrites each of them to a private-use marker in `U+F0000..=U+F00FF` first.
/// Those markers sit above `0x80` too, so the one range test admits an identifier written in
/// UTF-8 and one written in a single-byte encoding without knowing which it is looking at.
pub(super) fn is_ident_start(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphabetic() || ch >= '\u{80}'
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

/// Converts radix digits to an integer, or to a float when they overflow `i64`.
///
/// PHP promotes an over-large literal in any base to a float rather than rejecting it, so the
/// accumulation mirrors the compiler's `radix_digits_to_float`.
fn eval_radix_int_or_float(digits: &str, radix: u32) -> TokenKind {
    if let Ok(value) = i64::from_str_radix(digits, radix) {
        return TokenKind::Int(value);
    }
    let radix_float = f64::from(radix);
    let mut value = 0.0_f64;
    for ch in digits.chars() {
        let digit = ch
            .to_digit(radix)
            .expect("scanner only passes digits valid for this radix");
        value = value * radix_float + f64::from(digit);
    }
    TokenKind::Float(value)
}
