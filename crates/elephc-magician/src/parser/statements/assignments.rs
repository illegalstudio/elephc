//! Purpose:
//! Parses for clauses, variable stores, property/static assignments, and increment/decrement forms.
//!
//! Called from:
//! - Statement dispatch and for-loop clause parsing.
//!
//! Key details:
//! - Compound assignment and property targets lower directly into explicit EvalIR statement variants.

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

impl Parser {
    /// Parses short array destructuring assignment into ordered destructuring targets.
    pub(in crate::parser) fn parse_array_destructure_stmt(
        &mut self,
    ) -> Result<Vec<EvalStmt>, EvalParseError> {
        let targets = self.parse_destructure_pattern()?;
        self.expect(TokenKind::Equal)?;
        let value = self.parse_expr()?;
        self.expect_semicolon()?;
        Ok(vec![EvalStmt::ArrayDestructure { targets, value }])
    }

    /// Parses one `[...]` destructuring PATTERN, which is not an array literal.
    ///
    /// Four things separate the two, and the old code -- a list of optional variable NAMES built
    /// by walking an already-parsed literal -- could express only the first: a hole where PHP
    /// allows a bare comma (`[, , , $x]`); a slot that is any LVALUE and not just a variable
    /// (`[$this->x, $h["k"], S::$p]`); a named key (`["a" => $x]`); and nesting
    /// (`[[$a, $b], $c]`). An array literal cannot even hold a hole, so the expression position
    /// had to stop converting one and parse the pattern directly.
    pub(in crate::parser) fn parse_destructure_pattern(
        &mut self,
    ) -> Result<Vec<Option<EvalDestructureTarget>>, EvalParseError> {
        // PHP spells the same pattern two ways. `list(...)` is the older one and is still
        // written -- it is not a function call, and parsing it as one is why `list($a, $b) = $v;`
        // died on the `=`.
        if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "list")) {
            self.advance();
            return self.parse_destructure_pattern_between(TokenKind::LParen, TokenKind::RParen);
        }
        self.parse_destructure_pattern_between(TokenKind::LBracket, TokenKind::RBracket)
    }

    /// Parses a destructuring pattern between one pair of delimiters.
    fn parse_destructure_pattern_between(
        &mut self,
        open: TokenKind,
        close: TokenKind,
    ) -> Result<Vec<Option<EvalDestructureTarget>>, EvalParseError> {
        self.expect(open)?;
        let mut targets = Vec::new();
        loop {
            if self.consume(close.clone()) {
                break;
            }
            if self.consume(TokenKind::Comma) {
                targets.push(None);
                continue;
            }
            targets.push(Some(self.parse_destructure_target()?));
            if self.consume(close.clone()) {
                break;
            }
            self.expect(TokenKind::Comma)?;
        }
        if targets.is_empty() {
            return Err(EvalParseError::UnexpectedToken);
        }
        Ok(targets)
    }

    /// Parses one non-empty destructuring slot, with its optional `KEY =>` prefix.
    fn parse_destructure_target(&mut self) -> Result<EvalDestructureTarget, EvalParseError> {
        if matches!(self.current(), TokenKind::LBracket)
            || self.current_starts_list_destructure_pattern()
        {
            return Ok(EvalDestructureTarget {
                key: None,
                slot: EvalDestructureSlot::Nested(self.parse_destructure_pattern()?),
            });
        }
        let first = self.parse_ternary()?;
        if !self.consume(TokenKind::FatArrow) {
            if !is_assignment_target(&first) {
                return Err(EvalParseError::ExpectedVariable);
            }
            return Ok(EvalDestructureTarget {
                key: None,
                slot: EvalDestructureSlot::Lvalue(first),
            });
        }
        if matches!(self.current(), TokenKind::LBracket)
            || self.current_starts_list_destructure_pattern()
        {
            return Ok(EvalDestructureTarget {
                key: Some(first),
                slot: EvalDestructureSlot::Nested(self.parse_destructure_pattern()?),
            });
        }
        let target = self.parse_ternary()?;
        if !is_assignment_target(&target) {
            return Err(EvalParseError::ExpectedVariable);
        }
        Ok(EvalDestructureTarget {
            key: Some(first),
            slot: EvalDestructureSlot::Lvalue(target),
        })
    }

    /// Parses the optional first clause of a `for` loop.
    pub(in crate::parser) fn parse_for_init_clause(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        if matches!(self.current(), TokenKind::Semicolon) {
            return Ok(Vec::new());
        }
        self.parse_for_clause_stmt_list()
    }

    /// Parses the optional update clause of a `for` loop.
    pub(in crate::parser) fn parse_for_update_clause(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        if self.consume(TokenKind::RParen) {
            return Ok(Vec::new());
        }
        let statements = self.parse_for_clause_stmt_list()?;
        self.expect(TokenKind::RParen)?;
        Ok(statements)
    }

    /// Parses one comma-separated `for` clause list.
    ///
    /// PHP's grammar makes each of the three `for` clauses a `for_exprs` LIST, so
    /// `for ($i = 0, $n = 3; $i < $n; ++$i, --$n)` runs both elements of the first clause once and
    /// both elements of the third on every iteration, in source order. Each element lowers to its
    /// own statement, which is exactly the vector the `For` statement's init and update fields
    /// already hold. Ten files of the Symfony tree are written this way.
    fn parse_for_clause_stmt_list(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        let mut statements = self.parse_for_clause_stmt()?;
        while self.consume(TokenKind::Comma) {
            statements.extend(self.parse_for_clause_stmt()?);
        }
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
            TokenKind::Ident(_) | TokenKind::ReservedNameHead(_) | TokenKind::Backslash
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

    /// Parses `$name[index] = expr`, `$name[] = expr`, and nested `??=` in a `for` clause.
    pub(in crate::parser) fn parse_array_set_clause(
        &mut self,
        name: String,
    ) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        self.expect(TokenKind::LBracket)?;
        if self.consume(TokenKind::RBracket) {
            self.expect(TokenKind::Equal)?;
            if self.consume(TokenKind::Ampersand) {
                let source = self.parse_reference_source_expr()?;
                return Ok(vec![EvalStmt::ArrayAppendReferenceBind {
                    target: EvalExpr::LoadVar(name),
                    source,
                }]);
            }
            let value = self.parse_expr()?;
            return Ok(vec![EvalStmt::ArrayAppendVar { name, value }]);
        }
        let index = self.parse_expr()?;
        self.expect(TokenKind::RBracket)?;
        let mut target = EvalExpr::ArrayGet {
            array: Box::new(EvalExpr::LoadVar(name.clone())),
            index: Box::new(index.clone()),
        };
        let mut nested = false;
        while self.consume(TokenKind::LBracket) {
            if self.consume(TokenKind::RBracket) {
                self.expect(TokenKind::Equal)?;
                // Same rule as the statement form: an append can BIND as well as store.
                if self.consume(TokenKind::Ampersand) {
                    let source = self.parse_reference_source_expr()?;
                    return Ok(vec![EvalStmt::ArrayAppendReferenceBind { target, source }]);
                }
                let value = self.parse_expr()?;
                return Ok(vec![EvalStmt::ArrayAppend { target, value }]);
            }
            nested = true;
            let nested_index = self.parse_expr()?;
            self.expect(TokenKind::RBracket)?;
            target = EvalExpr::ArrayGet {
                array: Box::new(target),
                index: Box::new(nested_index),
            };
        }
        if self.consume(TokenKind::QuestionQuestionEqual) {
            let default = self.parse_expr()?;
            return Ok(vec![EvalStmt::Expr(EvalExpr::NullCoalesceAssign {
                target: Box::new(target),
                default: Box::new(default),
            })]);
        }
        if matches!(self.current(), TokenKind::Equal)
            && matches!(self.peek(), TokenKind::Ampersand)
        {
            self.advance();
            self.advance();
            let source = self.parse_reference_source_expr()?;
            return Ok(vec![EvalStmt::ArrayReferenceBind { target, source }]);
        }
        let Some(op) = assignment_op(self.current()) else {
            return Err(EvalParseError::UnexpectedToken);
        };
        self.advance();
        if op.is_none() && self.consume(TokenKind::Ampersand) {
            let source = self.parse_reference_source_expr()?;
            return Ok(vec![EvalStmt::ArrayReferenceBind { target, source }]);
        }
        let value = self.parse_expr()?;
        if nested || op.is_some() {
            let expression = match op {
                Some(op) => EvalExpr::CompoundAssign {
                    target: Box::new(target),
                    op,
                    value: Box::new(value),
                },
                None => EvalExpr::Assign {
                    target: Box::new(target),
                    value: Box::new(value),
                },
            };
            return Ok(vec![EvalStmt::Expr(expression)]);
        }
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
            let source = self.parse_reference_source_name()?;
            if require_semicolon {
                self.expect_semicolon()?;
            }
            return Ok(vec![match source {
                ReferenceSource::Variable(source) => EvalStmt::ReferenceAssign {
                    target: name,
                    source,
                },
                ReferenceSource::Lvalue(source) => EvalStmt::VarReferenceBind {
                    target: name,
                    source,
                },
            }]);
        }
        let value = self.parse_assignment()?;
        if matches!(self.current(), TokenKind::Ident(word) if ident_eq(word, "and") || ident_eq(word, "xor") || ident_eq(word, "or")) {
            let value = assignment_value(&name, op, value);
            let assigned = EvalExpr::Assign {
                target: Box::new(EvalExpr::LoadVar(name)),
                value: Box::new(value),
            };
            let expr = self.parse_keyword_logical_from(assigned)?;
            if require_semicolon {
                self.expect_semicolon()?;
            }
            return Ok(vec![EvalStmt::Expr(expr)]);
        }
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
        let class_name = self.parse_class_reference_name(true)?;
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
            if op.is_none() && self.consume(TokenKind::Ampersand) {
                // An ELEMENT of a static property binds like any other array element, and
                // `EvalStmt::ArrayReferenceBind` already writes through a general lvalue --
                // `evaluate_plain_assignment_location` handles a static-property root. Only this
                // parse path never offered it the chance: it read the `=` and then demanded a
                // VALUE, so `self::$cache[$a] = &self::$cache[$b];` died on the `&`.
                let source = self.parse_reference_source_expr()?;
                if require_semicolon {
                    self.expect_semicolon()?;
                }
                return Ok(vec![EvalStmt::ArrayReferenceBind {
                    target: EvalExpr::ArrayGet {
                        array: Box::new(EvalExpr::StaticPropertyGet {
                            class_name,
                            property,
                        }),
                        index: Box::new(index),
                    },
                    source,
                }]);
            }
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
            let (mut stmts, source) = self.parse_reference_source_via_alias()?;
            if require_semicolon {
                self.expect_semicolon()?;
            }
            stmts.push(EvalStmt::StaticPropertyReferenceBind {
                class_name,
                property,
                source,
            });
            return Ok(stmts);
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
            let (mut stmts, source) = self.parse_reference_source_via_alias()?;
            if require_semicolon {
                self.expect_semicolon()?;
            }
            stmts.push(EvalStmt::DynamicStaticPropertyReferenceBind {
                class_name,
                property,
                source,
            });
            return Ok(stmts);
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
        let class_name = self.parse_class_reference_name(true)?;
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
        self.inc_dec_stmt_for_target(target, increment)
    }

    /// Lowers one parsed increment/decrement target to a statement.
    ///
    /// A PROPERTY target keeps its dedicated statement. Anything else -- `++$a["k"]["c"];` and
    /// `++$this->m["k"];` write through an array ELEMENT, not through a property -- had no
    /// statement at all and was refused. The semicolon had already been consumed by then, so the
    /// diagnostic named the NEXT statement's first token, and because the parser records only
    /// its FIRST failure position across backtracking the reported line could belong to another
    /// statement entirely. An element target becomes the same read-modify-write expression
    /// `parse_prefix_inc_dec_expr` builds, so the statement and expression spellings agree by
    /// construction rather than by coincidence.
    fn inc_dec_stmt_for_target(
        &mut self,
        target: EvalExpr,
        increment: bool,
    ) -> Result<Vec<EvalStmt>, EvalParseError> {
        match property_inc_dec_stmt(target.clone(), increment) {
            Ok(stmt) => Ok(vec![stmt]),
            Err(_) if is_assignment_target(&target) => {
                Ok(vec![EvalStmt::Expr(EvalExpr::CompoundAssign {
                    target: Box::new(target),
                    op: if increment {
                        EvalBinOp::Add
                    } else {
                        EvalBinOp::Sub
                    },
                    value: Box::new(EvalExpr::Const(EvalConst::Int(1))),
                })])
            }
            Err(error) => Err(error),
        }
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
        self.inc_dec_stmt_for_target(target, increment)
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
        // The expression parser now recognises `TARGET[] = value`, so a WHOLE-STATEMENT append
        // arrives here already folded into one expression instead of leaving the `[` for the
        // loop below. A statement append keeps its dedicated statement: `PropertyArrayAppend`
        // and its dynamic and static siblings run through the property-aware read/write helpers
        // (`eval_property_get_result` / `eval_property_set_result`), which enforce visibility
        // and consult the dynamic-property overlay. Unfolding it back is what keeps that path,
        // and every expectation that names those statements, exactly as it was.
        // A whole-STATEMENT reference bind keeps its dedicated statement. The expression parser
        // now folds `TARGET = &SOURCE` into one node so it can also be a condition, and this
        // unfolds it back: `PropertyReferenceBind` and its siblings resolve their source through
        // `scope.reference_target()`, which an expression node does not do.
        if let EvalExpr::ReferenceBind {
            target: bind_target,
            source,
        } = target
        {
            if require_semicolon {
                self.expect_semicolon()?;
            }
            return reference_bind_statements(*bind_target, *source);
        }
        if let EvalExpr::ArrayAppendAssign {
            target: append_target,
            value,
        } = target
        {
            if require_semicolon {
                self.expect_semicolon()?;
            }
            return match property_array_append_stmt((*append_target).clone(), (*value).clone()) {
                Ok(stmt) => Ok(vec![stmt]),
                // A target with no dedicated statement -- `$this->rows["k"][] = 1;` writes
                // through an ARRAY ELEMENT, not through a property -- keeps the expression.
                Err(_) => Ok(vec![EvalStmt::Expr(EvalExpr::ArrayAppendAssign {
                    target: append_target,
                    value,
                })]),
            };
        }
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
        // A target that turns out not to be assignable is only discovered after the whole
        // right-hand side has been read, so remember the operator PHP names in that diagnostic.
        let operator_pos = self.pos;
        self.advance();
        if op.is_none() && self.consume(TokenKind::Ampersand) {
            // An ELEMENT target is not a property target. `property_reference_bind_stmt` knows
            // property shapes only, so `$this->data["bag"] = &$rows;` was refused even though
            // `EvalStmt::ArrayReferenceBind` writes through exactly this lvalue already.
            if matches!(target, EvalExpr::ArrayGet { .. }) {
                let source = self.parse_reference_source_expr()?;
                if require_semicolon {
                    self.expect_semicolon()?;
                }
                return Ok(vec![EvalStmt::ArrayReferenceBind { target, source }]);
            }
            let (mut stmts, source) = self.parse_reference_source_via_alias()?;
            if require_semicolon {
                self.expect_semicolon()?;
            }
            stmts.push(property_reference_bind_stmt(target, source)?);
            return Ok(stmts);
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
            _ => Err(self.fail_at(operator_pos, EvalParseError::UnexpectedToken)),
        }
    }

    /// Parses the source of a `= &` reference binding.
    ///
    /// PHP accepts any assignable expression after `=&`, not just a plain variable. The
    /// interpreter's reference machinery can alias every storage shape `eval_call_arg_value()`
    /// resolves to an `EvalReferenceTarget`: a variable, an array element (including a nested
    /// one), an object property, a static property, and their dynamic-name forms, and a CALL,
    /// whose reference comes from a by-reference callee at run time (a by-value callee binds a
    /// copy after php's "Only variables should be assigned by reference" notice). A literal or an
    /// operator expression names no storage and is reported as an unsupported construct.
    pub(in crate::parser) fn parse_reference_source_expr(
        &mut self,
    ) -> Result<EvalExpr, EvalParseError> {
        let source = self.parse_expr()?;
        if !eval_expr_binds_a_reference(&source) {
            return Err(EvalParseError::UnsupportedConstruct);
        }
        // `parse_postfix` stops in front of an empty `[]`, so a source that ends in one arrives
        // here with the brackets unconsumed and the statement then demanded its semicolon at the
        // `[`. As a reference SOURCE that `[]` names the element the bind should create:
        // `$closure = &$this->optimized[$e][];` appends null and aliases it.
        if matches!(self.current(), TokenKind::LBracket)
            && matches!(self.peek(), TokenKind::RBracket)
        {
            self.advance();
            self.advance();
            return Ok(EvalExpr::ArrayAppendSlot {
                target: Box::new(source),
            });
        }
        Ok(source)
    }

    /// Parses a `= &` source and splits it into the plain-variable and general-lvalue cases.
    ///
    /// The variable-to-variable case stays on `EvalStmt::ReferenceAssign` because PHP aliases
    /// two scope names symmetrically there — writing either name updates both — which the
    /// scope's named-alias table models and a one-way `EvalReferenceTarget` write-back does not.
    fn parse_reference_source_name(&mut self) -> Result<ReferenceSource, EvalParseError> {
        match self.parse_reference_source_expr()? {
            EvalExpr::LoadVar(name) => Ok(ReferenceSource::Variable(name)),
            source => Ok(ReferenceSource::Lvalue(source)),
        }
    }

    /// Parses a `= &` source and lowers a non-variable one through a hidden binding variable.
    ///
    /// `PropertyReferenceBind` and its static-property siblings identify their source by scope
    /// name, and each resolves it through `scope.reference_target()` first. Binding the general
    /// lvalue to a hidden name first therefore hands those statements the exact reference target
    /// they need without teaching each one to evaluate an expression.
    fn parse_reference_source_via_alias(
        &mut self,
    ) -> Result<(Vec<EvalStmt>, String), EvalParseError> {
        match self.parse_reference_source_name()? {
            ReferenceSource::Variable(name) => Ok((Vec::new(), name)),
            ReferenceSource::Lvalue(source) => {
                let target = next_reference_binding_name();
                Ok((
                    vec![EvalStmt::VarReferenceBind {
                        target: target.clone(),
                        source,
                    }],
                    target,
                ))
            }
        }
    }
}

