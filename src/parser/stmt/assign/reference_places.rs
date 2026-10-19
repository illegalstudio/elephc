//! Purpose:
//! Parses `<place> = &<source>;` for places other than a plain local: an object property
//! (`$this->options = &$options`) or an array element at any depth (`$a[$k]['x'] = &$v`).
//!
//! Called from:
//! - `crate::parser::stmt::assign::postfix::try_parse_postfix_assignment()`.
//!
//! Key details:
//! - The result is always a `StmtKind::RefAssignPlace` whose source is a PLAIN VARIABLE. Any
//!   other reference source (`&$pipes[0]`, `&$this->stream`, `&self::$cache`, `&f()`) is first
//!   bound to a hidden local with the ordinary `$tmp = &<source>` statement, so the checker and
//!   lowering only ever see one source shape and reuse every source rule `RefAssign` has.
//! - PHP evaluates the place's sub-expressions BEFORE the source (`zend_compile_assign_ref`
//!   delay-compiles the target). When the source is hoisted, every index and receiver of the
//!   place that is not already a literal or a plain variable is evaluated into a hidden local
//!   first, so the hoist cannot reorder side effects.
//! - The hidden locals are unset at the end, which only drops their own link: the place keeps
//!   the reference set alive.

use crate::errors::CompileError;
use crate::lexer::{SpannedToken, Token};
use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind};
use crate::parser::expr::parse_expr;
use crate::span::Span;

use super::super::expect_semicolon;

/// Message for a place this lowering cannot bind by reference.
const UNSUPPORTED_PLACE: &str = "Reference assignment target must be a variable, an object property \
     (`$o->p = &$x`) or an array element (`$a[$k] = &$x`)";

/// Parses the `& <source> ;` tail of `<place> = &<source>;`; `*pos` is on the `&`.
pub(super) fn parse_place_ref_assign(
    tokens: &[SpannedToken],
    pos: &mut usize,
    target: Expr,
    span: Span,
) -> Result<Stmt, CompileError> {
    let source = parse_ref_source(tokens, pos, span)?;
    lower_place_ref_assign_source(target, source, span)
}

/// Parses the `& <source> ;` tail of `<container>[] = &<source>;`; `*pos` is on the `&`.
pub(super) fn parse_append_ref_assign(
    tokens: &[SpannedToken],
    pos: &mut usize,
    container: Expr,
    span: Span,
) -> Result<Stmt, CompileError> {
    let source = parse_ref_source(tokens, pos, span)?;
    lower_append_ref_assign(container, source, span)
}

