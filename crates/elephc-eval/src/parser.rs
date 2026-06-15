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
//! - Fragment spans must be based on call-site metadata when implemented.

use crate::errors::EvalParseError;
use crate::eval_ir::{EvalArrayElement, EvalBinOp, EvalConst, EvalExpr, EvalProgram, EvalStmt};

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
    Int(i64),
    Float(f64),
    String(String),
    Plus,
    Minus,
    Star,
    Dot,
    Equal,
    FatArrow,
    Semicolon,
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Comma,
    Eof,
}

/// Converts a UTF-8 eval source fragment into parser tokens.
struct Lexer<'a> {
    source: &'a str,
    pos: usize,
}

impl<'a> Lexer<'a> {
    /// Creates a lexer over a UTF-8 eval fragment.
    fn new(source: &'a str) -> Self {
        Self { source, pos: 0 }
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
                Ok(TokenKind::Minus)
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
                if self.peek_char() == Some('>') {
                    self.bump_char();
                    Ok(TokenKind::FatArrow)
                } else {
                    Ok(TokenKind::Equal)
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
            _ if is_ident_start(ch) => Ok(TokenKind::Ident(self.lex_ident().to_ascii_lowercase())),
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
            TokenKind::Ident(name) if name == "break" => {
                self.advance();
                self.expect_semicolon()?;
                Ok(vec![EvalStmt::Break])
            }
            TokenKind::Ident(name) if name == "continue" => {
                self.advance();
                self.expect_semicolon()?;
                Ok(vec![EvalStmt::Continue])
            }
            TokenKind::Ident(name) if name == "echo" => {
                self.advance();
                let expr = self.parse_expr()?;
                self.expect_semicolon()?;
                Ok(vec![EvalStmt::Echo(expr)])
            }
            TokenKind::Ident(name) if name == "for" => self.parse_for_stmt(),
            TokenKind::Ident(name) if name == "if" => self.parse_if_stmt(),
            TokenKind::Ident(name) if name == "return" => {
                self.advance();
                if self.consume_semicolon() {
                    return Ok(vec![EvalStmt::Return(None)]);
                }
                let expr = self.parse_expr()?;
                self.expect_semicolon()?;
                Ok(vec![EvalStmt::Return(Some(expr))])
            }
            TokenKind::Ident(name) if name == "unset" => self.parse_unset_stmt(),
            TokenKind::Ident(name) if name == "while" => self.parse_while_stmt(),
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
        let body = self.parse_block()?;
        Ok(vec![EvalStmt::For {
            init,
            condition,
            update,
            body,
        }])
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

    /// Parses `if (expr) { ... } [else { ... }]`.
    fn parse_if_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        self.expect(TokenKind::LParen)?;
        let condition = self.parse_expr()?;
        self.expect(TokenKind::RParen)?;
        let then_branch = self.parse_block()?;
        let else_branch = if matches!(self.current(), TokenKind::Ident(name) if name == "else") {
            self.advance();
            self.parse_block()?
        } else {
            Vec::new()
        };
        Ok(vec![EvalStmt::If {
            condition,
            then_branch,
            else_branch,
        }])
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
        let body = self.parse_block()?;
        Ok(vec![EvalStmt::While { condition, body }])
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

    /// Parses an expression using PHP-like precedence for `+` over `.`.
    fn parse_expr(&mut self) -> Result<EvalExpr, EvalParseError> {
        self.parse_concat()
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
        let mut expr = self.parse_postfix()?;
        while self.consume(TokenKind::Star) {
            let right = self.parse_postfix()?;
            expr = EvalExpr::Binary {
                op: EvalBinOp::Mul,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    /// Parses postfix array reads after a primary expression.
    fn parse_postfix(&mut self) -> Result<EvalExpr, EvalParseError> {
        let mut expr = self.parse_primary()?;
        while self.consume(TokenKind::LBracket) {
            let index = self.parse_expr()?;
            self.expect(TokenKind::RBracket)?;
            expr = EvalExpr::ArrayGet {
                array: Box::new(expr),
                index: Box::new(index),
            };
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
            TokenKind::Ident(name) if name == "null" => {
                self.advance();
                Ok(EvalExpr::Const(EvalConst::Null))
            }
            TokenKind::Ident(name) if name == "true" => {
                self.advance();
                Ok(EvalExpr::Const(EvalConst::Bool(true)))
            }
            TokenKind::Ident(name) if name == "false" => {
                self.advance();
                Ok(EvalExpr::Const(EvalConst::Bool(false)))
            }
            TokenKind::Ident(name) if name == "print" => {
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
        self.expect(TokenKind::LParen)?;
        let mut args = Vec::new();
        if self.consume(TokenKind::RParen) {
            return Ok(EvalExpr::Call { name, args });
        }
        loop {
            args.push(self.parse_expr()?);
            if !self.consume(TokenKind::Comma) {
                break;
            }
            if self.consume(TokenKind::RParen) {
                return Ok(EvalExpr::Call { name, args });
            }
        }
        self.expect(TokenKind::RParen)?;
        Ok(EvalExpr::Call { name, args })
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
