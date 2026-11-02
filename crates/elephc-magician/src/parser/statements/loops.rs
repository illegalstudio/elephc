//! Purpose:
//! Parses do-while, for, foreach, and array-set loop-adjacent statements.
//!
//! Called from:
//! - `Parser::parse_stmt()` and for-clause parsing.
//!
//! Key details:
//! - Foreach key/value targets and statement bodies retain EvalIR source order.
//! - `for` and `foreach` accept PHP's alternative `:` … `endfor;`/`endforeach;` bodies, which
//!   lower to the same EvalIR as the brace form.

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

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

    /// Parses `$name[index] = expr;`, `$name[] = expr;`, and nested `??=` writes.
    pub(in crate::parser) fn parse_array_set_stmt(
        &mut self,
        name: String,
    ) -> Result<Vec<EvalStmt>, EvalParseError> {
        // A dimension can also begin a call, such as $attributes[0]->newInstance().
        // Keep the dedicated store parser for assignments and empty append dimensions.
        let mut expression = self.clone();
        if let Ok(value) = expression.parse_expr() {
            if expression.consume(TokenKind::Semicolon) {
                *self = expression;
                return Ok(vec![EvalStmt::Expr(value)]);
            }
        }
        self.advance();
        self.expect(TokenKind::LBracket)?;
        if self.consume(TokenKind::RBracket) {
            self.expect(TokenKind::Equal)?;
            if self.consume(TokenKind::Ampersand) {
                let source = self.parse_expr()?;
                self.expect_semicolon()?;
                return Ok(vec![EvalStmt::ArrayAppendReferenceBind {
                    target: EvalExpr::LoadVar(name),
                    source,
                }]);
            }
            let value = self.parse_expr()?;
            self.expect_semicolon()?;
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
                // `$loops[$k][] = &$path;` BINDS a newly appended element rather than storing a
                // value in it. The `&` was never looked for here, so the append branch went
                // straight to `parse_expr`, which cannot start with an ampersand.
                if self.consume(TokenKind::Ampersand) {
                    let source = self.parse_reference_source_expr()?;
                    self.expect_semicolon()?;
                    return Ok(vec![EvalStmt::ArrayAppendReferenceBind { target, source }]);
                }
                let value = self.parse_expr()?;
                self.expect_semicolon()?;
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
            self.expect_semicolon()?;
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
            let source = self.parse_expr()?;
            self.expect_semicolon()?;
            return Ok(vec![EvalStmt::ArrayReferenceBind { target, source }]);
        }
        let Some(op) = assignment_op(self.current()) else {
            return Err(EvalParseError::UnexpectedToken);
        };
        self.advance();
        if op.is_none() && self.consume(TokenKind::Ampersand) {
            let source = self.parse_expr()?;
            self.expect_semicolon()?;
            return Ok(vec![EvalStmt::ArrayReferenceBind { target, source }]);
        }
        let value = self.parse_expr()?;
        self.expect_semicolon()?;
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
        let body = self.parse_statement_body_or_alternative("endfor")?;
        Ok(vec![EvalStmt::For {
            init,
            condition,
            update,
            body,
        }])
    }

    /// Parses value, key-value, and array-destructuring `foreach` targets.
    pub(in crate::parser) fn parse_foreach_stmt(&mut self) -> Result<Vec<EvalStmt>, EvalParseError> {
        self.advance();
        self.expect(TokenKind::LParen)?;
        let array = self.parse_expr()?;
        if !matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "as")) {
            return Err(EvalParseError::UnexpectedToken);
        }
        self.advance();
        let first = self.parse_foreach_value_target()?;
        let (key, value, value_by_ref) = if matches!(self.current(), TokenKind::FatArrow) {
            self.advance();
            if !matches!(first.slot, EvalForeachSlot::Name(_) | EvalForeachSlot::Lvalue(_))
                || first.by_ref
            {
                return Err(EvalParseError::ExpectedVariable);
            }
            let value = self.parse_foreach_value_target()?;
            (Some(first), value.slot, value.by_ref)
        } else {
            (None, first.slot, first.by_ref)
        };
        self.expect(TokenKind::RParen)?;
        // A foreach binds its key and value to NAMES. Anything else -- a pattern, a property, an
        // element -- is bound to a hidden name and copied into place by a statement prepended to
        // the body, which is what `[$a, $b]` already did and what `$this->k =>` now does too.
        let (value_name, value_prologue) = self.foreach_target_binding(value);
        let (key_name, key_prologue) = match key {
            Some(key) => {
                let (name, prologue) = self.foreach_target_binding(key.slot);
                (Some(name), prologue)
            }
            None => (None, Vec::new()),
        };
        let mut body = self.parse_statement_body_or_alternative("endforeach")?;
        body.splice(0..0, key_prologue.into_iter().chain(value_prologue));
        Ok(vec![EvalStmt::Foreach {
            array,
            key_name,
            value_name,
            value_by_ref,
            body,
        }])
    }

    /// Parses one foreach key or value target: a variable, a pattern, or any other lvalue.
    fn parse_foreach_value_target(&mut self) -> Result<EvalForeachTarget, EvalParseError> {
        let by_ref = self.consume(TokenKind::Ampersand);
        if let TokenKind::DollarIdent(value_name) = self.current() {
            if !matches!(
                self.peek(),
                TokenKind::Arrow | TokenKind::LBracket | TokenKind::DoubleColon
            ) {
                let value_name = value_name.clone();
                self.advance();
                return Ok(EvalForeachTarget {
                    slot: EvalForeachSlot::Name(value_name),
                    by_ref,
                });
            }
        }
        if by_ref {
            return Err(EvalParseError::ExpectedVariable);
        }
        if matches!(self.current(), TokenKind::LBracket) {
            if let Some(elements) = self.try_parse_flat_reference_pattern() {
                return Ok(EvalForeachTarget {
                    slot: EvalForeachSlot::ReferencePattern(elements),
                    by_ref: true,
                });
            }
            return Ok(EvalForeachTarget {
                slot: EvalForeachSlot::Pattern(self.parse_destructure_pattern()?),
                by_ref,
            });
        }
        let target = self.parse_ternary()?;
        if !is_assignment_target(&target) {
            return Err(EvalParseError::ExpectedVariable);
        }
        // `parse_postfix` stops in front of an empty `[]`, so `foreach ($x as $k => $list[])`
        // arrives here with the brackets unconsumed: that target APPENDS each value, the way
        // Symfony's `IsGrantedAttributeListener` collects `$attributes[]` from a generator.
        if matches!(self.current(), TokenKind::LBracket) && matches!(self.peek(), TokenKind::RBracket)
        {
            self.advance();
            self.advance();
            return Ok(EvalForeachTarget {
                slot: EvalForeachSlot::Append(target),
                by_ref,
            });
        }
        Ok(EvalForeachTarget {
            slot: EvalForeachSlot::Lvalue(target),
            by_ref,
        })
    }

    /// Parses `[$a, &$b, ...]` -- positional, flat, only variables, at least one `&` -- or leaves
    /// the cursor untouched and returns `None` for every other pattern.
    fn try_parse_flat_reference_pattern(&mut self) -> Option<Vec<(String, bool)>> {
        let mut pos = self.pos + 1;
        let mut elements = Vec::new();
        loop {
            let by_ref = matches!(self.tokens.get(pos), Some(TokenKind::Ampersand));
            if by_ref {
                pos += 1;
            }
            let Some(TokenKind::DollarIdent(name)) = self.tokens.get(pos) else {
                return None;
            };
            elements.push((name.clone(), by_ref));
            pos += 1;
            match self.tokens.get(pos) {
                Some(TokenKind::Comma) => pos += 1,
                Some(TokenKind::RBracket) => {
                    pos += 1;
                    break;
                }
                _ => return None,
            }
        }
        if !elements.iter().any(|(_, by_ref)| *by_ref) {
            return None;
        }
        self.pos = pos;
        Some(elements)
    }

    /// Returns the name a foreach slot binds to, plus the statement that moves it into place.
    fn foreach_target_binding(&mut self, slot: EvalForeachSlot) -> (String, Vec<EvalStmt>) {
        match slot {
            EvalForeachSlot::Name(name) => (name, Vec::new()),
            EvalForeachSlot::Pattern(targets) => {
                let name = next_foreach_binding_name();
                (
                    name.clone(),
                    vec![EvalStmt::ArrayDestructure {
                        targets,
                        value: EvalExpr::LoadVar(name),
                    }],
                )
            }
            EvalForeachSlot::Lvalue(target) => {
                let name = next_foreach_binding_name();
                (
                    name.clone(),
                    vec![EvalStmt::Expr(EvalExpr::Assign {
                        target: Box::new(target),
                        value: Box::new(EvalExpr::LoadVar(name)),
                    })],
                )
            }
            EvalForeachSlot::ReferencePattern(elements) => {
                let name = next_foreach_binding_name();
                let statements = elements
                    .into_iter()
                    .enumerate()
                    .map(|(index, (variable, by_ref))| {
                        let element = EvalExpr::ArrayGet {
                            array: Box::new(EvalExpr::LoadVar(name.clone())),
                            index: Box::new(EvalExpr::Const(EvalConst::Int(index as i64))),
                        };
                        if by_ref {
                            EvalStmt::VarReferenceBind { target: variable, source: element }
                        } else {
                            EvalStmt::StoreVar { name: variable, value: element }
                        }
                    })
                    .collect();
                (name, statements)
            }
            EvalForeachSlot::Append(target) => {
                let name = next_foreach_binding_name();
                (
                    name.clone(),
                    vec![EvalStmt::Expr(EvalExpr::ArrayAppendAssign {
                        target: Box::new(target),
                        value: Box::new(EvalExpr::LoadVar(name)),
                    })],
                )
            }
        }
    }
}

/// One parsed foreach key or value target.
pub(in crate::parser) struct EvalForeachTarget {
    slot: EvalForeachSlot,
    by_ref: bool,
}

/// What a foreach key or value target names.
pub(in crate::parser) enum EvalForeachSlot {
    /// A plain `$name`, which the loop can bind directly.
    Name(String),
    /// A `[...]` destructuring pattern.
    Pattern(Vec<Option<EvalDestructureTarget>>),
    /// Any other writable lvalue, such as `$this->k`.
    Lvalue(EvalExpr),
    /// `$list[]`: each iteration appends its value to the array `$list` names.
    Append(EvalExpr),
    /// A flat positional pattern with at least one `&` element, `[$multi, &$responses]`: the
    /// loop iterates BY REFERENCE and each element is bound from the referenced value, by
    /// reference where the pattern says so (symfony/http-client's `TransportResponseTrait`).
    ReferencePattern(Vec<(String, bool)>),
}

/// Returns a scope name for one hidden foreach binding.
///
/// The leading NUL byte cannot appear in a PHP variable name, so the binding is invisible to
/// user code.
fn next_foreach_binding_name() -> String {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("\0elephc_foreach_binding:{id}")
}
