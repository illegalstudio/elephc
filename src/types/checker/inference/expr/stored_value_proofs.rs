//! Purpose:
//! Proves that a completed expression cannot store PHP null into a nullable binding.
//!
//! Called from:
//! - Ordinary/coalescing assignment checks and loop-header read normalization.
//!
//! Key details:
//! - Composite results require guarantees for every returning arm, not inferred array types.
//! - Loop callers filter variable dependencies whose entry facts a body write can invalidate.

use crate::parser::ast::{BinOp, Expr, ExprKind};
use crate::types::{PhpType, PropertyNameResolution, TypeEnv};

use super::super::super::Checker;
use super::null_coalesce::{intrinsic_non_null_value, static_receiver_class_name, type_admits_null};

impl Checker {
    /// Proves a completed store without changing the binding or replaying expression effects.
    pub(in crate::types::checker) fn stored_value_is_non_null(
        &self, value: &Expr, ty: &PhpType, env: &TypeEnv,
    ) -> bool {
        self.stored_value_is_non_null_with_source_filter(value, ty, env, &|_| true)
    }

    /// Applies the same structural proof while excluding unstable loop-carried dependencies.
    pub(in crate::types::checker) fn stored_value_is_non_null_with_source_filter(
        &self, value: &Expr, ty: &PhpType, env: &TypeEnv,
        source_is_stable: &dyn Fn(&str) -> bool,
    ) -> bool {
        !type_admits_null(ty) && self.stored_value_shape_is_non_null(value, env, source_is_stable, 0)
    }

    /// Follows value-producing children, with a bound for recursive constant definitions.
    fn stored_value_shape_is_non_null(
        &self, value: &Expr, env: &TypeEnv, source_is_stable: &dyn Fn(&str) -> bool, depth: usize,
    ) -> bool {
        if depth > 128 { return false; }
        if intrinsic_non_null_value(self, value, env) || self.expr_is_declared_never_call(value) {
            return true;
        }
        let child = |expr: &Expr| self.stored_value_shape_is_non_null(expr, env, source_is_stable, depth + 1);
        match &value.kind {
            ExprKind::Cast { .. } | ExprKind::Not(_) | ExprKind::InstanceOf { .. }
                | ExprKind::Print(_) | ExprKind::Negate(_) | ExprKind::BitNot(_)
                | ExprKind::PreIncrement(_) | ExprKind::Throw(_)
                | ExprKind::ClassConstant { .. } | ExprKind::ObjectClassName { .. } => true,
            ExprKind::BinaryOp { left, op: BinOp::NullCoalesce, right } => child(left) || child(right),
            ExprKind::BinaryOp { .. } => true,
            ExprKind::Variable(name) => source_is_stable(name)
                && env.get(name).is_some_and(|ty| !type_admits_null(ty))
                && self.local_flow_read_is_stable(name) && self.definitely_bound_locals.contains(name)
                && self.guarded_local_storage_type(value, env).is_some(),
            ExprKind::Ternary { then_expr, else_expr, .. } => child(then_expr) && child(else_expr),
            ExprKind::ShortTernary { default, .. } => child(default),
            ExprKind::NullCoalesce { value, default } => child(value) || child(default),
            ExprKind::Match { arms, default, .. } => arms.iter().all(|(_, result)| child(result))
                && default.as_deref().is_none_or(child),
            ExprKind::ErrorSuppress(inner) => child(inner),
            ExprKind::Assignment { target, value, result_target: None, prelude, .. }
                if prelude.is_empty() && matches!(target.kind, ExprKind::Variable(_)) => child(value),
            ExprKind::ConstRef(name) => self.constants.get(name.as_str().trim_start_matches('\\'))
                .is_some_and(|ty| !type_admits_null(ty)),
            ExprKind::ScopedConstantAccess { receiver, name } => {
                let Some(mut owner) = static_receiver_class_name(self, receiver) else { return false; };
                if self.enums.get(&owner).is_some_and(|info| info.cases.iter().any(|case| case.name == *name)) {
                    return true;
                }
                while let Some(info) = self.classes.get(&owner) {
                    if let Some(constant) = info.constants.get(name) { return child(constant); }
                    let Some(parent) = &info.parent else { break; };
                    owner = parent.clone();
                }
                self.interfaces.get(&owner).and_then(|info| info.constants.get(name)).is_some_and(child)
            }
            ExprKind::PropertyAccess { object, property } => {
                self.declared_property_is_non_null(value, object, property, env, source_is_stable)
            }
            ExprKind::StaticPropertyAccess { receiver, property } => {
                static_receiver_class_name(self, receiver).and_then(|owner| self.classes.get(&owner))
                    .is_some_and(|info| info.declared_static_properties.contains(property)
                        && info.static_properties.iter().any(|(name, ty)| name == property && !type_admits_null(ty)))
            }
            ExprKind::FunctionCall { name, .. } if self.builtin_call_types.contains_key(&value.span) => {
                crate::builtins::registry::lookup(name.as_str()).is_some_and(|def|
                    !type_admits_null(&def.return_type) || matches!(def.spec.semantics.lowering,
                        crate::builtins::semantics::BuiltinLowering::Runtime(
                            crate::ir::RuntimeCallTarget::Function(crate::ir::RuntimeFnId::Fopen
                                | crate::ir::RuntimeFnId::ArrayValues))))
            }
            _ => false,
        }
    }

    /// Uses a declared slot contract, excluding dynamic/untyped properties and stale receivers.
    fn declared_property_is_non_null(
        &self, expr: &Expr, object: &Expr, property: &str, env: &TypeEnv,
        source_is_stable: &dyn Fn(&str) -> bool,
    ) -> bool {
        if let ExprKind::Variable(name) = &object.kind {
            if !source_is_stable(name) { return false; }
        }
        let receiver = match &object.kind {
            ExprKind::This => self.current_class.clone().map(PhpType::Object),
            ExprKind::Variable(name) => env.get(name).cloned(),
            _ => self.member_receiver_types.get(&(self.current_loop_storage_scope.clone(), expr.span))
                .and_then(Clone::clone),
        };
        let Some(receiver) = receiver else { return false; };
        let classes: Vec<_> = super::null_coalesce::union_members(&receiver).filter_map(|member| match member {
            PhpType::Object(owner) => Some(owner), _ => None,
        }).collect();
        !classes.is_empty() && classes.iter().all(|owner| {
            let resolution = crate::types::resolve_property_name(&self.classes, owner, property, self.current_class.as_deref());
            let (owner, index) = match &resolution {
                PropertyNameResolution::Visible => (*owner, None),
                PropertyNameResolution::ScopePrivate { scope, index } => (scope, Some(*index)),
                _ => return false,
            };
            self.classes.get(owner).is_some_and(|info| match index {
                Some(index) => info.property_slot_is_declared(index, property)
                    && info.properties.get(index).is_some_and(|(_, ty)| !type_admits_null(ty)),
                None => info.visible_property_is_declared(property)
                    && self.infer_property_on_class_type(owner, property, expr).is_ok_and(|ty| !type_admits_null(&ty)),
            })
        })
    }
}
