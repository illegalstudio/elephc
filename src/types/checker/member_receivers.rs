//! Purpose:
//! Carry flow-sensitive member receiver types from checking into native EIR lowering.
//!
//! Called from:
//! - Property-access and method-call inference.
//!
//! Key details:
//! - Scope and source identity distinguish copied method bodies and included files.
//! - A conflicting checker visit permanently disables refinement at that use site.
//! - Storage probes omit guards in the queried body and its lexical closure descendants.

use super::Checker;
use crate::span::Span;
use crate::types::PhpType;

impl Checker {
    /// Records a receiver view only when authoritative flow-sensitive checks agree on its type.
    pub(crate) fn record_member_receiver_type(&mut self, span: Span, ty: &PhpType) {
        if !span.identifies_a_node() { return; }
        if self.member_receiver_analysis_scopes.iter().any(|(function, scope)| {
            function == &self.current_function
                && crate::types::loop_storage_scope_is_within(&self.current_loop_storage_scope, scope)
        }) {
            return;
        }
        self.member_receiver_types
            .entry((self.current_loop_storage_scope.clone(), span))
            .and_modify(|previous| {
                if previous.as_ref() != Some(ty) { *previous = None; }
            })
            .or_insert_with(|| Some(ty.clone()));
    }
}