/// The two shapes a `= &` source lowers to.
enum ReferenceSource {
    /// A plain variable, aliased by name in the scope.
    Variable(String),
    /// Any other assignable expression, aliased through its reference target.
    Lvalue(EvalExpr),
}

/// Returns whether an expression names storage PHP can bind a reference to.
fn eval_expr_binds_a_reference(expr: &EvalExpr) -> bool {
    matches!(
        expr,
        // A CALL is a legal source: `$r = &$obj->get();` binds what a function declared
        // `function &get()` returned. Whether the callee actually returned a reference is a
        // RUNTIME question -- php answers a plain `return 5;` there with a notice and a copy,
        // not a parse error -- so the grammar accepts every call shape.
        EvalExpr::Call { .. }
            | EvalExpr::NamespacedCall { .. }
            | EvalExpr::DynamicCall { .. }
            | EvalExpr::MethodCall { .. }
            | EvalExpr::NullsafeMethodCall { .. }
            | EvalExpr::DynamicMethodCall { .. }
            | EvalExpr::NullsafeDynamicMethodCall { .. }
            | EvalExpr::StaticMethodCall { .. }
            | EvalExpr::DynamicStaticMethodCall { .. }
            | EvalExpr::ArrayAppendSlot { .. }
            | EvalExpr::LoadVar(_)
            | EvalExpr::ArrayGet { .. }
            | EvalExpr::PropertyGet { .. }
            | EvalExpr::DynamicPropertyGet { .. }
            | EvalExpr::StaticPropertyGet { .. }
            | EvalExpr::DynamicStaticPropertyGet { .. }
            | EvalExpr::DynamicStaticPropertyNameGet { .. }
    )
}

