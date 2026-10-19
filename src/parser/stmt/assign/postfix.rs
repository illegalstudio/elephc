//! Purpose:
//! Detects and lowers postfix assignments for complex expression targets.
//! Replays parseable l-values and creates effect-preserving lowerings for property/static assignments.
//!
//! Called from:
//! - `crate::parser::stmt::simple::parse_expr_stmt()` and assignment statement dispatch.
//!
//! Key details:
//! - Complex target lowering must not duplicate side effects while preserving PHP source evaluation order.

use crate::errors::CompileError;
use crate::lexer::{SpannedToken, Token};
use crate::parser::ast::{
    BinOp, Expr, ExprKind, InstanceOfTarget, Stmt, StmtKind, NESTED_APPEND_TEMP_PREFIX,
};
use crate::parser::expr::{parse_assignment_value_expr, parse_expr};
use crate::span::Span;

use super::super::expect_semicolon;
use super::compound::{assignment_operator, assignment_value, AssignmentOperator};

/// Parses a postfix assignment where the target involves property access, array access,
/// or other complex expressions. Detects `+=` append style via `[]` in the target.
/// Returns the lowered `StmtKind` directly for simple targets, or synthesizes a
/// temporary-variable sequence for effectful (compound operator) targets that cannot
/// be replayed safely.
/// Returns `Ok(None)` if the token range does not contain a postfix assignment pattern.
pub(in crate::parser::stmt) fn try_parse_postfix_assignment(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Option<Stmt>, CompileError> {
    let start = *pos;
    let Some((assign_pos, op)) = find_top_level_assignment(tokens, start) else {
        return Ok(None);
    };
    if assign_pos < start + 3 {
        return Ok(None);
    }

    let lhs = &tokens[start..assign_pos];
    if let Some(split) = find_mid_chain_append(lhs) {
        if op != AssignmentOperator::Assign {
            // `$a[][0] .= v` reads the new element PHP has no value for.
            return Err(CompileError::new(span, "Cannot use [] for reading"));
        }
        return parse_mid_chain_append_assignment(tokens, pos, start, split, assign_pos, span)
            .map(Some);
    }
    let is_append = lhs.len() >= 3
        && lhs[lhs.len() - 2].0 == Token::LBracket
        && lhs[lhs.len() - 1].0 == Token::RBracket;
    if is_append && op != AssignmentOperator::Assign {
        return Err(CompileError::new(span, "Invalid assignment target"));
    }
    let contains_postfix = lhs
        .iter()
        .skip(1)
        .any(|(token, _)| matches!(token, Token::Arrow | Token::QuestionArrow | Token::LBracket));
    if !contains_postfix {
        return Ok(None);
    }

    let mut lhs_pos = 0;
    let lhs_expr_tokens = if is_append {
        &lhs[..lhs.len() - 2]
    } else {
        lhs
    };
    let Some(lhs_expr) = parse_statement_assignment_lhs(lhs_expr_tokens, &mut lhs_pos) else {
        // `cond ?: $o->p = v;`, `$a || $b[k] = v;`, `!$o->p = f();`: the first top-level
        // assignment operator belongs to the RIGHT operand of an operator, so the tokens before
        // it are not a place. PHP parses these as `cond ?: ($o->p = v)`; the expression parser
        // already binds an assignment to the variable that precedes it, so defer to it.
        return Ok(None);
    };
    if crate::parser::expr::is_dynamic_class_member_read(&lhs_expr) {
        // `$class::$prop[] = v` / `$class::$prop[k] = v`: the read desugar is a value, so the
        // write would land in a temporary.
        return Err(CompileError::new(
            span,
            crate::parser::expr::DYNAMIC_STATIC_PROPERTY_WRITE_ERROR,
        ));
    }

    crate::parser::expr::reject_nullsafe_write(&lhs_expr, span)?;

    *pos = assign_pos + 1;
    if op == AssignmentOperator::Assign
        && matches!(tokens.get(*pos).map(|(token, _)| token), Some(Token::Ampersand))
    {
        if is_append {
            return super::reference_places::parse_append_ref_assign(tokens, pos, lhs_expr, span)
                .map(Some);
        }
        return super::reference_places::parse_place_ref_assign(tokens, pos, lhs_expr, span)
            .map(Some);
    }
    let rhs = parse_assignment_value_expr(tokens, pos)?;
    expect_semicolon(tokens, pos)?;
    if !is_append && crate::parser::expr::is_call_dimension_place(&lhs_expr) {
        return call_dimension_assignment_stmt(lhs_expr, op, rhs, span).map(Some);
    }
    if op != AssignmentOperator::Assign && !can_replay_assignment_target(&lhs_expr) {
        return lower_effectful_postfix_assignment(lhs_expr, op, rhs, span).map(Some);
    }
    // A compound write reads its element at store time too, so a right-hand side that
    // reassigns a plain-variable index decides which element is READ as well as which is
    // written: `$c = [10, 20]; $k = 0; $c[$k] += ($k = 1);` leaves `[10, 21]` in PHP. The
    // desugar below embeds that read INSIDE the right-hand side, where it would still see
    // the old index, so the right-hand side is settled into a temporary first.
    //
    // `??=` is deliberately excluded although it is also "not `Assign`": its right-hand
    // side is LAZY, evaluated only when the key is absent. Hoisting it runs it every time
    // AND before the presence check, so `$a[$slot] ??= ($slot = 0)` would test the slot the
    // right-hand side just selected instead of the one written in the source.
    let mut hoisted = EffectfulTargetLowerer::new(span);
    let rhs = if matches!(op, AssignmentOperator::Compound(_))
        && compound_rhs_can_disturb_index(&lhs_expr, &rhs)
    {
        hoisted.stabilize_unconditionally(rhs)
    } else {
        rhs
    };
    // A compound operation reads and writes one dimension. Capture a mutable index after
    // any eager RHS evaluation, before a diagnostic handler can change its source variable.
    let lhs_expr = if matches!(op, AssignmentOperator::Compound(_)) {
        hoisted.snapshot_update_dimension(lhs_expr)
    } else {
        lhs_expr
    };
    let lhs_span = lhs_expr.span;
    if is_append {
        return push_statement(lhs_expr, rhs, span).map(Some);
    }

    let value = assignment_value(lhs_expr.clone(), op, rhs, span);

    let stmt = match lhs_expr.kind {
        ExprKind::ArrayAccess { array, index } => match array.kind {
            ExprKind::Variable(array) => StmtKind::ArrayAssign {
                array,
                index: *index,
                value,
            },
            ExprKind::PropertyAccess { object, property } => StmtKind::PropertyArrayAssign {
                object,
                property,
                index: *index,
                value,
            },
            _ => StmtKind::NestedArrayAssign {
                target: Expr::new(ExprKind::ArrayAccess { array, index }, span),
                value,
            },
        },
        ExprKind::PropertyAccess { object, property } => StmtKind::PropertyAssign {
            object,
            property,
            value,
        },
        ExprKind::DynamicPropertyAccess { object, property } => StmtKind::ExprStmt(Expr::new(
            ExprKind::Assignment {
                target: Box::new(Expr::new(
                    ExprKind::DynamicPropertyAccess { object, property },
                    lhs_span,
                )),
                value: Box::new(value),
                result_target: None,
                prelude: Vec::new(),
                conditional_value_temp: None,
            },
            span,
        )),
        _ => return Err(CompileError::new(span, "Invalid assignment target")),
    };

    Ok(Some(hoisted.finish_if_used(stmt, span)))
}

/// True when `target` writes an element at a plain-VARIABLE index and `rhs` can reassign
/// that variable before the write lands.
///
/// A replay-safe right-hand side has no effects at all, so it can disturb nothing — which is
/// what keeps `$counts[$k]++` and `$sums[$k] += $n` on the existing path, emitting no temporary.
/// Every other right-hand side is settled first.
///
/// The gate used to also require the right-hand side to MENTION the index by name, which was
/// unsound: `$c[$k] += (function() use (&$k) { $k = 1; return 1; })();` answered `10,11` where
/// PHP answers `10,21`, because the mention sits in a closure the call expression does not scan.
/// Widening it costs a temporary on shapes like `$sums[$k] += f()` and costs no correctness:
/// PHP evaluates a compound assignment's right-hand side BEFORE reading the element, so settling
/// it early is what the language does anyway.
fn compound_rhs_can_disturb_index(target: &Expr, rhs: &Expr) -> bool {
    let ExprKind::ArrayAccess { index, .. } = &target.kind else {
        return false;
    };
    if !matches!(index.kind, ExprKind::Variable(_)) {
        return false;
    }
    !can_replay_assignment_target(rhs)
}

/// Lowers an append through a nested array target (`$a[0][] = $value`) into a
/// synthetic read/append/write-back sequence. The temporary append triggers the
/// existing copy-on-write split, and the final assignment stores the detached
/// nested array back into the original slot.
pub(crate) fn lower_nested_append_assignment(
    target: Expr,
    value: Expr,
    span: Span,
) -> Result<Stmt, CompileError> {
    let mut lowerer = EffectfulTargetLowerer::new(span);
    let target = lowerer.stabilize_array_target(target);
    // PHP fetches a dynamic property for writing only after the right-hand side ran, so a
    // right-hand side that appends to the same property must not be overwritten by a copy
    // read before it.
    let value = if dynamic_property_rooted(&target) && !can_replay_assignment_target(&value) {
        lowerer.stabilize_unconditionally(value)
    } else {
        value
    };
    let temp = lowerer.next_nested_append_temp_name();
    lowerer.stmts.push(Stmt::new(
        StmtKind::Assign {
            name: temp.clone(),
            value: target.clone(),
        },
        span,
    ));
    lowerer.stmts.push(Stmt::new(
        StmtKind::ArrayPush {
            array: temp.clone(),
            value,
        },
        span,
    ));
    let write_back =
        assignment_target_store_stmt(target, Expr::new(ExprKind::Variable(temp), span), span)?;
    Ok(lowerer.finish(write_back))
}

/// Returns whether an append target's base chain is rooted in `$obj->{$name}`.
fn dynamic_property_rooted(target: &Expr) -> bool {
    match &target.kind {
        ExprKind::DynamicPropertyAccess { .. } => true,
        ExprKind::ArrayAccess { array, .. } => dynamic_property_rooted(array),
        _ => false,
    }
}

/// Builds the statement that writes `value` back into an already-stabilized
/// assignment target. Supports the same local, property, static property, and
/// array target families as postfix assignment lowering.
pub(crate) fn assignment_target_store_stmt(
    target: Expr,
    value: Expr,
    span: Span,
) -> Result<StmtKind, CompileError> {
    match target.kind {
        ExprKind::Variable(name) => Ok(StmtKind::Assign { name, value }),
        ExprKind::PropertyAccess { object, property } => Ok(StmtKind::PropertyAssign {
            object,
            property,
            value,
        }),
        ExprKind::StaticPropertyAccess { receiver, property } => {
            Ok(StmtKind::StaticPropertyAssign {
                receiver,
                property,
                value,
            })
        }
        ExprKind::DynamicPropertyAccess { object, property } => Ok(StmtKind::ExprStmt(Expr::new(
            ExprKind::Assignment {
                target: Box::new(Expr::new(
                    ExprKind::DynamicPropertyAccess { object, property },
                    target.span,
                )),
                value: Box::new(value),
                result_target: None,
                prelude: Vec::new(),
                conditional_value_temp: None,
            },
            span,
        ))),
        ExprKind::ArrayAccess { array, index } => match array.kind {
            ExprKind::Variable(array) => Ok(StmtKind::ArrayAssign {
                array,
                index: *index,
                value,
            }),
            ExprKind::PropertyAccess { object, property } => Ok(StmtKind::PropertyArrayAssign {
                object,
                property,
                index: *index,
                value,
            }),
            ExprKind::StaticPropertyAccess { receiver, property } => {
                Ok(StmtKind::StaticPropertyArrayAssign {
                    receiver,
                    property,
                    index: *index,
                    value,
                })
            }
            _ => Ok(StmtKind::NestedArrayAssign {
                target: Expr::new(ExprKind::ArrayAccess { array, index }, span),
                value,
            }),
        },
        _ => Err(CompileError::new(span, "Invalid assignment target")),
    }
}

/// Parses discarded post-increment/decrement on a scoped (static class member) l-value target.
/// Handles `A::$x++`, `static::$x--`, `parent::$y++`, etc. Returns `Ok(None)` when no
/// postfix `++`/`--` is found at the top level of the statement.
pub(in crate::parser::stmt) fn try_parse_scoped_postfix_incdec(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Option<Stmt>, CompileError> {
    let start = *pos;
    let Some((incdec_pos, is_increment)) = find_top_level_postfix_incdec(tokens, start) else {
        return Ok(None);
    };
    if incdec_pos < start + 3 {
        return Ok(None);
    }
    if assignment_precedes(tokens, start, incdec_pos) {
        return Ok(None);
    }

    let lhs = &tokens[start..incdec_pos];
    let mut lhs_pos = 0;
    let lhs_expr = parse_expr(lhs, &mut lhs_pos)?;
    if lhs_pos != lhs.len() {
        return Err(CompileError::new(span, "Invalid increment target"));
    }

    if !matches!(lhs_expr.kind, ExprKind::StaticPropertyAccess { .. })
        && !crate::variable_variables::static_props::is_dynamic_static_place(&lhs_expr)
    {
        return Ok(None);
    }

    *pos = incdec_pos + 1;
    expect_semicolon(tokens, pos)?;

    lower_postfix_incdec_assignment(lhs_expr, is_increment, span).map(Some)
}

/// Parses discarded post-increment/decrement on a complex l-value target.
///
/// For statement contexts the original expression result is unused, so `$a[0]++`
/// can be lowered to the same read-modify-write shape as `$a[0] += 1`.
/// Simple local `$x++` is left to the existing local-variable parser.
pub(in crate::parser::stmt) fn try_parse_postfix_incdec(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Option<Stmt>, CompileError> {
    let start = *pos;
    let Some((incdec_pos, is_increment)) = find_top_level_postfix_incdec(tokens, start) else {
        return Ok(None);
    };
    if incdec_pos < start + 3 {
        return Ok(None);
    }
    if assignment_precedes(tokens, start, incdec_pos) {
        return Ok(None);
    }

    let lhs = &tokens[start..incdec_pos];
    let contains_complex_target = lhs
        .iter()
        .skip(1)
        .any(|(token, _)| matches!(token, Token::Arrow | Token::QuestionArrow | Token::LBracket));
    if !contains_complex_target {
        return Ok(None);
    }

    let mut lhs_pos = 0;
    let lhs_expr = parse_expr(lhs, &mut lhs_pos)?;
    if lhs_pos != lhs.len() {
        return Err(CompileError::new(span, "Invalid increment target"));
    }

    *pos = incdec_pos + 1;
    expect_semicolon(tokens, pos)?;

    lower_postfix_incdec_assignment(lhs_expr, is_increment, span).map(Some)
}

/// Reports whether a top-level assignment operator comes BEFORE `incdec_pos`.
///
/// When it does, the `++`/`--` belongs to the assignment's right-hand side, not to the statement:
/// `$t += $b[0]++;` increments `$b[0]`, it does not increment `$t += $b[0]`. Both postfix-incdec
/// statement parsers used to claim such a statement anyway, parse the whole `$t += $b[0]` as their
/// target, and reject it with `Invalid assignment target` (issue #682). Declining here hands the
/// statement to the assignment parsers, which lower the increment inside the value expression.
fn assignment_precedes(tokens: &[SpannedToken], start: usize, incdec_pos: usize) -> bool {
    find_top_level_assignment(tokens, start)
        .is_some_and(|(assign_pos, _)| assign_pos < incdec_pos)
}

/// Parses a scoped (static class member) postfix assignment, handling targets like
/// `$obj::prop`, `$obj::$prop`, and `$obj::prop[]`. Detects `+=` append style via `[]`.
/// For compound operators on static properties that cannot be replayed safely, lowers
/// to a temporary-variable sequence via `lower_effectful_static_assignment`.
/// Returns `Ok(None)` when no scoped assignment pattern is found.
pub(in crate::parser::stmt) fn try_parse_scoped_property_assignment(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Option<Stmt>, CompileError> {
    let start = *pos;
    let Some((assign_pos, op)) = find_top_level_assignment(tokens, start) else {
        return Ok(None);
    };
    if assign_pos < start + 3 {
        return Ok(None);
    }

    let lhs = &tokens[start..assign_pos];
    if let Some(split) = find_mid_chain_append(lhs) {
        if op != AssignmentOperator::Assign {
            // `$a[][0] .= v` reads the new element PHP has no value for.
            return Err(CompileError::new(span, "Cannot use [] for reading"));
        }
        return parse_mid_chain_append_assignment(tokens, pos, start, split, assign_pos, span)
            .map(Some);
    }
    let is_append = lhs.len() >= 3
        && lhs[lhs.len() - 2].0 == Token::LBracket
        && lhs[lhs.len() - 1].0 == Token::RBracket;
    if is_append && op != AssignmentOperator::Assign {
        return Err(CompileError::new(span, "Invalid assignment target"));
    }
    let mut lhs_pos = 0;
    let lhs_expr_tokens = if is_append {
        &lhs[..lhs.len() - 2]
    } else {
        lhs
    };
    let Some(lhs_expr) = parse_statement_assignment_lhs(lhs_expr_tokens, &mut lhs_pos) else {
        // `A::f() || self::$x = v;`: the assignment is the right operand of an operator, which
        // the expression parser handles (see `try_parse_postfix_assignment`).
        return Ok(None);
    };

    *pos = assign_pos + 1;
    // `self::$darwinCache[$dir] = &self::$darwinCache[$k];` (symfony DebugClassLoader): the
    // same place-reference lowering as the local and property targets.
    if op == AssignmentOperator::Assign
        && matches!(tokens.get(*pos).map(|(token, _)| token), Some(Token::Ampersand))
    {
        if is_append {
            return Err(CompileError::new(
                span,
                "Appending a reference (`$a[] = &$x`) is not supported; use an explicit key",
            ));
        }
        return super::reference_places::parse_place_ref_assign(tokens, pos, lhs_expr, span)
            .map(Some);
    }
    // `self::$x = require 'f.php';` needs no special case: the include is an ordinary
    // expression, and `crate::resolver::value_include_hoist` binds it to a hidden local first.
    let rhs = parse_assignment_value_expr(tokens, pos)?;
    expect_semicolon(tokens, pos)?;
    if !is_append && crate::parser::expr::is_call_dimension_place(&lhs_expr) {
        return call_dimension_assignment_stmt(lhs_expr, op, rhs, span).map(Some);
    }
    if op != AssignmentOperator::Assign && !can_replay_assignment_target(&lhs_expr) {
        return lower_effectful_static_assignment(lhs_expr, op, rhs, span).map(Some);
    }
    // A compound update settles a disturbing right-hand side, then captures a mutable index,
    // exactly as `try_parse_postfix_assignment` does for local and property targets.
    let mut hoisted = EffectfulTargetLowerer::new(span);
    let (lhs_expr, rhs) = if matches!(op, AssignmentOperator::Compound(_)) {
        let rhs = if compound_rhs_can_disturb_index(&lhs_expr, &rhs) {
            hoisted.stabilize_unconditionally(rhs)
        } else {
            rhs
        };
        (hoisted.snapshot_update_dimension(lhs_expr), rhs)
    } else {
        (lhs_expr, rhs)
    };
    let value = assignment_value(lhs_expr.clone(), op, rhs, span);

    // `self::$b[$k][] = $v` (and its `static::` / `parent::` / `Named::` siblings) is an append
    // through a *nested* target. The trailing `[]` was stripped from the LHS tokens above, so
    // `lhs_expr` is an `ExprKind::ArrayAccess` and the `if is_append` guard on the bare
    // `StaticPropertyAccess` arm below — which only ever handles `self::$b[] = $v` — cannot
    // match it. Left alone it falls into the plain `ArrayAccess` arm, which ignores `is_append`
    // entirely and emits `StaticPropertyArrayAssign`: the append is silently dropped and the
    // bucket is OVERWRITTEN with the single value. Route it through the same read/append/
    // write-back desugar every other nested-append target already uses; the write-back builder
    // (`assignment_target_store_stmt`) already supports the static-property family.
    if is_append && matches!(lhs_expr.kind, ExprKind::ArrayAccess { .. }) {
        return lower_nested_append_assignment(lhs_expr, value, span).map(Some);
    }

    let stmt = match lhs_expr.kind {
        ExprKind::StaticPropertyAccess { receiver, property } if is_append => {
            StmtKind::StaticPropertyArrayPush {
                receiver,
                property,
                value,
            }
        }
        ExprKind::ArrayAccess { array, index } => match array.kind {
            ExprKind::StaticPropertyAccess { receiver, property } => {
                StmtKind::StaticPropertyArrayAssign {
                    receiver,
                    property,
                    index: *index,
                    value,
                }
            }
            _ => StmtKind::NestedArrayAssign {
                target: Expr::new(ExprKind::ArrayAccess { array, index }, span),
                value,
            },
        },
        ExprKind::StaticPropertyAccess { receiver, property } => StmtKind::StaticPropertyAssign {
            receiver,
            property,
            value,
        },
        _ => return Err(CompileError::new(span, "Invalid assignment target")),
    };

    Ok(Some(hoisted.finish_if_used(stmt, span)))
}

/// Lowers the statement `call()[k] = v;`, PHP's write on a temporary (see
/// `call_dimension_assignment_expr`). A compound or `??=` update is rejected.
fn call_dimension_assignment_stmt(
    target: Expr,
    op: AssignmentOperator,
    rhs: Expr,
    span: Span,
) -> Result<Stmt, CompileError> {
    if op != AssignmentOperator::Assign {
        return Err(CompileError::new(
            span,
            "Compound assignment to a dimension of a call result (`f()[k] += v`) is not supported; assign the result to a variable first",
        ));
    }
    let write = crate::parser::expr::call_dimension_assignment_expr(target, rhs, span)?;
    Ok(Stmt::new(StmtKind::ExprStmt(write), span))
}

/// Parses the tokens before a statement's first top-level assignment operator as its target.
///
/// Returns `None` when they are not a place (or a call, whose dimension write PHP evaluates on
/// a temporary): an operator expression such as `cond ?: $o->p` or `!$o->p`, or tokens that do
/// not parse as one expression at all (`c ? $x`, the head of `c ? $x = 1 : $y = 2`). In PHP the
/// assignment then belongs to the operator's right operand, so the statement is an expression
/// statement and the caller hands it to the expression parser, which reports its own errors.
fn parse_statement_assignment_lhs(tokens: &[SpannedToken], pos: &mut usize) -> Option<Expr> {
    let expr = parse_expr(tokens, pos).ok()?;
    if *pos != tokens.len() {
        return None;
    }
    matches!(
        expr.kind,
        ExprKind::Variable(_)
            | ExprKind::This
            | ExprKind::ArrayAccess { .. }
            | ExprKind::PropertyAccess { .. }
            | ExprKind::DynamicPropertyAccess { .. }
            | ExprKind::StaticPropertyAccess { .. }
            | ExprKind::NullsafePropertyAccess { .. }
            | ExprKind::NullsafeDynamicPropertyAccess { .. }
            | ExprKind::FunctionCall { .. }
            | ExprKind::MethodCall { .. }
            | ExprKind::NullsafeMethodCall { .. }
            | ExprKind::StaticMethodCall { .. }
            | ExprKind::ClosureCall { .. }
            | ExprKind::ExprCall { .. }
    )
    .then_some(expr)
}

/// One dimension written after a mid-chain `[]`.
enum AppendedDimension {
    /// `[]`: one more new element.
    Append,
    /// `[key]` of the new element.
    Key(Expr),
}

/// Returns the offset in `lhs` of the first top-level `[]` that more dimensions follow
/// (`$body['p'][]['to'] = v`), or `None` when `[]` is absent or only ends the target.
fn find_mid_chain_append(lhs: &[SpannedToken]) -> Option<usize> {
    let mut depth = 0usize;
    for (index, (token, _)) in lhs.iter().enumerate() {
        match token {
            Token::LParen | Token::LBrace => depth += 1,
            Token::RParen | Token::RBrace => depth = depth.saturating_sub(1),
            Token::LBracket => {
                if depth == 0
                    && index > 0
                    && matches!(lhs.get(index + 1).map(|(token, _)| token), Some(Token::RBracket))
                    && index + 2 < lhs.len()
                {
                    return Some(index);
                }
                depth += 1;
            }
            Token::RBracket => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    None
}

/// Lowers `$place[]<dims> = v;` where only `[key]` / `[]` dimensions follow the first `[]`
/// (SendGridHandler's `$body['personalizations'][]['to'][]['email'] = $r`, `$s[][] = v`).
///
/// Every `[]` creates a fresh element, so everything after the first one writes into a new
/// value: the statement is `$place[] = <dims applied to null>`, and writing `[k]` / `[]` into
/// null builds `[k => …]` / `[…]`. The nested literal evaluates the keys in source order and
/// then the value, as PHP does.
fn parse_mid_chain_append_assignment(
    tokens: &[SpannedToken],
    pos: &mut usize,
    start: usize,
    split: usize,
    assign_pos: usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    let place_tokens = &tokens[start..start + split];
    let mut place_pos = 0;
    let Some(place) = parse_statement_assignment_lhs(place_tokens, &mut place_pos) else {
        return Err(CompileError::new(span, "Cannot use [] for reading"));
    };
    // Parse the dimensions that follow the first `[]` against the full token stream.
    let mut dims = Vec::new();
    let mut cursor = start + split + 2;
    while cursor < assign_pos {
        if tokens[cursor].0 != Token::LBracket {
            // `$a[]->p = v` / `$a[]::$x = v`: PHP has no element to fetch for reading.
            return Err(CompileError::new(span, "Cannot use [] for reading"));
        }
        cursor += 1;
        if matches!(tokens.get(cursor).map(|(token, _)| token), Some(Token::RBracket)) {
            dims.push(AppendedDimension::Append);
            cursor += 1;
            continue;
        }
        let key = parse_expr(tokens, &mut cursor)?;
        if !matches!(tokens.get(cursor).map(|(token, _)| token), Some(Token::RBracket)) {
            return Err(CompileError::new(span, "Expected ']'"));
        }
        cursor += 1;
        dims.push(AppendedDimension::Key(key));
    }
    if cursor != assign_pos {
        return Err(CompileError::new(span, "Cannot use [] for reading"));
    }
    *pos = assign_pos + 1;
    let rhs = parse_assignment_value_expr(tokens, pos)?;
    expect_semicolon(tokens, pos)?;
    let element = dims.into_iter().rev().fold(rhs, |inner, dim| {
        let inner_span = inner.span;
        match dim {
            AppendedDimension::Append => Expr::new(ExprKind::ArrayLiteral(vec![inner]), inner_span),
            AppendedDimension::Key(key) => {
                Expr::new(ExprKind::ArrayLiteralAssoc(vec![(key, inner)]), inner_span)
            }
        }
    });
    mid_chain_append_stmt(place, element, span)
}

/// `$place[]<dims> = v` as `$place[] = <literal>`, guarded against an object place.
///
/// On an ArrayAccess object PHP fetches the new element through `offsetGet(null)` and writes
/// into what it returns (normally nothing, with an "Indirect modification" notice); appending
/// the literal would call `offsetSet(null, …)` instead. The parser cannot see the place's type,
/// so an object place throws an explicit `Error` at run time. The place's effectful parts are
/// evaluated once, before the check.
fn mid_chain_append_stmt(place: Expr, element: Expr, span: Span) -> Result<Stmt, CompileError> {
    let mut lowerer = EffectfulTargetLowerer::new(span);
    let place = match place.kind {
        ExprKind::Variable(_) | ExprKind::This => place,
        _ => lowerer.stabilize_array_base(place),
    };
    let call = |name: &str, args: Vec<Expr>| {
        Expr::new(
            ExprKind::FunctionCall { name: crate::names::Name::unqualified(name), args },
            span,
        )
    };
    let is_object_place = if matches!(place.kind, ExprKind::This) {
        Expr::new(ExprKind::BoolLiteral(true), span)
    } else {
        Expr::new(
            ExprKind::BinaryOp {
                left: Box::new(call("isset", vec![place.clone()])),
                op: crate::parser::ast::BinOp::And,
                right: Box::new(call("is_object", vec![place.clone()])),
            },
            span,
        )
    };
    let error = Expr::new(
        ExprKind::NewObject {
            class_name: crate::names::Name::unqualified("Error"),
            args: vec![Expr::new(
                ExprKind::StringLiteral(
                    "elephc: a write through `[]` into an object (`$obj[][...] = v`) is not supported: PHP fetches the new element with offsetGet(); write the element explicitly"
                        .to_string(),
                ),
                span,
            )],
        },
        span,
    );
    let guard_name = crate::names::generated_local_name(&format!(
        "__elephc_mid_append_guard_{}_{}",
        span.line, span.col
    ));
    lowerer.stmts.push(Stmt::new(
        StmtKind::Assign {
            name: guard_name,
            value: Expr::new(
                ExprKind::Ternary {
                    condition: Box::new(is_object_place),
                    then_expr: Box::new(Expr::new(ExprKind::Throw(Box::new(error)), span)),
                    else_expr: Box::new(Expr::new(ExprKind::Null, span)),
                },
                span,
            ),
        },
        span,
    ));
    let push = append_to_place_stmt(place, element, span)?;
    lowerer.stmts.push(push);
    Ok(Stmt::new(StmtKind::Synthetic(lowerer.stmts), span))
}

/// Builds the statement for `$place[] = value` from an already-parsed place.
pub(crate) fn append_to_place_stmt(place: Expr, element: Expr, span: Span) -> Result<Stmt, CompileError> {
    crate::parser::expr::reject_nullsafe_write(&place, span)?;
    if crate::parser::expr::is_dynamic_class_member_read(&place) {
        return Err(CompileError::new(
            span,
            crate::parser::expr::DYNAMIC_STATIC_PROPERTY_WRITE_ERROR,
        ));
    }
    let place_span = place.span;
    let stmt = match place.kind {
        ExprKind::Variable(array) => StmtKind::ArrayPush { array, value: element },
        ExprKind::PropertyAccess { object, property } => {
            StmtKind::PropertyArrayPush { object, property, value: element }
        }
        ExprKind::This => StmtKind::ArrayPush { array: "this".to_string(), value: element },
        ExprKind::DynamicPropertyAccess { object, property } => {
            dynamic_property_push_stmt(*object, *property, element, span)
        }
        kind @ ExprKind::ArrayAccess { .. } => {
            return lower_nested_append_assignment(Expr::new(kind, place_span), element, span);
        }
        other => StmtKind::ExprStmt(crate::parser::expr::append_assignment_expr(
            Expr::new(other, place_span),
            element,
            span,
        )?),
    };
    Ok(Stmt::new(stmt, span))
}

/// `$obj->{$name}[] = v` as `__elephc_dynamic_property_push($obj, $name, v)`.
///
/// A read-append-write-back through the runtime name would store a boxed array into whatever
/// slot the name selects, which a slot refined to a raw array cannot hold. The primitive instead
/// dispatches on the name in `ir_lower` and pushes onto the selected slot with its own static
/// type; the arguments keep PHP's order (object, name, value, then the fetch for writing).
fn dynamic_property_push_stmt(object: Expr, property: Expr, value: Expr, span: Span) -> StmtKind {
    StmtKind::ExprStmt(Expr::new(
        ExprKind::FunctionCall {
            name: crate::names::Name::unqualified("__elephc_dynamic_property_push"),
            args: vec![object, property, value],
        },
        span,
    ))
}

/// Scans tokens starting from `start` (skipping nested parentheses, brackets, and braces)
/// and returns the position and operator of the first top-level assignment at nesting depth 0.
/// Returns `None` if no assignment operator is found before a semicolon at depth 0.
fn find_top_level_assignment(
    tokens: &[SpannedToken],
    start: usize,
) -> Option<(usize, AssignmentOperator)> {
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut brace_depth = 0usize;
    let mut pos = start;

    while pos < tokens.len() {
        match tokens[pos].0 {
            Token::LParen => paren_depth += 1,
            Token::RParen => paren_depth = paren_depth.saturating_sub(1),
            Token::LBracket => bracket_depth += 1,
            Token::RBracket => bracket_depth = bracket_depth.saturating_sub(1),
            Token::LBrace => brace_depth += 1,
            Token::RBrace => brace_depth = brace_depth.saturating_sub(1),
            Token::Semicolon if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 => {
                return None;
            }
            _ if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 => {
                if let Some(op) = assignment_operator(&tokens[pos].0) {
                    return Some((pos, op));
                }
            }
            _ => {}
        }
        pos += 1;
    }

    None
}

/// Finds a top-level `++` or `--` before the statement semicolon.
///
/// Nested occurrences inside indexes or call arguments are ignored so expressions
/// such as `$items[$i++] = 1` remain assignment statements with an effectful index.
fn find_top_level_postfix_incdec(tokens: &[SpannedToken], start: usize) -> Option<(usize, bool)> {
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut brace_depth = 0usize;
    let mut pos = start;

    while pos < tokens.len() {
        match tokens[pos].0 {
            Token::LParen => paren_depth += 1,
            Token::RParen => paren_depth = paren_depth.saturating_sub(1),
            Token::LBracket => bracket_depth += 1,
            Token::RBracket => bracket_depth = bracket_depth.saturating_sub(1),
            Token::LBrace => brace_depth += 1,
            Token::RBrace => brace_depth = brace_depth.saturating_sub(1),
            Token::Semicolon if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 => {
                return None;
            }
            // `$k = $c->i++;` is an ASSIGNMENT whose right-hand side ends in `++`, not an
            // increment statement targeting `$k = $c->i`. Stopping at a top-level `=` hands
            // it to the expression parser, which desugars the increment in place; without
            // this the whole line was claimed here and rejected as an invalid target.
            Token::Assign if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 => {
                return None;
            }
            // A top-level `?` opens a ternary, so the `++` sits in one of its BRANCHES:
            // `$c ? $a[0]++ : $b;` increments `$a[0]`, it does not increment `$c ? $a[0]`.
            // Claiming it anyway made the scan parse `$c ? $a[0]` as its target and report
            // `Expected ':' in ternary operator` from the middle of a statement the scan had
            // truncated itself (issue #1139). Declining leaves the honest diagnostic, which is
            // that a ternary at statement position is not parsed at all yet (#827, #841).
            // `??` and `?->` are their own tokens, so neither reaches this arm.
            Token::Question if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 => {
                return None;
            }
            Token::PlusPlus if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 => {
                return Some((pos, true));
            }
            Token::MinusMinus if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 => {
                return Some((pos, false));
            }
            _ => {}
        }
        pos += 1;
    }

    None
}

/// Returns `true` if the expression is safe to replay as an l-value in a compound assignment,
/// meaning its value can be read multiple times without observable side effects.
/// Replayable expressions include variables, literals, property/static-property access on
/// replayable bases, and recursively their sub-expressions.
/// Function calls, new[], and most other `ExprKind` variants return `false`.
pub(crate) fn can_replay_assignment_target(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Variable(_) | ExprKind::This | ExprKind::StaticPropertyAccess { .. } => true,
        ExprKind::ArrayAccess { array, index } => {
            can_replay_assignment_target(array) && can_replay_assignment_target(index)
        }
        ExprKind::PropertyAccess { object, .. } => can_replay_assignment_target(object),
        ExprKind::DynamicPropertyAccess { object, property } => {
            can_replay_assignment_target(object) && can_replay_assignment_target(property)
        }
        ExprKind::BinaryOp { left, right, .. } => {
            can_replay_assignment_target(left) && can_replay_assignment_target(right)
        }
        ExprKind::InstanceOf { value, target } => {
            can_replay_assignment_target(value) && can_replay_instanceof_target(target)
        }
        ExprKind::Negate(value)
        | ExprKind::Not(value)
        | ExprKind::BitNot(value)
        | ExprKind::Cast { expr: value, .. }
        | ExprKind::PtrCast { expr: value, .. }
        | ExprKind::NamedArg { value, .. }
        | ExprKind::Spread(value) => can_replay_assignment_target(value),
        ExprKind::NullCoalesce { value, default } | ExprKind::ShortTernary { value, default } => {
            can_replay_assignment_target(value) && can_replay_assignment_target(default)
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            can_replay_assignment_target(condition)
                && can_replay_assignment_target(then_expr)
                && can_replay_assignment_target(else_expr)
        }
        ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::StringLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::ConstRef(_)
        | ExprKind::ClassConstant { .. }
        | ExprKind::ScopedConstantAccess { .. }
        | ExprKind::MagicConstant(_) => true,
        _ => false,
    }
}

/// Returns whether an update's array base is one whose single dimension gets an index snapshot.
///
/// A local, a property, or a static property is written by the statement itself, so its key
/// can be captured before the read. Deeper places keep their own nested-write handling.
pub(crate) fn update_dimension_base_is_snapshotted(array: &Expr) -> bool {
    matches!(
        array.kind,
        ExprKind::Variable(_) | ExprKind::PropertyAccess { .. } | ExprKind::StaticPropertyAccess { .. }
    )
}

/// Returns whether an update must capture an index before its read and write halves.
pub(crate) fn update_index_needs_snapshot(index: &Expr) -> bool {
    match &index.kind {
        ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::StringLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null => false,
        ExprKind::Variable(name) if crate::names::is_generated_local_name(name) => false,
        _ => true,
    }
}

/// Recursively checks whether `target` can be used as an r-value in a replay-safe assignment.
fn can_replay_instanceof_target(target: &InstanceOfTarget) -> bool {
    match target {
        InstanceOfTarget::Name(_) => true,
        InstanceOfTarget::Generic(_) => true,
        InstanceOfTarget::Expr(expr) => can_replay_assignment_target(expr),
    }
}

/// Lowers a compound postfix assignment (e.g., `+=`, `-=`) on a non-replayable l-value
/// by extracting sub-expressions into temporary variables so each is evaluated exactly once.
/// Builds a `Synthetic` statement containing the temporaries followed by the final assignment.
fn lower_effectful_postfix_assignment(
    lhs_expr: Expr,
    op: AssignmentOperator,
    rhs: Expr,
    span: Span,
) -> Result<Stmt, CompileError> {
    let mut lowerer = EffectfulTargetLowerer::new(span);
    let lowered = match lhs_expr.kind {
        ExprKind::ArrayAccess { array, index } => match array.kind {
            ExprKind::Variable(array) => {
                let index = lowerer.stabilize(*index);
                let target = Expr::new(
                    ExprKind::ArrayAccess {
                        array: Box::new(Expr::new(ExprKind::Variable(array.clone()), span)),
                        index: Box::new(index.clone()),
                    },
                    span,
                );
                let value = assignment_value(target, op, rhs, span);
                StmtKind::ArrayAssign {
                    array,
                    index,
                    value,
                }
            }
            ExprKind::PropertyAccess { object, property } => {
                let object = Box::new(lowerer.stabilize(*object));
                let index = lowerer.stabilize(*index);
                let target = Expr::new(
                    ExprKind::ArrayAccess {
                        array: Box::new(Expr::new(
                            ExprKind::PropertyAccess {
                                object: object.clone(),
                                property: property.clone(),
                            },
                            span,
                        )),
                        index: Box::new(index.clone()),
                    },
                    span,
                );
                let value = assignment_value(target, op, rhs, span);
                StmtKind::PropertyArrayAssign {
                    object,
                    property,
                    index,
                    value,
                }
            }
            // `++self::$a[f('k')];`: the index is evaluated once into a hidden local.
            ExprKind::StaticPropertyAccess { receiver, property } => {
                let index = lowerer.stabilize(*index);
                let target = Expr::new(
                    ExprKind::ArrayAccess {
                        array: Box::new(Expr::new(
                            ExprKind::StaticPropertyAccess {
                                receiver: receiver.clone(),
                                property: property.clone(),
                            },
                            span,
                        )),
                        index: Box::new(index.clone()),
                    },
                    span,
                );
                let value = assignment_value(target, op, rhs, span);
                StmtKind::StaticPropertyArrayAssign {
                    receiver,
                    property,
                    index,
                    value,
                }
            }
            _ => {
                let target = lowerer.stabilize_array_target(Expr::new(
                    ExprKind::ArrayAccess { array, index },
                    span,
                ));
                let value = assignment_value(target.clone(), op, rhs, span);
                StmtKind::NestedArrayAssign { target, value }
            }
        },
        ExprKind::PropertyAccess { object, property } => {
            let object = Box::new(lowerer.stabilize(*object));
            let target = Expr::new(
                ExprKind::PropertyAccess {
                    object: object.clone(),
                    property: property.clone(),
                },
                span,
            );
            let value = assignment_value(target, op, rhs, span);
            StmtKind::PropertyAssign {
                object,
                property,
                value,
            }
        }
        _ => return Err(CompileError::new(span, "Invalid assignment target")),
    };
    Ok(lowerer.finish(lowered))
}

/// Lowers discarded post-increment/decrement to the existing assignment statement forms.
///
/// Statement position discards the operator's value, so prefix `++$obj->n;` lowers through
/// here too: with the result unused, `++X` and `X++` are both `X += 1`.
pub(crate) fn lower_postfix_incdec_assignment(
    lhs_expr: Expr,
    is_increment: bool,
    span: Span,
) -> Result<Stmt, CompileError> {
    crate::parser::expr::reject_dynamic_member_write(&lhs_expr, span)?;
    let op = if is_increment {
        AssignmentOperator::Compound(BinOp::Add)
    } else {
        AssignmentOperator::Compound(BinOp::Sub)
    };
    let one = Expr::new(ExprKind::IntLiteral(1), span);

    if !can_replay_assignment_target(&lhs_expr) {
        return lower_effectful_postfix_assignment(lhs_expr, op, one, span);
    }

    let mut lowerer = EffectfulTargetLowerer::new(span);
    let lhs_expr = lowerer.snapshot_update_dimension(lhs_expr);
    let value = assignment_value(lhs_expr.clone(), op, one, span);
    let kind = match lhs_expr.kind {
        ExprKind::ArrayAccess { array, index } => match array.kind {
            ExprKind::Variable(array) => StmtKind::ArrayAssign {
                array,
                index: *index,
                value,
            },
            ExprKind::PropertyAccess { object, property } => StmtKind::PropertyArrayAssign {
                object,
                property,
                index: *index,
                value,
            },
            // `++self::$rand[$i];` / `static::$n[$k]--;`
            ExprKind::StaticPropertyAccess { receiver, property } => {
                StmtKind::StaticPropertyArrayAssign {
                    receiver,
                    property,
                    index: *index,
                    value,
                }
            }
            _ => StmtKind::NestedArrayAssign {
                target: Expr::new(ExprKind::ArrayAccess { array, index }, span),
                value,
            },
        },
        ExprKind::PropertyAccess { object, property } => StmtKind::PropertyAssign {
            object,
            property,
            value,
        },
        ExprKind::StaticPropertyAccess { receiver, property } => {
            StmtKind::StaticPropertyAssign {
                receiver,
                property,
                value,
            }
        }
        _ => return Err(CompileError::new(span, "Invalid increment target")),
    };

    Ok(lowerer.finish_if_used(kind, span))
}

/// Lowers a compound static property assignment where the target cannot be replayed safely.
/// Temporaries are created for any sub-expression that could produce observable side effects
/// (e.g., method calls on the object or array accesses). Returns a `Synthetic` statement.
fn lower_effectful_static_assignment(
    lhs_expr: Expr,
    op: AssignmentOperator,
    rhs: Expr,
    span: Span,
) -> Result<Stmt, CompileError> {
    let mut lowerer = EffectfulTargetLowerer::new(span);
    let lowered = match lhs_expr.kind {
        ExprKind::ArrayAccess { array, index } => match array.kind {
            ExprKind::StaticPropertyAccess { receiver, property } => {
                let index = lowerer.stabilize(*index);
                let target = Expr::new(
                    ExprKind::ArrayAccess {
                        array: Box::new(Expr::new(
                            ExprKind::StaticPropertyAccess {
                                receiver: receiver.clone(),
                                property: property.clone(),
                            },
                            span,
                        )),
                        index: Box::new(index.clone()),
                    },
                    span,
                );
                let value = assignment_value(target, op, rhs, span);
                StmtKind::StaticPropertyArrayAssign {
                    receiver,
                    property,
                    index,
                    value,
                }
            }
            _ => {
                let target = lowerer.stabilize_array_target(Expr::new(
                    ExprKind::ArrayAccess { array, index },
                    span,
                ));
                let value = assignment_value(target.clone(), op, rhs, span);
                StmtKind::NestedArrayAssign { target, value }
            }
        },
        ExprKind::StaticPropertyAccess { receiver, property } => {
            let target = Expr::new(
                ExprKind::StaticPropertyAccess {
                    receiver: receiver.clone(),
                    property: property.clone(),
                },
                span,
            );
            let value = assignment_value(target, op, rhs, span);
            StmtKind::StaticPropertyAssign {
                receiver,
                property,
                value,
            }
        }
        _ => return Err(CompileError::new(span, "Invalid assignment target")),
    };
    Ok(lowerer.finish(lowered))
}

/// Helper that rewrites complex l-value targets into sequences of temporary-variable
/// assignments so that source evaluation order is preserved and side effects are not duplicated.
struct EffectfulTargetLowerer {
    span: Span,
    next_temp: usize,
    stmts: Vec<Stmt>,
}

impl EffectfulTargetLowerer {
    /// Initializes the lowerer with the source span used for all synthesized statements
    /// and temporary variable names.
    fn new(span: Span) -> Self {
        Self {
            span,
            next_temp: 0,
            stmts: Vec::new(),
        }
    }

    /// If `expr` is replay-safe, returns it unchanged. Otherwise, emits an `Assign`
    /// statement to a uniquely-named temporary and returns a `Variable` reference to it.
    /// Increments `next_temp` to keep temporary names unique across the same statement.
    fn stabilize(&mut self, expr: Expr) -> Expr {
        if can_replay_assignment_target(&expr) {
            return expr;
        }
        let name = self.next_temp_name();
        self.stmts.push(Stmt::new(
            StmtKind::Assign {
                name: name.clone(),
                value: expr,
            },
            self.span,
        ));
        Expr::new(ExprKind::Variable(name), self.span)
    }

    /// Captures a mutable index once for both halves of an update.
    ///
    /// Covers one dimension of a local, property, or static-property array: the read reports a
    /// float key's conversion, and the write must land on that same key even when a diagnostic
    /// handler reassigns the index variable in between.
    fn snapshot_update_dimension(&mut self, target: Expr) -> Expr {
        let span = target.span;
        match target.kind {
            ExprKind::ArrayAccess { array, index }
                if update_dimension_base_is_snapshotted(&array) =>
            {
                let index = if update_index_needs_snapshot(&index) {
                    self.stabilize_unconditionally(*index)
                } else {
                    *index
                };
                Expr::new(ExprKind::ArrayAccess {
                    array,
                    index: Box::new(index),
                }, span)
            }
            kind => Expr::new(kind, span),
        }
    }

    /// Returns a unique synthetic temporary name for this lowered statement.
    fn next_temp_name(&mut self) -> String {
        let name = crate::names::generated_local_name(&format!(
            "__elephc_compound_{}_{}_{}",
            self.span.line, self.span.col, self.next_temp
        ));
        self.next_temp += 1;
        name
    }

    /// Mints the temporary that holds the bucket of a nested append, under its own reserved
    /// prefix.
    ///
    /// The prefix is what lets IR lowering recognize a nested-append `Synthetic` group and
    /// lower it as a fused, in-place append instead of the read/copy/write-back it desugars
    /// to here. `next_temp_name`'s prefix is shared with the `.=` / `+=` desugars, which emit
    /// the same statement shapes, so it cannot serve as that signal. PHP source cannot forge
    /// either lock: the name carries `crate::names::GENERATED_LOCAL_MARKER`, whose `#` is not a
    /// legal PHP identifier character, and `StmtKind::Synthetic` has no surface syntax.
    fn next_nested_append_temp_name(&mut self) -> String {
        let name = crate::names::generated_local_name(&format!(
            "{}{}_{}_{}",
            NESTED_APPEND_TEMP_PREFIX, self.span.line, self.span.col, self.next_temp
        ));
        self.next_temp += 1;
        name
    }

    /// Stabilizes an array-access target, recursively stabilizing both the array base
    /// and the index. For simple array bases (Variable, This, StaticPropertyAccess),
    /// the array base is kept as-is; deeper bases are stabilized via `stabilize_array_base`.
    fn stabilize_array_target(&mut self, expr: Expr) -> Expr {
        let span = expr.span;
        match expr.kind {
            ExprKind::ArrayAccess { array, index } => Expr::new(
                ExprKind::ArrayAccess {
                    array: Box::new(self.stabilize_array_base(*array)),
                    index: Box::new(self.stabilize(*index)),
                },
                span,
            ),
            // `$this->{$name}[] = v`: the property itself is the appended array.
            ExprKind::DynamicPropertyAccess { .. } => self.stabilize_array_base(expr),
            _ => self.stabilize(expr),
        }
    }

    /// Stabilizes the base of a nested array access chain. Recursively processes
    /// `ArrayAccess` and `PropertyAccess` chains; returns `Variable`, `This`,
    /// and `StaticPropertyAccess` directly; calls `stabilize` for all other expressions.
    fn stabilize_array_base(&mut self, expr: Expr) -> Expr {
        let span = expr.span;
        match expr.kind {
            ExprKind::ArrayAccess { array, index } => Expr::new(
                ExprKind::ArrayAccess {
                    array: Box::new(self.stabilize_array_base(*array)),
                    index: Box::new(self.stabilize(*index)),
                },
                span,
            ),
            ExprKind::PropertyAccess { object, property } => Expr::new(
                ExprKind::PropertyAccess {
                    object: Box::new(self.stabilize_array_base(*object)),
                    property,
                },
                span,
            ),
            ExprKind::DynamicPropertyAccess { object, property } => Expr::new(
                ExprKind::DynamicPropertyAccess {
                    object: Box::new(self.stabilize_array_base(*object)),
                    property: Box::new(self.stabilize(*property)),
                },
                span,
            ),
            ExprKind::Variable(_) | ExprKind::This | ExprKind::StaticPropertyAccess { .. } => expr,
            _ => self.stabilize(expr),
        }
    }

    /// Appends `final_stmt` as the last statement and wraps the entire sequence
    /// in a `Synthetic` statement node returned as a single `Stmt`.
    fn finish(mut self, final_stmt: StmtKind) -> Stmt {
        self.stmts.push(Stmt::new(final_stmt, self.span));
        Stmt::new(StmtKind::Synthetic(self.stmts), self.span)
    }

    /// Returns `final_stmt` unwrapped when nothing was hoisted, so a caller that only
    /// SOMETIMES needs a temporary does not wrap every other statement in a `Synthetic`
    /// group it has no use for.
    fn finish_if_used(self, final_stmt: StmtKind, span: Span) -> Stmt {
        if self.stmts.is_empty() {
            return Stmt::new(final_stmt, span);
        }
        self.finish(final_stmt)
    }

    /// Emits `expr` into a fresh temporary and returns a reference to it, even when the
    /// expression is replay-safe. `stabilize` exists to make an effectful expression
    /// evaluate ONCE; this exists to make it evaluate EARLY.
    fn stabilize_unconditionally(&mut self, expr: Expr) -> Expr {
        let name = self.next_temp_name();
        self.stmts.push(Stmt::new(
            StmtKind::Assign {
                name: name.clone(),
                value: expr,
            },
            self.span,
        ));
        Expr::new(ExprKind::Variable(name), self.span)
    }
}

/// Builds the statement for `<container>[] = <value>;` (`lhs_expr` is the container).
pub(super) fn push_statement(lhs_expr: Expr, rhs: Expr, span: Span) -> Result<Stmt, CompileError> {
    let lhs_span = lhs_expr.span;
    let stmt = match lhs_expr.kind {
        ExprKind::Variable(array) => StmtKind::ArrayPush { array, value: rhs },
        ExprKind::PropertyAccess { object, property } => StmtKind::PropertyArrayPush {
            object,
            property,
            value: rhs,
        },
        ExprKind::ArrayAccess { array, index } => {
            return lower_nested_append_assignment(
                Expr::new(ExprKind::ArrayAccess { array, index }, lhs_span),
                rhs,
                span,
            );
        }
        // `$this[] = v;` on an ArrayAccess `$this` is `$this->offsetSet(null, v)`, which the
        // local push lowering dispatches for an object receiver.
        ExprKind::This => StmtKind::ArrayPush { array: "this".to_string(), value: rhs },
        // `$this->{$property}[] = v;` (Mailable): pushes onto the slot the name selects.
        ExprKind::DynamicPropertyAccess { object, property } => {
            dynamic_property_push_stmt(*object, *property, rhs, span)
        }
        // `call()[] = v;`: the expression form already appends to a call temporary.
        other => {
            let append = crate::parser::expr::append_assignment_expr(
                Expr::new(other, lhs_span),
                rhs,
                span,
            )?;
            StmtKind::ExprStmt(append)
        }
    };
    Ok(Stmt::new(stmt, span))
}
