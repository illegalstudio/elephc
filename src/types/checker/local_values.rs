//! Purpose:
//! Carries source-point local value contracts into EIR without changing storage.
//!
//! Called from:
//! - Variable inference and by-value closure capture checking.
//!
//! Key details:
//! - Conflicting visits join conservatively at that scoped local use.
//! - Storage discovery probes do not publish executable read facts.

use super::Checker;
use crate::parser::ast::Expr;
use crate::span::Span;
use crate::types::{PhpType, TypeEnv};

impl Checker {
    /// Records a value contract and reports whether every authoritative visit proves an integer.
    pub(crate) fn record_local_value_read(&mut self, name: &str, span: Span, ty: &PhpType, env: &TypeEnv) -> bool {
        if !span.identifies_a_node() { return false; }
        if self.member_receiver_analysis_scopes.iter().any(|(function, scope)| {
            function == &self.current_function
                && crate::types::loop_storage_scope_is_within(&self.current_loop_storage_scope, scope)
        }) { return false; }
        let integer = *ty == PhpType::Int && self.local_flow_read_is_stable(name)
            && self.definitely_bound_locals.contains(name);
        let read = if *ty == PhpType::Int && !integer {
            self.guarded_local_storage_type(&Expr::var(name), env)
                .filter(|storage| storage.codegen_repr() == PhpType::TaggedScalar)
                .cloned().unwrap_or(PhpType::Mixed)
        } else { ty.clone() };
        let key = (self.current_loop_storage_scope.clone(), span, name.to_string());
        let read = self.local_value_read_types.get(&key).map_or_else(|| read.clone(),
            |previous| self.join_flow_views(previous, &read));
        let proven = integer && read == PhpType::Int;
        self.local_value_read_types.insert(key, read);
        proven
    }
}
