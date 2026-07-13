//! Purpose:
//! Parses do-while, for, foreach, and array-set loop-adjacent statements.
//!
//! Called from:
//! - `Parser::parse_stmt()` and for-clause parsing.
//!
//! Key details:
//! - Foreach key/value targets and statement bodies retain EvalIR source order.

use super::*;

impl Parser {
    /// Parses `do { ... } while (expr);`.
    pub(in crate::parser) fn parse_do_while_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
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
    pub(in crate::parser) fn parse_array_set_stmt(
        &mut self,
        name: String,
    ) -> Result<Vec<EvalStmt>, EvalParseError> {
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
    pub(in crate::parser) fn parse_for_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
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
    pub(in crate::parser) fn parse_foreach_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
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
}