/// Returns a scope name for one hidden reference binding.
///
/// The leading NUL byte cannot appear in a PHP variable name, so the binding is invisible to
/// user code the way `eval_property_reference_alias_name()` already keeps property aliases out
/// of the visible scope.
fn next_reference_binding_name() -> String {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("\0elephc_reference_source:{id}")
}

/// Lowers one parsed `TARGET = &SOURCE` into the statement family that binds it.
///
/// Each target family has its own statement because each resolves its source differently: an
/// element writes through `ArrayReferenceBind`, a plain name aliases symmetrically through
/// `ReferenceAssign`, and a property identifies its source by SCOPE NAME -- so a source that is
/// not already a name is bound to a hidden one first, which is what
/// `parse_reference_source_via_alias` does when the source is parsed rather than handed over.
fn reference_bind_statements(
    target: EvalExpr,
    source: EvalExpr,
) -> Result<Vec<EvalStmt>, EvalParseError> {
    if matches!(target, EvalExpr::ArrayGet { .. }) {
        return Ok(vec![EvalStmt::ArrayReferenceBind { target, source }]);
    }
    if let EvalExpr::LoadVar(name) = target {
        return Ok(vec![match source {
            EvalExpr::LoadVar(source) => EvalStmt::ReferenceAssign {
                target: name,
                source,
            },
            source => EvalStmt::VarReferenceBind {
                target: name,
                source,
            },
        }]);
    }
    let (mut stmts, source) = match source {
        EvalExpr::LoadVar(name) => (Vec::new(), name),
        source => {
            let alias = next_reference_binding_name();
            (
                vec![EvalStmt::VarReferenceBind {
                    target: alias.clone(),
                    source,
                }],
                alias,
            )
        }
    };
    stmts.push(property_reference_bind_stmt(target, source)?);
    Ok(stmts)
}
