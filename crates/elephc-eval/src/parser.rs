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
    EvalArrayElement, EvalBinOp, EvalConst, EvalExpr, EvalMagicConst, EvalProgram, EvalStmt,
    EvalSwitchCase, EvalUnaryOp,
};

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
    Minus,
    Arrow,
    Star,
    Dot,
    Equal,
    EqualEqual,
    Bang,
    NotEqual,
    AndAnd,
    OrOr,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    FatArrow,
    Semicolon,
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Comma,
    Colon,
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
        self.skip_ws();
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
                Ok(TokenKind::Plus)
            }
            '-' => {
                self.bump_char();
                if self.peek_char() == Some('>') {
                    self.bump_char();
                    Ok(TokenKind::Arrow)
                } else {
                    Ok(TokenKind::Minus)
                }
            }
            '*' => {
                self.bump_char();
                Ok(TokenKind::Star)
            }
            '.' => {
                self.bump_char();
                Ok(TokenKind::Dot)
            }
            '=' => {
                self.bump_char();
                if self.peek_char() == Some('=') {
                    self.bump_char();
                    Ok(TokenKind::EqualEqual)
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
                    Ok(TokenKind::NotEqual)
                } else {
                    Ok(TokenKind::Bang)
                }
            }
            '&' => {
                self.bump_char();
                if self.peek_char() == Some('&') {
                    self.bump_char();
                    Ok(TokenKind::AndAnd)
                } else {
                    Err(EvalParseError::UnexpectedToken)
                }
            }
            '|' => {
                self.bump_char();
                if self.peek_char() == Some('|') {
                    self.bump_char();
                    Ok(TokenKind::OrOr)
                } else {
                    Err(EvalParseError::UnexpectedToken)
                }
            }
            '<' => {
                self.bump_char();
                if self.peek_char() == Some('=') {
                    self.bump_char();
                    Ok(TokenKind::LessEqual)
                } else {
                    Ok(TokenKind::Less)
                }
            }
            '>' => {
                self.bump_char();
                if self.peek_char() == Some('=') {
                    self.bump_char();
                    Ok(TokenKind::GreaterEqual)
                } else {
                    Ok(TokenKind::Greater)
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
                out.push(match escaped {
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    '\\' => '\\',
                    '\'' => '\'',
                    '"' => '"',
                    other => other,
                });
            } else {
                out.push(ch);
            }
        }
        Err(EvalParseError::UnterminatedString)
    }

    /// Advances past ASCII and Unicode whitespace.
    fn skip_ws(&mut self) {
        while self.peek_char().is_some_and(char::is_whitespace) {
            self.bump_char();
        }
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
}