/// Parses `& <source> ;` (`*pos` on the `&`) into a reference source, including `&$a[]`.
fn parse_ref_source(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<RefSource, CompileError> {
    debug_assert!(matches!(tokens.get(*pos), Some((Token::Ampersand, _))));
    *pos += 1;
    if let Some(container) = parse_new_element_source(tokens, pos)? {
        expect_semicolon(tokens, pos)?;
        return Ok(RefSource::NewElement(container));
    }
    let source = parse_expr(tokens, pos)?;
    crate::parser::expr::reject_dynamic_member_write(&source, span)?;
    crate::parser::expr::reject_nullsafe_write(&source, span)?;
    if !super::compound::is_valid_reference_source(&source.kind) {
        return Err(CompileError::new(
            span,
            "Reference assignment source must be a variable, array/property element, or a by-reference call",
        ));
    }
    expect_semicolon(tokens, pos)?;
    Ok(RefSource::Expr(source))
}

/// Builds the statement(s) for `<target> = &<source>` once both sides are parsed.
pub(crate) fn lower_place_ref_assign(
    target: Expr,
    source: Expr,
    span: Span,
) -> Result<Stmt, CompileError> {
    lower_place_ref_assign_source(target, RefSource::Expr(source), span)
}

/// The source of a reference binding: an ordinary reference source, or the new element of a
/// container (`&$a[]`, `&$this->list[$k][]`).
pub(crate) enum RefSource {
    Expr(Expr),
    NewElement(Expr),
}

/// Builds the statement(s) for `<target> = &<source>`, where the target is a property or an
/// element place, or `<container>[]` (`$a[] = &$x`, a NEW element of the container).
pub(crate) fn lower_place_ref_assign_source(
    target: Expr,
    source: RefSource,
    span: Span,
) -> Result<Stmt, CompileError> {
    lower_place_ref_assign_inner(PlaceTarget::Place(target), source, span)
}

/// Builds `<container>[] = &<source>;` (`$a[] = &$x`, `$o->{$k}[] = &$v`).
pub(crate) fn lower_append_ref_assign(
    container: Expr,
    source: RefSource,
    span: Span,
) -> Result<Stmt, CompileError> {
    lower_place_ref_assign_inner(PlaceTarget::NewElement(container), source, span)
}

/// A reference place: an existing place, or the new element `[]` of a container.
enum PlaceTarget {
    Place(Expr),
    NewElement(Expr),
}

fn lower_place_ref_assign_inner(
    target: PlaceTarget,
    source: RefSource,
    span: Span,
) -> Result<Stmt, CompileError> {
    match &target {
        PlaceTarget::Place(place) if !is_reference_place(place) => {
            return Err(CompileError::new(span, UNSUPPORTED_PLACE));
        }
        PlaceTarget::NewElement(container) => check_new_element_container(container, span)?,
        PlaceTarget::Place(_) => {}
    }
    let (target, source) = match (target, source) {
        (PlaceTarget::Place(target), RefSource::Expr(source))
            if matches!(source.kind, ExprKind::Variable(_)) =>
        {
            return Ok(Stmt::new(StmtKind::RefAssignPlace { target, source }, span));
        }
        other => other,
    };
    let mut prelude = Vec::new();
    let mut temps = Vec::new();
    // PHP evaluates the place's sub-expressions first, then the source, and only then fetches
    // the place for writing, which is when `[]` appends.
    let target = match target {
        PlaceTarget::Place(place) => PlaceTarget::Place(stabilize_place(place, span, &mut prelude, &mut temps)),
        PlaceTarget::NewElement(container) => {
            PlaceTarget::NewElement(stabilize_place(container, span, &mut prelude, &mut temps))
        }
    };
    let source = match source {
        RefSource::Expr(source) if matches!(source.kind, ExprKind::Variable(_)) => source,
        source => {
            let cell = crate::names::generated_local_name(&format!(
                "__elephc_ref_place_{}_{}",
                span.line, span.col
            ));
            push_local_ref_assign(&mut prelude, &mut temps, cell.clone(), source, span)?;
            temps.push(cell.clone());
            Expr::new(ExprKind::Variable(cell), span)
        }
    };
    let target = match target {
        PlaceTarget::Place(place) => place,
        PlaceTarget::NewElement(container) => {
            prelude.push(push_null(container.clone(), span)?);
            new_element_place(container)
        }
    };
    prelude.push(Stmt::new(StmtKind::RefAssignPlace { target, source }, span));
    push_unset(&mut prelude, temps, span);
    Ok(Stmt::new(StmtKind::Synthetic(prelude), span))
}

/// Builds `$target = &<source>;` for a local target, where the source may be a new element.
pub(crate) fn lower_local_ref_assign(
    target: String,
    source: RefSource,
    span: Span,
) -> Result<Stmt, CompileError> {
    match source {
        RefSource::Expr(source) if dynamic_element_source(&source).is_none() => {
            Ok(Stmt::new(StmtKind::RefAssign { target, source }, span))
        }
        source => {
            let mut prelude = Vec::new();
            let mut temps = Vec::new();
            push_local_ref_assign(&mut prelude, &mut temps, target, source, span)?;
            push_unset(&mut prelude, temps, span);
            Ok(Stmt::new(StmtKind::Synthetic(prelude), span))
        }
    }
}

/// Appends `$target = &<source>;` to `prelude`. A new-element source (`&$a[]`) creates the
/// element as null first (`$a[] = null`), exactly like PHP's fetch for writing, and then binds
/// the element `[]` created. The container's effectful sub-expressions are evaluated once.
fn push_local_ref_assign(
    prelude: &mut Vec<Stmt>,
    temps: &mut Vec<String>,
    target: String,
    source: RefSource,
    span: Span,
) -> Result<(), CompileError> {
    let source = match source {
        RefSource::Expr(source) => match dynamic_element_source(&source) {
            // `$r = &$o->{$k}[$i]`: bind a hidden local to the runtime-named property first
            // (`$c = &$o->{$k}`, which fetches it for writing), then take the element through it.
            Some(root) => {
                let root = stabilize_place(root.clone(), span, prelude, temps);
                let cell = crate::names::generated_local_name(&format!(
                    "__elephc_ref_dyn_source_{}_{}_{}",
                    span.line,
                    span.col,
                    prelude.len()
                ));
                prelude.push(Stmt::new(StmtKind::RefAssign { target: cell.clone(), source: root }, span));
                temps.push(cell.clone());
                let source = replace_dynamic_root(source, Expr::new(ExprKind::Variable(cell), span));
                stabilize_place(source, span, prelude, temps)
            }
            None => source,
        },
        RefSource::NewElement(container) => {
            check_new_element_container(&container, span)?;
            let container = stabilize_place(container, span, prelude, temps);
            prelude.push(push_null(container.clone(), span)?);
            new_element_place(container)
        }
    };
    prelude.push(Stmt::new(StmtKind::RefAssign { target, source }, span));
    Ok(())
}

/// `<container>[] = null;`
fn push_null(container: Expr, span: Span) -> Result<Stmt, CompileError> {
    super::postfix::push_statement(container, Expr::new(ExprKind::Null, span), span)
}

/// The element `<container>[] = null` just created: `[]` appends at the container's next
/// integer key, which is the last key once the append succeeded.
fn new_element_place(container: Expr) -> Expr {
    // The checker records builtin call types by span, and the statement span is shared by the
    // hidden `unset(...)` calls of the same desugar, so the key call takes the container's span.
    let span = container.span;
    let last_key = Expr::new(
        ExprKind::FunctionCall {
            name: crate::names::Name::from_parts(
                crate::names::NameKind::FullyQualified,
                vec!["array_key_last".to_string()],
            ),
            args: vec![container.clone()],
        },
        span,
    );
    let key = Expr::new(
        ExprKind::Cast {
            target: crate::parser::ast::CastType::Int,
            expr: Box::new(last_key),
        },
        span,
    );
    Expr::new(
        ExprKind::ArrayAccess {
            array: Box::new(container),
            index: Box::new(key),
        },
        span,
    )
}

/// Validates the container of a new-element reference (`&<container>[]`, `<container>[] = &`).
fn check_new_element_container(container: &Expr, span: Span) -> Result<(), CompileError> {
    crate::parser::expr::reject_dynamic_member_write(container, span)?;
    crate::parser::expr::reject_nullsafe_write(container, span)?;
    if is_reference_container(container) {
        return Ok(());
    }
    Err(CompileError::new(
        span,
        "A reference to a new array element (`&$a[]`, `$a[] = &$x`) needs a variable, property \
         or array element as its container",
    ))
}

/// `unset(<temps>);` when there are hidden locals to drop.
fn push_unset(prelude: &mut Vec<Stmt>, temps: Vec<String>, span: Span) {
    if temps.is_empty() {
        return;
    }
    let args = temps
        .into_iter()
        .map(|name| Expr::new(ExprKind::Variable(name), span))
        .collect();
    prelude.push(Stmt::new(
        StmtKind::ExprStmt(Expr::new(
            ExprKind::FunctionCall {
                name: "unset".into(),
                args,
            },
            span,
        )),
        span,
    ));
}

/// Parses `<container>[]` when it is the whole remaining reference source (`&$a[];`), leaving
/// `*pos` on the `;`. Returns `None` (and leaves `*pos` alone) for any other source.
pub(crate) fn parse_new_element_source(
    tokens: &[SpannedToken],
    pos: &mut usize,
) -> Result<Option<Expr>, CompileError> {
    let mut depth = 0usize;
    let mut end = None;
    for (index, (token, _)) in tokens.iter().enumerate().skip(*pos) {
        match token {
            Token::LParen | Token::LBracket | Token::LBrace => depth += 1,
            Token::RParen | Token::RBracket | Token::RBrace => {
                if depth == 0 {
                    return Ok(None);
                }
                depth -= 1;
            }
            Token::Semicolon if depth == 0 => {
                end = Some(index);
                break;
            }
            _ => {}
        }
    }
    let Some(end) = end else { return Ok(None) };
    if end < *pos + 3
        || tokens[end - 2].0 != Token::LBracket
        || tokens[end - 1].0 != Token::RBracket
    {
        return Ok(None);
    }
    let mut cursor = *pos;
    let container = parse_expr(&tokens[..end - 2], &mut cursor)?;
    if cursor != end - 2 {
        return Ok(None);
    }
    *pos = end;
    Ok(Some(container))
}

/// Returns whether `expr` is a place `RefAssignPlace` can bind: a property of any receiver, or
/// an element whose container chain bottoms out in a variable, `$this`, or a property.
pub(crate) fn is_reference_place(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::PropertyAccess { .. } | ExprKind::DynamicPropertyAccess { .. } => true,
        ExprKind::ArrayAccess { array, .. } => is_element_place_container(array),
        _ => false,
    }
}

