//! Purpose:
//! Desugars writes whose l-value holds an append dimension (`[]`) somewhere other than at the
//! very end of a variable/property chain: `$a['k'][]['x'] = $v`, `$this[] = $v`,
//! `f()[] = $v`, and the value-position form `($a[] = $v)`.
//!
//! Called from:
//! - `crate::parser::stmt::assign::postfix::try_parse_postfix_assignment()` (statements).
//! - `crate::parser::expr::pratt` (assignment expressions whose target contains `[]`).
//!
//! Key details:
//! - Each `[]` creates a FRESH, empty element, so everything the rest of the chain writes into
//!   it is known in full: `$a['k'][]['to'][]['email'] = $v` stores exactly
//!   `['to' => [['email' => $v]]]` (a further `[]` on a fresh array lands at key 0). The desugar
//!   therefore appends that nested literal through the existing append statements, and the outer
//!   write still takes the copy-on-write-safe, auto-vivifying nested-append path.
//! - The container's non-replayable indexes are stabilized FIRST. The literal then evaluates the
//!   remaining keys left to right and the value last, matching PHP's "dimensions, then value,
//!   then write" order; the value also sees the container before the new element exists.
//! - A container that is not a storage location (`$this`, a call result) is bound to a
//!   temporary and appended through it: an `ArrayAccess` object then receives
//!   `offsetSet(null, $v)`, and an array result is written to a copy PHP discards anyway.

use crate::errors::CompileError;
use crate::lexer::{SpannedToken, Token};
use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind};
use crate::parser::expr::parse_expr;
use crate::span::Span;

use super::postfix::{lower_nested_append_assignment, EffectfulTargetLowerer};

/// Diagnostic for `$a[] = &$x`: an array element cannot alias a variable's storage here.
pub(crate) const REFERENCE_APPEND_UNSUPPORTED: &str =
    "Appending a reference (`$a[] = &$x`) is not supported: an array element cannot alias a \
     variable's storage. Append the value instead, or alias an existing element with \
     `$b =& $a[0]`";

