//! Purpose:
//! Parses runtime PHP eval fragments into the initial EvalIR statement form.
//! The parser handles a small statement subset now and keeps unsupported syntax
//! as parse failure until the full eval parser is implemented.
//!
//! Called from:
//! - `crate::__elephc_eval_execute()`
//! - Future `crate::interpreter` entry points.
//!
//! Key details:
//! - PHP eval fragments are statement fragments and must not include opening
//!   `<?` / `<?php` tags.
//! - Fragment line metadata is tracked by the lexer; file and directory metadata
//!   is supplied by the eval context at execution time.

use crate::errors::EvalParseError;
use crate::eval_ir::{
    EvalArrayElement, EvalBinOp, EvalCallArg, EvalConst, EvalExpr, EvalMagicConst, EvalMatchArm,
    EvalProgram, EvalStmt, EvalSwitchCase, EvalUnaryOp,
};
use std::collections::HashMap;

/// Parses an eval fragment into by-name EvalIR statements.
pub fn parse_fragment(code: &[u8]) -> Result<EvalProgram, EvalParseError> {
    if contains_php_open_tag(code) {
        return Err(EvalParseError::PhpOpenTag);
    }
    let source = std::str::from_utf8(code).map_err(|_| EvalParseError::InvalidUtf8)?;
    let tokens = Lexer::new(source).tokenize()?;
    Parser::new(tokens, code.len()).parse_program()
}

/// Returns true when a fragment contains a PHP opening tag sequence.
fn contains_php_open_tag(code: &[u8]) -> bool {
    code.windows(2).any(|window| window == b"<?")
}

/// Token kinds used by the initial eval fragment parser.
#[derive(Debug, Clone, PartialEq)]
enum TokenKind {
    DollarIdent(String),
    Ident(String),
    Magic(EvalMagicConst),
    Int(i64),
    Float(f64),
    String(String),
    Plus,
    PlusPlus,
    PlusEqual,
    Minus,
    MinusMinus,
    MinusEqual,
    Arrow,
    Star,
    StarStar,
    StarStarEqual,
    StarEqual,
    Slash,
    SlashEqual,
    Percent,
    PercentEqual,
    Ampersand,
    AmpEqual,
    Pipe,
    PipeEqual,
    Caret,
    CaretEqual,
    Tilde,
    Dot,
    DotEqual,
    Ellipsis,
    Equal,
    EqualEqual,
    EqualEqualEqual,
    Bang,
    NotEqual,
    NotEqualEqual,
    AndAnd,
    OrOr,
    Less,
    LessEqual,
    Spaceship,
    LessLess,
    LessLessEqual,
    Greater,
    GreaterEqual,
    GreaterGreater,
    GreaterGreaterEqual,
    FatArrow,
    Question,
    QuestionQuestion,
    Semicolon,
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Comma,
    Colon,
    Backslash,
    Eof,
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
                Ok(TokenKind::Colon)
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

/// Parses tokenized eval fragments into EvalIR.
struct Parser {
    tokens: Vec<TokenKind>,
    pos: usize,
    source_len: usize,
    namespace: String,
    imports: NamespaceImports,
    allow_use_imports: bool,
}

/// A parsed PHP name plus whether it used a leading global namespace separator.
struct ParsedQualifiedName {
    name: String,
    absolute: bool,
}

/// Import alias tables active for the current namespace declaration region.
#[derive(Default)]
struct NamespaceImports {
    classes: HashMap<String, String>,
    functions: HashMap<String, String>,
    constants: HashMap<String, String>,
}

/// The `use` declaration namespace being imported.
#[derive(Copy, Clone, Eq, PartialEq)]
enum UseImportKind {
    Class,
    Function,
    Const,
}

impl NamespaceImports {
    /// Stores one class import under PHP's case-insensitive class alias key.
    fn insert_class(&mut self, alias: String, name: String) {
        self.classes.insert(alias.to_ascii_lowercase(), name);
    }

    /// Stores one function import under PHP's case-insensitive function alias key.
    fn insert_function(&mut self, alias: String, name: String) {
        self.functions.insert(alias.to_ascii_lowercase(), name);
    }

    /// Stores one constant import under PHP's case-sensitive constant alias key.
    fn insert_constant(&mut self, alias: String, name: String) {
        self.constants.insert(alias, name);
    }

    /// Resolves a class import, including aliases used as the first segment of a class name.
    fn resolve_class(&self, name: &str) -> Option<String> {
        let (first, tail) = split_first_name_segment(name);
        let imported = self.classes.get(&first.to_ascii_lowercase())?;
        Some(match tail {
            Some(tail) => format!("{imported}\\{tail}"),
            None => imported.clone(),
        })
    }

    /// Resolves an unqualified function alias.
    fn resolve_function(&self, name: &str) -> Option<&str> {
        self.functions
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }

    /// Resolves a case-sensitive unqualified constant alias.
    fn resolve_constant(&self, name: &str) -> Option<&str> {
        self.constants.get(name).map(String::as_str)
    }
}

impl Parser {
    /// Creates a parser over tokens produced from a source fragment.
    fn new(tokens: Vec<TokenKind>, source_len: usize) -> Self {
        Self {
            tokens,
            pos: 0,
            source_len,
            namespace: String::new(),
            imports: NamespaceImports::default(),
            allow_use_imports: true,
        }
    }

    /// Parses a complete eval fragment until EOF.
    fn parse_program(mut self) -> Result<EvalProgram, EvalParseError> {
        let mut statements = Vec::new();
        while !matches!(self.current(), TokenKind::Eof) {
            statements.extend(self.parse_stmt()?);
        }
        Ok(EvalProgram::new(self.source_len, statements))
    }

    /// Parses one source statement, expanding `unset($a, $b)` to one statement per variable.
    fn parse_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        match self.current() {
            TokenKind::Ident(name) if ident_eq(name, "break") => {
                self.advance();
                self.expect_semicolon()?;
                Ok(vec![EvalStmt::Break])
            }
            TokenKind::Ident(name) if ident_eq(name, "continue") => {
                self.advance();
                self.expect_semicolon()?;
                Ok(vec![EvalStmt::Continue])
            }
            TokenKind::Ident(name) if ident_eq(name, "do") => self.parse_do_while_stmt(),
            TokenKind::Ident(name) if ident_eq(name, "echo") => {
                self.advance();
                let mut statements = vec![EvalStmt::Echo(self.parse_expr()?)];
                while self.consume(TokenKind::Comma) {
                    statements.push(EvalStmt::Echo(self.parse_expr()?));
                }
                self.expect_semicolon()?;
                Ok(statements)
            }
            TokenKind::Ident(name) if ident_eq(name, "for") => self.parse_for_stmt(),
            TokenKind::Ident(name) if ident_eq(name, "foreach") => self.parse_foreach_stmt(),
            TokenKind::Ident(name) if ident_eq(name, "class") => self.parse_class_decl_stmt(),
            TokenKind::Ident(name) if ident_eq(name, "function") => self.parse_function_decl_stmt(),
            TokenKind::Ident(name) if ident_eq(name, "global") => self.parse_global_stmt(),
            TokenKind::Ident(name) if ident_eq(name, "if") => self.parse_if_stmt(),
            TokenKind::Ident(name) if ident_eq(name, "namespace") => self.parse_namespace_stmt(),
            TokenKind::Ident(name) if ident_eq(name, "return") => {
                self.advance();
                if self.consume_semicolon() {
                    return Ok(vec![EvalStmt::Return(None)]);
                }
                let expr = self.parse_expr()?;
                self.expect_semicolon()?;
                Ok(vec![EvalStmt::Return(Some(expr))])
            }
            TokenKind::Ident(name) if ident_eq(name, "static") => self.parse_static_var_stmt(),
            TokenKind::Ident(name) if ident_eq(name, "switch") => self.parse_switch_stmt(),
            TokenKind::Ident(name) if ident_eq(name, "throw") => self.parse_throw_stmt(),
            TokenKind::Ident(name) if ident_eq(name, "unset") => self.parse_unset_stmt(),
            TokenKind::Ident(name) if ident_eq(name, "use") && self.allow_use_imports => {
                self.parse_use_stmt()
            }
            TokenKind::Ident(name) if ident_eq(name, "use") => {
                Err(EvalParseError::UnsupportedConstruct)
            }
            TokenKind::Ident(name) if ident_eq(name, "while") => self.parse_while_stmt(),
            TokenKind::Ident(name) if is_unsupported_statement_keyword(name) => {
                Err(EvalParseError::UnsupportedConstruct)
            }
            TokenKind::PlusPlus | TokenKind::MinusMinus => self.parse_prefix_inc_dec_stmt(true),
            TokenKind::DollarIdent(_) if matches!(self.peek(), TokenKind::Arrow) => {
                self.parse_property_stmt(true)
            }
            TokenKind::DollarIdent(name) if matches!(self.peek(), TokenKind::LBracket) => {
                self.parse_array_set_stmt(name.clone())
            }
            TokenKind::DollarIdent(name)
                if matches!(self.peek(), TokenKind::PlusPlus | TokenKind::MinusMinus) =>
            {
                self.parse_postfix_inc_dec_stmt(name.clone(), true)
            }
            TokenKind::DollarIdent(name) if assignment_op(self.peek()).is_some() => {
                let name = name.clone();
                self.parse_var_store_stmt(name, true)
            }
            TokenKind::Eof => Err(EvalParseError::UnexpectedEof),
            _ => {
                let expr = self.parse_expr()?;
                self.expect_semicolon()?;
                Ok(vec![EvalStmt::Expr(expr)])
            }
        }
    }