/// Returns whether `expr` can hold an EXISTING element reference place: any reference container,
/// or a static property at its root (`self::$c[$d] = &self::$c[$k]`, symfony DebugClassLoader).
fn is_element_place_container(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::StaticPropertyAccess { .. } => true,
        ExprKind::ArrayAccess { array, .. } => is_element_place_container(array),
        _ => is_reference_container(expr),
    }
}

/// Returns whether `expr` can hold an element reference place.
fn is_reference_container(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Variable(_) | ExprKind::PropertyAccess { .. } | ExprKind::DynamicPropertyAccess { .. } => true,
        ExprKind::ArrayAccess { array, .. } => is_reference_container(array),
        _ => false,
    }
}

/// Evaluates every index and receiver of `place` that could observe or cause a side effect into
/// a hidden local, root first, so the later source binding cannot run before them.
pub(super) fn stabilize_place(place: Expr, span: Span, prelude: &mut Vec<Stmt>, temps: &mut Vec<String>) -> Expr {
    let place_span = place.span;
    match place.kind {
        ExprKind::ArrayAccess { array, index } => {
            let array = stabilize_place(*array, span, prelude, temps);
            let index = stabilize_key(*index, span, prelude, temps);
            Expr::new(
                ExprKind::ArrayAccess {
                    array: Box::new(array),
                    index: Box::new(index),
                },
                place_span,
            )
        }
        ExprKind::PropertyAccess { object, property } => {
            let object = match object.kind {
                ExprKind::Variable(_) | ExprKind::This => *object,
                ExprKind::PropertyAccess { .. } | ExprKind::ArrayAccess { .. } => {
                    stabilize_place(*object, span, prelude, temps)
                }
                _ => stabilize_operand(*object, span, prelude, temps),
            };
            Expr::new(
                ExprKind::PropertyAccess {
                    object: Box::new(object),
                    property,
                },
                place_span,
            )
        }
        ExprKind::DynamicPropertyAccess { object, property } => {
            let object = match object.kind {
                ExprKind::Variable(_) | ExprKind::This => *object,
                ExprKind::PropertyAccess { .. } | ExprKind::ArrayAccess { .. } => {
                    stabilize_place(*object, span, prelude, temps)
                }
                _ => stabilize_operand(*object, span, prelude, temps),
            };
            let property = stabilize_key(*property, span, prelude, temps);
            Expr::new(
                ExprKind::DynamicPropertyAccess {
                    object: Box::new(object),
                    property: Box::new(property),
                },
                place_span,
            )
        }
        kind => Expr::new(kind, place_span),
    }
}

