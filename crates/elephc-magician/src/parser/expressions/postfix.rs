//! Purpose:
//! Parses `instanceof`, exponentiation, and postfix property, method, index, and
//! dynamic-member expression forms.
//!
//! Called from:
//! - The unary/precedence parser and primary-expression parser.
//!
//! Key details:
//! - Postfix operations preserve PHP chaining and dynamic member evaluation order.

use super::*;
use super::precedence::is_assignment_target;

impl Parser {

    /// Parses left-associative `instanceof` with PHP's high operator precedence.
    pub(in crate::parser) fn parse_instanceof(&mut self) -> Result<EvalExpr, EvalParseError> {
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
    pub(in crate::parser) fn parse_instanceof_target(
        &mut self,
    ) -> Result<EvalInstanceOfTarget, EvalParseError> {
        if self.consume(TokenKind::LParen) {
            let expr = self.parse_expr()?;
            self.expect(TokenKind::RParen)?;
            return Ok(EvalInstanceOfTarget::Expr(Box::new(expr)));
        }
        if matches!(self.current(), TokenKind::DollarIdent(_)) {
            let target = self.parse_variable_class_name_target(false)?;
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

    /// Parses an unparenthesized dynamic class-name variable/property/array target.
    ///
    /// `ctor_position` says a `(` after the target opens a CONSTRUCTOR argument list rather than
    /// a method call. That difference is the whole reason `new $obj->prop()` was refused: the
    /// `(` guard below exists so `instanceof $obj->method()` -- which PHP's grammar does not
    /// allow -- cannot be read as a class name, and it was rejecting the constructor's own
    /// parentheses along with it.
    pub(in crate::parser) fn parse_variable_class_name_target(
        &mut self,
        ctor_position: bool,
    ) -> Result<EvalExpr, EvalParseError> {
        let TokenKind::DollarIdent(name) = self.current() else {
            return Err(EvalParseError::UnexpectedToken);
        };
        let expr = EvalExpr::LoadVar(name.clone());
        self.advance();
        self.parse_class_name_target_chain(expr, ctor_position)
    }

    /// Continues a dynamic class-name target with its `[index]`, `::$static` and `->member` chain.
    ///
    /// Shared by `new $var...` and `new Name::$static...`: the latter is how Laravel writes
    /// `new static::$resolvedCollectionClasses[static::class]($models)`.
    pub(in crate::parser) fn parse_class_name_target_chain(
        &mut self,
        mut expr: EvalExpr,
        ctor_position: bool,
    ) -> Result<EvalExpr, EvalParseError> {
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
                if ctor_position && matches!(self.current(), TokenKind::LParen) {
                    break;
                }
                continue;
            }
            if self.consume(TokenKind::DoubleColon) {
                // `new $obj::$name()` reads a STATIC property of the object's class and uses what
                // it holds as the class name.
                let TokenKind::DollarIdent(property) = self.current() else {
                    return Err(EvalParseError::UnexpectedToken);
                };
                let property = property.clone();
                self.advance();
                expr = EvalExpr::DynamicStaticPropertyGet {
                    class_name: Box::new(expr),
                    property,
                };
                if ctor_position && matches!(self.current(), TokenKind::LParen) {
                    break;
                }
                continue;
            }
            if self.consume(TokenKind::Arrow) {
                let TokenKind::Ident(member) = self.current() else {
                    return Err(EvalParseError::UnexpectedToken);
                };
                let member = member.clone();
                self.advance();
                let opens_call = matches!(self.current(), TokenKind::LParen);
                if opens_call && !ctor_position {
                    return Err(EvalParseError::UnexpectedToken);
                }
                expr = EvalExpr::PropertyGet {
                    object: Box::new(expr),
                    property: member,
                };
                if opens_call {
                    break;
                }
                continue;
            }
            break;
        }
        Ok(expr)
    }

    /// Parses right-associative exponentiation with higher precedence than unary prefix operators.
    pub(in crate::parser) fn parse_power(&mut self) -> Result<EvalExpr, EvalParseError> {
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
    pub(in crate::parser) fn parse_postfix(&mut self) -> Result<EvalExpr, EvalParseError> {
        let mut expr = self.parse_primary()?;
        loop {
            if matches!(self.current(), TokenKind::LParen) {
                if self.consume_first_class_callable_marker() {
                    expr = Self::invokable_callable_expr(expr);
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
            if self.consume(TokenKind::PlusPlus) {
                if !is_assignment_target(&expr) {
                    return Err(EvalParseError::UnexpectedToken);
                }
                return Ok(EvalExpr::PostfixIncDec {
                    target: Box::new(expr),
                    increment: true,
                });
            }
            if self.consume(TokenKind::MinusMinus) {
                if !is_assignment_target(&expr) {
                    return Err(EvalParseError::UnexpectedToken);
                }
                return Ok(EvalExpr::PostfixIncDec {
                    target: Box::new(expr),
                    increment: false,
                });
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
    pub(super) fn parse_object_member_postfix(
        &mut self,
        object: EvalExpr,
        nullsafe: bool,
    ) -> Result<EvalExpr, EvalParseError> {
        match self.current() {
            TokenKind::Ident(member) => {
                let member = member.clone();
                self.advance();
                let member = self.parse_written_name(member, false)?;
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
    pub(super) fn parse_named_object_member_postfix(
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
                return Ok(Self::method_callable_expr(
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
    pub(super) fn parse_dynamic_object_member_postfix(
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
                return Ok(Self::method_callable_expr(object, member));
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

}