    /// Parses `do { ... } while (expr);`.
    fn parse_do_while_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        let body = self.parse_statement_body()?;
        if !matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "while")) {
            return Err(EvalParseError::UnexpectedToken);
        }
        self.advance();
        self.expect(TokenKind::LParen)?;
        let condition = self.parse_expr()?;
        self.expect(TokenKind::RParen)?;
        self.expect_semicolon()?;
        Ok(vec![EvalStmt::DoWhile { body, condition }])
    }

    /// Parses `$name[index] = expr;` and `$name[] = expr;` eval writes.
    fn parse_array_set_stmt(&mut self, name: String) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        self.expect(TokenKind::LBracket)?;
        if self.consume(TokenKind::RBracket) {
            self.expect(TokenKind::Equal)?;
            let value = self.parse_expr()?;
            self.expect_semicolon()?;
            return Ok(vec![EvalStmt::ArrayAppendVar { name, value }]);
        }
        let index = self.parse_expr()?;
        self.expect(TokenKind::RBracket)?;
        self.expect(TokenKind::Equal)?;
        let value = self.parse_expr()?;
        self.expect_semicolon()?;
        Ok(vec![EvalStmt::ArraySetVar { name, index, value }])
    }

    /// Parses `for (init; condition; update) { ... }`.
    fn parse_for_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        self.expect(TokenKind::LParen)?;
        let init = self.parse_for_init_clause()?;
        self.expect_semicolon()?;
        let condition = if matches!(self.current(), TokenKind::Semicolon) {
            None
        } else {
            Some(self.parse_expr()?)
        };
        self.expect_semicolon()?;
        let update = self.parse_for_update_clause()?;
        let body = self.parse_statement_body()?;
        Ok(vec![EvalStmt::For {
            init,
            condition,
            update,
            body,
        }])
    }

    /// Parses `foreach (expr as $value) { ... }` or `foreach (expr as $key => $value) { ... }`.
    fn parse_foreach_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        self.expect(TokenKind::LParen)?;
        let array = self.parse_expr()?;
        if !matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "as")) {
            return Err(EvalParseError::UnexpectedToken);
        }
        self.advance();
        let TokenKind::DollarIdent(value_name) = self.current() else {
            return Err(EvalParseError::ExpectedVariable);
        };
        let value_name = value_name.clone();
        self.advance();
        let (key_name, value_name) = if matches!(self.current(), TokenKind::FatArrow) {
            self.advance();
            let TokenKind::DollarIdent(next_value_name) = self.current() else {
                return Err(EvalParseError::ExpectedVariable);
            };
            let key_name = value_name;
            let value_name = next_value_name.clone();
            self.advance();
            (Some(key_name), value_name)
        } else {
            (None, value_name)
        };
        self.expect(TokenKind::RParen)?;
        let body = self.parse_statement_body()?;
        Ok(vec![EvalStmt::Foreach {
            array,
            key_name,
            value_name,
            body,
        }])
    }

    /// Parses an empty `class Name {}` declaration for dynamic class-name registration.
    fn parse_class_decl_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        let TokenKind::Ident(name) = self.current() else {
            return Err(EvalParseError::UnexpectedToken);
        };
        let name = self.qualify_name_in_current_namespace(name);
        self.advance();
        self.expect(TokenKind::LBrace)?;
        if !self.consume(TokenKind::RBrace) {
            return Err(EvalParseError::UnsupportedConstruct);
        }
        self.consume_semicolon();
        Ok(vec![EvalStmt::ClassDecl { name }])
    }

    /// Parses `function name($param, ...) { ... }` declarations.
    fn parse_function_decl_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        let TokenKind::Ident(name) = self.current() else {
            return Err(EvalParseError::UnexpectedToken);
        };
        let name = self.qualify_name_in_current_namespace(name);
        self.advance();
        self.expect(TokenKind::LParen)?;
        let params = self.parse_function_params()?;
        let body = self.parse_block()?;
        Ok(vec![EvalStmt::FunctionDecl { name, params, body }])
    }

    /// Parses `namespace Name;` or `namespace Name { ... }` eval namespace blocks.
    fn parse_namespace_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        let namespace = if self.consume(TokenKind::LBrace) {
            return self.parse_namespace_block(String::new());
        } else {
            self.parse_namespace_name()?
        };
        if self.consume_semicolon() {
            self.namespace = namespace;
            self.imports = NamespaceImports::default();
            return Ok(Vec::new());
        }
        self.expect(TokenKind::LBrace)?;
        self.parse_namespace_block(namespace)
    }

    /// Parses statements inside an already opened namespace block.
    fn parse_namespace_block(&mut self, namespace: String) -> Result<Vec<EvalStmt>, EvalParseError> {
        let previous = std::mem::replace(&mut self.namespace, namespace);
        let previous_imports = std::mem::take(&mut self.imports);
        let previous_allow_use_imports = std::mem::replace(&mut self.allow_use_imports, true);
        let result = self.parse_block_contents();
        self.namespace = previous;
        self.imports = previous_imports;
        self.allow_use_imports = previous_allow_use_imports;
        result
    }

    /// Parses a namespace declaration name without a leading global separator.
    fn parse_namespace_name(&mut self) -> Result<String, EvalParseError> {
        let name = self.parse_qualified_name()?;
        if name.absolute {
            return Err(EvalParseError::UnexpectedToken);
        }
        Ok(name.name)
    }

    /// Parses PHP `use`, `use function`, and `use const` import declarations.
    fn parse_use_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        let kind = self.parse_use_import_kind();

        loop {
            self.parse_use_import(kind)?;
            if !self.consume(TokenKind::Comma) {
                break;
            }
        }
        self.expect_semicolon()?;
        Ok(Vec::new())
    }

    /// Parses an optional top-level `function` or `const` use-import kind.
    fn parse_use_import_kind(&mut self) -> UseImportKind {
        if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "function")) {
            self.advance();
            UseImportKind::Function
        } else if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "const")) {
            self.advance();
            UseImportKind::Const
        } else {
            UseImportKind::Class
        }
    }

    /// Parses and registers one comma-separated import entry.
    fn parse_use_import(&mut self, kind: UseImportKind) -> Result<(), EvalParseError> {
        let (name, grouped) = self.parse_use_name_or_group_start()?;
        if grouped {
            return self.parse_grouped_use_imports(kind, name);
        }
        self.parse_use_alias_and_register(kind, name)
    }

    /// Parses a use-import name, stopping after a trailing namespace separator before `{`.
    fn parse_use_name_or_group_start(&mut self) -> Result<(String, bool), EvalParseError> {
        let _ = self.consume(TokenKind::Backslash);
        let TokenKind::Ident(first) = self.current() else {
            return Err(EvalParseError::UnexpectedToken);
        };
        let mut name = first.clone();
        self.advance();
        while self.consume(TokenKind::Backslash) {
            if self.consume(TokenKind::LBrace) {
                return Ok((name, true));
            }
            let TokenKind::Ident(part) = self.current() else {
                return Err(EvalParseError::UnexpectedToken);
            };
            name.push('\\');
            name.push_str(part);
            self.advance();
        }
        Ok((name, false))
    }

    /// Parses all members inside a grouped namespace import declaration.
    fn parse_grouped_use_imports(
        &mut self,
        default_kind: UseImportKind,
        prefix: String,
    ) -> Result<(), EvalParseError> {
        if matches!(self.current(), TokenKind::RBrace) {
            return Err(EvalParseError::UnexpectedToken);
        }
        loop {
            let kind = self.parse_grouped_use_entry_kind(default_kind)?;
            let member = self.parse_grouped_use_member_name()?;
            let name = join_grouped_use_name(&prefix, &member);
            self.parse_use_alias_and_register(kind, name)?;
            if !self.consume(TokenKind::Comma) {
                break;
            }
            if self.consume(TokenKind::RBrace) {
                return Ok(());
            }
        }
        self.expect(TokenKind::RBrace)
    }

    /// Parses an optional per-entry grouped import kind, matching PHP's mixed group rules.
    fn parse_grouped_use_entry_kind(
        &mut self,
        default_kind: UseImportKind,
    ) -> Result<UseImportKind, EvalParseError> {
        if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "function")) {
            if default_kind != UseImportKind::Class {
                return Err(EvalParseError::UnexpectedToken);
            }
            self.advance();
            return Ok(UseImportKind::Function);
        }
        if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "const")) {
            if default_kind != UseImportKind::Class {
                return Err(EvalParseError::UnexpectedToken);
            }
            self.advance();
            return Ok(UseImportKind::Const);
        }
        Ok(default_kind)
    }

    /// Parses one non-absolute member name inside a grouped use declaration.
    fn parse_grouped_use_member_name(&mut self) -> Result<String, EvalParseError> {
        let name = self.parse_qualified_name()?;
        if name.absolute {
            return Err(EvalParseError::UnexpectedToken);
        }
        Ok(name.name)
    }

    /// Parses an optional alias and stores one namespace import.
    fn parse_use_alias_and_register(
        &mut self,
        kind: UseImportKind,
        name: String,
    ) -> Result<(), EvalParseError> {
        let alias = if matches!(
            self.current(),
            TokenKind::Ident(keyword) if ident_eq(keyword, "as")
        ) {
            self.advance();
            let TokenKind::Ident(alias) = self.current() else {
                return Err(EvalParseError::UnexpectedToken);
            };
            let alias = alias.clone();
            self.advance();
            alias
        } else {
            last_name_segment(&name).to_string()
        };

        match kind {
            UseImportKind::Class => self.imports.insert_class(alias, name),
            UseImportKind::Function => self.imports.insert_function(alias, name),
            UseImportKind::Const => self.imports.insert_constant(alias, name),
        }
        Ok(())
    }

    /// Parses `global $name, $other;` declarations in eval fragments.
    fn parse_global_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        let mut vars = Vec::new();
        loop {
            let TokenKind::DollarIdent(name) = self.current() else {
                return Err(EvalParseError::ExpectedVariable);
            };
            vars.push(name.clone());
            self.advance();
            if !self.consume(TokenKind::Comma) {
                break;
            }
        }
        self.expect_semicolon()?;
        Ok(vec![EvalStmt::Global { vars }])
    }

    /// Parses `static $name = expr;` declarations in eval fragments.
    fn parse_static_var_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        let TokenKind::DollarIdent(name) = self.current() else {
            return Err(EvalParseError::ExpectedVariable);
        };
        let name = name.clone();
        self.advance();
        self.expect(TokenKind::Equal)?;
        let init = self.parse_expr()?;
        self.expect_semicolon()?;
        Ok(vec![EvalStmt::StaticVar { name, init }])
    }

    /// Parses `throw expr;` statements in eval fragments.
    fn parse_throw_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        let expr = self.parse_expr()?;
        self.expect_semicolon()?;
        Ok(vec![EvalStmt::Throw(expr)])
    }

    /// Parses a dynamic function declaration parameter list after `(`.
    fn parse_function_params(&mut self) -> Result<Vec<String>, EvalParseError> {
        let mut params = Vec::new();
        if self.consume(TokenKind::RParen) {
            return Ok(params);
        }
        loop {
            let TokenKind::DollarIdent(name) = self.current() else {
                return Err(EvalParseError::ExpectedVariable);
            };
            params.push(name.clone());
            self.advance();
            if !self.consume(TokenKind::Comma) {
                break;
            }
            if matches!(self.current(), TokenKind::RParen) {
                return Err(EvalParseError::ExpectedVariable);
            }
        }
        self.expect(TokenKind::RParen)?;
        Ok(params)
    }

    /// Parses the optional first clause of a `for` loop.
    fn parse_for_init_clause(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        if matches!(self.current(), TokenKind::Semicolon) {
            return Ok(Vec::new());
        }
        self.parse_for_clause_stmt()
    }

    /// Parses the optional update clause of a `for` loop.
    fn parse_for_update_clause(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        if self.consume(TokenKind::RParen) {
            return Ok(Vec::new());
        }
        let statements = self.parse_for_clause_stmt()?;
        self.expect(TokenKind::RParen)?;
        Ok(statements)
    }

    /// Parses one statement-like `for` clause without consuming a delimiter.
    fn parse_for_clause_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        match self.current() {
            TokenKind::PlusPlus | TokenKind::MinusMinus => self.parse_prefix_inc_dec_stmt(false),
            TokenKind::DollarIdent(name) if matches!(self.peek(), TokenKind::LBracket) => {
                self.parse_array_set_clause(name.clone())
            }
            TokenKind::DollarIdent(_) if matches!(self.peek(), TokenKind::Arrow) => {
                self.parse_property_stmt(false)
            }
            TokenKind::DollarIdent(name)
                if matches!(self.peek(), TokenKind::PlusPlus | TokenKind::MinusMinus) =>
            {
                self.parse_postfix_inc_dec_stmt(name.clone(), false)
            }
            TokenKind::DollarIdent(name) if assignment_op(self.peek()).is_some() => {
                let name = name.clone();
                self.parse_var_store_stmt(name, false)
            }
            _ => {
                let expr = self.parse_expr()?;
                Ok(vec![EvalStmt::Expr(expr)])
            }
        }
    }

    /// Parses `$name[index] = expr` and `$name[] = expr` in a `for` clause.
    fn parse_array_set_clause(&mut self, name: String) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        self.expect(TokenKind::LBracket)?;
        if self.consume(TokenKind::RBracket) {
            self.expect(TokenKind::Equal)?;
            let value = self.parse_expr()?;
            return Ok(vec![EvalStmt::ArrayAppendVar { name, value }]);
        }
        let index = self.parse_expr()?;
        self.expect(TokenKind::RBracket)?;
        self.expect(TokenKind::Equal)?;
        let value = self.parse_expr()?;
        Ok(vec![EvalStmt::ArraySetVar { name, index, value }])
    }

    /// Parses `$name = expr` and simple variable compound assignments.
    fn parse_var_store_stmt(
        &mut self,
        name: String,
        require_semicolon: bool,
    ) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        let Some(op) = assignment_op(self.current()) else {
            return Err(EvalParseError::UnexpectedToken);
        };
        self.advance();
        if op.is_none() && matches!(self.current(), TokenKind::Ampersand) {
            self.advance();
            let TokenKind::DollarIdent(source) = self.current() else {
                return Err(EvalParseError::ExpectedVariable);
            };
            let source = source.clone();
            self.advance();
            if require_semicolon {
                self.expect_semicolon()?;
            }
            return Ok(vec![EvalStmt::ReferenceAssign {
                target: name,
                source,
            }]);
        }
        let value = self.parse_expr()?;
        if require_semicolon {
            self.expect_semicolon()?;
        }
        let value = assignment_value(&name, op, value);
        Ok(vec![EvalStmt::StoreVar { name, value }])
    }

    /// Parses prefix `++$name` and `--$name` as simple statement effects.
    fn parse_prefix_inc_dec_stmt(
        &mut self,
        require_semicolon: bool,
    ) -> Result<Vec<EvalStmt>, EvalParseError> {
        let increment = matches!(self.current(), TokenKind::PlusPlus);
        self.advance();
        let TokenKind::DollarIdent(name) = self.current() else {
            return Err(EvalParseError::ExpectedVariable);
        };
        let name = name.clone();
        self.advance();
        if require_semicolon {
            self.expect_semicolon()?;
        }
        Ok(vec![inc_dec_store(name, increment)])
    }

    /// Parses postfix `$name++` and `$name--` as simple statement effects.
    fn parse_postfix_inc_dec_stmt(
        &mut self,
        name: String,
        require_semicolon: bool,
    ) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        let increment = matches!(self.current(), TokenKind::PlusPlus);
        self.advance();
        if require_semicolon {
            self.expect_semicolon()?;
        }
        Ok(vec![inc_dec_store(name, increment)])
    }

    /// Parses `$object->property` as either an expression statement or property write.
    fn parse_property_stmt(
        &mut self,
        require_semicolon: bool,
    ) -> Result<Vec<EvalStmt>, EvalParseError> {
        let target = self.parse_expr()?;
        if !self.consume(TokenKind::Equal) {
            if require_semicolon {
                self.expect_semicolon()?;
            }
            return Ok(vec![EvalStmt::Expr(target)]);
        }
        let EvalExpr::PropertyGet { object, property } = target else {
            return Err(EvalParseError::UnexpectedToken);
        };
        let value = self.parse_expr()?;
        if require_semicolon {
            self.expect_semicolon()?;
        }
        Ok(vec![EvalStmt::PropertySet {
            object: *object,
            property,
            value,
        }])
    }

    /// Parses a complete `if` statement after consuming the `if` keyword.
    fn parse_if_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        Ok(vec![self.parse_if_after_keyword()?])
    }

    /// Parses the condition, then block, and optional else branch for an `if` chain.
    fn parse_if_after_keyword(&mut self) -> Result<EvalStmt, EvalParseError> {
        self.expect(TokenKind::LParen)?;
        let condition = self.parse_expr()?;
        self.expect(TokenKind::RParen)?;
        let then_branch = self.parse_statement_body()?;
        let else_branch = self.parse_optional_else_branch()?;
        Ok(EvalStmt::If {
            condition,
            then_branch,
            else_branch,
        })
    }

    /// Parses `elseif`, `else if`, or `else` branches after an `if` body.
    fn parse_optional_else_branch(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "elseif")) {
            self.advance();
            return Ok(vec![self.parse_if_after_keyword()?]);
        }
        if !matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "else")) {
            return Ok(Vec::new());
        }
        self.advance();
        if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "if")) {
            self.advance();
            Ok(vec![self.parse_if_after_keyword()?])
        } else {
            self.parse_statement_body()
        }
    }

    /// Parses `switch (expr) { case expr: ... default: ... }`.
    fn parse_switch_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        self.expect(TokenKind::LParen)?;
        let expr = self.parse_expr()?;
        self.expect(TokenKind::RParen)?;
        self.expect(TokenKind::LBrace)?;
        let mut cases = Vec::new();
        while !matches!(self.current(), TokenKind::RBrace) {
            if matches!(self.current(), TokenKind::Eof) {
                return Err(EvalParseError::UnexpectedEof);
            }
            cases.push(self.parse_switch_case()?);
        }
        self.expect(TokenKind::RBrace)?;
        Ok(vec![EvalStmt::Switch { expr, cases }])
    }

    /// Parses one `case` or `default` arm inside a switch body.
    fn parse_switch_case(&mut self) -> Result<EvalSwitchCase, EvalParseError> {
        let condition = if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "case"))
        {
            self.advance();
            let condition = self.parse_expr()?;
            Some(condition)
        } else if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "default")) {
            self.advance();
            None
        } else {
            return Err(EvalParseError::UnexpectedToken);
        };
        self.expect(TokenKind::Colon)?;
        let body = self.parse_switch_case_body()?;
        Ok(EvalSwitchCase { condition, body })
    }

    /// Parses case body statements until the next case boundary or switch close.
    fn parse_switch_case_body(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        let mut body = Vec::new();
        while !is_switch_case_boundary(self.current()) {
            if matches!(self.current(), TokenKind::Eof) {
                return Err(EvalParseError::UnexpectedEof);
            }
            body.extend(self.parse_stmt()?);
        }
        Ok(body)
    }

    /// Parses `unset($name[, ...]);`.
    fn parse_unset_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        self.expect(TokenKind::LParen)?;
        let mut statements = Vec::new();
        loop {
            let TokenKind::DollarIdent(name) = self.current() else {
                return Err(EvalParseError::ExpectedVariable);
            };
            statements.push(EvalStmt::UnsetVar { name: name.clone() });
            self.advance();
            if !self.consume(TokenKind::Comma) {
                break;
            }
        }
        self.expect(TokenKind::RParen)?;
        self.expect_semicolon()?;
        Ok(statements)
    }

    /// Parses `while (expr) { ... }`.
    fn parse_while_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        self.expect(TokenKind::LParen)?;
        let condition = self.parse_expr()?;
        self.expect(TokenKind::RParen)?;
        let body = self.parse_statement_body()?;
        Ok(vec![EvalStmt::While { condition, body }])
    }

    /// Parses either a brace-delimited block or one braceless statement body.
    fn parse_statement_body(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        if matches!(self.current(), TokenKind::LBrace) {
            self.parse_block()
        } else {
            self.parse_nested_stmt()
        }
    }

    /// Parses a brace-delimited statement block.
    fn parse_block(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.expect(TokenKind::LBrace)?;
        self.parse_nested_block_contents()
    }

    /// Parses one nested statement where import declarations are not legal.
    fn parse_nested_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        let previous = std::mem::replace(&mut self.allow_use_imports, false);
        let result = self.parse_stmt();
        self.allow_use_imports = previous;
        result
    }

    /// Parses a nested block while preserving active imports for name resolution.
    fn parse_nested_block_contents(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        let previous = std::mem::replace(&mut self.allow_use_imports, false);
        let result = self.parse_block_contents();
        self.allow_use_imports = previous;
        result
    }

    /// Parses statements until the closing brace for the current block.
    fn parse_block_contents(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        let mut statements = Vec::new();
        while !matches!(self.current(), TokenKind::RBrace) {
            if matches!(self.current(), TokenKind::Eof) {
                return Err(EvalParseError::UnexpectedEof);
            }
            statements.extend(self.parse_stmt()?);
        }
        self.expect(TokenKind::RBrace)?;
        Ok(statements)
    }

    /// Parses an expression using PHP-like logical, comparison, concatenation, and arithmetic precedence.
    fn parse_expr(&mut self) -> Result<EvalExpr, EvalParseError> {
        self.parse_keyword_or()
    }

    /// Parses PHP keyword `or`, whose precedence is lower than `xor`, `and`, and ternary.
    fn parse_keyword_or(&mut self) -> Result<EvalExpr, EvalParseError> {
        let mut expr = self.parse_keyword_xor()?;
        while matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "or")) {
            self.advance();
            let right = self.parse_keyword_xor()?;
            expr = EvalExpr::Binary {
                op: EvalBinOp::LogicalOr,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    /// Parses PHP keyword `xor`, whose operands are evaluated before boolean XOR.
    fn parse_keyword_xor(&mut self) -> Result<EvalExpr, EvalParseError> {
        let mut expr = self.parse_keyword_and()?;
        while matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "xor")) {
            self.advance();
            let right = self.parse_keyword_and()?;
            expr = EvalExpr::Binary {
                op: EvalBinOp::LogicalXor,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    /// Parses PHP keyword `and`, whose precedence is lower than ternary and `&&`.
    fn parse_keyword_and(&mut self) -> Result<EvalExpr, EvalParseError> {
        let mut expr = self.parse_ternary()?;
        while matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "and")) {
            self.advance();
            let right = self.parse_ternary()?;
            expr = EvalExpr::Binary {
                op: EvalBinOp::LogicalAnd,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    /// Parses PHP ternary expressions, including the short `expr ?: fallback` form.
    fn parse_ternary(&mut self) -> Result<EvalExpr, EvalParseError> {
        let condition = self.parse_null_coalesce()?;
        if !self.consume(TokenKind::Question) {
            return Ok(condition);
        }
        let then_branch = if self.consume(TokenKind::Colon) {
            None
        } else {
            let expr = self.parse_expr()?;
            self.expect(TokenKind::Colon)?;
            Some(Box::new(expr))
        };
        let else_branch = self.parse_expr()?;
        Ok(EvalExpr::Ternary {
            condition: Box::new(condition),
            then_branch,
            else_branch: Box::new(else_branch),
        })
    }

    /// Parses right-associative null coalescing below logical OR and above ternary.
    fn parse_null_coalesce(&mut self) -> Result<EvalExpr, EvalParseError> {
        let value = self.parse_logical_or()?;
        if !self.consume(TokenKind::QuestionQuestion) {
            return Ok(value);
        }
        let default = self.parse_null_coalesce()?;
        Ok(EvalExpr::NullCoalesce {
            value: Box::new(value),
            default: Box::new(default),
        })
    }

    /// Parses left-associative logical OR with lower precedence than logical AND.
    fn parse_logical_or(&mut self) -> Result<EvalExpr, EvalParseError> {
        let mut expr = self.parse_logical_and()?;
        while self.consume(TokenKind::OrOr) {
            let right = self.parse_logical_and()?;
            expr = EvalExpr::Binary {
                op: EvalBinOp::LogicalOr,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    /// Parses left-associative logical AND with lower precedence than equality.
    fn parse_logical_and(&mut self) -> Result<EvalExpr, EvalParseError> {
        let mut expr = self.parse_bit_or()?;
        while self.consume(TokenKind::AndAnd) {
            let right = self.parse_bit_or()?;
            expr = EvalExpr::Binary {
                op: EvalBinOp::LogicalAnd,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    /// Parses left-associative bitwise OR with lower precedence than bitwise XOR.
    fn parse_bit_or(&mut self) -> Result<EvalExpr, EvalParseError> {
        let mut expr = self.parse_bit_xor()?;
        while self.consume(TokenKind::Pipe) {
            let right = self.parse_bit_xor()?;
            expr = EvalExpr::Binary {
                op: EvalBinOp::BitOr,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    /// Parses left-associative bitwise XOR with lower precedence than bitwise AND.
    fn parse_bit_xor(&mut self) -> Result<EvalExpr, EvalParseError> {
        let mut expr = self.parse_bit_and()?;
        while self.consume(TokenKind::Caret) {
            let right = self.parse_bit_and()?;
            expr = EvalExpr::Binary {
                op: EvalBinOp::BitXor,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    /// Parses left-associative bitwise AND with lower precedence than equality.
    fn parse_bit_and(&mut self) -> Result<EvalExpr, EvalParseError> {
        let mut expr = self.parse_equality()?;
        while self.consume(TokenKind::Ampersand) {
            let right = self.parse_equality()?;
            expr = EvalExpr::Binary {
                op: EvalBinOp::BitAnd,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    /// Parses left-associative equality and inequality comparisons.
    fn parse_equality(&mut self) -> Result<EvalExpr, EvalParseError> {
        let mut expr = self.parse_ordering()?;
        loop {
            let op = if self.consume(TokenKind::EqualEqual) {
                EvalBinOp::LooseEq
            } else if self.consume(TokenKind::NotEqual) {
                EvalBinOp::LooseNotEq
            } else if self.consume(TokenKind::EqualEqualEqual) {
                EvalBinOp::StrictEq
            } else if self.consume(TokenKind::NotEqualEqual) {
                EvalBinOp::StrictNotEq
            } else {
                break;
            };
            let right = self.parse_ordering()?;
            expr = EvalExpr::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    /// Parses left-associative ordered comparisons.
    fn parse_ordering(&mut self) -> Result<EvalExpr, EvalParseError> {
        let mut expr = self.parse_shift()?;
        loop {
            let op = if self.consume(TokenKind::Less) {
                EvalBinOp::Lt
            } else if self.consume(TokenKind::LessEqual) {
                EvalBinOp::LtEq
            } else if self.consume(TokenKind::Greater) {
                EvalBinOp::Gt
            } else if self.consume(TokenKind::GreaterEqual) {
                EvalBinOp::GtEq
            } else if self.consume(TokenKind::Spaceship) {
                EvalBinOp::Spaceship
            } else {
                break;
            };
            let right = self.parse_shift()?;
            expr = EvalExpr::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    /// Parses left-associative integer shift operators.
    fn parse_shift(&mut self) -> Result<EvalExpr, EvalParseError> {
        let mut expr = self.parse_concat()?;
        loop {
            let op = if self.consume(TokenKind::LessLess) {
                EvalBinOp::ShiftLeft
            } else if self.consume(TokenKind::GreaterGreater) {
                EvalBinOp::ShiftRight
            } else {
                break;
            };
            let right = self.parse_concat()?;
            expr = EvalExpr::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    /// Parses left-associative string concatenation.
    fn parse_concat(&mut self) -> Result<EvalExpr, EvalParseError> {
        let mut expr = self.parse_add()?;
        while self.consume(TokenKind::Dot) {
            let right = self.parse_add()?;
            expr = EvalExpr::Binary {
                op: EvalBinOp::Concat,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    /// Parses left-associative numeric addition and subtraction.
    fn parse_add(&mut self) -> Result<EvalExpr, EvalParseError> {
        let mut expr = self.parse_mul()?;
        loop {
            let op = if self.consume(TokenKind::Plus) {
                EvalBinOp::Add
            } else if self.consume(TokenKind::Minus) {
                EvalBinOp::Sub
            } else {
                break;
            };
            let right = self.parse_mul()?;
            expr = EvalExpr::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    /// Parses left-associative numeric multiplication, division, and modulo.
    fn parse_mul(&mut self) -> Result<EvalExpr, EvalParseError> {
        let mut expr = self.parse_unary()?;
        loop {
            let op = if self.consume(TokenKind::Star) {
                EvalBinOp::Mul
            } else if self.consume(TokenKind::Slash) {
                EvalBinOp::Div
            } else if self.consume(TokenKind::Percent) {
                EvalBinOp::Mod
            } else {
                break;
            };
            let right = self.parse_unary()?;
            expr = EvalExpr::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    /// Parses right-associative unary prefix expressions.
    fn parse_unary(&mut self) -> Result<EvalExpr, EvalParseError> {
        if self.consume(TokenKind::Plus) {
            let expr = self.parse_unary()?;
            return Ok(EvalExpr::Unary {
                op: EvalUnaryOp::Plus,
                expr: Box::new(expr),
            });
        }
        if self.consume(TokenKind::Minus) {
            let expr = self.parse_unary()?;
            return Ok(EvalExpr::Unary {
                op: EvalUnaryOp::Negate,
                expr: Box::new(expr),
            });
        }
        if self.consume(TokenKind::Bang) {
            let expr = self.parse_unary()?;
            return Ok(EvalExpr::Unary {
                op: EvalUnaryOp::LogicalNot,
                expr: Box::new(expr),
            });
        }
        if self.consume(TokenKind::Tilde) {
            let expr = self.parse_unary()?;
            return Ok(EvalExpr::Unary {
                op: EvalUnaryOp::BitNot,
                expr: Box::new(expr),
            });
        }
        self.parse_power()
    }

    /// Parses right-associative exponentiation with higher precedence than unary prefix operators.
    fn parse_power(&mut self) -> Result<EvalExpr, EvalParseError> {
        let mut expr = self.parse_postfix()?;
        if self.consume(TokenKind::StarStar) {
            let right = self.parse_unary()?;
            expr = EvalExpr::Binary {
                op: EvalBinOp::Pow,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    /// Parses postfix array reads, property reads, method calls, and dynamic calls.
    fn parse_postfix(&mut self) -> Result<EvalExpr, EvalParseError> {
        let mut expr = self.parse_primary()?;
        loop {
            if matches!(self.current(), TokenKind::LParen) {
                let args = self.parse_call_args()?;
                expr = EvalExpr::DynamicCall {
                    callee: Box::new(expr),
                    args,
                };
                continue;
            }
            if self.consume(TokenKind::LBracket) {
                let index = self.parse_expr()?;
                self.expect(TokenKind::RBracket)?;
                expr = EvalExpr::ArrayGet {
                    array: Box::new(expr),
                    index: Box::new(index),
                };
                continue;
            }
            if self.consume(TokenKind::Arrow) {
                let TokenKind::Ident(member) = self.current() else {
                    return Err(EvalParseError::UnexpectedToken);
                };
                let member = member.clone();
                self.advance();
                if matches!(self.current(), TokenKind::LParen) {
                    let args = self.parse_call_args()?;
                    expr = EvalExpr::MethodCall {
                        object: Box::new(expr),
                        method: member.to_ascii_lowercase(),
                        args,
                    };
                } else {
                    expr = EvalExpr::PropertyGet {
                        object: Box::new(expr),
                        property: member,
                    };
                }
                continue;
            }
            break;
        }
        Ok(expr)
    }

    /// Parses primary expressions supported by the initial eval subset.
    fn parse_primary(&mut self) -> Result<EvalExpr, EvalParseError> {
        match self.current() {
            TokenKind::Int(value) => {
                let value = *value;
                self.advance();
                Ok(EvalExpr::Const(EvalConst::Int(value)))
            }
            TokenKind::Float(value) => {
                let value = *value;
                self.advance();
                Ok(EvalExpr::Const(EvalConst::Float(value)))
            }
            TokenKind::String(value) => {
                let value = value.clone();
                self.advance();
                Ok(EvalExpr::Const(EvalConst::String(value)))
            }
            TokenKind::DollarIdent(name) => {
                let name = name.clone();
                self.advance();
                Ok(EvalExpr::LoadVar(name))
            }
            TokenKind::Magic(EvalMagicConst::Namespace) => {
                let namespace = self.namespace.clone();
                self.advance();
                Ok(EvalExpr::Const(EvalConst::String(namespace)))
            }
            TokenKind::Magic(magic) => {
                let magic = magic.clone();
                self.advance();
                Ok(EvalExpr::Magic(magic))
            }
            TokenKind::Ident(name) if ident_eq(name, "null") => {
                self.advance();
                Ok(EvalExpr::Const(EvalConst::Null))
            }
            TokenKind::Ident(name) if ident_eq(name, "true") => {
                self.advance();
                Ok(EvalExpr::Const(EvalConst::Bool(true)))
            }
            TokenKind::Ident(name) if ident_eq(name, "false") => {
                self.advance();
                Ok(EvalExpr::Const(EvalConst::Bool(false)))
            }
            TokenKind::Ident(name) if ident_eq(name, "print") => {
                self.advance();
                let expr = self.parse_expr()?;
                Ok(EvalExpr::Print(Box::new(expr)))
            }
            TokenKind::Ident(_) if self.current_starts_legacy_array_literal() => {
                self.parse_legacy_array_literal()
            }
            TokenKind::Ident(name) if is_include_construct_name(name) => self.parse_include_expr(),
            TokenKind::Ident(name) if ident_eq(name, "match") => self.parse_match_expr(),
            TokenKind::Ident(name) if ident_eq(name, "new") => self.parse_new_object_expr(),
            TokenKind::Ident(name) if is_unsupported_expression_keyword(name) => {
                Err(EvalParseError::UnsupportedConstruct)
            }
            TokenKind::Backslash => self.parse_qualified_name_expr(),
            TokenKind::Ident(_) if matches!(self.peek(), TokenKind::Backslash) => {
                self.parse_qualified_name_expr()
            }
            TokenKind::Ident(name) if matches!(self.peek(), TokenKind::LParen) => {
                self.parse_call_expr(name.clone())
            }
            TokenKind::Ident(name) => {
                let name = name.clone();
                self.advance();
                Ok(self.const_fetch_expr(name))
            }
            TokenKind::LBracket => self.parse_array_literal(),
            TokenKind::LParen => {
                self.advance();
                let expr = self.parse_expr()?;
                self.expect(TokenKind::RParen)?;
                Ok(expr)
            }
            TokenKind::Eof => Err(EvalParseError::UnexpectedEof),
            _ => Err(EvalParseError::UnexpectedToken),
        }
    }

    /// Parses PHP include/require expression constructs and their path expression.
    fn parse_include_expr(&mut self) -> Result<EvalExpr, EvalParseError> {
        let TokenKind::Ident(name) = self.current() else {
            return Err(EvalParseError::UnexpectedToken);
        };
        let required = ident_eq(name, "require") || ident_eq(name, "require_once");
        let once = ident_eq(name, "include_once") || ident_eq(name, "require_once");
        self.advance();
        let path = if self.consume(TokenKind::LParen) {
            let path = self.parse_expr()?;
            self.expect(TokenKind::RParen)?;
            path
        } else {
            self.parse_expr()?
        };
        Ok(EvalExpr::Include {
            path: Box::new(path),
            required,
            once,
        })
    }

    /// Parses `match (expr) { pattern, other => value, default => fallback }`.
    fn parse_match_expr(&mut self) -> Result<EvalExpr, EvalParseError> {
        self.advance();
        self.expect(TokenKind::LParen)?;
        let subject = self.parse_expr()?;
        self.expect(TokenKind::RParen)?;
        self.expect(TokenKind::LBrace)?;

        let mut arms = Vec::new();
        let mut default = None;
        while !self.consume(TokenKind::RBrace) {
            if matches!(self.current(), TokenKind::Eof) {
                return Err(EvalParseError::UnexpectedEof);
            }
            if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "default")) {
                self.advance();
                self.expect(TokenKind::FatArrow)?;
                default = Some(Box::new(self.parse_expr()?));
            } else {
                arms.push(self.parse_match_arm()?);
            }
            if self.consume(TokenKind::Comma) {
                continue;
            }
            self.expect(TokenKind::RBrace)?;
            break;
        }

        Ok(EvalExpr::Match {
            subject: Box::new(subject),
            arms,
            default,
        })
    }

    /// Parses one non-default `match` arm and its comma-separated pattern list.
    fn parse_match_arm(&mut self) -> Result<EvalMatchArm, EvalParseError> {
        let mut patterns = Vec::new();
        loop {
            patterns.push(self.parse_expr()?);
            if !self.consume(TokenKind::Comma) {
                break;
            }
            if matches!(self.current(), TokenKind::FatArrow) {
                return Err(EvalParseError::UnexpectedToken);
            }
            if matches!(self.current(), TokenKind::Eof | TokenKind::RBrace) {
                return Err(EvalParseError::UnexpectedToken);
            }
        }
        self.expect(TokenKind::FatArrow)?;
        let value = self.parse_expr()?;
        Ok(EvalMatchArm { patterns, value })
    }

    /// Parses a function-like call expression and its source-order arguments.
    fn parse_call_expr(&mut self, name: String) -> Result<EvalExpr, EvalParseError> {
        self.advance();
        let args = self.parse_call_args()?;
        Ok(self.call_expr(name, args))
    }

    /// Parses an explicitly qualified call or constant-fetch expression.
    fn parse_qualified_name_expr(&mut self) -> Result<EvalExpr, EvalParseError> {
        let name = self.parse_qualified_name()?;
        let name = self.resolve_qualified_name(name);
        if matches!(self.current(), TokenKind::LParen) {
            let args = self.parse_call_args()?;
            return Ok(EvalExpr::Call {
                name: name.to_ascii_lowercase(),
                args,
            });
        }
        Ok(EvalExpr::ConstFetch(name))
    }

    /// Parses `new ClassName(...)` expressions in eval fragments.
    fn parse_new_object_expr(&mut self) -> Result<EvalExpr, EvalParseError> {
        self.advance();
        let class_name = self.parse_qualified_name()?;
        let class_name = self.resolve_class_name(class_name);
        let args = self.parse_call_args()?;
        Ok(EvalExpr::NewObject { class_name, args })
    }

    /// Parses a simple or explicitly qualified PHP name.
    fn parse_qualified_name(&mut self) -> Result<ParsedQualifiedName, EvalParseError> {
        let absolute = self.consume(TokenKind::Backslash);
        let TokenKind::Ident(first) = self.current() else {
            return Err(EvalParseError::UnexpectedToken);
        };
        let mut name = first.clone();
        self.advance();
        while self.consume(TokenKind::Backslash) {
            let TokenKind::Ident(part) = self.current() else {
                return Err(EvalParseError::UnexpectedToken);
            };
            name.push('\\');
            name.push_str(part);
            self.advance();
        }
        Ok(ParsedQualifiedName { name, absolute })
    }

    /// Builds a call expression, adding namespace fallback for unqualified names.
    fn call_expr(&self, name: String, args: Vec<EvalCallArg>) -> EvalExpr {
        if let Some(imported) = self.imports.resolve_function(&name) {
            return EvalExpr::Call {
                name: imported.to_ascii_lowercase(),
                args,
            };
        }
        let fallback_name = name.to_ascii_lowercase();
        if self.namespace.is_empty() {
            EvalExpr::Call {
                name: fallback_name,
                args,
            }
        } else {
            EvalExpr::NamespacedCall {
                name: self
                    .qualify_name_in_current_namespace(&name)
                    .to_ascii_lowercase(),
                fallback_name,
                args,
            }
        }
    }

    /// Builds a constant fetch expression, adding namespace fallback for unqualified names.
    fn const_fetch_expr(&self, name: String) -> EvalExpr {
        if let Some(imported) = self.imports.resolve_constant(&name) {
            return EvalExpr::ConstFetch(imported.to_string());
        }
        if self.namespace.is_empty() {
            EvalExpr::ConstFetch(name)
        } else {
            EvalExpr::NamespacedConstFetch {
                name: self.qualify_name_in_current_namespace(&name),
                fallback_name: name,
            }
        }
    }

    /// Prefixes a name with the parser's current namespace when one is active.
    fn qualify_name_in_current_namespace(&self, name: &str) -> String {
        if self.namespace.is_empty() {
            name.to_string()
        } else {
            format!("{}\\{}", self.namespace, name)
        }
    }

    /// Resolves a class name through active imports before namespace qualification.
    fn resolve_class_name(&self, name: ParsedQualifiedName) -> String {
        if name.absolute {
            return name.name;
        }
        if let Some(imported) = self.imports.resolve_class(&name.name) {
            return imported;
        }
        self.resolve_qualified_name(name)
    }

    /// Resolves a parsed PHP name according to the current namespace.
    fn resolve_qualified_name(&self, name: ParsedQualifiedName) -> String {
        if name.absolute || self.namespace.is_empty() {
            name.name
        } else {
            self.qualify_name_in_current_namespace(&name.name)
        }
    }

    /// Parses a parenthesized source-order argument list.
    fn parse_call_args(&mut self) -> Result<Vec<EvalCallArg>, EvalParseError> {
        self.expect(TokenKind::LParen)?;
        let mut args = Vec::new();
        if self.consume(TokenKind::RParen) {
            return Ok(args);
        }
        loop {
            args.push(self.parse_call_arg()?);
            if !self.consume(TokenKind::Comma) {
                break;
            }
            if self.consume(TokenKind::RParen) {
                return Ok(args);
            }
        }
        self.expect(TokenKind::RParen)?;
        Ok(args)
    }

    /// Parses one positional or named argument within a call argument list.
    fn parse_call_arg(&mut self) -> Result<EvalCallArg, EvalParseError> {
        if self.consume(TokenKind::Ellipsis) {
            return self.parse_expr().map(EvalCallArg::spread);
        }
        if matches!(self.peek(), TokenKind::Colon) {
            if let TokenKind::Ident(name) = self.current() {
                let name = name.clone();
                self.advance();
                self.expect(TokenKind::Colon)?;
                let value = self.parse_expr()?;
                return Ok(EvalCallArg::named(name, value));
            }
        }
        self.parse_expr().map(EvalCallArg::positional)
    }

    /// Parses an array literal with source-order optional key/value element expressions.
    fn parse_array_literal(&mut self) -> Result<EvalExpr, EvalParseError> {
        self.expect(TokenKind::LBracket)?;
        self.parse_array_elements_until(TokenKind::RBracket)
    }

    /// Parses PHP's legacy `array(...)` literal into the same EvalIR node as `[...]`.
    fn parse_legacy_array_literal(&mut self) -> Result<EvalExpr, EvalParseError> {
        self.advance();
        self.expect(TokenKind::LParen)?;
        self.parse_array_elements_until(TokenKind::RParen)
    }

    /// Returns whether the current token starts PHP's legacy `array(...)` literal syntax.
    fn current_starts_legacy_array_literal(&self) -> bool {
        matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "array"))
            && matches!(self.peek(), TokenKind::LParen)
    }

    /// Parses comma-separated array elements until the supplied closing delimiter.
    fn parse_array_elements_until(&mut self, close: TokenKind) -> Result<EvalExpr, EvalParseError> {
        let mut elements = Vec::new();
        if self.consume(close.clone()) {
            return Ok(EvalExpr::Array(elements));
        }
        loop {
            let first = self.parse_expr()?;
            if self.consume(TokenKind::FatArrow) {
                let value = self.parse_expr()?;
                elements.push(EvalArrayElement::KeyValue { key: first, value });
            } else {
                elements.push(EvalArrayElement::Value(first));
            }
            if !self.consume(TokenKind::Comma) {
                break;
            }
            if self.consume(close.clone()) {
                return Ok(EvalExpr::Array(elements));
            }
        }
        self.expect(close)?;
        Ok(EvalExpr::Array(elements))
    }

    /// Consumes `expected` or returns a parse error.
    fn expect(&mut self, expected: TokenKind) -> Result<(), EvalParseError> {
        if self.consume(expected) {
            Ok(())
        } else {
            Err(EvalParseError::UnexpectedToken)
        }
    }

    /// Consumes a semicolon or returns the semicolon-specific parse error.
    fn expect_semicolon(&mut self) -> Result<(), EvalParseError> {
        if self.consume_semicolon() {
            Ok(())
        } else {
            Err(EvalParseError::ExpectedSemicolon)
        }
    }

    /// Consumes a semicolon if present.
    fn consume_semicolon(&mut self) -> bool {
        self.consume(TokenKind::Semicolon)
    }

    /// Consumes `expected` if the current token matches it.
    fn consume(&mut self, expected: TokenKind) -> bool {
        if *self.current() == expected {
            self.advance();
            true
        } else {
            false
        }
    }

    /// Returns the current token.
    fn current(&self) -> &TokenKind {
        self.tokens.get(self.pos).unwrap_or(&TokenKind::Eof)
    }

    /// Returns the next token without advancing.
    fn peek(&self) -> &TokenKind {
        self.tokens.get(self.pos + 1).unwrap_or(&TokenKind::Eof)
    }

    /// Advances to the next token.
    fn advance(&mut self) {
        if self.pos < self.tokens.len() {
            self.pos += 1;
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

/// Returns true when the current token closes or starts a switch case arm.
fn is_switch_case_boundary(token: &TokenKind) -> bool {
    matches!(token, TokenKind::RBrace)
        || matches!(token, TokenKind::Ident(name) if ident_eq(name, "case") || ident_eq(name, "default"))
}

/// Maps simple variable assignment tokens to an optional compound EvalIR operator.
fn assignment_op(token: &TokenKind) -> Option<Option<EvalBinOp>> {
    match token {
        TokenKind::Equal => Some(None),
        TokenKind::PlusEqual => Some(Some(EvalBinOp::Add)),
        TokenKind::MinusEqual => Some(Some(EvalBinOp::Sub)),
        TokenKind::StarEqual => Some(Some(EvalBinOp::Mul)),
        TokenKind::StarStarEqual => Some(Some(EvalBinOp::Pow)),
        TokenKind::SlashEqual => Some(Some(EvalBinOp::Div)),
        TokenKind::PercentEqual => Some(Some(EvalBinOp::Mod)),
        TokenKind::AmpEqual => Some(Some(EvalBinOp::BitAnd)),
        TokenKind::PipeEqual => Some(Some(EvalBinOp::BitOr)),
        TokenKind::CaretEqual => Some(Some(EvalBinOp::BitXor)),
        TokenKind::LessLessEqual => Some(Some(EvalBinOp::ShiftLeft)),
        TokenKind::GreaterGreaterEqual => Some(Some(EvalBinOp::ShiftRight)),
        TokenKind::DotEqual => Some(Some(EvalBinOp::Concat)),
        _ => None,
    }
}

/// Builds the assigned value expression for plain and compound variable assignment.
fn assignment_value(name: &str, op: Option<EvalBinOp>, value: EvalExpr) -> EvalExpr {
    match op {
        Some(op) => EvalExpr::Binary {
            op,
            left: Box::new(EvalExpr::LoadVar(name.to_string())),
            right: Box::new(value),
        },
        None => value,
    }
}

/// Builds the StoreVar statement for a simple variable increment or decrement.
fn inc_dec_store(name: String, increment: bool) -> EvalStmt {
    EvalStmt::StoreVar {
        value: EvalExpr::Binary {
            op: if increment {
                EvalBinOp::Add
            } else {
                EvalBinOp::Sub
            },
            left: Box::new(EvalExpr::LoadVar(name.clone())),
            right: Box::new(EvalExpr::Const(EvalConst::Int(1))),
        },
        name,
    }
}

/// Compares a source identifier to a PHP keyword using ASCII case-insensitive rules.
fn ident_eq(actual: &str, expected: &str) -> bool {
    actual.eq_ignore_ascii_case(expected)
}

/// Returns true for PHP statement forms that the eval subset intentionally does not parse yet.
fn is_unsupported_statement_keyword(name: &str) -> bool {
    ["enum", "interface", "trait", "try"]
        .iter()
        .any(|keyword| ident_eq(name, keyword))
}

/// Returns true when an identifier is an include/require expression construct.
fn is_include_construct_name(name: &str) -> bool {
    ["include", "include_once", "require", "require_once"]
        .iter()
        .any(|keyword| ident_eq(name, keyword))
}

/// Returns the first namespace segment and the optional remaining suffix.
fn split_first_name_segment(name: &str) -> (&str, Option<&str>) {
    name.split_once('\\')
        .map_or((name, None), |(first, tail)| (first, Some(tail)))
}

/// Returns the final segment of a PHP qualified name.
fn last_name_segment(name: &str) -> &str {
    name.rsplit('\\').next().unwrap_or(name)
}

/// Combines a grouped use prefix with one relative member name.
fn join_grouped_use_name(prefix: &str, member: &str) -> String {
    format!("{prefix}\\{member}")
}

/// Returns true for PHP expression forms that the eval subset intentionally does not parse yet.
fn is_unsupported_expression_keyword(name: &str) -> bool {
    ["clone", "yield"]
        .iter()
        .any(|keyword| ident_eq(name, keyword))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies assignment fragments lower to by-name StoreVar statements.
    #[test]
    fn parse_fragment_accepts_assignment_source() {
        let program = parse_fragment(b"$x = 1;").expect("fragment should parse");
        assert_eq!(program.source_len(), 7);
        assert_eq!(
            program.statements(),
            &[EvalStmt::StoreVar {
                name: "x".to_string(),
                value: EvalExpr::Const(EvalConst::Int(1)),
            }]
        );
    }

    /// Verifies reference assignments lower to by-name ReferenceAssign statements.
    #[test]
    fn parse_fragment_accepts_reference_assignment_source() {
        let program = parse_fragment(b"$left =& $right;").expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::ReferenceAssign {
                target: "left".to_string(),
                source: "right".to_string(),
            }]
        );
    }

    /// Verifies multiplicative operators preserve PHP precedence and associativity.
    #[test]
    fn parse_fragment_accepts_division_and_modulo_source() {
        let program = parse_fragment(b"return 10 / 4 % 3;").expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Return(Some(EvalExpr::Binary {
                op: EvalBinOp::Mod,
                left: Box::new(EvalExpr::Binary {
                    op: EvalBinOp::Div,
                    left: Box::new(EvalExpr::Const(EvalConst::Int(10))),
                    right: Box::new(EvalExpr::Const(EvalConst::Int(4))),
                }),
                right: Box::new(EvalExpr::Const(EvalConst::Int(3))),
            }))]
        );
    }

    /// Verifies exponentiation is right-associative and binds tighter than unary negation.
    #[test]
    fn parse_fragment_accepts_power_source() {
        let program =
            parse_fragment(b"return -2 ** 2; return 2 ** 3 ** 2;").expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[
                EvalStmt::Return(Some(EvalExpr::Unary {
                    op: EvalUnaryOp::Negate,
                    expr: Box::new(EvalExpr::Binary {
                        op: EvalBinOp::Pow,
                        left: Box::new(EvalExpr::Const(EvalConst::Int(2))),
                        right: Box::new(EvalExpr::Const(EvalConst::Int(2))),
                    }),
                })),
                EvalStmt::Return(Some(EvalExpr::Binary {
                    op: EvalBinOp::Pow,
                    left: Box::new(EvalExpr::Const(EvalConst::Int(2))),
                    right: Box::new(EvalExpr::Binary {
                        op: EvalBinOp::Pow,
                        left: Box::new(EvalExpr::Const(EvalConst::Int(3))),
                        right: Box::new(EvalExpr::Const(EvalConst::Int(2))),
                    }),
                })),
            ]
        );
    }

    /// Verifies bitwise operators preserve PHP precedence.
    #[test]
    fn parse_fragment_accepts_bitwise_source() {
        let program = parse_fragment(b"return ~0 | 2 ^ 3 & 4;").expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Return(Some(EvalExpr::Binary {
                op: EvalBinOp::BitOr,
                left: Box::new(EvalExpr::Unary {
                    op: EvalUnaryOp::BitNot,
                    expr: Box::new(EvalExpr::Const(EvalConst::Int(0))),
                }),
                right: Box::new(EvalExpr::Binary {
                    op: EvalBinOp::BitXor,
                    left: Box::new(EvalExpr::Const(EvalConst::Int(2))),
                    right: Box::new(EvalExpr::Binary {
                        op: EvalBinOp::BitAnd,
                        left: Box::new(EvalExpr::Const(EvalConst::Int(3))),
                        right: Box::new(EvalExpr::Const(EvalConst::Int(4))),
                    }),
                }),
            }))]
        );
    }

    /// Verifies shift operators bind lower than additive expressions.
    #[test]
    fn parse_fragment_accepts_shift_source() {
        let program = parse_fragment(b"return 1 + 2 << 3;").expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Return(Some(EvalExpr::Binary {
                op: EvalBinOp::ShiftLeft,
                left: Box::new(EvalExpr::Binary {
                    op: EvalBinOp::Add,
                    left: Box::new(EvalExpr::Const(EvalConst::Int(1))),
                    right: Box::new(EvalExpr::Const(EvalConst::Int(2))),
                }),
                right: Box::new(EvalExpr::Const(EvalConst::Int(3))),
            }))]
        );
    }

    /// Verifies simple variable compound assignments lower to StoreVar with binary expressions.
    #[test]
    fn parse_fragment_accepts_compound_assignment_source() {
        let program =
            parse_fragment(br#"$x += 2; $x -= 1; $x *= 3; $x /= 2; $x %= 5; $s .= "ok";"#)
                .expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[
                EvalStmt::StoreVar {
                    name: "x".to_string(),
                    value: EvalExpr::Binary {
                        op: EvalBinOp::Add,
                        left: Box::new(EvalExpr::LoadVar("x".to_string())),
                        right: Box::new(EvalExpr::Const(EvalConst::Int(2))),
                    },
                },
                EvalStmt::StoreVar {
                    name: "x".to_string(),
                    value: EvalExpr::Binary {
                        op: EvalBinOp::Sub,
                        left: Box::new(EvalExpr::LoadVar("x".to_string())),
                        right: Box::new(EvalExpr::Const(EvalConst::Int(1))),
                    },
                },
                EvalStmt::StoreVar {
                    name: "x".to_string(),
                    value: EvalExpr::Binary {
                        op: EvalBinOp::Mul,
                        left: Box::new(EvalExpr::LoadVar("x".to_string())),
                        right: Box::new(EvalExpr::Const(EvalConst::Int(3))),
                    },
                },
                EvalStmt::StoreVar {
                    name: "x".to_string(),
                    value: EvalExpr::Binary {
                        op: EvalBinOp::Div,
                        left: Box::new(EvalExpr::LoadVar("x".to_string())),
                        right: Box::new(EvalExpr::Const(EvalConst::Int(2))),
                    },
                },
                EvalStmt::StoreVar {
                    name: "x".to_string(),
                    value: EvalExpr::Binary {
                        op: EvalBinOp::Mod,
                        left: Box::new(EvalExpr::LoadVar("x".to_string())),
                        right: Box::new(EvalExpr::Const(EvalConst::Int(5))),
                    },
                },
                EvalStmt::StoreVar {
                    name: "s".to_string(),
                    value: EvalExpr::Binary {
                        op: EvalBinOp::Concat,
                        left: Box::new(EvalExpr::LoadVar("s".to_string())),
                        right: Box::new(EvalExpr::Const(EvalConst::String("ok".to_string()))),
                    },
                },
            ]
        );
    }

    /// Verifies exponentiation compound assignment lowers through the binary power operator.
    #[test]
    fn parse_fragment_accepts_power_compound_assignment_source() {
        let program = parse_fragment(br#"$x **= 3;"#).expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::StoreVar {
                name: "x".to_string(),
                value: EvalExpr::Binary {
                    op: EvalBinOp::Pow,
                    left: Box::new(EvalExpr::LoadVar("x".to_string())),
                    right: Box::new(EvalExpr::Const(EvalConst::Int(3))),
                },
            }]
        );
    }

    /// Verifies bitwise compound assignments lower to StoreVar with binary expressions.
    #[test]
    fn parse_fragment_accepts_bitwise_compound_assignment_source() {
        let program = parse_fragment(br#"$x &= 3; $x |= 1; $x ^= 2; $x <<= 4; $x >>= 1;"#)
            .expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[
                EvalStmt::StoreVar {
                    name: "x".to_string(),
                    value: EvalExpr::Binary {
                        op: EvalBinOp::BitAnd,
                        left: Box::new(EvalExpr::LoadVar("x".to_string())),
                        right: Box::new(EvalExpr::Const(EvalConst::Int(3))),
                    },
                },
                EvalStmt::StoreVar {
                    name: "x".to_string(),
                    value: EvalExpr::Binary {
                        op: EvalBinOp::BitOr,
                        left: Box::new(EvalExpr::LoadVar("x".to_string())),
                        right: Box::new(EvalExpr::Const(EvalConst::Int(1))),
                    },
                },
                EvalStmt::StoreVar {
                    name: "x".to_string(),
                    value: EvalExpr::Binary {
                        op: EvalBinOp::BitXor,
                        left: Box::new(EvalExpr::LoadVar("x".to_string())),
                        right: Box::new(EvalExpr::Const(EvalConst::Int(2))),
                    },
                },
                EvalStmt::StoreVar {
                    name: "x".to_string(),
                    value: EvalExpr::Binary {
                        op: EvalBinOp::ShiftLeft,
                        left: Box::new(EvalExpr::LoadVar("x".to_string())),
                        right: Box::new(EvalExpr::Const(EvalConst::Int(4))),
                    },
                },
                EvalStmt::StoreVar {
                    name: "x".to_string(),
                    value: EvalExpr::Binary {
                        op: EvalBinOp::ShiftRight,
                        left: Box::new(EvalExpr::LoadVar("x".to_string())),
                        right: Box::new(EvalExpr::Const(EvalConst::Int(1))),
                    },
                },
            ]
        );
    }

    /// Verifies simple variable increment and decrement statements lower to StoreVar.
    #[test]
    fn parse_fragment_accepts_inc_dec_statement_source() {
        let program = parse_fragment(br#"$i++; ++$j; $k--; --$m;"#).expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[
                inc_dec_store("i".to_string(), true),
                inc_dec_store("j".to_string(), true),
                inc_dec_store("k".to_string(), false),
                inc_dec_store("m".to_string(), false),
            ]
        );
    }

    /// Verifies echo fragments preserve expression source order.
    #[test]
    fn parse_fragment_accepts_echo_source() {
        let program = parse_fragment(br#"echo "hi" . $name;"#).expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Echo(EvalExpr::Binary {
                op: EvalBinOp::Concat,
                left: Box::new(EvalExpr::Const(EvalConst::String("hi".to_string()))),
                right: Box::new(EvalExpr::LoadVar("name".to_string())),
            })]
        );
    }

    /// Verifies PHP echo comma lists lower to one EvalIR echo statement per expression.
    #[test]
    fn parse_fragment_accepts_echo_comma_list_source() {
        let program = parse_fragment(br#"echo "a", $b, "c";"#).expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[
                EvalStmt::Echo(EvalExpr::Const(EvalConst::String("a".to_string()))),
                EvalStmt::Echo(EvalExpr::LoadVar("b".to_string())),
                EvalStmt::Echo(EvalExpr::Const(EvalConst::String("c".to_string()))),
            ]
        );
    }

    /// Verifies if/else fragments lower to branch statements with nested blocks.
    #[test]
    fn parse_fragment_accepts_if_else_source() {
        let program = parse_fragment(br#"if ($flag) { $x = "yes"; } else { $x = "no"; }"#)
            .expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::If {
                condition: EvalExpr::LoadVar("flag".to_string()),
                then_branch: vec![EvalStmt::StoreVar {
                    name: "x".to_string(),
                    value: EvalExpr::Const(EvalConst::String("yes".to_string())),
                }],
                else_branch: vec![EvalStmt::StoreVar {
                    name: "x".to_string(),
                    value: EvalExpr::Const(EvalConst::String("no".to_string())),
                }],
            }]
        );
    }

    /// Verifies braceless if/else bodies parse as single-statement branch bodies.
    #[test]
    fn parse_fragment_accepts_braceless_if_else_source() {
        let program = parse_fragment(br#"if ($flag) echo "yes"; else echo "no";"#)
            .expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::If {
                condition: EvalExpr::LoadVar("flag".to_string()),
                then_branch: vec![EvalStmt::Echo(EvalExpr::Const(EvalConst::String(
                    "yes".to_string()
                )))],
                else_branch: vec![EvalStmt::Echo(EvalExpr::Const(EvalConst::String(
                    "no".to_string()
                )))],
            }]
        );
    }

    /// Verifies elseif fragments lower to nested if statements in the else branch.
    #[test]
    fn parse_fragment_accepts_elseif_source() {
        let program = parse_fragment(br#"if ($a) { $x = "a"; } elseif ($b) { $x = "b"; }"#)
            .expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::If {
                condition: EvalExpr::LoadVar("a".to_string()),
                then_branch: vec![EvalStmt::StoreVar {
                    name: "x".to_string(),
                    value: EvalExpr::Const(EvalConst::String("a".to_string())),
                }],
                else_branch: vec![EvalStmt::If {
                    condition: EvalExpr::LoadVar("b".to_string()),
                    then_branch: vec![EvalStmt::StoreVar {
                        name: "x".to_string(),
                        value: EvalExpr::Const(EvalConst::String("b".to_string())),
                    }],
                    else_branch: Vec::new(),
                }],
            }]
        );
    }

    /// Verifies PHP's `else if` spelling follows the same nested branch shape.
    #[test]
    fn parse_fragment_accepts_else_if_source() {
        let program = parse_fragment(br#"if ($a) { $x = "a"; } else if ($b) { $x = "b"; }"#)
            .expect("fragment should parse");

        assert!(matches!(
            program.statements(),
            [EvalStmt::If {
                else_branch,
                ..
            }] if matches!(else_branch.as_slice(), [EvalStmt::If { .. }])
        ));
    }

    /// Verifies for loops lower clauses and body statements separately.
    #[test]
    fn parse_fragment_accepts_for_source() {
        let program = parse_fragment(br#"for ($i = 2; $i; $i = $i - 1) { echo $i; }"#)
            .expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::For {
                init: vec![EvalStmt::StoreVar {
                    name: "i".to_string(),
                    value: EvalExpr::Const(EvalConst::Int(2)),
                }],
                condition: Some(EvalExpr::LoadVar("i".to_string())),
                update: vec![EvalStmt::StoreVar {
                    name: "i".to_string(),
                    value: EvalExpr::Binary {
                        op: EvalBinOp::Sub,
                        left: Box::new(EvalExpr::LoadVar("i".to_string())),
                        right: Box::new(EvalExpr::Const(EvalConst::Int(1))),
                    },
                }],
                body: vec![EvalStmt::Echo(EvalExpr::LoadVar("i".to_string()))],
            }]
        );
    }

    /// Verifies switch fragments preserve ordered case and default bodies.
    #[test]
    fn parse_fragment_accepts_switch_source() {
        let program =
            parse_fragment(br#"switch ($x) { case 1: echo "one"; break; default: echo "other"; }"#)
                .expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Switch {
                expr: EvalExpr::LoadVar("x".to_string()),
                cases: vec![
                    EvalSwitchCase {
                        condition: Some(EvalExpr::Const(EvalConst::Int(1))),
                        body: vec![
                            EvalStmt::Echo(EvalExpr::Const(EvalConst::String("one".to_string()))),
                            EvalStmt::Break,
                        ],
                    },
                    EvalSwitchCase {
                        condition: None,
                        body: vec![EvalStmt::Echo(EvalExpr::Const(EvalConst::String(
                            "other".to_string()
                        )))],
                    },
                ],
            }]
        );
    }

    /// Verifies value-only foreach loops lower to an array expression, value target, and body.
    #[test]
    fn parse_fragment_accepts_foreach_source() {
        let program =
            parse_fragment(br#"foreach ($items as $item) { echo $item; }"#).expect("parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Foreach {
                array: EvalExpr::LoadVar("items".to_string()),
                key_name: None,
                value_name: "item".to_string(),
                body: vec![EvalStmt::Echo(EvalExpr::LoadVar("item".to_string()))],
            }]
        );
    }

    /// Verifies key-value foreach loops preserve both loop target names in EvalIR.
    #[test]
    fn parse_fragment_accepts_foreach_key_value_source() {
        let program =
            parse_fragment(br#"foreach ($items as $key => $item) { echo $key . $item; }"#)
                .expect("parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Foreach {
                array: EvalExpr::LoadVar("items".to_string()),
                key_name: Some("key".to_string()),
                value_name: "item".to_string(),
                body: vec![EvalStmt::Echo(EvalExpr::Binary {
                    op: EvalBinOp::Concat,
                    left: Box::new(EvalExpr::LoadVar("key".to_string())),
                    right: Box::new(EvalExpr::LoadVar("item".to_string())),
                })],
            }]
        );
    }

    /// Verifies dynamic function declarations preserve name, parameters, and body.
    #[test]
    fn parse_fragment_accepts_function_declaration_source() {
        let program = parse_fragment(br#"function dyn($x) { return $x + 1; }"#)
            .expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::FunctionDecl {
                name: "dyn".to_string(),
                params: vec!["x".to_string()],
                body: vec![EvalStmt::Return(Some(EvalExpr::Binary {
                    op: EvalBinOp::Add,
                    left: Box::new(EvalExpr::LoadVar("x".to_string())),
                    right: Box::new(EvalExpr::Const(EvalConst::Int(1))),
                }))],
            }]
        );
    }

    /// Verifies semicolon namespace declarations qualify functions and unqualified calls.
    #[test]
    fn parse_fragment_accepts_semicolon_namespace_source() {
        let program = parse_fragment(
            br#"namespace Eval\Ns;
function dyn() { return __NAMESPACE__; }
return dyn();"#,
        )
        .expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[
                EvalStmt::FunctionDecl {
                    name: "Eval\\Ns\\dyn".to_string(),
                    params: Vec::new(),
                    body: vec![EvalStmt::Return(Some(EvalExpr::Const(EvalConst::String(
                        "Eval\\Ns".to_string()
                    ))))],
                },
                EvalStmt::Return(Some(EvalExpr::NamespacedCall {
                    name: "eval\\ns\\dyn".to_string(),
                    fallback_name: "dyn".to_string(),
                    args: Vec::new(),
                })),
            ]
        );
    }

    /// Verifies braced namespace declarations restore the previous namespace afterward.
    #[test]
    fn parse_fragment_accepts_braced_namespace_source() {
        let program = parse_fragment(
            br#"namespace Eval\Block {
    class Box {}
    return new Box();
}
return Box;"#,
        )
        .expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[
                EvalStmt::ClassDecl {
                    name: "Eval\\Block\\Box".to_string(),
                },
                EvalStmt::Return(Some(EvalExpr::NewObject {
                    class_name: "Eval\\Block\\Box".to_string(),
                    args: Vec::new(),
                })),
                EvalStmt::Return(Some(EvalExpr::ConstFetch("Box".to_string()))),
            ]
        );
    }

    /// Verifies namespace import declarations resolve functions, constants, and class aliases.
    #[test]
    fn parse_fragment_accepts_namespace_use_imports() {
        let program = parse_fragment(
            br#"namespace Eval\UseNs;
use function Lib\strlen as Alias;
use const Lib\VALUE as LocalValue;
use Lib\Box as BoxAlias;
return Alias(LocalValue, new BoxAlias\Inner());"#,
        )
        .expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Return(Some(EvalExpr::Call {
                name: "lib\\strlen".to_string(),
                args: vec![
                    EvalCallArg::positional(EvalExpr::ConstFetch("Lib\\VALUE".to_string())),
                    EvalCallArg::positional(EvalExpr::NewObject {
                        class_name: "Lib\\Box\\Inner".to_string(),
                        args: Vec::new(),
                    }),
                ],
            }))]
        );
    }

    /// Verifies grouped namespace imports resolve functions, constants, and class aliases.
    #[test]
    fn parse_fragment_accepts_grouped_namespace_use_imports() {
        let program = parse_fragment(
            br#"namespace Eval\UseNs;
use Lib\{Box as BoxAlias, Sub\Thing, function imported_func as Alias};
use const Lib\{VALUE as LocalValue, OTHER};
return Alias(LocalValue, OTHER, new BoxAlias\Inner(), new Thing());"#,
        )
        .expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Return(Some(EvalExpr::Call {
                name: "lib\\imported_func".to_string(),
                args: vec![
                    EvalCallArg::positional(EvalExpr::ConstFetch("Lib\\VALUE".to_string())),
                    EvalCallArg::positional(EvalExpr::ConstFetch("Lib\\OTHER".to_string())),
                    EvalCallArg::positional(EvalExpr::NewObject {
                        class_name: "Lib\\Box\\Inner".to_string(),
                        args: Vec::new(),
                    }),
                    EvalCallArg::positional(EvalExpr::NewObject {
                        class_name: "Lib\\Sub\\Thing".to_string(),
                        args: Vec::new(),
                    }),
                ],
            }))]
        );
    }

    /// Verifies typed grouped namespace imports reject mixed per-entry kinds.
    #[test]
    fn parse_fragment_rejects_mixed_kind_typed_grouped_use_imports() {
        assert_eq!(
            parse_fragment(br#"use function Lib\{target, const VALUE};"#),
            Err(EvalParseError::UnexpectedToken)
        );
    }

    /// Verifies namespace blocks restore imports when control returns to the outer namespace.
    #[test]
    fn parse_fragment_restores_use_imports_after_namespace_block() {
        let program = parse_fragment(
            br#"namespace Eval\Outer;
use function Lib\outer_func;
namespace Eval\Block {
    use function Lib\inner_func as alias;
    return alias();
}
return outer_func();"#,
        )
        .expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[
                EvalStmt::Return(Some(EvalExpr::Call {
                    name: "lib\\inner_func".to_string(),
                    args: Vec::new(),
                })),
                EvalStmt::Return(Some(EvalExpr::Call {
                    name: "lib\\outer_func".to_string(),
                    args: Vec::new(),
                })),
            ]
        );
    }

    /// Verifies imported aliases remain visible while parsing eval-declared function bodies.
    #[test]
    fn parse_fragment_applies_use_imports_inside_function_body() {
        let program = parse_fragment(
            br#"namespace Eval\UseNs;
use function Lib\target as alias;
function dyn() { return alias(); }"#,
        )
        .expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::FunctionDecl {
                name: "Eval\\UseNs\\dyn".to_string(),
                params: Vec::new(),
                body: vec![EvalStmt::Return(Some(EvalExpr::Call {
                    name: "lib\\target".to_string(),
                    args: Vec::new(),
                }))],
            }]
        );
    }

    /// Verifies import declarations are rejected inside eval-declared function bodies.
    #[test]
    fn parse_fragment_rejects_use_import_inside_function_body() {
        assert_eq!(
            parse_fragment(br#"function dyn() { use function Lib\target; }"#),
            Err(EvalParseError::UnsupportedConstruct)
        );
    }

    /// Verifies static local declarations preserve the target name and initializer expression.
    #[test]
    fn parse_fragment_accepts_static_var_source() {
        let program = parse_fragment(br#"static $n = 1 + 1;"#).expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::StaticVar {
                name: "n".to_string(),
                init: EvalExpr::Binary {
                    op: EvalBinOp::Add,
                    left: Box::new(EvalExpr::Const(EvalConst::Int(1))),
                    right: Box::new(EvalExpr::Const(EvalConst::Int(1))),
                },
            }]
        );
    }

    /// Verifies global declarations preserve source-order variable names.
    #[test]
    fn parse_fragment_accepts_global_source() {
        let program = parse_fragment(br#"global $left, $right;"#).expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Global {
                vars: vec!["left".to_string(), "right".to_string()],
            }]
        );
    }

    /// Verifies eval magic constants lower to explicit EvalIR nodes with fragment line metadata.
    #[test]
    fn parse_fragment_accepts_magic_constants() {
        let program =
            parse_fragment(b"\nreturn __line__ . __FUNCTION__;").expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Return(Some(EvalExpr::Binary {
                op: EvalBinOp::Concat,
                left: Box::new(EvalExpr::Magic(EvalMagicConst::Line(2))),
                right: Box::new(EvalExpr::Magic(EvalMagicConst::Function)),
            }))]
        );
    }

    /// Verifies file-dependent eval magic constants lower to EvalIR nodes.
    #[test]
    fn parse_fragment_accepts_file_magic_constants() {
        let program = parse_fragment(b"return __FILE__ . __dir__;").expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Return(Some(EvalExpr::Binary {
                op: EvalBinOp::Concat,
                left: Box::new(EvalExpr::Magic(EvalMagicConst::File)),
                right: Box::new(EvalExpr::Magic(EvalMagicConst::Dir)),
            }))]
        );
    }

    /// Verifies eval scope magic constants lower with namespace resolved at parse time.
    #[test]
    fn parse_fragment_accepts_scope_magic_constants() {
        let program = parse_fragment(b"return __CLASS__ . __NAMESPACE__ . __TRAIT__ . __METHOD__;")
            .expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Return(Some(EvalExpr::Binary {
                op: EvalBinOp::Concat,
                left: Box::new(EvalExpr::Binary {
                    op: EvalBinOp::Concat,
                    left: Box::new(EvalExpr::Binary {
                        op: EvalBinOp::Concat,
                        left: Box::new(EvalExpr::Magic(EvalMagicConst::Class)),
                        right: Box::new(EvalExpr::Const(EvalConst::String(String::new()))),
                    }),
                    right: Box::new(EvalExpr::Magic(EvalMagicConst::Trait)),
                }),
                right: Box::new(EvalExpr::Magic(EvalMagicConst::Method)),
            }))]
        );
    }

    /// Verifies PHP comments are skipped while preserving fragment line numbers.
    #[test]
    fn parse_fragment_skips_comments_and_preserves_line_metadata() {
        let program = parse_fragment(b"// leading\n# hash\n/* block\ncomment */ return __LINE__;")
            .expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Return(Some(EvalExpr::Magic(
                EvalMagicConst::Line(4)
            )))]
        );
    }

    /// Verifies unterminated block comments fail before partial EvalIR is returned.
    #[test]
    fn parse_fragment_rejects_unterminated_block_comment() {
        assert_eq!(
            parse_fragment(b"/* open").unwrap_err(),
            EvalParseError::UnterminatedComment
        );
    }

    /// Verifies comparison operators parse with lower precedence than arithmetic.
    #[test]
    fn parse_fragment_accepts_comparison_source() {
        let program = parse_fragment(br#"return $i + 1 < 3;"#).expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Return(Some(EvalExpr::Binary {
                op: EvalBinOp::Lt,
                left: Box::new(EvalExpr::Binary {
                    op: EvalBinOp::Add,
                    left: Box::new(EvalExpr::LoadVar("i".to_string())),
                    right: Box::new(EvalExpr::Const(EvalConst::Int(1))),
                }),
                right: Box::new(EvalExpr::Const(EvalConst::Int(3))),
            }))]
        );
    }

    /// Verifies the spaceship operator parses at ordered-comparison precedence.
    #[test]
    fn parse_fragment_accepts_spaceship_source() {
        let program = parse_fragment(br#"return $i + 1 <=> 3;"#).expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Return(Some(EvalExpr::Binary {
                op: EvalBinOp::Spaceship,
                left: Box::new(EvalExpr::Binary {
                    op: EvalBinOp::Add,
                    left: Box::new(EvalExpr::LoadVar("i".to_string())),
                    right: Box::new(EvalExpr::Const(EvalConst::Int(1))),
                }),
                right: Box::new(EvalExpr::Const(EvalConst::Int(3))),
            }))]
        );
    }

    /// Verifies loose equality operators parse as binary EvalIR expressions.
    #[test]
    fn parse_fragment_accepts_loose_equality_source() {
        let program = parse_fragment(br#"return "a" != "b";"#).expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Return(Some(EvalExpr::Binary {
                op: EvalBinOp::LooseNotEq,
                left: Box::new(EvalExpr::Const(EvalConst::String("a".to_string()))),
                right: Box::new(EvalExpr::Const(EvalConst::String("b".to_string()))),
            }))]
        );
    }

    /// Verifies strict equality operators parse as distinct EvalIR comparisons.
    #[test]
    fn parse_fragment_accepts_strict_equality_source() {
        let program = parse_fragment(br#"return "10" === "10" && "10" !== 10;"#)
            .expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Return(Some(EvalExpr::Binary {
                op: EvalBinOp::LogicalAnd,
                left: Box::new(EvalExpr::Binary {
                    op: EvalBinOp::StrictEq,
                    left: Box::new(EvalExpr::Const(EvalConst::String("10".to_string()))),
                    right: Box::new(EvalExpr::Const(EvalConst::String("10".to_string()))),
                }),
                right: Box::new(EvalExpr::Binary {
                    op: EvalBinOp::StrictNotEq,
                    left: Box::new(EvalExpr::Const(EvalConst::String("10".to_string()))),
                    right: Box::new(EvalExpr::Const(EvalConst::Int(10))),
                }),
            }))]
        );
    }

    /// Verifies logical operators parse with `&&` binding tighter than `||`.
    #[test]
    fn parse_fragment_accepts_short_circuit_logical_source() {
        let program =
            parse_fragment(br#"return $a && $b || false;"#).expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Return(Some(EvalExpr::Binary {
                op: EvalBinOp::LogicalOr,
                left: Box::new(EvalExpr::Binary {
                    op: EvalBinOp::LogicalAnd,
                    left: Box::new(EvalExpr::LoadVar("a".to_string())),
                    right: Box::new(EvalExpr::LoadVar("b".to_string())),
                }),
                right: Box::new(EvalExpr::Const(EvalConst::Bool(false))),
            }))]
        );
    }

    /// Verifies PHP logical keywords parse case-insensitively with their own precedence.
    #[test]
    fn parse_fragment_accepts_keyword_logical_source() {
        let program =
            parse_fragment(br#"return false || true AnD false;"#).expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Return(Some(EvalExpr::Binary {
                op: EvalBinOp::LogicalAnd,
                left: Box::new(EvalExpr::Binary {
                    op: EvalBinOp::LogicalOr,
                    left: Box::new(EvalExpr::Const(EvalConst::Bool(false))),
                    right: Box::new(EvalExpr::Const(EvalConst::Bool(true))),
                }),
                right: Box::new(EvalExpr::Const(EvalConst::Bool(false))),
            }))]
        );
    }

    /// Verifies PHP `xor` binds between `or` and `and` in eval expressions.
    #[test]
    fn parse_fragment_accepts_keyword_xor_source() {
        let program =
            parse_fragment(br#"return true XoR false or false;"#).expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Return(Some(EvalExpr::Binary {
                op: EvalBinOp::LogicalOr,
                left: Box::new(EvalExpr::Binary {
                    op: EvalBinOp::LogicalXor,
                    left: Box::new(EvalExpr::Const(EvalConst::Bool(true))),
                    right: Box::new(EvalExpr::Const(EvalConst::Bool(false))),
                }),
                right: Box::new(EvalExpr::Const(EvalConst::Bool(false))),
            }))]
        );
    }

    /// Verifies ternary expressions parse below logical OR and preserve both branches.
    #[test]
    fn parse_fragment_accepts_ternary_source() {
        let program =
            parse_fragment(br#"return $a || $b ? "yes" : "no";"#).expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Return(Some(EvalExpr::Ternary {
                condition: Box::new(EvalExpr::Binary {
                    op: EvalBinOp::LogicalOr,
                    left: Box::new(EvalExpr::LoadVar("a".to_string())),
                    right: Box::new(EvalExpr::LoadVar("b".to_string())),
                }),
                then_branch: Some(Box::new(EvalExpr::Const(EvalConst::String(
                    "yes".to_string()
                )))),
                else_branch: Box::new(EvalExpr::Const(EvalConst::String("no".to_string()))),
            }))]
        );
    }

    /// Verifies PHP's short ternary form omits the explicit then branch in EvalIR.
    #[test]
    fn parse_fragment_accepts_short_ternary_source() {
        let program =
            parse_fragment(br#"return $name ?: "fallback";"#).expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Return(Some(EvalExpr::Ternary {
                condition: Box::new(EvalExpr::LoadVar("name".to_string())),
                then_branch: None,
                else_branch: Box::new(EvalExpr::Const(EvalConst::String("fallback".to_string()))),
            }))]
        );
    }

    /// Verifies null coalescing parses as a right-associative expression.
    #[test]
    fn parse_fragment_accepts_null_coalesce_source() {
        let program =
            parse_fragment(br#"return $a ?? $b ?? "fallback";"#).expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Return(Some(EvalExpr::NullCoalesce {
                value: Box::new(EvalExpr::LoadVar("a".to_string())),
                default: Box::new(EvalExpr::NullCoalesce {
                    value: Box::new(EvalExpr::LoadVar("b".to_string())),
                    default: Box::new(EvalExpr::Const(EvalConst::String("fallback".to_string()))),
                }),
            }))]
        );
    }

    /// Verifies match expressions preserve subject, patterns, and default expression.
    #[test]
    fn parse_fragment_accepts_match_source() {
        let program = parse_fragment(br#"return match ($x) { 1, 2 => "small", default => "other" };"#)
            .expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Return(Some(EvalExpr::Match {
                subject: Box::new(EvalExpr::LoadVar("x".to_string())),
                arms: vec![EvalMatchArm {
                    patterns: vec![
                        EvalExpr::Const(EvalConst::Int(1)),
                        EvalExpr::Const(EvalConst::Int(2)),
                    ],
                    value: EvalExpr::Const(EvalConst::String("small".to_string())),
                }],
                default: Some(Box::new(EvalExpr::Const(EvalConst::String(
                    "other".to_string()
                )))),
            }))]
        );
    }

    /// Verifies null coalescing binds tighter than PHP ternary expressions.
    #[test]
    fn parse_fragment_null_coalesce_binds_tighter_than_ternary() {
        let program =
            parse_fragment(br#"return $a ?? $b ? "yes" : "no";"#).expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Return(Some(EvalExpr::Ternary {
                condition: Box::new(EvalExpr::NullCoalesce {
                    value: Box::new(EvalExpr::LoadVar("a".to_string())),
                    default: Box::new(EvalExpr::LoadVar("b".to_string())),
                }),
                then_branch: Some(Box::new(EvalExpr::Const(EvalConst::String(
                    "yes".to_string()
                )))),
                else_branch: Box::new(EvalExpr::Const(EvalConst::String("no".to_string()))),
            }))]
        );
    }

    /// Verifies logical negation parses as a unary expression before comparisons.
    #[test]
    fn parse_fragment_accepts_logical_not_source() {
        let program = parse_fragment(br#"return !$flag == true;"#).expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Return(Some(EvalExpr::Binary {
                op: EvalBinOp::LooseEq,
                left: Box::new(EvalExpr::Unary {
                    op: EvalUnaryOp::LogicalNot,
                    expr: Box::new(EvalExpr::LoadVar("flag".to_string())),
                }),
                right: Box::new(EvalExpr::Const(EvalConst::Bool(true))),
            }))]
        );
    }

    /// Verifies unary numeric operators bind tighter than multiplication.
    #[test]
    fn parse_fragment_accepts_unary_numeric_source() {
        let program = parse_fragment(br#"return -$x * +2;"#).expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Return(Some(EvalExpr::Binary {
                op: EvalBinOp::Mul,
                left: Box::new(EvalExpr::Unary {
                    op: EvalUnaryOp::Negate,
                    expr: Box::new(EvalExpr::LoadVar("x".to_string())),
                }),
                right: Box::new(EvalExpr::Unary {
                    op: EvalUnaryOp::Plus,
                    expr: Box::new(EvalExpr::Const(EvalConst::Int(2))),
                }),
            }))]
        );
    }

    /// Verifies print fragments lower to expression-form print with the printed value.
    #[test]
    fn parse_fragment_accepts_print_source() {
        let program = parse_fragment(br#"print "hi";"#).expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Expr(EvalExpr::Print(Box::new(EvalExpr::Const(
                EvalConst::String("hi".to_string())
            ))))]
        );
    }

    /// Verifies single- and double-quoted strings keep PHP-compatible simple escapes.
    #[test]
    fn parse_fragment_preserves_php_string_escape_semantics() {
        let program =
            parse_fragment(br#"return ['A\nB', "A\qB", "A\v\e\fB", 'It\'s'];"#)
                .expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Return(Some(EvalExpr::Array(vec![
                EvalArrayElement::Value(EvalExpr::Const(EvalConst::String(
                    "A\\nB".to_string()
                ))),
                EvalArrayElement::Value(EvalExpr::Const(EvalConst::String(
                    "A\\qB".to_string()
                ))),
                EvalArrayElement::Value(EvalExpr::Const(EvalConst::String(
                    "A\x0b\x1b\x0cB".to_string()
                ))),
                EvalArrayElement::Value(EvalExpr::Const(EvalConst::String("It's".to_string()))),
            ])))]
        );
    }

    /// Verifies call expressions preserve their callee name and source-order arguments.
    #[test]
    fn parse_fragment_accepts_call_expression_source() {
        let program =
            parse_fragment(br#"return eval("return 1;");"#).expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Return(Some(EvalExpr::Call {
                name: "eval".to_string(),
                args: vec![EvalCallArg::positional(EvalExpr::Const(EvalConst::String(
                    "return 1;".to_string()
                )))],
            }))]
        );
    }

    /// Verifies include and require constructs parse as expressions with path metadata.
    #[test]
    fn parse_fragment_accepts_include_require_expression_source() {
        let program = parse_fragment(br#"return include "a" . ".php"; require_once("b.php");"#)
            .expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[
                EvalStmt::Return(Some(EvalExpr::Include {
                    path: Box::new(EvalExpr::Binary {
                        op: EvalBinOp::Concat,
                        left: Box::new(EvalExpr::Const(EvalConst::String("a".to_string()))),
                        right: Box::new(EvalExpr::Const(EvalConst::String(".php".to_string()))),
                    }),
                    required: false,
                    once: false,
                })),
                EvalStmt::Expr(EvalExpr::Include {
                    path: Box::new(EvalExpr::Const(EvalConst::String("b.php".to_string()))),
                    required: true,
                    once: true,
                }),
            ]
        );
    }

    /// Verifies explicitly qualified call expressions normalize away the leading slash.
    #[test]
    fn parse_fragment_accepts_qualified_call_expression_source() {
        let program = parse_fragment(br#"return \strlen("abcd");"#).expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Return(Some(EvalExpr::Call {
                name: "strlen".to_string(),
                args: vec![EvalCallArg::positional(EvalExpr::Const(EvalConst::String(
                    "abcd".to_string()
                )))],
            }))]
        );
    }

    /// Verifies variable callable expressions lower to dynamic calls with source-order args.
    #[test]
    fn parse_fragment_accepts_dynamic_call_expression_source() {
        let program = parse_fragment(br#"return $fn(first: "a", ...$rest);"#)
            .expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Return(Some(EvalExpr::DynamicCall {
                callee: Box::new(EvalExpr::LoadVar("fn".to_string())),
                args: vec![
                    EvalCallArg::named(
                        "first",
                        EvalExpr::Const(EvalConst::String("a".to_string())),
                    ),
                    EvalCallArg::spread(EvalExpr::LoadVar("rest".to_string())),
                ],
            }))]
        );
    }

    /// Verifies dynamic calls can be applied after another postfix expression.
    #[test]
    fn parse_fragment_accepts_postfix_dynamic_call_source() {
        let program =
            parse_fragment(br#"return $callbacks[0]("abcd");"#).expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Return(Some(EvalExpr::DynamicCall {
                callee: Box::new(EvalExpr::ArrayGet {
                    array: Box::new(EvalExpr::LoadVar("callbacks".to_string())),
                    index: Box::new(EvalExpr::Const(EvalConst::Int(0))),
                }),
                args: vec![EvalCallArg::positional(EvalExpr::Const(EvalConst::String(
                    "abcd".to_string()
                )))],
            }))]
        );
    }

    /// Verifies bare constant names lower to dynamic constant-fetch expressions.
    #[test]
    fn parse_fragment_accepts_constant_fetch_source() {
        let program = parse_fragment(br#"return \Dyn\EvalConst;"#).expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Return(Some(EvalExpr::ConstFetch(
                "Dyn\\EvalConst".to_string()
            )))]
        );
    }

    /// Verifies function calls preserve named arguments in source order.
    #[test]
    fn parse_fragment_accepts_named_call_argument_source() {
        let program = parse_fragment(br#"return add(y: 2, x: 1);"#).expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Return(Some(EvalExpr::Call {
                name: "add".to_string(),
                args: vec![
                    EvalCallArg::named("y", EvalExpr::Const(EvalConst::Int(2))),
                    EvalCallArg::named("x", EvalExpr::Const(EvalConst::Int(1))),
                ],
            }))]
        );
    }

    /// Verifies function calls preserve spread arguments in source order.
    #[test]
    fn parse_fragment_accepts_spread_call_argument_source() {
        let program = parse_fragment(br#"return add(...$args);"#).expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Return(Some(EvalExpr::Call {
                name: "add".to_string(),
                args: vec![EvalCallArg::spread(EvalExpr::LoadVar("args".to_string()))],
            }))]
        );
    }

    /// Verifies `isset` parses as a case-insensitive function-like expression.
    #[test]
    fn parse_fragment_accepts_isset_source() {
        let program =
            parse_fragment(br#"return ISSET($x, $items["k"]);"#).expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Return(Some(EvalExpr::Call {
                name: "isset".to_string(),
                args: vec![
                    EvalCallArg::positional(EvalExpr::LoadVar("x".to_string())),
                    EvalCallArg::positional(EvalExpr::ArrayGet {
                        array: Box::new(EvalExpr::LoadVar("items".to_string())),
                        index: Box::new(EvalExpr::Const(EvalConst::String("k".to_string()))),
                    }),
                ],
            }))]
        );
    }

    /// Verifies `empty` parses as a case-insensitive function-like expression.
    #[test]
    fn parse_fragment_accepts_empty_source() {
        let program =
            parse_fragment(br#"return EMPTY($items["k"]);"#).expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Return(Some(EvalExpr::Call {
                name: "empty".to_string(),
                args: vec![EvalCallArg::positional(EvalExpr::ArrayGet {
                    array: Box::new(EvalExpr::LoadVar("items".to_string())),
                    index: Box::new(EvalExpr::Const(EvalConst::String("k".to_string()))),
                })],
            }))]
        );
    }

    /// Verifies indexed array literals and reads parse as runtime array expressions.
    #[test]
    fn parse_fragment_accepts_indexed_array_read_source() {
        let program = parse_fragment(br#"return [1, 2][0];"#).expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Return(Some(EvalExpr::ArrayGet {
                array: Box::new(EvalExpr::Array(vec![
                    EvalArrayElement::Value(EvalExpr::Const(EvalConst::Int(1))),
                    EvalArrayElement::Value(EvalExpr::Const(EvalConst::Int(2))),
                ])),
                index: Box::new(EvalExpr::Const(EvalConst::Int(0))),
            }))]
        );
    }

    /// Verifies legacy `array(...)` literals parse through the same EvalIR array node.
    #[test]
    fn parse_fragment_accepts_legacy_array_literal_source() {
        let program = parse_fragment(br#"return array(1, "name" => "Ada",)[1];"#)
            .expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Return(Some(EvalExpr::ArrayGet {
                array: Box::new(EvalExpr::Array(vec![
                    EvalArrayElement::Value(EvalExpr::Const(EvalConst::Int(1))),
                    EvalArrayElement::KeyValue {
                        key: EvalExpr::Const(EvalConst::String("name".to_string())),
                        value: EvalExpr::Const(EvalConst::String("Ada".to_string())),
                    },
                ])),
                index: Box::new(EvalExpr::Const(EvalConst::Int(1))),
            }))]
        );
    }

    /// Verifies associative array literals preserve explicit key/value expressions.
    #[test]
    fn parse_fragment_accepts_assoc_array_literal_source() {
        let program =
            parse_fragment(br#"return ["name" => "Ada"];"#).expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Return(Some(EvalExpr::Array(vec![
                EvalArrayElement::KeyValue {
                    key: EvalExpr::Const(EvalConst::String("name".to_string())),
                    value: EvalExpr::Const(EvalConst::String("Ada".to_string())),
                }
            ])))]
        );
    }

    /// Verifies indexed array writes parse as variable-target array set statements.
    #[test]
    fn parse_fragment_accepts_indexed_array_write_source() {
        let program = parse_fragment(br#"$items[1] = "x";"#).expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::ArraySetVar {
                name: "items".to_string(),
                index: EvalExpr::Const(EvalConst::Int(1)),
                value: EvalExpr::Const(EvalConst::String("x".to_string())),
            }]
        );
    }

    /// Verifies indexed array append syntax parses as a variable-target append statement.
    #[test]
    fn parse_fragment_accepts_indexed_array_append_source() {
        let program = parse_fragment(br#"$items[] = "x";"#).expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::ArrayAppendVar {
                name: "items".to_string(),
                value: EvalExpr::Const(EvalConst::String("x".to_string())),
            }]
        );
    }

    /// Verifies array append syntax is accepted inside `for` update clauses.
    #[test]
    fn parse_fragment_accepts_array_append_in_for_update_source() {
        let program = parse_fragment(br#"for ($i = 0; $i < 2; $items[] = $i) { $i += 1; }"#)
            .expect("fragment should parse");
        let [EvalStmt::For { update, .. }] = program.statements() else {
            panic!("expected for statement");
        };
        assert_eq!(
            update,
            &vec![EvalStmt::ArrayAppendVar {
                name: "items".to_string(),
                value: EvalExpr::LoadVar("i".to_string()),
            }]
        );
    }

    /// Verifies object property reads parse as postfix EvalIR expressions.
    #[test]
    fn parse_fragment_accepts_property_read_source() {
        let program = parse_fragment(br#"return $this->x;"#).expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Return(Some(EvalExpr::PropertyGet {
                object: Box::new(EvalExpr::LoadVar("this".to_string())),
                property: "x".to_string(),
            }))]
        );
    }

    /// Verifies property names preserve source case while keywords remain case-insensitive.
    #[test]
    fn parse_fragment_preserves_property_case_source() {
        let program =
            parse_fragment(br#"RETURN $this->camelName;"#).expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Return(Some(EvalExpr::PropertyGet {
                object: Box::new(EvalExpr::LoadVar("this".to_string())),
                property: "camelName".to_string(),
            }))]
        );
    }

    /// Verifies object method calls parse as postfix EvalIR call expressions.
    #[test]
    fn parse_fragment_accepts_method_call_source() {
        let program = parse_fragment(br#"return $this->Answer();"#).expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Return(Some(EvalExpr::MethodCall {
                object: Box::new(EvalExpr::LoadVar("this".to_string())),
                method: "answer".to_string(),
                args: Vec::new(),
            }))]
        );
    }

    /// Verifies object construction parses as a named EvalIR expression.
    #[test]
    fn parse_fragment_accepts_new_object_source() {
        let program = parse_fragment(br#"return new Box();"#).expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Return(Some(EvalExpr::NewObject {
                class_name: "Box".to_string(),
                args: Vec::new(),
            }))]
        );
    }

    /// Verifies object construction accepts explicitly qualified class names.
    #[test]
    fn parse_fragment_accepts_qualified_new_object_source() {
        let program =
            parse_fragment(br#"return new \EvalNs\Box();"#).expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Return(Some(EvalExpr::NewObject {
                class_name: "EvalNs\\Box".to_string(),
                args: Vec::new(),
            }))]
        );
    }

    /// Verifies object method calls preserve source-order argument expressions.
    #[test]
    fn parse_fragment_accepts_method_call_args_source() {
        let program =
            parse_fragment(br#"return $this->add($x + 1);"#).expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Return(Some(EvalExpr::MethodCall {
                object: Box::new(EvalExpr::LoadVar("this".to_string())),
                method: "add".to_string(),
                args: vec![EvalCallArg::positional(EvalExpr::Binary {
                    op: EvalBinOp::Add,
                    left: Box::new(EvalExpr::LoadVar("x".to_string())),
                    right: Box::new(EvalExpr::Const(EvalConst::Int(1))),
                })],
            }))]
        );
    }

    /// Verifies object method calls parse multiple argument expressions in source order.
    #[test]
    fn parse_fragment_accepts_method_call_multiple_args_source() {
        let program =
            parse_fragment(br#"return $this->label($x, "ok");"#).expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Return(Some(EvalExpr::MethodCall {
                object: Box::new(EvalExpr::LoadVar("this".to_string())),
                method: "label".to_string(),
                args: vec![
                    EvalCallArg::positional(EvalExpr::LoadVar("x".to_string())),
                    EvalCallArg::positional(EvalExpr::Const(EvalConst::String("ok".to_string()))),
                ],
            }))]
        );
    }

    /// Verifies object property writes parse as dedicated EvalIR statements.
    #[test]
    fn parse_fragment_accepts_property_write_source() {
        let program =
            parse_fragment(br#"$this->x = $this->x + 1;"#).expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::PropertySet {
                object: EvalExpr::LoadVar("this".to_string()),
                property: "x".to_string(),
                value: EvalExpr::Binary {
                    op: EvalBinOp::Add,
                    left: Box::new(EvalExpr::PropertyGet {
                        object: Box::new(EvalExpr::LoadVar("this".to_string())),
                        property: "x".to_string(),
                    }),
                    right: Box::new(EvalExpr::Const(EvalConst::Int(1))),
                },
            }]
        );
    }

    /// Verifies while fragments lower to loop statements with a nested block.
    #[test]
    fn parse_fragment_accepts_while_source() {
        let program = parse_fragment(br#"while ($flag) { echo $flag; $flag = false; }"#)
            .expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::While {
                condition: EvalExpr::LoadVar("flag".to_string()),
                body: vec![
                    EvalStmt::Echo(EvalExpr::LoadVar("flag".to_string())),
                    EvalStmt::StoreVar {
                        name: "flag".to_string(),
                        value: EvalExpr::Const(EvalConst::Bool(false)),
                    },
                ],
            }]
        );
    }

    /// Verifies do/while fragments lower to body-first loop statements.
    #[test]
    fn parse_fragment_accepts_do_while_source() {
        let program = parse_fragment(br#"do { echo $flag; $flag = false; } while ($flag);"#)
            .expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::DoWhile {
                body: vec![
                    EvalStmt::Echo(EvalExpr::LoadVar("flag".to_string())),
                    EvalStmt::StoreVar {
                        name: "flag".to_string(),
                        value: EvalExpr::Const(EvalConst::Bool(false)),
                    },
                ],
                condition: EvalExpr::LoadVar("flag".to_string()),
            }]
        );
    }

    /// Verifies loop control statements parse inside while blocks.
    #[test]
    fn parse_fragment_accepts_break_and_continue_source() {
        let program = parse_fragment(br#"while ($flag) { continue; break; }"#)
            .expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::While {
                condition: EvalExpr::LoadVar("flag".to_string()),
                body: vec![EvalStmt::Continue, EvalStmt::Break],
            }]
        );
    }

    /// Verifies return fragments parse optional return expressions.
    #[test]
    fn parse_fragment_accepts_return_source() {
        let program = parse_fragment(b"return ($x - 1) * 4;").expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Return(Some(EvalExpr::Binary {
                op: EvalBinOp::Mul,
                left: Box::new(EvalExpr::Binary {
                    op: EvalBinOp::Sub,
                    left: Box::new(EvalExpr::LoadVar("x".to_string())),
                    right: Box::new(EvalExpr::Const(EvalConst::Int(1))),
                }),
                right: Box::new(EvalExpr::Const(EvalConst::Int(4))),
            }))]
        );
    }

    /// Verifies throw statements lower to a Throwable expression carried by EvalIR.
    #[test]
    fn parse_fragment_accepts_throw_source() {
        let program =
            parse_fragment(br#"throw new Exception("eval boom");"#).expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Throw(EvalExpr::NewObject {
                class_name: "Exception".to_string(),
                args: vec![EvalCallArg::positional(EvalExpr::Const(EvalConst::String(
                    "eval boom".to_string()
                )))],
            })]
        );
    }

    /// Verifies unset fragments expand to one by-name unset statement per variable.
    #[test]
    fn parse_fragment_accepts_unset_source() {
        let program = parse_fragment(b"unset($x, $y);").expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[
                EvalStmt::UnsetVar {
                    name: "x".to_string()
                },
                EvalStmt::UnsetVar {
                    name: "y".to_string()
                },
            ]
        );
    }

    /// Verifies eval fragments reject PHP opening tags.
    #[test]
    fn parse_fragment_rejects_opening_tag() {
        assert_eq!(
            parse_fragment(b"<?php echo 1;"),
            Err(EvalParseError::PhpOpenTag)
        );
    }

    /// Verifies empty class declarations lower to dynamic class-registration statements.
    #[test]
    fn parse_fragment_accepts_empty_class_declaration_source() {
        let program = parse_fragment(b"class DynEvalClass {};").expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::ClassDecl {
                name: "DynEvalClass".to_string(),
            }]
        );
    }

    /// Verifies non-empty class declarations stay outside the supported eval subset.
    #[test]
    fn parse_fragment_rejects_non_empty_class_as_unsupported_construct() {
        assert_eq!(
            parse_fragment(b"class DynEvalUnsupported { public int $x = 1; }"),
            Err(EvalParseError::UnsupportedConstruct)
        );
    }

    /// Verifies malformed object construction reports an unexpected token.
    #[test]
    fn parse_fragment_rejects_new_without_class_name() {
        assert_eq!(
            parse_fragment(b"return new ();"),
            Err(EvalParseError::UnexpectedToken)
        );
    }

    /// Verifies unsupported expression keywords report the unsupported construct status.
    #[test]
    fn parse_fragment_rejects_expression_keywords_as_unsupported_constructs() {
        for source in [
            b"return clone $value;" as &[u8],
            b"return yield 1;" as &[u8],
        ] {
            assert_eq!(
                parse_fragment(source),
                Err(EvalParseError::UnsupportedConstruct)
            );
        }
    }

    /// Verifies malformed statements report parse errors instead of partial IR.
    #[test]
    fn parse_fragment_rejects_missing_semicolon() {
        assert_eq!(
            parse_fragment(b"$x = 1"),
            Err(EvalParseError::ExpectedSemicolon)
        );
    }
}