impl Parser {
    /// Creates a parser over tokens produced from a source fragment.
    fn new(tokens: Vec<TokenKind>, source_len: usize) -> Self {
        Self {
            tokens,
            pos: 0,
            source_len,
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
                let expr = self.parse_expr()?;
                self.expect_semicolon()?;
                Ok(vec![EvalStmt::Echo(expr)])
            }
            TokenKind::Ident(name) if ident_eq(name, "for") => self.parse_for_stmt(),
            TokenKind::Ident(name) if ident_eq(name, "foreach") => self.parse_foreach_stmt(),
            TokenKind::Ident(name) if ident_eq(name, "function") => self.parse_function_decl_stmt(),
            TokenKind::Ident(name) if ident_eq(name, "if") => self.parse_if_stmt(),
            TokenKind::Ident(name) if ident_eq(name, "return") => {
                self.advance();
                if self.consume_semicolon() {
                    return Ok(vec![EvalStmt::Return(None)]);
                }
                let expr = self.parse_expr()?;
                self.expect_semicolon()?;
                Ok(vec![EvalStmt::Return(Some(expr))])
            }
            TokenKind::Ident(name) if ident_eq(name, "switch") => self.parse_switch_stmt(),
            TokenKind::Ident(name) if ident_eq(name, "unset") => self.parse_unset_stmt(),
            TokenKind::Ident(name) if ident_eq(name, "while") => self.parse_while_stmt(),
            TokenKind::DollarIdent(_) if matches!(self.peek(), TokenKind::Arrow) => {
                self.parse_property_stmt(true)
            }
            TokenKind::DollarIdent(name) if matches!(self.peek(), TokenKind::LBracket) => {
                self.parse_array_set_stmt(name.clone())
            }
            TokenKind::DollarIdent(name) if matches!(self.peek(), TokenKind::Equal) => {
                let name = name.clone();
                self.advance();
                self.advance();
                let value = self.parse_expr()?;
                self.expect_semicolon()?;
                Ok(vec![EvalStmt::StoreVar { name, value }])
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

    /// Parses `$name[index] = expr;` for indexed-array eval writes.
    fn parse_array_set_stmt(&mut self, name: String) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        self.expect(TokenKind::LBracket)?;
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

    /// Parses `foreach (expr as $value) { ... }`.
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
        self.expect(TokenKind::RParen)?;
        let body = self.parse_statement_body()?;
        Ok(vec![EvalStmt::Foreach {
            array,
            value_name,
            body,
        }])
    }

    /// Parses `function name($param, ...) { ... }` declarations.
    fn parse_function_decl_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        let TokenKind::Ident(name) = self.current() else {
            return Err(EvalParseError::UnexpectedToken);
        };
        let name = name.clone();
        self.advance();
        self.expect(TokenKind::LParen)?;
        let params = self.parse_function_params()?;
        let body = self.parse_block()?;
        Ok(vec![EvalStmt::FunctionDecl { name, params, body }])
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
            TokenKind::DollarIdent(name) if matches!(self.peek(), TokenKind::LBracket) => {
                self.parse_array_set_clause(name.clone())
            }
            TokenKind::DollarIdent(_) if matches!(self.peek(), TokenKind::Arrow) => {
                self.parse_property_stmt(false)
            }
            TokenKind::DollarIdent(name) if matches!(self.peek(), TokenKind::Equal) => {
                let name = name.clone();
                self.advance();
                self.advance();
                let value = self.parse_expr()?;
                Ok(vec![EvalStmt::StoreVar { name, value }])
            }
            _ => {
                let expr = self.parse_expr()?;
                Ok(vec![EvalStmt::Expr(expr)])
            }
        }
    }

    /// Parses `$name[index] = expr` in a `for` clause.
    fn parse_array_set_clause(&mut self, name: String) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        self.expect(TokenKind::LBracket)?;
        let index = self.parse_expr()?;
        self.expect(TokenKind::RBracket)?;
        self.expect(TokenKind::Equal)?;
        let value = self.parse_expr()?;
        Ok(vec![EvalStmt::ArraySetVar { name, index, value }])
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
            self.parse_stmt()
        }
    }

    /// Parses a brace-delimited statement block.
    fn parse_block(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.expect(TokenKind::LBrace)?;
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
        self.parse_logical_or()
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
        let mut expr = self.parse_equality()?;
        while self.consume(TokenKind::AndAnd) {
            let right = self.parse_equality()?;
            expr = EvalExpr::Binary {
                op: EvalBinOp::LogicalAnd,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    /// Parses left-associative loose equality and inequality comparisons.
    fn parse_equality(&mut self) -> Result<EvalExpr, EvalParseError> {
        let mut expr = self.parse_ordering()?;
        loop {
            let op = if self.consume(TokenKind::EqualEqual) {
                EvalBinOp::LooseEq
            } else if self.consume(TokenKind::NotEqual) {
                EvalBinOp::LooseNotEq
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
        let mut expr = self.parse_concat()?;
        loop {
            let op = if self.consume(TokenKind::Less) {
                EvalBinOp::Lt
            } else if self.consume(TokenKind::LessEqual) {
                EvalBinOp::LtEq
            } else if self.consume(TokenKind::Greater) {
                EvalBinOp::Gt
            } else if self.consume(TokenKind::GreaterEqual) {
                EvalBinOp::GtEq
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

    /// Parses left-associative numeric multiplication.
    fn parse_mul(&mut self) -> Result<EvalExpr, EvalParseError> {
        let mut expr = self.parse_unary()?;
        while self.consume(TokenKind::Star) {
            let right = self.parse_unary()?;
            expr = EvalExpr::Binary {
                op: EvalBinOp::Mul,
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
        self.parse_postfix()
    }

    /// Parses postfix array reads, property reads, and method calls after a primary expression.
    fn parse_postfix(&mut self) -> Result<EvalExpr, EvalParseError> {
        let mut expr = self.parse_primary()?;
        loop {
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
            TokenKind::Ident(name) if matches!(self.peek(), TokenKind::LParen) => {
                self.parse_call_expr(name.clone())
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

    /// Parses a function-like call expression and its source-order arguments.
    fn parse_call_expr(&mut self, name: String) -> Result<EvalExpr, EvalParseError> {
        self.advance();
        let args = self.parse_call_args()?;
        Ok(EvalExpr::Call {
            name: name.to_ascii_lowercase(),
            args,
        })
    }

    /// Parses a parenthesized source-order argument list.
    fn parse_call_args(&mut self) -> Result<Vec<EvalExpr>, EvalParseError> {
        self.expect(TokenKind::LParen)?;
        let mut args = Vec::new();
        if self.consume(TokenKind::RParen) {
            return Ok(args);
        }
        loop {
            args.push(self.parse_expr()?);
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

    /// Parses an array literal with source-order optional key/value element expressions.
    fn parse_array_literal(&mut self) -> Result<EvalExpr, EvalParseError> {
        self.expect(TokenKind::LBracket)?;
        let mut elements = Vec::new();
        if self.consume(TokenKind::RBracket) {
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
            if self.consume(TokenKind::RBracket) {
                return Ok(EvalExpr::Array(elements));
            }
        }
        self.expect(TokenKind::RBracket)?;
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

/// Compares a source identifier to a PHP keyword using ASCII case-insensitive rules.
fn ident_eq(actual: &str, expected: &str) -> bool {
    actual.eq_ignore_ascii_case(expected)
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
                value_name: "item".to_string(),
                body: vec![EvalStmt::Echo(EvalExpr::LoadVar("item".to_string()))],
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

    /// Verifies call expressions preserve their callee name and source-order arguments.
    #[test]
    fn parse_fragment_accepts_call_expression_source() {
        let program =
            parse_fragment(br#"return eval("return 1;");"#).expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Return(Some(EvalExpr::Call {
                name: "eval".to_string(),
                args: vec![EvalExpr::Const(EvalConst::String("return 1;".to_string()))],
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
                    EvalExpr::LoadVar("x".to_string()),
                    EvalExpr::ArrayGet {
                        array: Box::new(EvalExpr::LoadVar("items".to_string())),
                        index: Box::new(EvalExpr::Const(EvalConst::String("k".to_string()))),
                    },
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
                args: vec![EvalExpr::ArrayGet {
                    array: Box::new(EvalExpr::LoadVar("items".to_string())),
                    index: Box::new(EvalExpr::Const(EvalConst::String("k".to_string()))),
                }],
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
                args: vec![EvalExpr::Binary {
                    op: EvalBinOp::Add,
                    left: Box::new(EvalExpr::LoadVar("x".to_string())),
                    right: Box::new(EvalExpr::Const(EvalConst::Int(1))),
                }],
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
                    EvalExpr::LoadVar("x".to_string()),
                    EvalExpr::Const(EvalConst::String("ok".to_string())),
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

    /// Verifies malformed statements report parse errors instead of partial IR.
    #[test]
    fn parse_fragment_rejects_missing_semicolon() {
        assert_eq!(
            parse_fragment(b"$x = 1"),
            Err(EvalParseError::ExpectedSemicolon)
        );
    }
}
