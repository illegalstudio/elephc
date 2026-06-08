//! Purpose:
//! Flow-sensitive type narrowing for `if`/`else` branches guarded by type predicates.
//! Narrows a union- or mixed-typed variable to the guarded type in the matching branch.
//!
//! Called from:
//! - `crate::types::checker::stmt_check::control_flow` when checking `StmtKind::If`.
//!
//! Key details:
//! - Recognizes `is_int`/`is_float`/`is_string`/`is_bool($var)` (and aliases) and `$var instanceof
//!   Class` guards, optionally negated with a leading `!`. Narrowing is applied to each clause in an
//!   if/elseif*/else chain (each subsequent clause, and the else, see the accumulated complement
//!   from previous guards). For a chain with no else where *every* clause body always diverges
//!   (return/throw/exit/die/never-function), the accumulated complement is applied to the statements
//!   after the entire if construct.
//! - Conservative: a concrete (non-union, non-mixed) type is left unchanged, and an empty narrowing
//!   result falls back to the original type, so valid code is never narrowed away to `Never`.

use crate::parser::ast::{Expr, ExprKind, InstanceOfTarget, Stmt, StmtKind};
use crate::types::{PhpType, TypeEnv};

use super::super::Checker;

/// A detected type-guard narrowing: the guarded variable and the types it takes in the
/// then-branch (guard true) and else-branch (guard false).
pub(crate) struct GuardNarrowing {
    /// Name of the guarded variable (without the leading `$`).
    pub var: String,
    /// Type the variable has where the guard is true.
    pub then_ty: PhpType,
    /// Type the variable has where the guard is false.
    pub else_ty: PhpType,
}

impl Checker {
    /// Detects a type-predicate guard in an `if` condition and computes the then/else narrowing
    /// for the guarded variable against the current environment. Handles the scalar `is_*`
    /// predicates and `$var instanceof Class`, with an optional leading `!` that swaps the
    /// branches. Returns `None` when the condition is not a recognized single-variable guard or the
    /// variable has no known type in `env`.
    pub(crate) fn type_guard_narrowing(
        &self,
        condition: &Expr,
        env: &TypeEnv,
    ) -> Option<GuardNarrowing> {
        let (cond, negated) = match &condition.kind {
            ExprKind::Not(inner) => (inner.as_ref(), true),
            _ => (condition, false),
        };
        let (var, target) = guard_var_and_type(cond)?;
        let current = env.get(&var)?.clone();
        let matched = self.narrow_to(&current, &target);
        let complement = self.narrow_complement(&current, &target);
        let (then_ty, else_ty) = if negated {
            (complement, matched)
        } else {
            (matched, complement)
        };
        Some(GuardNarrowing { var, then_ty, else_ty })
    }

    /// Narrows `current` to the guard-true type. Inside the branch the guard guarantees the target,
    /// so `Mixed` and any incompatible concrete type become `target`; a `Union` keeps only its
    /// matching members (falling back to `target` if none match); a concrete type already matching
    /// the guard is kept as-is (preserving a more specific class for `instanceof`).
    fn narrow_to(&self, current: &PhpType, target: &PhpType) -> PhpType {
        match current {
            PhpType::Union(members) => {
                let kept: Vec<PhpType> =
                    members.iter().filter(|m| guard_matches(m, target)).cloned().collect();
                if kept.is_empty() {
                    target.clone()
                } else {
                    self.normalize_union_type(kept)
                }
            }
            _ if guard_matches(current, target) => current.clone(),
            _ => target.clone(),
        }
    }

    /// Narrows `current` to the subset incompatible with `target` (the guard-false type): a `Union`
    /// drops its matching members, while `Mixed` and concrete types are returned unchanged (the
    /// complement of `Mixed` is not representable). An empty result falls back to `current`.
    fn narrow_complement(&self, current: &PhpType, target: &PhpType) -> PhpType {
        match current {
            PhpType::Union(members) => {
                let kept: Vec<PhpType> =
                    members.iter().filter(|m| !guard_matches(m, target)).cloned().collect();
                if kept.is_empty() {
                    current.clone()
                } else {
                    self.normalize_union_type(kept)
                }
            }
            _ => current.clone(),
        }
    }

    /// Returns true when a statement body always diverges.
    ///
    /// A body is considered diverging if its last statement is:
    /// - `return` or `throw`
    /// - a call to `exit()` or `die()`
    /// - a call to a user function whose declared return type is `never`
    ///
    /// This is used by type narrowing so that an `if (guard) { ... diverging ... }` (with no else)
    /// allows the statements *after* the if to be narrowed to the complement type.
    pub(crate) fn body_always_diverges(&self, body: &[Stmt]) -> bool {
        let Some(last) = body.last() else {
            return false;
        };

        match &last.kind {
            StmtKind::Return(_) | StmtKind::Throw(_) => true,
            StmtKind::ExprStmt(expr) => self.expr_always_diverges(expr),
            _ => false,
        }
    }

    /// Returns true if the expression is known to never return normally: a call to `exit()` or
    /// `die()` (recognized by name), or a call to a user function whose declared return type is
    /// `never`. The function name is resolved case-insensitively against the checker's function
    /// table, matching PHP's call semantics.
    fn expr_always_diverges(&self, expr: &Expr) -> bool {
        let ExprKind::FunctionCall { name, .. } = &expr.kind else {
            return false;
        };
        let lowered = name.to_ascii_lowercase();
        if lowered == "exit" || lowered == "die" {
            return true;
        }
        self.canonical_function_name_folded(name)
            .and_then(|canonical| self.functions.get(&canonical))
            .map(|sig| sig.return_type == PhpType::Never)
            .unwrap_or(false)
    }
}

/// Extracts the guarded variable name and the target type from a (non-negated) guard expression.
/// Recognizes the scalar `is_*` predicates and `instanceof <Name>`; returns `None` for anything
/// else (including guards on non-variable operands).
fn guard_var_and_type(cond: &Expr) -> Option<(String, PhpType)> {
    match &cond.kind {
        ExprKind::FunctionCall { name, args } if args.len() == 1 => {
            let ExprKind::Variable(var) = &args[0].kind else {
                return None;
            };
            let target = match name.as_str().to_ascii_lowercase().as_str() {
                "is_int" | "is_integer" | "is_long" => PhpType::Int,
                "is_float" | "is_double" => PhpType::Float,
                "is_string" => PhpType::Str,
                "is_bool" => PhpType::Bool,
                _ => return None,
            };
            Some((var.clone(), target))
        }
        ExprKind::InstanceOf { value, target } => {
            let ExprKind::Variable(var) = &value.kind else {
                return None;
            };
            let InstanceOfTarget::Name(class) = target else {
                return None;
            };
            Some((var.clone(), PhpType::Object(class.as_str().to_string())))
        }
        _ => None,
    }
}

/// Returns true when a union member is compatible with a guard target, used to keep (then) or drop
/// (else) members. Scalar targets require an exact variant match; an `Object` target matches an
/// object member with the same class name (inheritance-aware narrowing is left for the future).
fn guard_matches(member: &PhpType, target: &PhpType) -> bool {
    match (member, target) {
        (PhpType::Object(member_class), PhpType::Object(target_class)) => member_class == target_class,
        _ => member == target,
    }
}
