//! Purpose:
//! Lowers a computed-key `$GLOBALS[$name]` access (php 8.1+ allows reading and writing the
//! elements of `$GLOBALS`, only replacing the whole array is an error) onto the closed world's
//! global storage.
//!
//! Called from:
//! - `crate::pipeline`, before `crate::backend_gap_prelude::inject_if_used`.
//!
//! Key details:
//! - The parser keeps a computed key as an ordinary `$GLOBALS[$key]` element access on a variable
//!   named `GLOBALS`. A literal key never reaches this pass: the parser aliases it to the global's
//!   own storage (`crate::globals_array`).
//! - A READ becomes an element of the `__elephc_globals_array()` snapshot. A WRITE (`=`, compound
//!   operators, `??=`, `++`/`--`, `[] =` and `[k] =` below the element) calls
//!   `__elephc_globals_set($key, $value)`, and `unset($GLOBALS[$key])` calls
//!   `__elephc_globals_unset($key)`. Both are declared by `crate::globals_snapshot_prelude`, which
//!   knows every global the program can name and dispatches over them.
//! - A write below the element settles the element into a hidden local, writes into that copy and
//!   stores it back, which is array value semantics.
//! - A reference to an element (`$r = &$GLOBALS[$key]`) needs the dispatch to bind a reference, which
//!   is refused loudly rather than silently bound to a copy.

use crate::errors::CompileError;
use crate::names::{Name, NameKind};
use crate::parser::ast::visit::{walk_nodes_mut, NodeMut};
use crate::parser::ast::{CastType, Expr, ExprKind, Program, Stmt, StmtKind};
use crate::span::Span;

/// The helper a computed-key write calls: `__elephc_globals_set($key, $value)` stores and
/// answers the value.
pub const SET_FUNCTION: &str = "__elephc_globals_set";

/// The helper `unset($GLOBALS[$key])` calls.
pub const UNSET_FUNCTION: &str = "__elephc_globals_unset";

/// Prefix of the hidden locals a below-the-element write settles into.
pub const TEMP_PREFIX: &str = "__elephc_gw_";

const REFERENCE_MESSAGE: &str = concat!(
    "Unsupported use of $GLOBALS: a reference to a computed-key element ",
    "($r = &$GLOBALS[$name]) is not supported"
);

const NESTED_EXPRESSION_MESSAGE: &str = concat!(
    "Unsupported use of $GLOBALS: a write below a computed-key element ",
    "($GLOBALS[$name][...] = v) is only supported as a statement"
);