/// Moves one operand into a hidden local unless it is already free of side effects.
fn stabilize_operand(operand: Expr, span: Span, prelude: &mut Vec<Stmt>, temps: &mut Vec<String>) -> Expr {
    stabilize_operand_as(operand, span, prelude, temps, true)
}

/// Moves a key or property-name operand into a hidden local that is NOT unset afterwards: it
/// only holds an int or a string, which its next assignment releases, while an `unset` retypes
/// its frame slot and made every later read an owned detached string the binding never freed.
fn stabilize_key(operand: Expr, span: Span, prelude: &mut Vec<Stmt>, temps: &mut Vec<String>) -> Expr {
    stabilize_operand_as(operand, span, prelude, temps, false)
}

fn stabilize_operand_as(
    operand: Expr,
    span: Span,
    prelude: &mut Vec<Stmt>,
    temps: &mut Vec<String>,
    unset_after: bool,
) -> Expr {
    if matches!(
        operand.kind,
        ExprKind::IntLiteral(_)
            | ExprKind::StringLiteral(_)
            | ExprKind::BoolLiteral(_)
            | ExprKind::Null
            | ExprKind::Variable(_)
            | ExprKind::This
    ) {
        return operand;
    }
    let name = crate::names::generated_local_name(&format!(
        "__elephc_ref_place_key_{}_{}_{}",
        span.line,
        span.col,
        prelude.len()
    ));
    let operand_span = operand.span;
    prelude.push(Stmt::new(
        StmtKind::Assign {
            name: name.clone(),
            value: operand,
        },
        span,
    ));
    if unset_after {
        temps.push(name.clone());
    }
    Expr::new(ExprKind::Variable(name), operand_span)
}

/// The runtime-named property (`$o->{$k}`) an ELEMENT source (`&$o->{$k}[$i]`) is reached
/// through, if any.
fn dynamic_element_source(source: &Expr) -> Option<&Expr> {
    let ExprKind::ArrayAccess { array, .. } = &source.kind else {
        return None;
    };
    let mut current: &Expr = array;
    loop {
        match &current.kind {
            ExprKind::DynamicPropertyAccess { .. } => return Some(current),
            ExprKind::ArrayAccess { array, .. } => current = array,
            _ => return None,
        }
    }
}

/// Replaces the runtime-named root of an element chain.
fn replace_dynamic_root(expr: Expr, root: Expr) -> Expr {
    match expr.kind {
        ExprKind::DynamicPropertyAccess { .. } => root,
        ExprKind::ArrayAccess { array, index } => Expr::new(
            ExprKind::ArrayAccess { array: Box::new(replace_dynamic_root(*array, root)), index },
            expr.span,
        ),
        kind => Expr::new(kind, expr.span),
    }
}
