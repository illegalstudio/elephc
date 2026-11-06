//! Purpose:
//! Small expression/statement vocabulary shared by the synthetic sodium prelude builders.
//!
//! Called from:
//! - `crate::sodium_prelude::{validators, functions}`.
//!
//! Key details:
//! - Every helper returns ordinary parser AST nodes; nothing in this module parses PHP text.
//! - The names stay short because the prelude is a direct transcription of php-src's
//!   sodium adapters, and long constructors would bury the control flow being reviewed.

use crate::parser::ast::{BinOp, CastType, Expr, Stmt};
use crate::synthetic_class::*;

/// A variable read.
pub(super) fn v(name: &str) -> Expr {
    e_var(name)
}

/// A string literal.
pub(super) fn s(value: &str) -> Expr {
    e_str(value)
}

/// An integer literal.
pub(super) fn i(value: i64) -> Expr {
    e_int(value)
}

/// A boolean literal.
pub(super) fn b(value: bool) -> Expr {
    e_bool(value)
}

/// A global function call.
pub(super) fn call(name: &str, args: Vec<Expr>) -> Expr {
    e_call(name, args)
}

/// `(string) $value`.
pub(super) fn as_str(value: Expr) -> Expr {
    e_cast(CastType::String, value)
}

/// `(int) $value`.
pub(super) fn as_int(value: Expr) -> Expr {
    e_cast(CastType::Int, value)
}

/// `$left op $right`.
pub(super) fn bin(left: Expr, op: BinOp, right: Expr) -> Expr {
    e_binop(left, op, right)
}

/// Logical AND.
pub(super) fn and(left: Expr, right: Expr) -> Expr {
    e_binop(left, BinOp::And, right)
}

/// Logical OR.
pub(super) fn or(left: Expr, right: Expr) -> Expr {
    e_binop(left, BinOp::Or, right)
}

/// Logical negation.
pub(super) fn not(value: Expr) -> Expr {
    e_not(value)
}

/// String concatenation.
pub(super) fn concat(left: Expr, right: Expr) -> Expr {
    e_concat(left, right)
}

/// Concatenates every expression in order.
pub(super) fn concat_all(parts: Vec<Expr>) -> Expr {
    e_concat_all(parts)
}

/// A PHP indexed array literal.
pub(super) fn array(items: Vec<Expr>) -> Expr {
    e_array(items)
}

/// A PHP associative array literal.
pub(super) fn assoc(entries: Vec<(Expr, Expr)>) -> Expr {
    e_array_assoc(entries)
}

/// `$array[$index]`.
pub(super) fn index(array: Expr, index: Expr) -> Expr {
    e_index(array, index)
}

/// `$name = value;`
pub(super) fn assign(name: &str, value: Expr) -> Stmt {
    s_assign(name, value)
}

/// `return value;`
pub(super) fn ret(value: Expr) -> Stmt {
    s_return(value)
}

/// `expression;`
pub(super) fn expr(value: Expr) -> Stmt {
    s_expr(value)
}

/// `if (...) { ... }`
pub(super) fn if_then(condition: Expr, then_body: Vec<Stmt>) -> Stmt {
    s_if(condition, then_body, Vec::new(), None)
}

/// `throw new Class(message);`
pub(super) fn throw_new(class: &str, message: Expr) -> Stmt {
    s_throw(e_new_fq(class, vec![message]))
}