/// Rewrites every computed-key `$GLOBALS[$key]` access in `program`.
pub fn lower(program: &mut Program) -> Result<(), CompileError> {
    let mut error: Option<CompileError> = None;
    let mut next_temp = 0usize;
    walk_nodes_mut(program, &mut |node| match node {
        NodeMut::Stmt(stmt) => {
            if let Err(e) = lower_stmt(stmt, &mut next_temp) {
                error.get_or_insert(e);
            }
        }
        NodeMut::Expr(expr) => {
            if let Err(e) = lower_expr(expr) {
                error.get_or_insert(e);
            }
        }
    });
    match error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

fn global_call(name: &str, args: Vec<Expr>, span: Span) -> Expr {
    Expr::new(
        ExprKind::FunctionCall {
            name: Name::from_parts(NameKind::FullyQualified, vec![name.to_string()]),
            args,
        },
        span,
    )
}

/// `$GLOBALS[$key]` on the raw variable the parser leaves for a computed key.
fn raw_element(expr: &Expr) -> Option<(&Expr, &Expr)> {
    let ExprKind::ArrayAccess { array, index } = &expr.kind else {
        return None;
    };
    matches!(&array.kind, ExprKind::Variable(name) if name == "GLOBALS").then_some((&**array, &**index))
}

/// Whether the innermost base of an element chain is a raw `$GLOBALS[$key]`.
fn chain_roots_in_globals(expr: &Expr) -> bool {
    let mut base = expr;
    loop {
        if raw_element(base).is_some() {
            return true;
        }
        match &base.kind {
            ExprKind::ArrayAccess { array, .. } => base = array,
            _ => return false,
        }
    }
}

fn lower_expr(expr: &mut Expr) -> Result<(), CompileError> {
    let span = expr.span;
    match &mut expr.kind {
        // A read: an element of the snapshot.
        ExprKind::ArrayAccess { array, .. } => {
            if matches!(&array.kind, ExprKind::Variable(name) if name == "GLOBALS") {
                **array = crate::globals_array::snapshot_call(array.span);
            }
        }
        ExprKind::Assignment { target, value, prelude, .. } => {
            if let Some((_, key)) = raw_element(target) {
                if !prelude.is_empty() {
                    return Err(CompileError::new(span, NESTED_EXPRESSION_MESSAGE));
                }
                let key = key.clone();
                let value = (**value).clone();
                *expr = global_call(SET_FUNCTION, vec![key, value], span);
            } else if chain_roots_in_globals(target) {
                return Err(CompileError::new(span, NESTED_EXPRESSION_MESSAGE));
            }
        }
        _ => {}
    }
    Ok(())
}

fn lower_stmt(stmt: &mut Stmt, next_temp: &mut usize) -> Result<(), CompileError> {
    let span = stmt.span;
    match &mut stmt.kind {
        StmtKind::ArrayAssign { array, index, value } if array == "GLOBALS" => {
            let call = global_call(SET_FUNCTION, vec![index.clone(), value.clone()], span);
            stmt.kind = StmtKind::ExprStmt(call);
        }
        StmtKind::NestedArrayAssign { target, value } if chain_roots_in_globals(target) => {
            *next_temp += 1;
            let key_temp = format!("{TEMP_PREFIX}{next_temp}_k");
            let value_temp = format!("{TEMP_PREFIX}{next_temp}_v");
            let mut target = target.clone();
            let key = take_root_key(&mut target, &value_temp);
            let key_expr = key.ok_or_else(|| CompileError::new(span, NESTED_EXPRESSION_MESSAGE))?;
            let var = |name: &str| Expr::new(ExprKind::Variable(name.to_string()), span);
            let current = Expr::new(
                ExprKind::NullCoalesce {
                    value: Box::new(Expr::new(
                        ExprKind::ArrayAccess {
                            array: Box::new(crate::globals_array::snapshot_call(span)),
                            index: Box::new(var(&key_temp)),
                        },
                        span,
                    )),
                    default: Box::new(Expr::new(ExprKind::Null, span)),
                },
                span,
            );
            let key_as_string = Expr::new(
                ExprKind::Cast { target: CastType::String, expr: Box::new(key_expr) },
                span,
            );
            let body = vec![
                Stmt::new(StmtKind::Assign { name: key_temp.clone(), value: key_as_string }, span),
                Stmt::new(StmtKind::Assign { name: value_temp.clone(), value: current }, span),
                Stmt::new(
                    StmtKind::NestedArrayAssign { target, value: value.clone() },
                    span,
                ),
                Stmt::new(
                    StmtKind::ExprStmt(global_call(
                        SET_FUNCTION,
                        vec![var(&key_temp), var(&value_temp)],
                        span,
                    )),
                    span,
                ),
            ];
            stmt.kind = StmtKind::Synthetic(body);
        }
        StmtKind::RefAssign { source, .. } if chain_roots_in_globals(source) => {
            return Err(CompileError::new(span, REFERENCE_MESSAGE));
        }
        StmtKind::ExprStmt(expr) => {
            if let ExprKind::FunctionCall { name, args } = &expr.kind {
                if name.as_str().trim_start_matches('\\').eq_ignore_ascii_case("unset")
                    && args.iter().any(chain_roots_in_globals)
                {
                    let mut body = Vec::new();
                    for arg in args {
                        match raw_element(arg) {
                            Some((_, key)) => body.push(Stmt::new(
                                StmtKind::ExprStmt(global_call(UNSET_FUNCTION, vec![key.clone()], span)),
                                span,
                            )),
                            None if chain_roots_in_globals(arg) => {
                                // `unset($GLOBALS[$k][...])`: unset below the element.
                                return Err(CompileError::new(span, NESTED_EXPRESSION_MESSAGE));
                            }
                            None => body.push(Stmt::new(
                                StmtKind::ExprStmt(Expr::new(
                                    ExprKind::FunctionCall { name: name.clone(), args: vec![arg.clone()] },
                                    span,
                                )),
                                span,
                            )),
                        }
                    }
                    stmt.kind = StmtKind::Synthetic(body);
                }
            }
        }
        _ => {}
    }
    Ok(())
}

/// Replaces the raw `$GLOBALS[$key]` at the root of `target` with `$<value_temp>` and returns its key.
fn take_root_key(target: &mut Expr, value_temp: &str) -> Option<Expr> {
    if let Some((_, key)) = raw_element(target) {
        let key = key.clone();
        *target = Expr::new(ExprKind::Variable(value_temp.to_string()), target.span);
        return Some(key);
    }
    match &mut target.kind {
        ExprKind::ArrayAccess { array, .. } => take_root_key(array, value_temp),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lowered(source: &str) -> Result<Program, CompileError> {
        let tokens = crate::lexer::tokenize(source).unwrap();
        let mut program = crate::parser::parse(&tokens).unwrap();
        lower(&mut program).map(|_| program)
    }

    /// A reference to a computed-key element would bind to a copy, so it is refused.
    #[test]
    fn reference_to_computed_key_element_is_refused() {
        let error = lowered("<?php function f($k) { $r = &$GLOBALS[$k]; }").unwrap_err();
        assert!(error.message.contains("reference to a computed-key element"), "{}", error.message);
    }

    /// A computed-key write and unset become the helper calls.
    #[test]
    fn computed_key_write_calls_the_store_helper() {
        let program = lowered("<?php function f($k) { $GLOBALS[$k] = 1; unset($GLOBALS[$k]); }").unwrap();
        let mut calls = Vec::new();
        crate::parser::ast::visit::walk_nodes(&program, &mut |node| {
            if let crate::parser::ast::visit::Node::Expr(Expr { kind: ExprKind::FunctionCall { name, .. }, .. }) = node {
                calls.push(name.as_str().trim_start_matches('\\').to_string());
            }
        });
        assert!(calls.contains(&SET_FUNCTION.to_string()), "{calls:?}");
        assert!(calls.contains(&UNSET_FUNCTION.to_string()), "{calls:?}");
    }
}
