//! Purpose:
//! Parses primary literals, variables, closures, includes, match expressions,
//! calls, and qualified-name constants.
//!
//! Called from:
//! - `Parser::parse_postfix()`.
//!
//! Key details:
//! - Token dispatch remains centralized for exhaustive primary syntax handling.

use super::*;

impl Parser {

    /// Parses primary expressions supported by the initial eval subset.
    pub(in crate::parser) fn parse_primary(&mut self) -> Result<EvalExpr, EvalParseError> {
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
                let expr = EvalExpr::LoadVar(name);
                if self.consume(TokenKind::DoubleColon) {
                    self.parse_dynamic_static_member_expr(expr)
                } else {
                    Ok(expr)
                }
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
            TokenKind::Ident(name) if ident_eq(name, "function") => self.parse_closure_expr(false),
            TokenKind::Ident(name)
                if ident_eq(name, "static")
                    && matches!(self.peek(), TokenKind::Ident(next) if ident_eq(next, "function")) =>
            {
                self.parse_closure_expr(true)
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
            TokenKind::Ident(_)
                if matches!(self.peek(), TokenKind::Backslash | TokenKind::DoubleColon) =>
            {
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
    pub(in crate::parser) fn parse_include_expr(&mut self) -> Result<EvalExpr, EvalParseError> {
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
    pub(in crate::parser) fn parse_match_expr(&mut self) -> Result<EvalExpr, EvalParseError> {
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
    pub(in crate::parser) fn parse_match_arm(&mut self) -> Result<EvalMatchArm, EvalParseError> {
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
    pub(in crate::parser) fn parse_call_expr(&mut self, name: String) -> Result<EvalExpr, EvalParseError> {
        self.advance();
        if self.consume_first_class_callable_marker() {
            return Ok(self.function_callable_expr(name));
        }
        let args = self.parse_call_args()?;
        Ok(self.call_expr(name, args))
    }

    /// Parses an explicitly qualified call or constant-fetch expression.
    pub(in crate::parser) fn parse_qualified_name_expr(&mut self) -> Result<EvalExpr, EvalParseError> {
        let name = self.parse_qualified_name()?;
        if self.consume(TokenKind::DoubleColon) {
            let class_name = self.resolve_static_class_name(name);
            return self.parse_static_member_expr(class_name);
        }
        let name = self.resolve_qualified_name(name);
        if matches!(self.current(), TokenKind::LParen) {
            if self.consume_first_class_callable_marker() {
                return Ok(Self::function_callable_value(name.to_ascii_lowercase(), None));
            }
            let args = self.parse_call_args()?;
            return Ok(EvalExpr::Call {
                name: name.to_ascii_lowercase(),
                args,
            });
        }
        Ok(EvalExpr::ConstFetch(name))
    }

}
