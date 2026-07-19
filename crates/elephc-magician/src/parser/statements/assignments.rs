//! Purpose:
//! Parses for clauses, variable stores, property/static assignments, and increment/decrement forms.
//!
//! Called from:
//! - Statement dispatch and for-loop clause parsing.
//!
//! Key details:
//! - Compound assignment and property targets lower directly into explicit EvalIR statement variants.

use super::*;

impl Parser {
    /// Parses the optional first clause of a `for` loop.
    pub(in crate::parser) fn parse_for_init_clause(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        if matches!(self.current(), TokenKind::Semicolon) {
            return Ok(Vec::new());
        }
        self.parse_for_clause_stmt()
    }

    /// Parses the optional update clause of a `for` loop.
    pub(in crate::parser) fn parse_for_update_clause(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        if self.consume(TokenKind::RParen) {
            return Ok(Vec::new());
        }
        let statements = self.parse_for_clause_stmt()?;
        self.expect(TokenKind::RParen)?;
        Ok(statements)
    }

    /// Parses one statement-like `for` clause without consuming a delimiter.
    pub(in crate::parser) fn parse_for_clause_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        match self.current() {
            TokenKind::PlusPlus | TokenKind::MinusMinus
                if self.current_starts_prefixed_static_property_inc_dec() =>
            {
                self.parse_static_property_inc_dec_stmt(true, false)
            }
            TokenKind::PlusPlus | TokenKind::MinusMinus
                if self.current_starts_prefixed_dynamic_static_property_inc_dec() =>
            {
                self.parse_dynamic_static_property_inc_dec_stmt(true, false)
            }
            TokenKind::PlusPlus | TokenKind::MinusMinus
                if self.current_starts_prefixed_property_inc_dec() =>
            {
                self.parse_prefixed_property_inc_dec_stmt(false)
            }
            TokenKind::PlusPlus | TokenKind::MinusMinus => {
                self.parse_prefix_inc_dec_stmt(false)
            }
            TokenKind::Ident(_) | TokenKind::Backslash
                if self.current_starts_static_property_postfix_inc_dec() =>
            {
                self.parse_static_property_inc_dec_stmt(false, false)
            }
            TokenKind::DollarIdent(name) if matches!(self.peek(), TokenKind::LBracket) => {
                self.parse_array_set_clause(name.clone())
            }
            TokenKind::DollarIdent(_)
                if self.current_starts_dynamic_static_property_postfix_inc_dec() =>
            {
                self.parse_dynamic_static_property_inc_dec_stmt(false, false)
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
                self.parse_property_like_stmt_tail(expr, false)
            }
        }
    }

    /// Parses `$name[index] = expr` and `$name[] = expr` in a `for` clause.
    pub(in crate::parser) fn parse_array_set_clause(
        &mut self,
        name: String,
    ) -> Result<Vec<EvalStmt>, EvalParseError> {
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
    pub(in crate::parser) fn parse_var_store_stmt(
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

    /// Parses `Class::$property = expr` and simple static-property compound assignments.
    pub(in crate::parser) fn parse_static_property_set_stmt(
        &mut self,
        require_semicolon: bool,
    ) -> Result<Vec<EvalStmt>, EvalParseError> {
        let class_name = self.parse_qualified_name()?;
        let class_name = self.resolve_static_class_name(class_name);
        self.expect(TokenKind::DoubleColon)?;
        let TokenKind::DollarIdent(property) = self.current() else {
            return Err(EvalParseError::ExpectedVariable);
        };
        let property = property.clone();
        self.advance();
        if self.consume(TokenKind::LBracket) {
            if self.consume(TokenKind::RBracket) {
                self.expect(TokenKind::Equal)?;
                let value = self.parse_expr()?;
                if require_semicolon {
                    self.expect_semicolon()?;
                }
                return Ok(vec![EvalStmt::StaticPropertyArrayAppend {
                    class_name,
                    property,
                    value,
                }]);
            }
            let index = self.parse_expr()?;
            self.expect(TokenKind::RBracket)?;
            let Some(op) = assignment_op(self.current()) else {
                return Err(EvalParseError::UnexpectedToken);
            };
            self.advance();
            let value = self.parse_expr()?;
            if require_semicolon {
                self.expect_semicolon()?;
            }
            return Ok(vec![EvalStmt::StaticPropertyArraySet {
                class_name,
                property,
                index,
                op,
                value,
            }]);
        }
        let Some(op) = assignment_op(self.current()) else {
            return Err(EvalParseError::UnexpectedToken);
        };
        self.advance();
        if op.is_none() && self.consume(TokenKind::Ampersand) {
            let TokenKind::DollarIdent(source) = self.current() else {
                return Err(EvalParseError::ExpectedVariable);
            };
            let source = source.clone();
            self.advance();
            if require_semicolon {
                self.expect_semicolon()?;
            }
            return Ok(vec![EvalStmt::StaticPropertyReferenceBind {
                class_name,
                property,
                source,
            }]);
        }
        let value = self.parse_expr()?;
        if require_semicolon {
            self.expect_semicolon()?;
        }
        let value = match op {
            Some(op) => EvalExpr::Binary {
                op,
                left: Box::new(EvalExpr::StaticPropertyGet {
                    class_name: class_name.clone(),
                    property: property.clone(),
                }),
                right: Box::new(value),
            },
            None => value,
        };
        Ok(vec![EvalStmt::StaticPropertySet {
            class_name,
            property,
            value,
        }])
    }

    /// Parses `$class::$property = expr` and compound assignments with a dynamic receiver.
    pub(in crate::parser) fn parse_dynamic_static_property_set_stmt(
        &mut self,
        require_semicolon: bool,
    ) -> Result<Vec<EvalStmt>, EvalParseError> {
        let TokenKind::DollarIdent(class_name) = self.current() else {
            return Err(EvalParseError::ExpectedVariable);
        };
        let class_name = EvalExpr::LoadVar(class_name.clone());
        self.advance();
        self.expect(TokenKind::DoubleColon)?;
        let TokenKind::DollarIdent(property) = self.current() else {
            return Err(EvalParseError::ExpectedVariable);
        };
        let property = property.clone();
        self.advance();
        if self.consume(TokenKind::LBracket) {
            if self.consume(TokenKind::RBracket) {
                self.expect(TokenKind::Equal)?;
                let value = self.parse_expr()?;
                if require_semicolon {
                    self.expect_semicolon()?;
                }
                return Ok(vec![EvalStmt::DynamicStaticPropertyArrayAppend {
                    class_name,
                    property,
                    value,
                }]);
            }
            let index = self.parse_expr()?;
            self.expect(TokenKind::RBracket)?;
            let Some(op) = assignment_op(self.current()) else {
                return Err(EvalParseError::UnexpectedToken);
            };
            self.advance();
            let value = self.parse_expr()?;
            if require_semicolon {
                self.expect_semicolon()?;
            }
            return Ok(vec![EvalStmt::DynamicStaticPropertyArraySet {
                class_name,
                property,
                index,
                op,
                value,
            }]);
        }
        let Some(op) = assignment_op(self.current()) else {
            return Err(EvalParseError::UnexpectedToken);
        };
        self.advance();
        if op.is_none() && self.consume(TokenKind::Ampersand) {
            let TokenKind::DollarIdent(source) = self.current() else {
                return Err(EvalParseError::ExpectedVariable);
            };
            let source = source.clone();
            self.advance();
            if require_semicolon {
                self.expect_semicolon()?;
            }
            return Ok(vec![EvalStmt::DynamicStaticPropertyReferenceBind {
                class_name,
                property,
                source,
            }]);
        }
        let value = self.parse_expr()?;
        if require_semicolon {
            self.expect_semicolon()?;
        }
        let value = match op {
            Some(op) => EvalExpr::Binary {
                op,
                left: Box::new(EvalExpr::DynamicStaticPropertyGet {
                    class_name: Box::new(class_name.clone()),
                    property: property.clone(),
                }),
                right: Box::new(value),
            },
            None => value,
        };
        Ok(vec![EvalStmt::DynamicStaticPropertySet {
            class_name,
            property,
            value,
        }])
    }

    /// Parses static property increment/decrement as read-modify-write.
    pub(in crate::parser) fn parse_static_property_inc_dec_stmt(
        &mut self,
        prefixed: bool,
        require_semicolon: bool,
    ) -> Result<Vec<EvalStmt>, EvalParseError> {
        let prefix_increment = if prefixed {
            let increment = matches!(self.current(), TokenKind::PlusPlus);
            self.advance();
            Some(increment)
        } else {
            None
        };
        let class_name = self.parse_qualified_name()?;
        let class_name = self.resolve_static_class_name(class_name);
        self.expect(TokenKind::DoubleColon)?;
        let TokenKind::DollarIdent(property) = self.current() else {
            return Err(EvalParseError::ExpectedVariable);
        };
        let property = property.clone();
        self.advance();
        let increment = if let Some(increment) = prefix_increment {
            increment
        } else {
            let increment = matches!(self.current(), TokenKind::PlusPlus);
            self.advance();
            increment
        };
        if require_semicolon {
            self.expect_semicolon()?;
        }
        Ok(vec![EvalStmt::StaticPropertyIncDec {
            class_name,
            property,
            increment,
        }])
    }

    /// Parses dynamic static property increment/decrement as read-modify-write.
    pub(in crate::parser) fn parse_dynamic_static_property_inc_dec_stmt(
        &mut self,
        prefixed: bool,
        require_semicolon: bool,
    ) -> Result<Vec<EvalStmt>, EvalParseError> {
        let prefix_increment = if prefixed {
            let increment = matches!(self.current(), TokenKind::PlusPlus);
            self.advance();
            Some(increment)
        } else {
            None
        };
        let TokenKind::DollarIdent(class_name) = self.current() else {
            return Err(EvalParseError::ExpectedVariable);
        };
        let class_name = EvalExpr::LoadVar(class_name.clone());
        self.advance();
        self.expect(TokenKind::DoubleColon)?;
        let TokenKind::DollarIdent(property) = self.current() else {
            return Err(EvalParseError::ExpectedVariable);
        };
        let property = property.clone();
        self.advance();
        let increment = if let Some(increment) = prefix_increment {
            increment
        } else {
            let increment = matches!(self.current(), TokenKind::PlusPlus);
            self.advance();
            increment
        };
        if require_semicolon {
            self.expect_semicolon()?;
        }
        Ok(vec![EvalStmt::DynamicStaticPropertyIncDec {
            class_name,
            property,
            increment,
        }])
    }

    /// Parses prefix `++$name` / `--$name` and supported property-like prefix mutations.
    pub(in crate::parser) fn parse_prefix_inc_dec_stmt(
        &mut self,
        require_semicolon: bool,
    ) -> Result<Vec<EvalStmt>, EvalParseError> {
        let increment = matches!(self.current(), TokenKind::PlusPlus);
        self.advance();
        if let TokenKind::DollarIdent(name) = self.current() {
            if !matches!(
                self.peek(),
                TokenKind::DoubleColon
                    | TokenKind::Arrow
                    | TokenKind::QuestionArrow
                    | TokenKind::LBracket
            ) {
                let name = name.clone();
                self.advance();
                if require_semicolon {
                    self.expect_semicolon()?;
                }
                return Ok(vec![inc_dec_store(name, increment)]);
            }
        }
        let target = self.parse_expr()?;
        if require_semicolon {
            self.expect_semicolon()?;
        }
        property_inc_dec_stmt(target, increment).map(|stmt| vec![stmt])
    }

    /// Parses postfix `$name++` and `$name--` as simple statement effects.
    pub(in crate::parser) fn parse_postfix_inc_dec_stmt(
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

    /// Parses prefix property increment/decrement as read-modify-write.
    pub(in crate::parser) fn parse_prefixed_property_inc_dec_stmt(
        &mut self,
        require_semicolon: bool,
    ) -> Result<Vec<EvalStmt>, EvalParseError> {
        let increment = matches!(self.current(), TokenKind::PlusPlus);
        self.advance();
        let target = self.parse_expr()?;
        if require_semicolon {
            self.expect_semicolon()?;
        }
        property_inc_dec_stmt(target, increment).map(|stmt| vec![stmt])
    }

    /// Parses `$object->property` as either an expression statement or property write.
    pub(in crate::parser) fn parse_property_stmt(
        &mut self,
        require_semicolon: bool,
    ) -> Result<Vec<EvalStmt>, EvalParseError> {
        let target = self.parse_expr()?;
        self.parse_property_like_stmt_tail(target, require_semicolon)
    }

    /// Parses assignment, array-write, or inc/dec tails after a parsed property-like target.
    pub(super) fn parse_property_like_stmt_tail(
        &mut self,
        target: EvalExpr,
        require_semicolon: bool,
    ) -> Result<Vec<EvalStmt>, EvalParseError> {
        if matches!(self.current(), TokenKind::PlusPlus | TokenKind::MinusMinus) {
            let increment = matches!(self.current(), TokenKind::PlusPlus);
            self.advance();
            if require_semicolon {
                self.expect_semicolon()?;
            }
            return property_inc_dec_stmt(target, increment).map(|stmt| vec![stmt]);
        }
        if self.consume(TokenKind::LBracket) {
            if self.consume(TokenKind::RBracket) {
                self.expect(TokenKind::Equal)?;
                let value = self.parse_expr()?;
                if require_semicolon {
                    self.expect_semicolon()?;
                }
                return property_array_append_stmt(target, value).map(|stmt| vec![stmt]);
            }
            let index = self.parse_expr()?;
            self.expect(TokenKind::RBracket)?;
            let Some(op) = assignment_op(self.current()) else {
                return Err(EvalParseError::UnexpectedToken);
            };
            self.advance();
            let value = self.parse_expr()?;
            if require_semicolon {
                self.expect_semicolon()?;
            }
            return property_array_set_stmt(target, index, op, value).map(|stmt| vec![stmt]);
        }
        let Some(op) = assignment_op(self.current()) else {
            if require_semicolon {
                self.expect_semicolon()?;
            }
            return Ok(vec![EvalStmt::Expr(target)]);
        };
        self.advance();
        if op.is_none() && self.consume(TokenKind::Ampersand) {
            let TokenKind::DollarIdent(source) = self.current() else {
                return Err(EvalParseError::ExpectedVariable);
            };
            let source = source.clone();
            self.advance();
            if require_semicolon {
                self.expect_semicolon()?;
            }
            return property_reference_bind_stmt(target, source).map(|stmt| vec![stmt]);
        }
        let value = self.parse_expr()?;
        if require_semicolon {
            self.expect_semicolon()?;
        }
        match (target, op) {
            (EvalExpr::ArrayGet { array, index }, op) => {
                property_array_set_stmt(*array, *index, op, value).map(|stmt| vec![stmt])
            }
            (EvalExpr::PropertyGet { object, property }, None) => Ok(vec![EvalStmt::PropertySet {
                object: *object,
                property,
                value,
            }]),
            (EvalExpr::PropertyGet { object, property }, Some(op)) => {
                Ok(vec![EvalStmt::PropertyCompoundAssign {
                    object: *object,
                    property,
                    op,
                    value,
                }])
            }
            (EvalExpr::DynamicPropertyGet { object, property }, None) => {
                Ok(vec![EvalStmt::DynamicPropertySet {
                    object: *object,
                    property: *property,
                    value,
                }])
            }
            (EvalExpr::DynamicPropertyGet { object, property }, Some(op)) => {
                Ok(vec![EvalStmt::DynamicPropertyCompoundAssign {
                    object: *object,
                    property: *property,
                    op,
                    value,
                }])
            }
            (
                EvalExpr::DynamicStaticPropertyGet {
                    class_name,
                    property,
                },
                op,
            ) => {
                let class_name = *class_name;
                let value = match op {
                    Some(op) => EvalExpr::Binary {
                        op,
                        left: Box::new(EvalExpr::DynamicStaticPropertyGet {
                            class_name: Box::new(class_name.clone()),
                            property: property.clone(),
                        }),
                        right: Box::new(value),
                    },
                    None => value,
                };
                Ok(vec![EvalStmt::DynamicStaticPropertySet {
                    class_name,
                    property,
                    value,
                }])
            }
            (
                EvalExpr::DynamicStaticPropertyNameGet {
                    class_name,
                    property,
                },
                op,
            ) => {
                let class_name = *class_name;
                let property = *property;
                let value = match op {
                    Some(op) => EvalExpr::Binary {
                        op,
                        left: Box::new(EvalExpr::DynamicStaticPropertyNameGet {
                            class_name: Box::new(class_name.clone()),
                            property: Box::new(property.clone()),
                        }),
                        right: Box::new(value),
                    },
                    None => value,
                };
                Ok(vec![EvalStmt::DynamicStaticPropertyNameSet {
                    class_name,
                    property,
                    value,
                }])
            }
            _ => Err(EvalParseError::UnexpectedToken),
        }
    }
}
