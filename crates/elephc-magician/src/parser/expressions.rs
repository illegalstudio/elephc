//! Purpose:
//! Parses PHP eval expressions using PHP-compatible precedence and postfix syntax.
//!
//! Called from:
//! - `crate::parser::statements` for expression-bearing statements.
//!
//! Key details:
//! - Logical keyword precedence, ternary associativity, coalesce, and exponentiation follow PHP grammar.
//! - Name resolution uses parser namespace/import state while building EvalIR call and constant nodes.

use super::cursor::*;
use super::state::*;
use crate::errors::EvalParseError;
use crate::eval_ir::{
    EvalArrayElement, EvalBinOp, EvalCallArg, EvalCastType, EvalConst, EvalExpr,
    EvalInstanceOfTarget, EvalMagicConst, EvalMatchArm, EvalUnaryOp,
};
use crate::lexer::TokenKind;

impl Parser {
    /// Parses an expression using PHP-like logical, comparison, concatenation, and arithmetic precedence.
    pub(super) fn parse_expr(&mut self) -> Result<EvalExpr, EvalParseError> {
        self.parse_keyword_or()
    }

    /// Parses PHP keyword `or`, whose precedence is lower than `xor`, `and`, and ternary.
    pub(super) fn parse_keyword_or(&mut self) -> Result<EvalExpr, EvalParseError> {
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
    pub(super) fn parse_keyword_xor(&mut self) -> Result<EvalExpr, EvalParseError> {
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
    pub(super) fn parse_keyword_and(&mut self) -> Result<EvalExpr, EvalParseError> {
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
    pub(super) fn parse_ternary(&mut self) -> Result<EvalExpr, EvalParseError> {
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
    pub(super) fn parse_null_coalesce(&mut self) -> Result<EvalExpr, EvalParseError> {
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
    pub(super) fn parse_logical_or(&mut self) -> Result<EvalExpr, EvalParseError> {
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
    pub(super) fn parse_logical_and(&mut self) -> Result<EvalExpr, EvalParseError> {
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
    pub(super) fn parse_bit_or(&mut self) -> Result<EvalExpr, EvalParseError> {
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
    pub(super) fn parse_bit_xor(&mut self) -> Result<EvalExpr, EvalParseError> {
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
    pub(super) fn parse_bit_and(&mut self) -> Result<EvalExpr, EvalParseError> {
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
    pub(super) fn parse_equality(&mut self) -> Result<EvalExpr, EvalParseError> {
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
    pub(super) fn parse_ordering(&mut self) -> Result<EvalExpr, EvalParseError> {
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
    pub(super) fn parse_shift(&mut self) -> Result<EvalExpr, EvalParseError> {
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
    pub(super) fn parse_concat(&mut self) -> Result<EvalExpr, EvalParseError> {
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
    pub(super) fn parse_add(&mut self) -> Result<EvalExpr, EvalParseError> {
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
    pub(super) fn parse_mul(&mut self) -> Result<EvalExpr, EvalParseError> {
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
    pub(super) fn parse_unary(&mut self) -> Result<EvalExpr, EvalParseError> {
        if let Some(target) = self.peek_scalar_cast_type() {
            self.advance();
            self.advance();
            self.advance();
            let expr = self.parse_concat()?;
            return Ok(EvalExpr::Cast {
                target,
                expr: Box::new(expr),
            });
        }
        if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "clone")) {
            self.advance();
            let expr = self.parse_unary()?;
            return Ok(EvalExpr::Clone(Box::new(expr)));
        }
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
        self.parse_instanceof()
    }

    /// Returns the scalar cast target represented by the current `(type)` token window.
    fn peek_scalar_cast_type(&self) -> Option<EvalCastType> {
        if !matches!(self.current(), TokenKind::LParen) {
            return None;
        }
        let Some(TokenKind::Ident(name)) = self.tokens.get(self.pos + 1) else {
            return None;
        };
        if !matches!(self.tokens.get(self.pos + 2), Some(TokenKind::RParen)) {
            return None;
        }
        if ident_eq(name, "int") || ident_eq(name, "integer") {
            Some(EvalCastType::Int)
        } else if ident_eq(name, "float") || ident_eq(name, "double") || ident_eq(name, "real") {
            Some(EvalCastType::Float)
        } else if ident_eq(name, "string") {
            Some(EvalCastType::String)
        } else if ident_eq(name, "bool") || ident_eq(name, "boolean") {
            Some(EvalCastType::Bool)
        } else {
            None
        }
    }

    /// Parses left-associative `instanceof` with PHP's high operator precedence.
    pub(super) fn parse_instanceof(&mut self) -> Result<EvalExpr, EvalParseError> {
        let mut expr = self.parse_power()?;
        while matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "instanceof")) {
            self.advance();
            let target = self.parse_instanceof_target()?;
            expr = EvalExpr::InstanceOf {
                value: Box::new(expr),
                target,
            };
        }
        Ok(expr)
    }

    /// Parses a static or dynamic target after PHP's `instanceof` operator.
    pub(super) fn parse_instanceof_target(
        &mut self,
    ) -> Result<EvalInstanceOfTarget, EvalParseError> {
        if self.consume(TokenKind::LParen) {
            let expr = self.parse_expr()?;
            self.expect(TokenKind::RParen)?;
            return Ok(EvalInstanceOfTarget::Expr(Box::new(expr)));
        }
        if matches!(self.current(), TokenKind::DollarIdent(_)) {
            let target = self.parse_instanceof_variable_target()?;
            return Ok(EvalInstanceOfTarget::Expr(Box::new(target)));
        }
        let name = self.parse_class_reference_name(true)?;
        let class_name = self.resolve_static_class_name(name);
        if self.consume(TokenKind::DoubleColon) {
            let TokenKind::DollarIdent(property) = self.current() else {
                return Err(EvalParseError::UnexpectedToken);
            };
            let property = property.clone();
            self.advance();
            return Ok(EvalInstanceOfTarget::Expr(Box::new(
                EvalExpr::StaticPropertyGet {
                    class_name,
                    property,
                },
            )));
        }
        Ok(EvalInstanceOfTarget::ClassName(class_name))
    }

    /// Parses PHP's unparenthesized dynamic `instanceof` variable/property/array target.
    pub(super) fn parse_instanceof_variable_target(&mut self) -> Result<EvalExpr, EvalParseError> {
        let TokenKind::DollarIdent(name) = self.current() else {
            return Err(EvalParseError::UnexpectedToken);
        };
        let mut expr = EvalExpr::LoadVar(name.clone());
        self.advance();
        loop {
            if matches!(self.current(), TokenKind::LBracket)
                && matches!(self.tokens.get(self.pos + 1), Some(TokenKind::RBracket))
            {
                break;
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
                    return Err(EvalParseError::UnexpectedToken);
                }
                expr = EvalExpr::PropertyGet {
                    object: Box::new(expr),
                    property: member,
                };
                continue;
            }
            break;
        }
        Ok(expr)
    }

    /// Parses right-associative exponentiation with higher precedence than unary prefix operators.
    pub(super) fn parse_power(&mut self) -> Result<EvalExpr, EvalParseError> {
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
    pub(super) fn parse_postfix(&mut self) -> Result<EvalExpr, EvalParseError> {
        let mut expr = self.parse_primary()?;
        loop {
            if matches!(self.current(), TokenKind::LParen) {
                if self.consume_first_class_callable_marker() {
                    continue;
                }
                let args = self.parse_call_args()?;
                expr = EvalExpr::DynamicCall {
                    callee: Box::new(expr),
                    args,
                };
                continue;
            }
            if matches!(self.current(), TokenKind::LBracket)
                && matches!(self.tokens.get(self.pos + 1), Some(TokenKind::RBracket))
            {
                break;
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
            if self.consume(TokenKind::DoubleColon) {
                expr = self.parse_dynamic_static_member_expr(expr)?;
                continue;
            }
            let nullsafe = if self.consume(TokenKind::Arrow) {
                false
            } else if self.consume(TokenKind::QuestionArrow) {
                true
            } else {
                break;
            };
            expr = self.parse_object_member_postfix(expr, nullsafe)?;
            continue;
        }
        Ok(expr)
    }

    /// Parses the member name after `->` or `?->` and builds the corresponding postfix expression.
    fn parse_object_member_postfix(
        &mut self,
        object: EvalExpr,
        nullsafe: bool,
    ) -> Result<EvalExpr, EvalParseError> {
        match self.current() {
            TokenKind::Ident(member) => {
                let member = member.clone();
                self.advance();
                self.parse_named_object_member_postfix(object, member, nullsafe)
            }
            TokenKind::DollarIdent(name) => {
                let member = EvalExpr::LoadVar(name.clone());
                self.advance();
                self.parse_dynamic_object_member_postfix(object, member, nullsafe)
            }
            TokenKind::LBrace => {
                self.advance();
                let member = self.parse_expr()?;
                self.expect(TokenKind::RBrace)?;
                self.parse_dynamic_object_member_postfix(object, member, nullsafe)
            }
            _ => Err(EvalParseError::UnexpectedToken),
        }
    }

    /// Builds a static-name property read or method call after parsing the member name.
    fn parse_named_object_member_postfix(
        &mut self,
        object: EvalExpr,
        member: String,
        nullsafe: bool,
    ) -> Result<EvalExpr, EvalParseError> {
        if matches!(self.current(), TokenKind::LParen) {
            if self.consume_first_class_callable_marker() {
                if nullsafe {
                    return Err(EvalParseError::UnsupportedConstruct);
                }
                return Ok(Self::callable_array_expr(
                    object,
                    EvalExpr::Const(EvalConst::String(member)),
                ));
            }
            let args = self.parse_call_args()?;
            return Ok(if nullsafe {
                EvalExpr::NullsafeMethodCall {
                    object: Box::new(object),
                    method: member,
                    args,
                }
            } else {
                EvalExpr::MethodCall {
                    object: Box::new(object),
                    method: member,
                    args,
                }
            });
        }
        Ok(if nullsafe {
            EvalExpr::NullsafePropertyGet {
                object: Box::new(object),
                property: member,
            }
        } else {
            EvalExpr::PropertyGet {
                object: Box::new(object),
                property: member,
            }
        })
    }

    /// Builds a runtime-name property read or method call after parsing the member expression.
    fn parse_dynamic_object_member_postfix(
        &mut self,
        object: EvalExpr,
        member: EvalExpr,
        nullsafe: bool,
    ) -> Result<EvalExpr, EvalParseError> {
        if matches!(self.current(), TokenKind::LParen) {
            if self.consume_first_class_callable_marker() {
                if nullsafe {
                    return Err(EvalParseError::UnsupportedConstruct);
                }
                return Ok(Self::callable_array_expr(object, member));
            }
            let args = self.parse_call_args()?;
            return Ok(if nullsafe {
                EvalExpr::NullsafeDynamicMethodCall {
                    object: Box::new(object),
                    method: Box::new(member),
                    args,
                }
            } else {
                EvalExpr::DynamicMethodCall {
                    object: Box::new(object),
                    method: Box::new(member),
                    args,
                }
            });
        }
        Ok(if nullsafe {
            EvalExpr::NullsafeDynamicPropertyGet {
                object: Box::new(object),
                property: Box::new(member),
            }
        } else {
            EvalExpr::DynamicPropertyGet {
                object: Box::new(object),
                property: Box::new(member),
            }
        })
    }

    /// Parses primary expressions supported by the initial eval subset.
    pub(super) fn parse_primary(&mut self) -> Result<EvalExpr, EvalParseError> {
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
    pub(super) fn parse_include_expr(&mut self) -> Result<EvalExpr, EvalParseError> {
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
    pub(super) fn parse_match_expr(&mut self) -> Result<EvalExpr, EvalParseError> {
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
    pub(super) fn parse_match_arm(&mut self) -> Result<EvalMatchArm, EvalParseError> {
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
    pub(super) fn parse_call_expr(&mut self, name: String) -> Result<EvalExpr, EvalParseError> {
        self.advance();
        if self.consume_first_class_callable_marker() {
            return Ok(self.function_callable_expr(name));
        }
        let args = self.parse_call_args()?;
        Ok(self.call_expr(name, args))
    }

    /// Parses an explicitly qualified call or constant-fetch expression.
    pub(super) fn parse_qualified_name_expr(&mut self) -> Result<EvalExpr, EvalParseError> {
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

    /// Parses `Class::$property` and `Class::method(...)` expressions.
    pub(super) fn parse_static_member_expr(
        &mut self,
        class_name: String,
    ) -> Result<EvalExpr, EvalParseError> {
        match self.current() {
            TokenKind::DollarIdent(property) => {
                let property = property.clone();
                self.advance();
                if matches!(self.current(), TokenKind::LParen) {
                    if self.consume_first_class_callable_marker() {
                        return Ok(EvalExpr::StaticMethodCallable {
                            class_name,
                            method: Box::new(EvalExpr::LoadVar(property)),
                        });
                    }
                    let args = self.parse_call_args()?;
                    return Ok(EvalExpr::DynamicStaticMethodCall {
                        class_name: Box::new(EvalExpr::ClassNameFetch { class_name }),
                        method: Box::new(EvalExpr::LoadVar(property)),
                        args,
                    });
                }
                Ok(EvalExpr::StaticPropertyGet {
                    class_name,
                    property,
                })
            }
            TokenKind::DollarLBrace => {
                self.advance();
                let property = self.parse_expr()?;
                self.expect(TokenKind::RBrace)?;
                Ok(EvalExpr::DynamicStaticPropertyNameGet {
                    class_name: Box::new(EvalExpr::ClassNameFetch { class_name }),
                    property: Box::new(property),
                })
            }
            TokenKind::Ident(name)
                if ident_eq(name, "class") && !matches!(self.peek(), TokenKind::LParen) =>
            {
                self.advance();
                Ok(EvalExpr::ClassNameFetch { class_name })
            }
            TokenKind::Ident(method) if matches!(self.peek(), TokenKind::LParen) => {
                let method = method.clone();
                self.advance();
                if self.consume_first_class_callable_marker() {
                    return Ok(EvalExpr::StaticMethodCallable {
                        class_name,
                        method: Box::new(EvalExpr::Const(EvalConst::String(method))),
                    });
                }
                let args = self.parse_call_args()?;
                Ok(EvalExpr::StaticMethodCall {
                    class_name,
                    method,
                    args,
                })
            }
            TokenKind::LBrace => {
                self.advance();
                let member = self.parse_expr()?;
                self.expect(TokenKind::RBrace)?;
                if matches!(self.current(), TokenKind::LParen) {
                    if self.consume_first_class_callable_marker() {
                        return Ok(EvalExpr::StaticMethodCallable {
                            class_name,
                            method: Box::new(member),
                        });
                    }
                    let args = self.parse_call_args()?;
                    return Ok(EvalExpr::DynamicStaticMethodCall {
                        class_name: Box::new(EvalExpr::ClassNameFetch { class_name }),
                        method: Box::new(member),
                        args,
                    });
                }
                Ok(EvalExpr::DynamicClassConstantNameFetch {
                    class_name: Box::new(EvalExpr::ClassNameFetch { class_name }),
                    constant: Box::new(member),
                })
            }
            TokenKind::Ident(constant) => {
                let constant = constant.clone();
                self.advance();
                Ok(EvalExpr::ClassConstantFetch {
                    class_name,
                    constant,
                })
            }
            _ => Err(EvalParseError::UnsupportedConstruct),
        }
    }

    /// Parses `$class::member` expressions whose static receiver is runtime-valued.
    fn parse_dynamic_static_member_expr(
        &mut self,
        class_name: EvalExpr,
    ) -> Result<EvalExpr, EvalParseError> {
        match self.current() {
            TokenKind::DollarIdent(member) => {
                let member = member.clone();
                self.advance();
                if matches!(self.current(), TokenKind::LParen) {
                    if self.consume_first_class_callable_marker() {
                        return Ok(Self::callable_array_expr(
                            class_name,
                            EvalExpr::LoadVar(member),
                        ));
                    }
                    let args = self.parse_call_args()?;
                    return Ok(EvalExpr::DynamicStaticMethodCall {
                        class_name: Box::new(class_name),
                        method: Box::new(EvalExpr::LoadVar(member)),
                        args,
                    });
                }
                Ok(EvalExpr::DynamicStaticPropertyGet {
                    class_name: Box::new(class_name),
                    property: member,
                })
            }
            TokenKind::DollarLBrace => {
                self.advance();
                let property = self.parse_expr()?;
                self.expect(TokenKind::RBrace)?;
                Ok(EvalExpr::DynamicStaticPropertyNameGet {
                    class_name: Box::new(class_name),
                    property: Box::new(property),
                })
            }
            TokenKind::Ident(name)
                if ident_eq(name, "class") && !matches!(self.peek(), TokenKind::LParen) =>
            {
                self.advance();
                Ok(EvalExpr::DynamicClassNameFetch {
                    class_name: Box::new(class_name),
                })
            }
            TokenKind::Ident(method) if matches!(self.peek(), TokenKind::LParen) => {
                let method = method.clone();
                self.advance();
                if self.consume_first_class_callable_marker() {
                    return Ok(Self::callable_array_expr(
                        class_name,
                        EvalExpr::Const(EvalConst::String(method)),
                    ));
                }
                let args = self.parse_call_args()?;
                Ok(EvalExpr::DynamicStaticMethodCall {
                    class_name: Box::new(class_name),
                    method: Box::new(EvalExpr::Const(EvalConst::String(method))),
                    args,
                })
            }
            TokenKind::LBrace => {
                self.advance();
                let member = self.parse_expr()?;
                self.expect(TokenKind::RBrace)?;
                if matches!(self.current(), TokenKind::LParen) {
                    if self.consume_first_class_callable_marker() {
                        return Ok(Self::callable_array_expr(class_name, member));
                    }
                    let args = self.parse_call_args()?;
                    return Ok(EvalExpr::DynamicStaticMethodCall {
                        class_name: Box::new(class_name),
                        method: Box::new(member),
                        args,
                    });
                }
                Ok(EvalExpr::DynamicClassConstantNameFetch {
                    class_name: Box::new(class_name),
                    constant: Box::new(member),
                })
            }
            TokenKind::Ident(constant) => {
                let constant = constant.clone();
                self.advance();
                Ok(EvalExpr::DynamicClassConstantFetch {
                    class_name: Box::new(class_name),
                    constant,
                })
            }
            _ => Err(EvalParseError::UnsupportedConstruct),
        }
    }

    /// Parses `new ClassName(...)` and anonymous `new class {}` expressions in eval fragments.
    pub(super) fn parse_new_object_expr(&mut self) -> Result<EvalExpr, EvalParseError> {
        self.advance();
        let is_readonly_anonymous = matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "readonly"))
            && matches!(self.peek(), TokenKind::Ident(name) if ident_eq(name, "class"));
        if is_readonly_anonymous {
            self.advance();
        }
        if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "class")) {
            return self.parse_anonymous_class_expr(is_readonly_anonymous);
        }
        if let TokenKind::DollarIdent(name) = self.current() {
            let class_name = EvalExpr::LoadVar(name.clone());
            self.advance();
            let args = self.parse_optional_constructor_args()?;
            return Ok(EvalExpr::DynamicNewObject {
                class_name: Box::new(class_name),
                args,
            });
        }
        if self.consume(TokenKind::LParen) {
            let class_name = self.parse_expr()?;
            self.expect(TokenKind::RParen)?;
            let args = self.parse_optional_constructor_args()?;
            return Ok(EvalExpr::DynamicNewObject {
                class_name: Box::new(class_name),
                args,
            });
        }
        let class_name = self.parse_class_reference_name(true)?;
        let class_name = self.resolve_static_class_name(class_name);
        let args = self.parse_optional_constructor_args()?;
        Ok(EvalExpr::NewObject { class_name, args })
    }

    /// Parses an optional constructor argument list after `new` class targets.
    fn parse_optional_constructor_args(&mut self) -> Result<Vec<EvalCallArg>, EvalParseError> {
        if matches!(self.current(), TokenKind::LParen) {
            self.parse_call_args()
        } else {
            Ok(Vec::new())
        }
    }

    /// Parses a simple or explicitly qualified PHP name.
    pub(super) fn parse_qualified_name(&mut self) -> Result<ParsedQualifiedName, EvalParseError> {
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

    /// Parses a class-like reference name while rejecting PHP-reserved unqualified names.
    pub(super) fn parse_class_reference_name(
        &mut self,
        allow_relative_keywords: bool,
    ) -> Result<ParsedQualifiedName, EvalParseError> {
        let name = self.parse_qualified_name()?;
        if self.class_reference_name_is_reserved(&name, allow_relative_keywords) {
            return Err(EvalParseError::UnsupportedConstruct);
        }
        Ok(name)
    }

    /// Returns whether a parsed class-like reference uses a PHP-reserved bare name.
    pub(super) fn class_reference_name_is_reserved(
        &self,
        name: &ParsedQualifiedName,
        allow_relative_keywords: bool,
    ) -> bool {
        if name.absolute || name.name.contains('\\') {
            return false;
        }
        if allow_relative_keywords
            && ["self", "parent", "static"]
                .iter()
                .any(|keyword| ident_eq(&name.name, keyword))
        {
            return false;
        }
        is_reserved_class_like_name(&name.name)
    }

    /// Resolves a class name used before `::`, preserving PHP relative class keywords.
    pub(super) fn resolve_static_class_name(&self, name: ParsedQualifiedName) -> String {
        if !name.absolute
            && ["self", "parent", "static"]
                .iter()
                .any(|keyword| ident_eq(&name.name, keyword))
        {
            return name.name.to_ascii_lowercase();
        }
        self.resolve_class_name(name)
    }

    /// Builds a call expression, adding namespace fallback for unqualified names.
    pub(super) fn call_expr(&self, name: String, args: Vec<EvalCallArg>) -> EvalExpr {
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
    pub(super) fn const_fetch_expr(&self, name: String) -> EvalExpr {
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
    pub(super) fn qualify_name_in_current_namespace(&self, name: &str) -> String {
        if self.namespace.is_empty() {
            name.to_string()
        } else {
            format!("{}\\{}", self.namespace, name)
        }
    }

    /// Resolves a class name through active imports before namespace qualification.
    pub(super) fn resolve_class_name(&self, name: ParsedQualifiedName) -> String {
        if name.absolute {
            return name.name;
        }
        if let Some(imported) = self.imports.resolve_class(&name.name) {
            return imported;
        }
        self.resolve_qualified_name(name)
    }

    /// Resolves a parsed PHP name according to the current namespace.
    pub(super) fn resolve_qualified_name(&self, name: ParsedQualifiedName) -> String {
        if name.absolute || self.namespace.is_empty() {
            name.name
        } else {
            self.qualify_name_in_current_namespace(&name.name)
        }
    }

    /// Parses a parenthesized source-order argument list.
    pub(super) fn parse_call_args(&mut self) -> Result<Vec<EvalCallArg>, EvalParseError> {
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
    pub(super) fn parse_call_arg(&mut self) -> Result<EvalCallArg, EvalParseError> {
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

    /// Consumes PHP's `(...)` first-class callable marker when it is the whole argument list.
    fn consume_first_class_callable_marker(&mut self) -> bool {
        if matches!(self.current(), TokenKind::LParen)
            && matches!(self.tokens.get(self.pos + 1), Some(TokenKind::Ellipsis))
            && matches!(self.tokens.get(self.pos + 2), Some(TokenKind::RParen))
        {
            self.advance();
            self.advance();
            self.advance();
            true
        } else {
            false
        }
    }

    /// Builds an eval function-callable expression with namespace fallback metadata.
    fn function_callable_expr(&self, name: String) -> EvalExpr {
        if let Some(imported) = self.imports.resolve_function(&name) {
            return Self::function_callable_value(imported.to_ascii_lowercase(), None);
        }
        let fallback_name = name.to_ascii_lowercase();
        if self.namespace.is_empty() {
            Self::function_callable_value(fallback_name, None)
        } else {
            Self::function_callable_value(
                self.qualify_name_in_current_namespace(&name)
                    .to_ascii_lowercase(),
                Some(fallback_name),
            )
        }
    }

    /// Builds the EvalIR node that resolves a first-class function callable at runtime.
    fn function_callable_value(name: String, fallback_name: Option<String>) -> EvalExpr {
        EvalExpr::FunctionCallable {
            name,
            fallback_name,
        }
    }

    /// Builds the PHP callable-array value used for method first-class callables.
    fn callable_array_expr(receiver: EvalExpr, method: EvalExpr) -> EvalExpr {
        EvalExpr::Array(vec![
            EvalArrayElement::Value(receiver),
            EvalArrayElement::Value(method),
        ])
    }

    /// Parses an array literal with source-order optional key/value element expressions.
    pub(super) fn parse_array_literal(&mut self) -> Result<EvalExpr, EvalParseError> {
        self.expect(TokenKind::LBracket)?;
        self.parse_array_elements_until(TokenKind::RBracket)
    }

    /// Parses PHP's legacy `array(...)` literal into the same EvalIR node as `[...]`.
    pub(super) fn parse_legacy_array_literal(&mut self) -> Result<EvalExpr, EvalParseError> {
        self.advance();
        self.expect(TokenKind::LParen)?;
        self.parse_array_elements_until(TokenKind::RParen)
    }

    /// Returns whether the current token starts PHP's legacy `array(...)` literal syntax.
    pub(super) fn current_starts_legacy_array_literal(&self) -> bool {
        matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "array"))
            && matches!(self.peek(), TokenKind::LParen)
    }

    /// Parses comma-separated array elements until the supplied closing delimiter.
    pub(super) fn parse_array_elements_until(
        &mut self,
        close: TokenKind,
    ) -> Result<EvalExpr, EvalParseError> {
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
}
