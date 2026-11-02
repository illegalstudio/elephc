//! Purpose:
//! Parses PHP expression precedence from keyword logical operators through
//! unary operators and scalar casts.
//!
//! Called from:
//! - `Parser::parse_expr()` and the next tighter parser precedence layer.
//!
//! Key details:
//! - Ternary, coalesce, exponentiation handoff, and PHP keyword precedence are
//!   kept explicit.

use super::*;
use crate::parser::state::{EVAL_YIELD_FROM_INTRINSIC, EVAL_YIELD_INTRINSIC};

impl Parser {
    /// Parses an expression using PHP-like logical, comparison, concatenation, and arithmetic precedence.
    pub(in crate::parser) fn parse_expr(&mut self) -> Result<EvalExpr, EvalParseError> {
        self.parse_keyword_or()
    }

    /// Continues a statement-level keyword logical expression from an already parsed left side.
    ///
    /// `$x = true and false;` binds as `($x = true) and false;` because assignment binds
    /// tighter than `and`. The statement parser consumes the assignment itself, so it resumes
    /// here with the assignment expression as the left operand.
    pub(in crate::parser) fn parse_keyword_logical_from(
        &mut self,
        expr: EvalExpr,
    ) -> Result<EvalExpr, EvalParseError> {
        let expr = self.parse_keyword_and_from(expr)?;
        let expr = self.parse_keyword_xor_from(expr)?;
        self.parse_keyword_or_from(expr)
    }

