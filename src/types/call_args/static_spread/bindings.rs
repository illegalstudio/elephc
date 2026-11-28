//! Purpose:
//! Plans final static unpack bindings without discarding source evaluation requirements.
//!
//! Called from:
//! - Static argument expansion, unpack-order validation and generic inference.
//!
//! Key details:
//! - Typed duplicates keep their first position and last value.
//! - Execution must retain the array when overwrites or key diagnostics are observable.

use std::collections::HashMap;
use super::{ExpandedArgOrigin, StaticAssocSpreadKey, static_assoc_spread_key};
use crate::{parser::ast::{Expr, ExprKind}, span::Span};
use elephc_semantics::SemanticProfile;

/// Separates final binding metadata from the source expressions that execution must retain.
pub(super) struct StaticBindings<'a> {
    entries: Vec<(StaticAssocSpreadKey, &'a Expr)>,
    needs_array_evaluation: bool,
}

/// Resolves typed identities and last-value bindings without executing or deleting source nodes.
pub(super) fn plan(pairs: &[(Expr, Expr)], profile: SemanticProfile) -> Option<StaticBindings<'_>> {
    let mut entries: Vec<(StaticAssocSpreadKey, &Expr)> = Vec::with_capacity(pairs.len());
    let mut positions: HashMap<StaticAssocSpreadKey, usize> = HashMap::new();
    let mut needs_array_evaluation = false;
    for (key, value) in pairs {
        needs_array_evaluation |= matches!(key.kind, ExprKind::FloatLiteral(_));
        let key = static_assoc_spread_key(key, profile)?;
        if let Some(&index) = positions.get(&key) {
            entries[index].1 = value;
            needs_array_evaluation = true;
        } else {
            positions.insert(key.clone(), entries.len());
            entries.push((key, value));
        }
    }
    Some(StaticBindings { entries, needs_array_evaluation })
}

impl StaticBindings<'_> {
    /// Allows execution expansion only when no overwritten value or key diagnostic disappears.
    pub(super) fn can_expand(&self) -> bool { !self.needs_array_evaluation }

    /// Checks iteration order after duplicate writes have retained their original positions.
    pub(super) fn has_positional_after_named(&self) -> bool {
        let mut named = false;
        for (key, _) in &self.entries {
            match key {
                StaticAssocSpreadKey::Named(_) => named = true,
                StaticAssocSpreadKey::Positional(_) if named => return true,
                StaticAssocSpreadKey::Positional(_) => {}
            }
        }
        false
    }

    /// Projects binding expressions, preserving each value's physical source and producer metadata.
    pub(super) fn into_arguments(self, span: Span) -> Vec<(Expr, ExpandedArgOrigin)> {
        self.entries.into_iter().map(|(key, value)| match key {
            StaticAssocSpreadKey::Positional(_) => {
                let mut value = value.clone();
                value.span = span;
                (value, ExpandedArgOrigin::StaticAssocPositional)
            }
            StaticAssocSpreadKey::Named(name) => (Expr::new(ExprKind::NamedArg {
                name, value: Box::new(value.clone()),
            }, span), ExpandedArgOrigin::StaticAssocNamed),
        }).collect()
    }
}
