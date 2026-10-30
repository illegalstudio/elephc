//! Purpose:
//! Joins conditional operand evaluation for nullsafe access and source-ordered match labels.
//!
//! Called from:
//! - The assignment-effects expression walker.
//!
//! Key details:
//! - A null receiver skips names and arguments; each match label can enter its arm directly.
//! - False-label effects reach later labels and the default, while arm-body effects stay separate.

use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{PhpType, TypeEnv};

use super::buffer_merges::buffer_merge_result_type;
use super::merge_match_arm_result_type;
use super::super::super::{stmt_check::type_is_definitely_non_null, Checker};

impl Checker {
    /// Keeps the skipped receiver path when nullsafe names and arguments may not execute.
    pub(super) fn infer_nullsafe_effects(&mut self, expr: &Expr, env: &mut TypeEnv) -> Result<PhpType, CompileError> {
        let (object, name, args) = match &expr.kind {
            ExprKind::NullsafePropertyAccess { object, .. } => (object.as_ref(), None, &[][..]),
            ExprKind::NullsafeDynamicPropertyAccess { object, property } => (object.as_ref(), Some(property.as_ref()), &[][..]),
            ExprKind::NullsafeMethodCall { object, args, .. } => (object.as_ref(), None, args.as_slice()),
            ExprKind::NullsafeDynamicMethodCall { object, method, args } => (object.as_ref(), Some(method.as_ref()), args.as_slice()),
            _ => unreachable!("nullsafe effects require nullsafe access"),
        };
        let object_ty = self.infer_type_with_assignment_effects(object, env)?;
        let can_skip = !type_is_definitely_non_null(&object_ty)
            || matches!(&object.kind, ExprKind::Variable(name) if !self.local_flow_read_is_stable(name));
        let evaluate = |checker: &mut Self, env: &mut TypeEnv| {
            if let Some(name) = name { checker.infer_type_with_assignment_effects(name, env)?; }
            let expanded = crate::types::call_args::expand_static_assoc_spread_args(args);
            if let ExprKind::NullsafeMethodCall { method, .. } = &expr.kind {
                checker.promote_pdo_binding_ref_storage(object, method, &expanded, env)?;
            }
            for arg in &expanded { checker.infer_type_with_assignment_effects(arg, env)?; }
            checker.infer_type(expr, env)
        };
        let result = if can_skip {
            let mut flow = self.snapshot_conditional_flow(env);
            let mut branch = env.clone();
            let result = self.with_conditional_flow_arm(&mut flow, &mut branch, evaluate)?;
            Self::merge_short_circuit_storage_effects(env, &branch);
            self.finish_conditional_flow(flow, env);
            result
        } else { evaluate(self, env)? };
        Self::purge_property_narrowings(env);
        Ok(result)
    }

    /// Types arm results from actual label-entry states without replaying their effects.
    pub(super) fn infer_match_effects(&mut self, expr: &Expr, env: &mut TypeEnv) -> Result<PhpType, CompileError> {
        let ExprKind::Match { subject, arms, default } = &expr.kind else { unreachable!("match effects require match"); };
        self.infer_type_with_assignment_effects(subject, env)?;
        let mut flow = self.snapshot_conditional_flow(env);
        let mut missed = self.snapshot_flow_exit(env);
        let mut typed_arms = Vec::new();
        for (conditions, result) in arms {
            let mut label_env = missed.views.clone();
            self.restore_flow_snapshot(&missed, &mut label_env);
            let mut entries = Vec::new();
            for condition in conditions {
                self.infer_type_with_assignment_effects(condition, &mut label_env)?;
                entries.push(self.snapshot_flow_exit(&label_env));
            }
            missed = self.snapshot_flow_exit(&label_env);
            self.observe_flow_snapshot(&mut flow, &missed, false);
            let entry = self.join_flow_snapshots(&entries).unwrap_or_else(|| missed.clone());
            let mut arm_env = entry.views.clone();
            let ty = self.with_conditional_flow_arm(&mut flow, &mut arm_env, |checker, env| {
                checker.restore_flow_snapshot(&entry, env);
                checker.infer_type_with_assignment_effects(result, env)
            })?;
            Self::merge_short_circuit_storage_effects(env, &arm_env);
            typed_arms.push((result, if matches!(result.kind, ExprKind::Throw(_)) { PhpType::Never } else { ty }));
        }
        if let Some(result) = default {
            let mut arm_env = missed.views.clone();
            let ty = self.with_conditional_flow_arm(&mut flow, &mut arm_env, |checker, env| {
                checker.restore_flow_snapshot(&missed, env);
                checker.infer_type_with_assignment_effects(result, env)
            })?;
            Self::merge_short_circuit_storage_effects(env, &arm_env);
            typed_arms.push((result.as_ref(), if matches!(result.kind, ExprKind::Throw(_)) { PhpType::Never } else { ty }));
        }
        self.finish_conditional_flow(flow, env);
        if let Some(buffer) = buffer_merge_result_type("a match", &typed_arms, expr.span)? { return Ok(buffer); }
        Ok(typed_arms.into_iter().map(|(_, ty)| ty)
            .reduce(|left, right| merge_match_arm_result_type(self, left, right)).unwrap_or(PhpType::Void))
    }
}
