//! Purpose:
//! Parses if, switch, unset, while, and nested statement blocks.
//!
//! Called from:
//! - Statement dispatch and every construct that owns a nested body.
//!
//! Key details:
//! - Alternative PHP syntax and source-end lines are preserved in block parsing.

use super::*;

impl Parser {
    /// Parses a complete `if` statement after consuming the `if` keyword.
    pub(in crate::parser) fn parse_if_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        Ok(vec![self.parse_if_after_keyword()?])
    }

    /// Parses the condition, then block, and optional else branch for an `if` chain.
    ///
    /// Delegates to the alternative-syntax path when the body opens with `:`, which consumes the
    /// whole `elseif:`/`else:` chain up to and including `endif;`.
    pub(in crate::parser) fn parse_if_after_keyword(&mut self) -> Result<EvalStmt, EvalParseError> {
        self.expect(TokenKind::LParen)?;
        let condition = self.parse_expr()?;
        self.expect(TokenKind::RParen)?;
        if matches!(self.current(), TokenKind::Colon) {
            return self.parse_alternative_if_chain(condition);
        }
        let then_branch = self.parse_statement_body()?;
        let else_branch = self.parse_optional_else_branch()?;
        Ok(EvalStmt::If {
            condition,
            then_branch,
            else_branch,
        })
    }

    /// Parses a complete alternative-syntax `if` chain and consumes its closing `endif;`.
    ///
    /// `condition` is the already-parsed `if` condition and the cursor sits on the `:` that opens
    /// the `then` segment.
    fn parse_alternative_if_chain(
        &mut self,
        condition: EvalExpr,
    ) -> Result<EvalStmt, EvalParseError> {
        let statement = self.parse_alternative_if_segment(condition)?;
        self.expect_keyword("endif")?;
        self.expect_semicolon()?;
        Ok(statement)
    }

    /// Parses one `: body` segment plus any `elseif:`/`else:` continuation, without consuming
    /// the shared `endif;` that closes the whole chain.
    ///
    /// `elseif` recurses so the resulting `EvalStmt::If` nests exactly like the brace form.
    fn parse_alternative_if_segment(
        &mut self,
        condition: EvalExpr,
    ) -> Result<EvalStmt, EvalParseError> {
        self.expect(TokenKind::Colon)?;
        let then_branch = self.parse_alternative_body(&["elseif", "else", "endif"])?;
        let else_branch = if self.at_keyword("elseif") {
            self.advance();
            self.expect(TokenKind::LParen)?;
            let branch_condition = self.parse_expr()?;
            self.expect(TokenKind::RParen)?;
            vec![self.parse_alternative_if_segment(branch_condition)?]
        } else if self.at_keyword("else") {
            self.advance();
            self.expect(TokenKind::Colon)?;
            self.parse_alternative_body(&["endif"])?
        } else {
            Vec::new()
        };
        Ok(EvalStmt::If {
            condition,
            then_branch,
            else_branch,
        })
    }

    /// Parses statements until one of the `stops` keywords, leaving that keyword unconsumed.
    ///
    /// Returns `UnexpectedEof` when the fragment ends before a terminator is found.
    pub(in crate::parser) fn parse_alternative_body(
        &mut self,
        stops: &[&str],
    ) -> Result<Vec<EvalStmt>, EvalParseError> {
        let mut statements = Vec::new();
        loop {
            if matches!(self.current(), TokenKind::Eof) {
                return Err(EvalParseError::UnexpectedEof);
            }
            if self.at_any_keyword(stops) {
                break;
            }
            statements.extend(self.parse_nested_stmt()?);
        }
        Ok(statements)
    }

    /// Parses a loop body in brace/braceless form, or in the alternative `:` … `<terminator>;`
    /// form when the body opens with `:`.
    pub(in crate::parser) fn parse_statement_body_or_alternative(
        &mut self,
        terminator: &str,
    ) -> Result<Vec<EvalStmt>, EvalParseError> {
        if !matches!(self.current(), TokenKind::Colon) {
            return self.parse_statement_body();
        }
        self.advance();
        let body = self.parse_alternative_body(&[terminator])?;
        self.expect_keyword(terminator)?;
        self.expect_semicolon()?;
        Ok(body)
    }

    /// Parses `elseif`, `else if`, or `else` branches after an `if` body.
    pub(in crate::parser) fn parse_optional_else_branch(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
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

    /// Parses `switch (expr) { case expr: ... default: ... }`, or the alternative
    /// `switch (expr): case expr: ... endswitch;` form.
    pub(in crate::parser) fn parse_switch_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        self.expect(TokenKind::LParen)?;
        let expr = self.parse_expr()?;
        self.expect(TokenKind::RParen)?;
        // The case list is terminated by `}` in the brace form and by `endswitch` otherwise.
        let alternative = matches!(self.current(), TokenKind::Colon);
        if alternative {
            self.advance();
        } else {
            self.expect(TokenKind::LBrace)?;
        }
        let mut cases = Vec::new();
        loop {
            if matches!(self.current(), TokenKind::Eof) {
                return Err(EvalParseError::UnexpectedEof);
            }
            let at_end = if alternative {
                self.at_keyword("endswitch")
            } else {
                matches!(self.current(), TokenKind::RBrace)
            };
            if at_end {
                break;
            }
            cases.push(self.parse_switch_case()?);
        }
        if alternative {
            self.expect_keyword("endswitch")?;
            self.expect_semicolon()?;
        } else {
            self.expect(TokenKind::RBrace)?;
        }
        Ok(vec![EvalStmt::Switch { expr, cases }])
    }

    /// Parses one `case` or `default` arm inside a switch body.
    pub(in crate::parser) fn parse_switch_case(&mut self) -> Result<EvalSwitchCase, EvalParseError> {
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
    pub(in crate::parser) fn parse_switch_case_body(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        let mut body = Vec::new();
        while !is_switch_case_boundary(self.current()) {
            if matches!(self.current(), TokenKind::Eof) {
                return Err(EvalParseError::UnexpectedEof);
            }
            body.extend(self.parse_stmt()?);
        }
        Ok(body)
    }

    /// Removes unset operands in source order, followed by one cycle-collection safe point.
    pub(in crate::parser) fn parse_unset_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        self.expect(TokenKind::LParen)?;
        let mut statements = Vec::new();
        loop {
            let target = self.parse_expr()?;
            let stmt = match target {
                EvalExpr::ArrayGet { array, index } => EvalStmt::UnsetArrayElement {
                    array: *array,
                    index: *index,
                },
                EvalExpr::LoadVar(name) => EvalStmt::UnsetVar { name },
                EvalExpr::PropertyGet { object, property } => EvalStmt::UnsetProperty {
                    object: *object,
                    property,
                },
                EvalExpr::DynamicPropertyGet { object, property } => {
                    EvalStmt::UnsetDynamicProperty {
                        object: *object,
                        property: *property,
                    }
                }
                EvalExpr::StaticPropertyGet {
                    class_name,
                    property,
                } => EvalStmt::UnsetStaticProperty {
                    class_name,
                    property,
                },
                EvalExpr::DynamicStaticPropertyGet {
                    class_name,
                    property,
                } => EvalStmt::UnsetDynamicStaticProperty {
                    class_name: *class_name,
                    property,
                },
                EvalExpr::DynamicStaticPropertyNameGet {
                    class_name,
                    property,
                } => EvalStmt::UnsetDynamicStaticPropertyName {
                    class_name: *class_name,
                    property: *property,
                },
                _ => return Err(EvalParseError::ExpectedVariable),
            };
            statements.push(stmt);
            if !self.consume(TokenKind::Comma) {
                break;
            }
        }
        self.expect(TokenKind::RParen)?;
        self.expect_semicolon()?;
        statements.push(EvalStmt::GcCollect);
        Ok(statements)
    }

    /// Parses `while (expr) { ... }`, or the alternative `while (expr): ... endwhile;` form.
    pub(in crate::parser) fn parse_while_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        self.expect(TokenKind::LParen)?;
        let condition = self.parse_expr()?;
        self.expect(TokenKind::RParen)?;
        let body = self.parse_statement_body_or_alternative("endwhile")?;
        Ok(vec![EvalStmt::While { condition, body }])
    }

    /// Parses either a brace-delimited block or one braceless statement body.
    pub(in crate::parser) fn parse_statement_body(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        if matches!(self.current(), TokenKind::LBrace) {
            self.parse_block()
        } else {
            self.parse_nested_stmt()
        }
    }

    /// Parses a brace-delimited statement block.
    pub(in crate::parser) fn parse_block(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.expect(TokenKind::LBrace)?;
        self.parse_nested_block_contents()
    }

    /// Parses a brace-delimited statement block and returns the closing-brace line.
    pub(in crate::parser) fn parse_block_with_end_line(
        &mut self,
    ) -> Result<(Vec<EvalStmt>, i64), EvalParseError> {
        self.expect(TokenKind::LBrace)?;
        self.parse_nested_block_contents_with_end_line()
    }

    /// Parses one nested statement where import declarations are not legal.
    pub(in crate::parser) fn parse_nested_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        let previous = std::mem::replace(&mut self.allow_use_imports, false);
        let result = self.parse_stmt();
        self.allow_use_imports = previous;
        result
    }

    /// Parses a nested block while preserving active imports for name resolution.
    pub(in crate::parser) fn parse_nested_block_contents(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        let previous = std::mem::replace(&mut self.allow_use_imports, false);
        let result = self.parse_block_contents();
        self.allow_use_imports = previous;
        result
    }

    /// Parses a nested block and returns the closing-brace line.
    pub(in crate::parser) fn parse_nested_block_contents_with_end_line(
        &mut self,
    ) -> Result<(Vec<EvalStmt>, i64), EvalParseError> {
        let previous = std::mem::replace(&mut self.allow_use_imports, false);
        let result = self.parse_block_contents_with_end_line();
        self.allow_use_imports = previous;
        result
    }

    /// Parses statements until the closing brace for the current block.
    pub(in crate::parser) fn parse_block_contents(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
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

    /// Parses statements until the closing brace and returns that brace's line.
    pub(in crate::parser) fn parse_block_contents_with_end_line(
        &mut self,
    ) -> Result<(Vec<EvalStmt>, i64), EvalParseError> {
        let mut statements = Vec::new();
        while !matches!(self.current(), TokenKind::RBrace) {
            if matches!(self.current(), TokenKind::Eof) {
                return Err(EvalParseError::UnexpectedEof);
            }
            statements.extend(self.parse_stmt()?);
        }
        let source_end_line = self.current_line();
        self.expect(TokenKind::RBrace)?;
        Ok((statements, source_end_line))
    }
}
