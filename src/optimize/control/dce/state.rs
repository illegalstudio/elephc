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
/// Indicates where a dead-code tail should sink: either into the next statement
/// (`FallsThrough`) or out of a surrounding construct (`Breaks`).
pub(super) enum TailSinkTarget {
    FallsThrough,
    Breaks,
}

#[derive(Clone, Default)]
/// Tracks guard conditions discovered during DCE analysis.
///
/// `truthy_vars` / `falsy_vars` — variables known to be true/false at this point.
/// `bool_true_vars` / `bool_false_vars` — boolean-typed variables confirmed true/false.
/// `exact_guards` — variable = literal constraints currently active.
/// `excluded_guards` — variable = literal constraints ruled out at this point.
/// `condition_guards` — complex expression conditions mapped to a known boolean value
/// and the set of variables they constrain.
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
/// Records an exact constraint that a variable holds a specific literal value.
/// `name` is the variable name; `value` is the known literal.
pub(super) struct ExactGuard {
    pub(super) name: String,
    pub(super) value: GuardLiteral,
}

#[derive(Clone)]
/// Records a condition expression that evaluates to a known boolean value at this point.
/// `condition` is the expression; `value` is the known result; `names` are the variables
/// whose guard state is affected by this condition.
pub(super) struct ConditionGuard {
    pub(super) condition: Expr,
    pub(super) value: bool,
    pub(super) names: Vec<String>,
}

#[derive(Clone, PartialEq, Eq)]
/// The set of literal values a guard can constrain a variable to.
/// Used in `ExactGuard` to record variable = value constraints.
pub(super) enum GuardLiteral {
    Bool(bool),
    Null,
    Int(i64),
    Float(u64),
    String(String),
}
