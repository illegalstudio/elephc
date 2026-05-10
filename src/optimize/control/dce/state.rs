//! Purpose:
//! Handles DCE state cases.
//! Preserves observable effects while removing unreachable tails, redundant branches, or dead writes.
//!
//! Called from:
//! - `crate::optimize::control::dce`
//!
//! Key details:
//! - The pass must remain conservative around throws, finally blocks, switch fallthrough, method calls, and variable writes.

use crate::parser::ast::Expr;

#[derive(Clone, Copy)]
pub(super) enum TailSinkTarget {
    FallsThrough,
    Breaks,
}

#[derive(Clone, Default)]
pub(super) struct GuardState {
    pub(super) truthy_vars: Vec<String>,
    pub(super) falsy_vars: Vec<String>,
    pub(super) bool_true_vars: Vec<String>,
    pub(super) bool_false_vars: Vec<String>,
    pub(super) exact_guards: Vec<ExactGuard>,
    pub(super) excluded_guards: Vec<ExactGuard>,
    pub(super) condition_guards: Vec<ConditionGuard>,
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct ExactGuard {
    pub(super) name: String,
    pub(super) value: GuardLiteral,
}

#[derive(Clone)]
pub(super) struct ConditionGuard {
    pub(super) condition: Expr,
    pub(super) value: bool,
    pub(super) names: Vec<String>,
}

#[derive(Clone, PartialEq, Eq)]
pub(super) enum GuardLiteral {
    Bool(bool),
    Null,
    Int(i64),
    Float(u64),
    String(String),
}