    /// Parses the `and` tail after an already parsed left operand.
    fn parse_keyword_and_from(&mut self, mut expr: EvalExpr) -> Result<EvalExpr, EvalParseError> {
        while matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "and")) {
            self.advance();
            let right = self.parse_assignment()?;
            expr = EvalExpr::Binary {
                op: EvalBinOp::LogicalAnd,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    /// Parses the `xor` tail after an already parsed left operand.
    fn parse_keyword_xor_from(&mut self, mut expr: EvalExpr) -> Result<EvalExpr, EvalParseError> {
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

    /// Parses the `or` tail after an already parsed left operand.
    fn parse_keyword_or_from(&mut self, mut expr: EvalExpr) -> Result<EvalExpr, EvalParseError> {
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

    /// Parses PHP keyword `or`, whose precedence is lower than `xor`, `and`, and ternary.
    pub(in crate::parser) fn parse_keyword_or(&mut self) -> Result<EvalExpr, EvalParseError> {
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
    pub(in crate::parser) fn parse_keyword_xor(&mut self) -> Result<EvalExpr, EvalParseError> {
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

    /// Parses PHP keyword `and`, whose precedence is lower than assignment and `&&`.
    pub(in crate::parser) fn parse_keyword_and(&mut self) -> Result<EvalExpr, EvalParseError> {
        let mut expr = self.parse_assignment()?;
        while matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "and")) {
            self.advance();
            let right = self.parse_assignment()?;
            expr = EvalExpr::Binary {
                op: EvalBinOp::LogicalAnd,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    /// Parses one `yield`, `yield EXPR`, `yield KEY => VALUE` or `yield from EXPR` expression.
    ///
    /// PHP puts `yield` BELOW assignment in precedence, so `$a = yield 1;` is `$a = (yield 1)`
    /// and the operand runs to the end of the expression: `yield 1 + 2` yields three, not one.
    /// Parsing the operand with `parse_expr()` gives exactly that, and it still stops at a comma
    /// or a closing paren, so `f(yield $a, $b)` passes two arguments the way PHP does.
    ///
    /// A bare `yield` has no operand at all — `$x = yield;` is legal and yields null — so the
    /// operand is only parsed when a token that can begin an expression actually follows.
    fn parse_yield_expr(&mut self) -> Result<EvalExpr, EvalParseError> {
        self.advance();
        if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "from")) {
            self.advance();
            let source = self.parse_expr()?;
            return Ok(EvalExpr::Call {
                name: EVAL_YIELD_FROM_INTRINSIC.to_string(),
                args: vec![EvalCallArg::positional(source)],
            });
        }
        if !self.at_yield_operand() {
            return Ok(EvalExpr::Call {
                name: EVAL_YIELD_INTRINSIC.to_string(),
                args: vec![EvalCallArg::positional(EvalExpr::Const(EvalConst::Null))],
            });
        }
        let first = self.parse_expr()?;
        let args = if self.consume(TokenKind::FatArrow) {
            vec![
                EvalCallArg::positional(first),
                EvalCallArg::positional(self.parse_expr()?),
            ]
        } else {
            vec![EvalCallArg::positional(first)]
        };
        Ok(EvalExpr::Call {
            name: EVAL_YIELD_INTRINSIC.to_string(),
            args,
        })
    }

    /// Returns whether the current token can begin a `yield` operand.
    ///
    /// Everything that ENDS an expression means the yield had none. Listing the terminators
    /// rather than the starters is what keeps a bare `yield;`, `yield)` and `yield,` working
    /// without enumerating every token an expression may open with.
    fn at_yield_operand(&self) -> bool {
        !matches!(
            self.current(),
            TokenKind::Semicolon
                | TokenKind::RParen
                | TokenKind::RBracket
                | TokenKind::RBrace
                | TokenKind::Comma
                | TokenKind::Colon
                | TokenKind::Eof
        )
    }

    /// Parses supported right-associative assignment expressions.
    pub(in crate::parser) fn parse_assignment(&mut self) -> Result<EvalExpr, EvalParseError> {
        // A destructuring PATTERN is not an array literal and cannot be recovered from one: a
        // literal has no way to hold the hole in `[, , , $x]`. When the tokens ahead form a
        // pattern followed by `=`, parse it as a pattern before `parse_ternary` turns it into a
        // literal it cannot represent.
        if (matches!(self.current(), TokenKind::LBracket)
            || self.current_starts_list_destructure_pattern())
            && self.current_starts_array_destructure_assignment()
        {
            let targets = self.parse_destructure_pattern()?;
            self.expect(TokenKind::Equal)?;
            let value = self.parse_assignment()?;
            return Ok(EvalExpr::ArrayDestructureAssign {
                targets,
                value: Box::new(value),
            });
        }
        let target = self.parse_ternary()?;
        if let Some(append) = self.parse_array_append_assignment(&target)? {
            return Ok(append);
        }
        let nested_assignment_target = nested_assignment_target(&target);
        let null_coalescing = self.consume(TokenKind::QuestionQuestionEqual);
        let assignment = if null_coalescing {
            None
        } else {
            assignment_op(self.current())
        };
        if !null_coalescing && assignment.is_none() {
            return Ok(target);
        }
        if !is_assignment_target(&target)
            && !nested_assignment_target
        {
            return Err(EvalParseError::UnexpectedToken);
        }
        if null_coalescing {
            let default = self.parse_assignment()?;
            if nested_assignment_target {
                return nested_assignment_expr(&target, true, assignment, default)
                    .ok_or(EvalParseError::UnexpectedToken);
            }
            return Ok(EvalExpr::NullCoalesceAssign {
                target: Box::new(target),
                default: Box::new(default),
            });
        }
        if assignment == Some(None)
            && matches!(self.peek(), TokenKind::Ampersand)
            && !is_assignment_target(&target)
            && nested_assignment_target
        {
            // `null !== $exists = &self::$cache[$key]` binds the RIGHTMOST writable child, the
            // same rule `nested_assignment_expr` applies to an ordinary assignment.
            self.advance();
            self.advance();
            let source = self.parse_reference_source_expr()?;
            return nested_reference_bind_expr(&target, source)
                .ok_or(EvalParseError::UnexpectedToken);
        }
        if assignment == Some(None)
            && matches!(self.peek(), TokenKind::Ampersand)
            && is_assignment_target(&target)
        {
            // PHP's reference assignment is an EXPRESSION, and
            // `if (null !== $exists = &self::$cache[$key])` needs it to be one. The statement
            // tails unfold this node back into their dedicated binding statements, the way they
            // do for an append, so a whole-statement bind keeps the lowering it had.
            self.advance();
            self.advance();
            let source = self.parse_reference_source_expr()?;
            return Ok(EvalExpr::ReferenceBind {
                target: Box::new(target),
                source: Box::new(source),
            });
        }
        // `$name = &<lvalue>` where the result is used. PHP's `=` takes a `&` source wherever an
        // assignment is an expression, and `symfony/config/Resource/ClassExistenceResource.php`
        // writes `if (null !== $exists = &self::$existsCache[$this->resource])` — the file that
        // holds `throwOnRequiredClass`, the loader every Symfony `class_exists()` reaches.
        if assignment == Some(None) && matches!(self.peek(), TokenKind::Ampersand) {
            if let Some(name) = reference_bind_assign_name(&target, nested_assignment_target) {
                self.advance();
                self.advance();
                let source = self.parse_reference_source_expr()?;
                let bind = EvalExpr::ReferenceBindAssign {
                    target: name,
                    source: Box::new(source),
                };
                return if nested_assignment_target {
                    nested_assignment_replacement(&target, bind)
                        .ok_or(EvalParseError::UnexpectedToken)
                } else {
                    Ok(bind)
                };
            }
        }
        self.advance();
        let value = self.parse_assignment()?;
        if nested_assignment_target {
            return nested_assignment_expr(&target, false, assignment, value)
                .ok_or(EvalParseError::UnexpectedToken);
        }
        Ok(match assignment.expect("assignment operator was checked") {
            Some(op) => EvalExpr::CompoundAssign {
                target: Box::new(target),
                op,
                value: Box::new(value),
            },
            None => EvalExpr::Assign {
                target: Box::new(target),
                value: Box::new(value),
            },
        })
    }

    /// Parses `TARGET[] = value` as an EXPRESSION when an append is not the whole statement.
    ///
    /// `parse_postfix` deliberately stops in front of an empty `[]` so the statement parser can
    /// recognise `$a[] = 1;` and build one of the dedicated append statements. That left the
    /// append with no expression spelling at all, so `return $this->rules[] = $r;` and the
    /// chained `$dirs[] = $paths[] = $d;` were refused at the `[` with `ExpectedSemicolon` --
    /// the parser had a complete expression in hand and no rule that let a `[` follow it.
    ///
    /// This is the missing rule. It fires only where `parse_postfix` stopped, so a statement
    /// append still takes the statement path and keeps its existing lowering; only the nested
    /// and value-producing positions come here. `parse_assignment` for the right-hand side is
    /// what makes the chain right-associative, matching PHP.
    ///
    /// A REFERENCE append (`$a[] = &$b`) is deliberately not taken here: it binds rather than
    /// assigns, `EvalStmt::ArrayAppendReferenceBind` is its statement form, and the `&` family
    /// is its own gap. Leaving it to fail where it already failed is better than accepting it
    /// into a node that would silently copy.
    fn parse_array_append_assignment(
        &mut self,
        target: &EvalExpr,
    ) -> Result<Option<EvalExpr>, EvalParseError> {
        if !matches!(self.current(), TokenKind::LBracket)
            || !matches!(self.tokens.get(self.pos + 1), Some(TokenKind::RBracket))
            || !matches!(self.tokens.get(self.pos + 2), Some(TokenKind::Equal))
            || matches!(self.tokens.get(self.pos + 3), Some(TokenKind::Ampersand))
        {
            return Ok(None);
        }
        if !is_assignment_target(target) && !target.is_call_result() {
            return Err(EvalParseError::UnexpectedToken);
        }
        self.advance();
        self.advance();
        self.advance();
        let value = self.parse_assignment()?;
        Ok(Some(EvalExpr::ArrayAppendAssign {
            target: Box::new(target.clone()),
            value: Box::new(value),
        }))
    }

    /// Parses PHP ternary expressions, including the short `expr ?: fallback` form.
    pub(in crate::parser) fn parse_ternary(&mut self) -> Result<EvalExpr, EvalParseError> {
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
    pub(in crate::parser) fn parse_null_coalesce(&mut self) -> Result<EvalExpr, EvalParseError> {
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
    pub(in crate::parser) fn parse_logical_or(&mut self) -> Result<EvalExpr, EvalParseError> {
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
    pub(in crate::parser) fn parse_logical_and(&mut self) -> Result<EvalExpr, EvalParseError> {
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
    pub(in crate::parser) fn parse_bit_or(&mut self) -> Result<EvalExpr, EvalParseError> {
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
    pub(in crate::parser) fn parse_bit_xor(&mut self) -> Result<EvalExpr, EvalParseError> {
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
    pub(in crate::parser) fn parse_bit_and(&mut self) -> Result<EvalExpr, EvalParseError> {
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
    pub(in crate::parser) fn parse_equality(&mut self) -> Result<EvalExpr, EvalParseError> {
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
    pub(in crate::parser) fn parse_ordering(&mut self) -> Result<EvalExpr, EvalParseError> {
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
    pub(in crate::parser) fn parse_shift(&mut self) -> Result<EvalExpr, EvalParseError> {
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
    pub(in crate::parser) fn parse_concat(&mut self) -> Result<EvalExpr, EvalParseError> {
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
    pub(in crate::parser) fn parse_add(&mut self) -> Result<EvalExpr, EvalParseError> {
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
    pub(in crate::parser) fn parse_mul(&mut self) -> Result<EvalExpr, EvalParseError> {
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
    pub(in crate::parser) fn parse_unary(&mut self) -> Result<EvalExpr, EvalParseError> {
        if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "yield")) {
            return self.parse_yield_expr();
        }
        if let Some(target) = self.peek_cast_type() {
            self.advance();
            self.advance();
            self.advance();
            // A cast is a UNARY operator in PHP and binds exactly as tightly as the others, so
            // its operand is another unary expression -- not `parse_concat`, which swallowed the
            // whole concatenation and every additive term with it. `(int) $a . "4x"` is
            // `((int) $a) . "4x"` and prints `34x`; taking the concatenation as the operand made
            // it `(int) "34x"` and printed a NUMBER. `**` still binds tighter, because
            // `parse_unary` reaches `parse_power` on the way down: `(int) "2.9" ** 2` is 8 in
            // both, and `-(int) "3"` is -3.
            let expr = self.parse_unary()?;
            return Ok(EvalExpr::Cast {
                target,
                expr: Box::new(expr),
            });
        }
        if matches!(self.current(), TokenKind::Ident(name) if ident_eq(name, "clone"))
            && !matches!(self.peek(), TokenKind::LParen)
        {
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
        if self.consume(TokenKind::PlusPlus) {
            return self.parse_prefix_inc_dec_expr(true);
        }
        if self.consume(TokenKind::MinusMinus) {
            return self.parse_prefix_inc_dec_expr(false);
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
        if self.consume(TokenKind::At) {
            let expr = self.parse_unary()?;
            return Ok(EvalExpr::Unary {
                op: EvalUnaryOp::ErrorSuppress,
                expr: Box::new(expr),
            });
        }
        self.parse_instanceof()
    }

    /// Parses a prefix increment or decrement expression and returns its updated value.
    fn parse_prefix_inc_dec_expr(&mut self, increment: bool) -> Result<EvalExpr, EvalParseError> {
        let target = self.parse_unary()?;
        if !is_assignment_target(&target) {
            return Err(EvalParseError::UnexpectedToken);
        }
        Ok(EvalExpr::CompoundAssign {
            target: Box::new(target),
            op: if increment {
                EvalBinOp::Add
            } else {
                EvalBinOp::Sub
            },
            value: Box::new(EvalExpr::Const(EvalConst::Int(1))),
        })
    }

    /// Returns the cast target represented by the current `(type)` token window.
    pub(super) fn peek_cast_type(&self) -> Option<EvalCastType> {
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
        } else if ident_eq(name, "array") {
            Some(EvalCastType::Array)
        } else if ident_eq(name, "object") {
            Some(EvalCastType::Object)
        } else {
            None
        }
    }

}

/// Returns whether one expression is a regular PHP assignment lvalue in the EvalIR subset.
pub(in crate::parser) fn is_assignment_target(target: &EvalExpr) -> bool {
    matches!(
        target,
        EvalExpr::LoadVar(_)
            | EvalExpr::DynamicVar(_)
            | EvalExpr::ArrayGet { .. }
            | EvalExpr::PropertyGet { .. }
            | EvalExpr::DynamicPropertyGet { .. }
            | EvalExpr::StaticPropertyGet { .. }
            | EvalExpr::DynamicStaticPropertyGet { .. }
            | EvalExpr::DynamicStaticPropertyNameGet { .. }
    )
}

/// Returns the scope name a `= &` binding writes to when the assignment is used as a value.
///
/// Only a plain variable can be REBOUND as a reference: PHP aliases two scope names, which is
/// what `EvalStmt::VarReferenceBind` models and what `EvalExpr::ReferenceBindAssign` reuses. When
/// the assignment sits inside a larger expression — `null !== $exists = &…`, the shape PHP's
/// grammar produces because the left side of `=` must be a variable — the variable is the
/// rightmost assignable child, the same one `nested_assignment_target()` found.
fn reference_bind_assign_name(target: &EvalExpr, nested: bool) -> Option<String> {
    match rightmost_assignment_target(target, nested)? {
        EvalExpr::LoadVar(name) => Some(name.clone()),
        _ => None,
    }
}

/// The parent of an expression's right-edge child, kept so the parent can be rebuilt around a
/// replacement for that child.
///
/// PHP's grammar requires a variable on the left of `=`, so wherever a completed expression is
/// followed by an assignment operator the assignment really belongs to the expression's
/// RIGHTMOST operand: `$a . $b ??= v` is `$a . ($b ??= v)`, `$x ?? $x = 5` is `$x ?? ($x = 5)`,
/// and a prefix operator applies to the whole assignment -- `!$x = 0`, `-$z = 3`, `@$h = f()`,
/// and `(int) $pos = strpos($name, ':')`, which Symfony's `IconRenderer::renderIcon()` writes.
enum AssignmentEdge<'a> {
    /// `left OP <edge>`.
    Binary { op: EvalBinOp, left: &'a EvalExpr },
    /// `value ?? <edge>`.
    Coalesce { value: &'a EvalExpr },
    /// A prefix operator: `!`, `-`, `+`, `~`, `@`.
    Unary { op: EvalUnaryOp },
    /// A cast: `(int) <edge>`.
    Cast { target: EvalCastType },
}

impl AssignmentEdge<'_> {
    /// Rebuilds the parent around a new right-edge child.
    fn rebuild(&self, child: EvalExpr) -> EvalExpr {
        match self {
            AssignmentEdge::Binary { op, left } => EvalExpr::Binary {
                op: *op,
                left: Box::new((*left).clone()),
                right: Box::new(child),
            },
            AssignmentEdge::Coalesce { value } => EvalExpr::NullCoalesce {
                value: Box::new((*value).clone()),
                default: Box::new(child),
            },
            AssignmentEdge::Unary { op } => EvalExpr::Unary {
                op: *op,
                expr: Box::new(child),
            },
            AssignmentEdge::Cast { target } => EvalExpr::Cast {
                target: *target,
                expr: Box::new(child),
            },
        }
    }
}

/// Splits an expression into its right-edge child and the parent that holds it.
fn assignment_edge(expr: &EvalExpr) -> Option<(AssignmentEdge<'_>, &EvalExpr)> {
    match expr {
        EvalExpr::Binary { op, left, right } => {
            Some((AssignmentEdge::Binary { op: *op, left }, right.as_ref()))
        }
        EvalExpr::NullCoalesce { value, default } => {
            Some((AssignmentEdge::Coalesce { value }, default.as_ref()))
        }
        EvalExpr::Unary { op, expr } => Some((AssignmentEdge::Unary { op: *op }, expr.as_ref())),
        EvalExpr::Cast { target, expr } => {
            Some((AssignmentEdge::Cast { target: *target }, expr.as_ref()))
        }
        _ => None,
    }
}

/// Returns the assignable expression a nested assignment actually writes to.
fn rightmost_assignment_target(target: &EvalExpr, nested: bool) -> Option<&EvalExpr> {
    if !nested {
        return is_assignment_target(target).then_some(target);
    }
    let (_, child) = assignment_edge(target)?;
    if is_assignment_target(child) {
        return Some(child);
    }
    rightmost_assignment_target(child, true)
}

/// Rebuilds a nested-assignment target with its rightmost writable child replaced outright.
///
/// `nested_assignment_expr()` builds the replacement itself from an operator and a value; a
/// reference binding is neither, so it is handed in already built.
fn nested_assignment_replacement(target: &EvalExpr, replacement: EvalExpr) -> Option<EvalExpr> {
    let (edge, child) = assignment_edge(target)?;
    if is_assignment_target(child) {
        return Some(edge.rebuild(replacement));
    }
    nested_assignment_replacement(child, replacement).map(|child| edge.rebuild(child))
}

/// Returns whether an assignment belongs to a nested lvalue on an expression's right edge.
///
/// PHP parses `$prefix.$suffix ??= value` as `$prefix.($suffix ??= value)`, even though the
/// completed concatenation itself is not writable. The same right-edge recovery applies to every
/// binary operator, to `??`, to every prefix operator and to casts (see [`AssignmentEdge`]).
///
/// `??` IS THAT SAME RIGHT EDGE, and it is not a `Binary`. PHP's assignment binds looser than
/// `??`, so `$a ?? $a = 5` could only mean `($a ?? $a) = 5`, which is not derivable because the
/// left of `=` must be a variable — bison therefore reduces `$a ?? ($a = 5)`, and php prints
/// `55`. Five Symfony files depend on it, four as `$x ?? $x = …` and Container.php as
/// `$this->factories[$id] ?? self::$make ??= self::make(...)`.
fn nested_assignment_target(target: &EvalExpr) -> bool {
    let Some((_, child)) = assignment_edge(target) else {
        return false;
    };
    is_assignment_target(child) || nested_assignment_target(child)
}

/// Rebuilds an expression with its rightmost writable child replaced by a REFERENCE bind.
///
/// The mirror of `nested_assignment_expr`, for `= &`. PHP puts assignment below comparison, so
/// `null !== $e = &$src` binds `$e` and compares the result, and a condition written that way is
/// the whole reason the bind has to be an expression.
fn nested_reference_bind_expr(target: &EvalExpr, source: EvalExpr) -> Option<EvalExpr> {
    let (edge, child) = assignment_edge(target)?;
    let bound = if is_assignment_target(child) {
        EvalExpr::ReferenceBind {
            target: Box::new(child.clone()),
            source: Box::new(source),
        }
    } else {
        nested_reference_bind_expr(child, source)?
    };
    Some(edge.rebuild(bound))
}

/// Rebuilds an expression with its rightmost writable child replaced by an assignment.
fn nested_assignment_expr(
    target: &EvalExpr,
    null_coalescing: bool,
    assignment: Option<Option<EvalBinOp>>,
    value: EvalExpr,
) -> Option<EvalExpr> {
    let (edge, child) = assignment_edge(target)?;
    if is_assignment_target(child) {
        return Some(edge.rebuild(nested_assignment_value(
            child.clone(),
            null_coalescing,
            assignment,
            value,
        )));
    }
    nested_assignment_expr(child, null_coalescing, assignment, value).map(|child| edge.rebuild(child))
}

/// Builds the value expression stored in one recovered assignment target.
fn nested_assignment_value(
    target: EvalExpr,
    null_coalescing: bool,
    assignment: Option<Option<EvalBinOp>>,
    value: EvalExpr,
) -> EvalExpr {
    if null_coalescing {
        return EvalExpr::NullCoalesceAssign {
            target: Box::new(target),
            default: Box::new(value),
        };
    }
    match assignment.expect("assignment operator was checked") {
        Some(op) => EvalExpr::CompoundAssign {
            target: Box::new(target),
            op,
            value: Box::new(value),
        },
        None => EvalExpr::Assign {
            target: Box::new(target),
            value: Box::new(value),
        },
    }
}
