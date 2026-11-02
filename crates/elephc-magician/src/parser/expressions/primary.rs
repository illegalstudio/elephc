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
                Ok(EvalExpr::Const(eval_string_constant(value)))
            }
            // `$$name` / `$${expr}`: the inner variable (itself possibly variable) names this one.
            TokenKind::Dollar => {
                self.advance();
                let inner = match self.current() {
                    TokenKind::DollarIdent(name) => {
                        let name = name.clone();
                        self.advance();
                        EvalExpr::LoadVar(name)
                    }
                    TokenKind::Dollar | TokenKind::DollarLBrace => self.parse_primary()?,
                    _ => return Err(EvalParseError::ExpectedVariable),
                };
                Ok(EvalExpr::DynamicVar(Box::new(inner)))
            }
            // `${expr}` outside a string: the expression names the variable.
            TokenKind::DollarLBrace => {
                self.advance();
                let name = self.parse_expr()?;
                self.expect(TokenKind::RBrace)?;
                Ok(EvalExpr::DynamicVar(Box::new(name)))
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
            TokenKind::Ident(name) if ident_eq(name, "throw") => {
                self.advance();
                let expr = self.parse_expr()?;
                Ok(EvalExpr::Throw(Box::new(expr)))
            }
            TokenKind::AttributeStart => self.parse_attributed_closure_expr(),
            TokenKind::Ident(name) if ident_eq(name, "fn") => {
                self.parse_arrow_closure_expr(false, Vec::new())
            }
            TokenKind::Ident(name) if ident_eq(name, "function") => self.parse_closure_expr(false),
            TokenKind::Ident(name)
                if ident_eq(name, "static")
                    && matches!(self.peek(), TokenKind::Ident(next) if ident_eq(next, "fn")) =>
            {
                self.parse_arrow_closure_expr(true, Vec::new())
            }
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
            TokenKind::Backslash | TokenKind::ReservedNameHead(_) => self.parse_qualified_name_expr(),
            TokenKind::Ident(_)
                if matches!(self.peek(), TokenKind::Backslash | TokenKind::DoubleColon | TokenKind::Less) =>
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
            // PHP's `match_arm_cond_list` admits a trailing comma, so `'a', 'b', => …` is one arm
            // with two conditions, not a missing third one. A comma with nothing after it but the
            // closing brace stays a syntax error, which is the check below.
            if matches!(self.current(), TokenKind::FatArrow) {
                break;
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
        let type_index = self.pos;
        let mut name = self.parse_qualified_name()?;
        name.name = self.parse_written_name(name.name, false)?;
        if self.consume(TokenKind::DoubleColon) {
            if !name.absolute { self.mark_type_parameter_use(type_index, &name.name); }
            let class_name = self.resolve_static_class_name(name);
            return self.parse_static_member_expr(class_name);
        }
        if !name.absolute && !name.name.split('<').next().unwrap_or("").contains('\\') && matches!(self.current(), TokenKind::LParen) {
            if self.consume_first_class_callable_marker() { return Ok(self.function_callable_expr(name.name)); }
            let args = self.parse_call_args()?;
            return Ok(self.call_expr(name.name, args));
        }
        let name = self.resolve_qualified_name(name);
        if matches!(self.current(), TokenKind::LParen) {
            if self.consume_first_class_callable_marker() {
                return Ok(Self::function_callable_value(name, None));
            }
            let args = self.parse_call_args()?;
            return Ok(EvalExpr::Call {
                name,
                args,
            });
        }
        Ok(EvalExpr::ConstFetch(name))
    }

}

/// Restores binary-string marker characters emitted before lexing to their original bytes.
fn eval_string_constant(value: String) -> EvalConst {
    let mut bytes = Vec::with_capacity(value.len());
    let mut has_binary_marker = false;
    for ch in value.chars() {
        if (0xF0000..=0xF00FF).contains(&(ch as u32)) {
            let byte = (ch as u32 - 0xF0000) as u8;
            bytes.push(byte);
            has_binary_marker = true;
        } else if (0xE000..=0xE0FF).contains(&(ch as u32)) {
            // Main's shared literal marker (`elephc_builtin_contract::string_literal`).
            bytes.push((ch as u32 - 0xE000) as u8);
            has_binary_marker = true;
        } else {
            let mut encoded = [0; 4];
            bytes.extend_from_slice(ch.encode_utf8(&mut encoded).as_bytes());
        }
    }
    if has_binary_marker {
        match String::from_utf8(bytes) {
            Ok(text) => EvalConst::String(text),
            Err(error) => EvalConst::Bytes(error.into_bytes()),
        }
    } else {
        EvalConst::String(value)
    }
}
