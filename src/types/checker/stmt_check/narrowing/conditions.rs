//! Purpose:
//! Composes type-guard facts for short-circuit boolean conditions.
//!
//! Called from:
//! - Statement checking and expression inference through `Checker::guard_narrowings`.
//!
//! Key details:
//! - Ambiguous outcomes join both execution paths; unknown effects invalidate earlier facts.

use super::{guard_receiver_and_target, Checker, CompileError, Expr, ExprKind, GuardNarrowing, GuardTarget};
use crate::parser::ast::BinOp;
use crate::types::{PhpType, TypeEnv};
use std::collections::HashMap;

/// Flattens one associative boolean chain without recursing through its left prefix.
fn chain_operands<'a>(condition: &'a Expr, op: &BinOp) -> Vec<&'a Expr> {
    let mut pending = vec![condition];
    let mut operands = Vec::new();
    while let Some(expr) = pending.pop() {
        if let ExprKind::BinaryOp { left, op: nested, right } = &expr.kind {
            if nested == op {
                pending.push(right);
                pending.push(left);
                continue;
            }
        }
        operands.push(expr);
    }
    operands
}

impl Checker {
    /// Proves a strict null comparison from a plain local's current read type.
    pub(in crate::types::checker) fn known_null_guard_truth(&self, condition: &Expr, env: &TypeEnv) -> Option<bool> {
        if let ExprKind::Not(inner) = &condition.kind {
            return self.known_null_guard_truth(inner, env).map(|truth| !truth);
        }
        let ExprKind::BinaryOp { op, .. } = &condition.kind else { return None; };
        if !matches!(op, BinOp::StrictEq | BinOp::StrictNotEq) { return None; }
        let (receiver, GuardTarget::Exact(PhpType::Void), negated) = guard_receiver_and_target(condition)? else { return None; };
        // Only local reads are stable and free of user-code effects at this point.
        let ExprKind::Variable(name) = &receiver.kind else { return None; };
        if !self.local_flow_read_is_stable(name) { return None; }
        let current = env.get(name)?;
        let is_null = if *current == PhpType::Void { true }
            else if super::type_is_definitely_non_null(current) { false }
            else { return None; };
        Some(is_null ^ negated)
    }

    /// Rejects type-only proofs for bindings that user code can write through hidden aliases.
    pub(in crate::types::checker) fn local_flow_read_is_stable(&self, name: &str) -> bool {
        !(self.eval_barrier_active || self.ref_aliased_locals.contains(name)
            || self.by_ref_captured_locals.contains(name)
            || self.boxed_ref_aliased_locals.contains(name)
            || self.mixed_storage_locals.contains(name)
            || self.active_ref_params.contains(name)
            || self.active_external_ref_bindings.contains(name)
            || self.active_globals.contains(name) || self.static_local_names.contains(name)
            || self.top_level_binding_is_program_global(name))
    }

    /// Computes every binding's true and false views once per boolean-chain operand.
    pub(crate) fn guard_narrowings(
        &mut self,
        condition: &Expr,
        env: &TypeEnv,
    ) -> Result<Vec<GuardNarrowing>, CompileError> {
        if let ExprKind::Not(inner) = &condition.kind {
            let mut guards = self.guard_narrowings(inner, env)?;
            for guard in &mut guards {
                std::mem::swap(&mut guard.then_ty, &mut guard.else_ty);
            }
            return Ok(guards);
        }
        let ExprKind::BinaryOp { op, .. } = &condition.kind else {
            return Ok(self.guard_narrowing(condition, env)?.into_iter().collect());
        };
        if !matches!(op, BinOp::And | BinOp::Or) {
            return Ok(self.guard_narrowing(condition, env)?.into_iter().collect());
        }
        let operands = chain_operands(condition, op);
        let mut guards = self.guard_narrowings(operands[0], env)?;
        for right in &operands[1..] {
            guards = self.compose_boolean_guards(guards, right, op, env, None)?;
        }
        Ok(guards)
    }

