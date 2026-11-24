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

/// Span under which [`Checker::note_possibly_missing_read`] files its marker in the local value
/// read table: no source node has it, so no read of a real position can collide with a marker.
fn missing_read_marker_span() -> Span {
    Span::dummy()
}

/// Table key of the marker that says the checker found a read of `name` in `scope` possibly
/// unbound.
pub(crate) fn possibly_missing_read_key(scope: &str, name: &str) -> (String, Span, String) {
    (scope.to_string(), missing_read_marker_span(), name.to_string())
}

impl Checker {
    /// Files a marker for a read of `name` outside any loop that finds its binding possibly
    /// missing, so EIR lowering can give the name its definedness flag before the first lowering
    /// of the body instead of discovering the need by lowering the body twice. Only a hint: a
    /// name the checker does not file is still found, and replayed, by lowering itself.
    ///
    /// Only a name the environment already holds is filed: a name it lacks is either read before
    /// any store (lowering finds that one itself) or read by an inference probe that walks one
    /// expression without its body's bindings, which says nothing about the body's flow.
    pub(crate) fn note_possibly_missing_read(&mut self, name: &str, span: Span, env: &TypeEnv) {
        if !span.identifies_a_node()
            || !env.contains_key(name)
            || self.loop_stmt_depth != 0
            || self.definitely_bound_locals.contains(name)
            || name.starts_with('\u{1}')
            || crate::superglobals::is_superglobal(name)
            || super::builtins::is_callback_argument_placeholder(name)
            || self.member_receiver_analysis_scopes.iter().any(|(function, scope)| {
                function == &self.current_function
                    && crate::types::loop_storage_scope_is_within(&self.current_loop_storage_scope, scope)
            })
        {
            return;
        }
        let key = possibly_missing_read_key(&self.current_loop_storage_scope, name);
        self.local_value_read_types.insert(key, PhpType::Void);
    }

    /// Records a value contract and reports whether every authoritative visit proves an integer.
    pub(crate) fn record_local_value_read(&mut self, name: &str, span: Span, ty: &PhpType, env: &TypeEnv) -> bool {
        if !span.identifies_a_node() { return false; }
        if self.member_receiver_analysis_scopes.iter().any(|(function, scope)| {
            function == &self.current_function
                && crate::types::loop_storage_scope_is_within(&self.current_loop_storage_scope, scope)
        }) { return false; }
        let integer = *ty == PhpType::Int && self.local_flow_read_is_stable(name)
            && self.definitely_bound_locals.contains(name)
            && self.guarded_local_storage_type(&Expr::var(name), env)
                .is_some_and(|storage| storage.codegen_repr() == PhpType::TaggedScalar);
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