/// Returns the token index of the first top-level `[` that is immediately closed by `]`
/// (an append dimension) inside `lhs`, ignoring brackets nested in parentheses, braces, or
/// other brackets.
pub(super) fn find_append_dimension(lhs: &[SpannedToken]) -> Option<usize> {
    let mut depth = 0usize;
    for (idx, (token, _)) in lhs.iter().enumerate() {
        match token {
            Token::LBracket if depth == 0 => {
                if matches!(lhs.get(idx + 1).map(|(token, _)| token), Some(Token::RBracket)) {
                    return Some(idx);
                }
                depth += 1;
            }
            Token::LBracket | Token::LParen | Token::LBrace => depth += 1,
            Token::RBracket | Token::RParen | Token::RBrace => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    None
}

/// Parses a run of array dimensions starting at `pos`: `[expr]` yields `Some(expr)` and `[]`
/// yields `None`. Stops at the first token that does not open a dimension.
pub(crate) fn parse_append_dimensions(
    tokens: &[SpannedToken],
    pos: &mut usize,
) -> Result<Vec<Option<Expr>>, CompileError> {
    let mut dims = Vec::new();
    while let Some((Token::LBracket, metadata)) = tokens.get(*pos) {
        let span = metadata.span;
        *pos += 1;
        if matches!(tokens.get(*pos).map(|(token, _)| token), Some(Token::RBracket)) {
            *pos += 1;
            dims.push(None);
            continue;
        }
        let index = parse_expr(tokens, pos)?;
        if !matches!(tokens.get(*pos).map(|(token, _)| token), Some(Token::RBracket)) {
            return Err(CompileError::new(span, "Expected ']'"));
        }
        *pos += 1;
        dims.push(Some(index));
    }
    Ok(dims)
}

/// Lowers the statement `container[] <dims> = value` into an append statement, wrapped in a
/// `Synthetic` group when the container needed stabilizing temporaries.
///
/// `container` is the l-value before the first `[]`; `dims` are the dimensions after it
/// (`None` for a further `[]`). An empty `dims` is a plain append to `container`.
pub(super) fn lower_append_chain_stmt(
    container: Expr,
    dims: Vec<Option<Expr>>,
    value: Expr,
    span: Span,
) -> Result<Stmt, CompileError> {
    validate_container(&container, span)?;
    let mut lowerer = EffectfulTargetLowerer::new(span);
    let container = lowerer.stabilize_array_base(container);
    let element = fresh_element_literal(dims, value, span);
    let append = append_stmt(&mut lowerer, container, element, span)?;
    Ok(lowerer.finish_stmt(append))
}

/// Lowers the assignment EXPRESSION `container[] <dims> = value` into an `Assignment` node
/// whose prelude performs the write and whose result is the assigned value, as in PHP.
pub(crate) fn lower_append_chain_expr(
    container: Expr,
    dims: Vec<Option<Expr>>,
    value: Expr,
    span: Span,
) -> Result<Expr, CompileError> {
    validate_container(&container, span)?;
    let mut lowerer = EffectfulTargetLowerer::new(span);
    let container = lowerer.stabilize_array_base(container);
    // The literal would evaluate its keys before the value; binding the keys first keeps that
    // order once the value itself is bound, which it must be to serve as the result.
    let dims = dims
        .into_iter()
        .map(|dim| dim.map(|index| lowerer.stabilize(index)))
        .collect();
    let value = lowerer.stabilize_unconditionally(value);
    let element = fresh_element_literal(dims, value.clone(), span);
    let append = append_stmt(&mut lowerer, container, element, span)?;
    lowerer.push_stmt(append);
    let result = crate::names::generated_local_name(&format!(
        "__elephc_append_result_{}_{}",
        span.line, span.col
    ));
    Ok(Expr::new(
        ExprKind::Assignment {
            target: Box::new(Expr::new(ExprKind::Variable(result), span)),
            value: Box::new(value),
            result_target: None,
            prelude: lowerer.into_stmts(),
            conditional_value_temp: None,
        },
        span,
    ))
}

/// Rejects a container PHP cannot write through: only a variable, `$this`, a property, a static
/// property, an array element of one of those, or a call result can precede `[]`. A literal or
/// an operator result (`(1 + 2)[] = 3`) is a compile-time error in PHP too.
fn validate_container(container: &Expr, span: Span) -> Result<(), CompileError> {
    let valid = match &container.kind {
        ExprKind::Variable(_)
        | ExprKind::This
        | ExprKind::PropertyAccess { .. }
        | ExprKind::StaticPropertyAccess { .. }
        | ExprKind::FunctionCall { .. }
        | ExprKind::MethodCall { .. }
        | ExprKind::StaticMethodCall { .. }
        | ExprKind::ClosureCall { .. }
        | ExprKind::ExprCall { .. } => true,
        ExprKind::ArrayAccess { array, .. } => return validate_container(array, span),
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(CompileError::new(span, "Invalid assignment target"))
    }
}

/// Builds the value a fresh element holds once `<dims> = value` has written into it.
///
/// The element starts empty, so a key dimension yields `[key => rest]` and a further `[]`
/// yields `[rest]` (key 0); with no dimensions left the element is the value itself.
fn fresh_element_literal(dims: Vec<Option<Expr>>, value: Expr, span: Span) -> Expr {
    dims.into_iter().rev().fold(value, |inner, dim| match dim {
        Some(key) => Expr::new(ExprKind::ArrayLiteralAssoc(vec![(key, inner)]), span),
        None => Expr::new(ExprKind::ArrayLiteral(vec![inner]), span),
    })
}

/// Builds the statement that appends `value` to `container`, reusing the existing append
/// statement for each storage family. A container that is not a storage location is bound to
/// a temporary first, so `$this[] = $v` and `f()[] = $v` reach the variable append path
/// (and with it `offsetSet(null, $v)` for an `ArrayAccess` object).
fn append_stmt(
    lowerer: &mut EffectfulTargetLowerer,
    container: Expr,
    value: Expr,
    span: Span,
) -> Result<Stmt, CompileError> {
    let kind = match container.kind {
        ExprKind::Variable(array) => StmtKind::ArrayPush { array, value },
        ExprKind::PropertyAccess { object, property } => StmtKind::PropertyArrayPush {
            object,
            property,
            value,
        },
        ExprKind::StaticPropertyAccess { receiver, property } => {
            StmtKind::StaticPropertyArrayPush {
                receiver,
                property,
                value,
            }
        }
        ExprKind::ArrayAccess { array, index } => {
            return lower_nested_append_assignment(
                Expr::new(ExprKind::ArrayAccess { array, index }, container.span),
                value,
                span,
            );
        }
        ExprKind::This
        | ExprKind::FunctionCall { .. }
        | ExprKind::MethodCall { .. }
        | ExprKind::StaticMethodCall { .. }
        | ExprKind::ClosureCall { .. }
        | ExprKind::ExprCall { .. } => {
            let holder = lowerer.fresh_temp_name();
            return Ok(Stmt::new(
                StmtKind::Synthetic(vec![
                    Stmt::new(
                        StmtKind::Assign {
                            name: holder.clone(),
                            value: Expr::new(container.kind, container.span),
                        },
                        span,
                    ),
                    Stmt::new(StmtKind::ArrayPush { array: holder, value }, span),
                ]),
                span,
            ));
        }
        _ => return Err(CompileError::new(span, "Invalid assignment target")),
    };
    Ok(Stmt::new(kind, span))
}