    /// Infers boolean operands while carrying their accumulated guard summary forward.
    ///
    /// Returning summaries from nested chains avoids recomputing every left prefix during
    /// inference. The separate statement/ternary guard query then traverses the chain once.
    pub(crate) fn infer_short_circuit_type(
        &mut self, condition: &Expr, env: &mut TypeEnv, with_effects: bool,
    ) -> Result<PhpType, CompileError> {
        self.infer_boolean_guard_summary(condition, env, with_effects)?;
        Ok(PhpType::Bool)
    }

    /// Infers a condition and returns the guard facts established by its final value.
    fn infer_boolean_guard_summary(
        &mut self, condition: &Expr, env: &mut TypeEnv, with_effects: bool,
    ) -> Result<Vec<GuardNarrowing>, CompileError> {
        if let ExprKind::Not(inner) = &condition.kind {
            let mut guards = self.infer_boolean_guard_summary(inner, env, with_effects)?;
            for guard in &mut guards {
                std::mem::swap(&mut guard.then_ty, &mut guard.else_ty);
            }
            return Ok(guards);
        }
        if let ExprKind::BinaryOp { op, .. } = &condition.kind {
            if matches!(op, BinOp::And | BinOp::Or) {
                let operands = chain_operands(condition, op);
                let mut guards = self.infer_boolean_guard_summary(operands[0], env, with_effects)?;
                for right in &operands[1..] {
                    let preserves_left = self.condition_preserves_guards(right, env);
                    let mut flow = self.snapshot_conditional_flow(env);
                    let mut right_env = env.clone();
                    let right_guards = self.with_conditional_flow_arm(&mut flow,
                        &mut right_env, |checker, env| {
                            checker.with_flow_narrowings(&guards, *op == BinOp::And, env,
                                |checker, env| checker.infer_boolean_guard_summary(right, env, with_effects))
                        })?;
                    if with_effects {
                        Self::merge_short_circuit_storage_effects(env, &right_env);
                        self.finish_conditional_flow(flow, env);
                    }
                    // Effects may invalidate the views used to check the operand's inputs.
                    // Such operands need a fresh summary from the unrefined entry environment.
                    let known_right = preserves_left.then_some(right_guards);
                    guards = self.compose_boolean_guards(guards, right, op, env, known_right)?;
                }
                return Ok(guards);
            }
        }
        if with_effects {
            self.infer_type_with_assignment_effects(condition, env)?;
        } else {
            self.infer_type(condition, env)?;
        }
        self.guard_narrowings(condition, env)
    }

    /// Joins one new operand with an already-composed boolean prefix using indexed key lookups.
    fn compose_boolean_guards(
        &mut self, left_guards: Vec<GuardNarrowing>, right: &Expr, op: &BinOp,
        env: &TypeEnv, known_right: Option<Vec<GuardNarrowing>>,
    ) -> Result<Vec<GuardNarrowing>, CompileError> {
        let preserves_left = self.condition_preserves_guards(right, env);
        let right_guards = if let Some(guards) = known_right {
            guards
        } else {
            let mut right_env = env.clone();
            if preserves_left {
                for guard in &left_guards {
                    right_env.insert(guard.var.clone(), if *op == BinOp::And {
                        guard.then_ty.clone()
                    } else {
                        guard.else_ty.clone()
                    });
                }
            }
            self.guard_narrowings(right, &right_env)?
        };
        let left: HashMap<_, _> = left_guards.into_iter().map(|guard| (guard.var.clone(), guard)).collect();
        let right: HashMap<_, _> = right_guards.into_iter().map(|guard| (guard.var.clone(), guard)).collect();
        let mut keys: Vec<_> = left.keys().chain(right.keys()).cloned().collect();
        keys.sort();
        keys.dedup();
        let mut guards = Vec::with_capacity(keys.len());
        for var in keys {
            let left = left.get(&var);
            let right = right.get(&var);
            let original = env.get(&var).cloned().unwrap_or_else(|| {
                let guard = left.or(right).expect("key collected from a guard");
                self.normalize_union_type(vec![guard.then_ty.clone(), guard.else_ty.clone()])
            });
            let left_true = left.map_or(&original, |guard| &guard.then_ty);
            let left_false = left.map_or(&original, |guard| &guard.else_ty);
            let evaluated_left = if !preserves_left { &original }
                else if *op == BinOp::And { left_true } else { left_false };
            let right_true = right.map_or(evaluated_left, |guard| &guard.then_ty);
            let right_false = right.map_or(evaluated_left, |guard| &guard.else_ty);
            let (then_ty, else_ty) = if *op == BinOp::And {
                (right_true.clone(), self.join_guard_alternatives(&original, left_false, right_false))
            } else {
                (self.join_guard_alternatives(&original, left_true, right_true), right_false.clone())
            };
            if then_ty != else_ty {
                guards.push(GuardNarrowing { var, then_ty, else_ty });
            }
        }
        Ok(guards)
    }

