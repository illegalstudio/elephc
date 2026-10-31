//! Purpose:
//! Carries source-point integer value proofs into EIR without changing nullable storage.
//!
//! Called from:
//! - Variable inference and by-value closure capture checking.
//!
//! Key details:
//! - Conflicting visits permanently disable projection at that scoped local use.
//! - Storage discovery probes do not publish executable read facts.

use super::Checker;
use crate::span::Span;
use crate::types::PhpType;

impl Checker {
    /// Records a value proof only when authoritative flow checks agree at every visit.
    pub(crate) fn record_integer_local_read(&mut self, name: &str, span: Span, ty: &PhpType) {
        if !span.identifies_a_node() { return; }
        if self.member_receiver_analysis_scopes.iter().any(|(function, scope)| {
            function == &self.current_function
                && crate::types::loop_storage_scope_is_within(&self.current_loop_storage_scope, scope)
        }) {
            return;
        }
        let is_integer = *ty == PhpType::Int && self.local_flow_read_is_stable(name);
        self.integer_local_read_sites
            .entry((self.current_loop_storage_scope.clone(), span, name.to_string()))
            .and_modify(|previous| *previous &= is_integer)
            .or_insert(is_integer);
    }
}
