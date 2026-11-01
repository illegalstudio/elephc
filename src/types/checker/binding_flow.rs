//! Purpose:
//! Tracks whether local bindings exist on every reaching execution path.
//!
//! Called from:
//! - Completed assignment checks, conditional flow joins and local reads.
//!
//! Key details:
//! - Binding presence is independent of first-binding depth and physical storage.
//! - Missing nullable bindings retain null in their read contract.

use super::{Checker, GuardedLocalStorage};
use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind};
use crate::types::{PhpType, TypeEnv};

impl Checker {
    /// Distinguishes raw value proofs from retained boxed or tagged storage contracts.
    pub(super) fn is_raw_non_null_contract(ty: &PhpType) -> bool {
        super::stmt_check::type_is_definitely_non_null(ty)
            && !matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::TaggedScalar)
    }

    /// Seeds entry proofs from declared parameter values in this body's actual environment.
    pub(super) fn seed_parameter_value_proofs(&mut self, env: &TypeEnv) {
        let parameters: Vec<_> = self.local_parameter_names.iter()
            .filter(|name| !self.boxed_parameter_locals.contains(*name)
                && self.local_flow_read_is_stable(name))
            .filter_map(|name| env.get(name).filter(|ty| Self::is_raw_non_null_contract(ty))
                .map(|ty| (name.clone(), ty.clone()))).collect();
        for (name, ty) in parameters {
            self.guarded_union_types.insert(name, GuardedLocalStorage { ty, coalesced: false });
        }
    }

    /// Records bindings only after the complete assignment operation has succeeded.
    pub(super) fn record_completed_binding(&mut self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::Assign { name, .. } | StmtKind::TypedAssign { name, .. }
            | StmtKind::StaticVar { name, .. } => { self.definitely_bound_locals.insert(name.clone()); }
            StmtKind::RefAssign { target, .. } => { self.definitely_bound_locals.insert(target.clone()); }
            StmtKind::ArrayAssign { array, .. } | StmtKind::ArrayPush { array, .. } => {
                self.definitely_bound_locals.insert(array.clone());
            }
            StmtKind::ListUnpack { vars, .. } | StmtKind::Global { vars } => {
                self.definitely_bound_locals.extend(vars.iter().cloned());
            }
            StmtKind::NestedArrayAssign { target, .. } => self.record_bound_target(target),
            _ => {}
        }
    }

    /// Follows indexed lvalues to the local created by a completed write.
    fn record_bound_target(&mut self, target: &Expr) {
        match &target.kind {
            ExprKind::Variable(name) => { self.definitely_bound_locals.insert(name.clone()); }
            ExprKind::ArrayAccess { array, .. } => self.record_bound_target(array),
            _ => {}
        }
    }

    /// Recognizes physical contracts whose missing binding is represented by PHP null.
    pub(super) fn binding_storage_admits_null(ty: &PhpType) -> bool {
        matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::TaggedScalar)
            || matches!(ty, PhpType::Union(members) if members.contains(&PhpType::Void))
    }

    /// Keeps a missing-binding path observable through storage that can represent null.
    pub(super) fn binding_read_type(&self, name: &str, read: &PhpType, env: &TypeEnv) -> PhpType {
        if name.starts_with('\u{1}') || super::builtins::is_callback_argument_placeholder(name)
            || self.definitely_bound_locals.contains(name)
        { return read.clone(); }
        let physical = self.guarded_local_storage_type(&Expr::var(name), env).unwrap_or(read);
        if Self::binding_storage_admits_null(physical) {
            self.join_flow_views(read, &PhpType::Void)
        } else { read.clone() }
    }

    /// Retracts non-null read views after paths with missing nullable bindings are joined.
    pub(super) fn publish_binding_read_views(&mut self, env: &mut TypeEnv) {
        let changed: Vec<_> = env.iter().filter(|(name, _)| !name.starts_with('\u{1}'))
            .filter_map(|(name, read)| {
                let joined = self.binding_read_type(name, read, env);
                (joined != *read).then(|| (name.clone(), joined))
            }).collect();
        for (name, read) in changed { env.insert(name, read); }
    }
}