    /// Preserves an original object supertype when both alternatives already fit its contract.
    fn join_guard_alternatives(&self, original: &PhpType, left: &PhpType, right: &PhpType) -> PhpType {
        if left == original || right == original {
            return original.clone();
        }
        let joined = self.normalize_union_type(vec![left.clone(), right.clone()]);
        if matches!(original, PhpType::Object(_))
            && matches!(&joined, PhpType::Union(members) if members.iter().all(|member| matches!(member, PhpType::Object(_))))
            && self.type_accepts(original, &joined)
        {
            original.clone()
        } else {
            joined
        }
    }

    /// Proves a later operand cannot overwrite an earlier guard's binding or run user code.
    ///
    /// Unknown expressions are barriers. In particular, a call may write a reference argument,
    /// and a property getter may mutate another guarded place before the condition finishes.
    fn condition_preserves_guards(&mut self, expr: &Expr, env: &TypeEnv) -> bool {
        match &expr.kind {
            ExprKind::Variable(_) | ExprKind::This | ExprKind::Null
            | ExprKind::BoolLiteral(_) | ExprKind::IntLiteral(_)
            | ExprKind::FloatLiteral(_) | ExprKind::StringLiteral(_) => true,
            ExprKind::Not(inner) | ExprKind::ErrorSuppress(inner) => {
                self.condition_preserves_guards(inner, env)
            }
            ExprKind::BinaryOp { left, op, right }
                if matches!(op, BinOp::And | BinOp::Or | BinOp::StrictEq | BinOp::StrictNotEq) =>
            {
                self.condition_preserves_guards(left, env)
                    && self.condition_preserves_guards(right, env)
            }
            ExprKind::FunctionCall { .. } => {
                guard_receiver_and_target(expr).is_some_and(|(receiver, _, _)| {
                    // Only plain-local predicates are known not to invoke a getter or autoload.
                    matches!(receiver.kind, ExprKind::Variable(_))
                        && !matches!(&expr.kind, ExprKind::FunctionCall { name, .. }
                            if crate::names::php_symbol_key(name.as_str()) == "is_callable")
                })
            }
            ExprKind::PropertyAccess { object, .. } => {
                matches!(object.kind, ExprKind::Variable(_) | ExprKind::This)
                    && matches!(self.infer_type(object, env), Ok(PhpType::Object(_)))
                    && !self.property_guard_receiver_is_unstable(expr, env).unwrap_or(true)
            }
            ExprKind::StaticPropertyAccess { .. } => true,
            ExprKind::InstanceOf { value, target: crate::parser::ast::InstanceOfTarget::Name(_) } => {
                self.condition_preserves_guards(value, env)
            }
            _ => false,
        }
    }
}
