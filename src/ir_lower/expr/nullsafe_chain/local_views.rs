//! Purpose:
//! Joins local representations observed at the exits of a lazy nullsafe chain.
//!
//! Called from:
//! - Nullsafe postfix chain lowering around its member segments and final merge.
//!
//! Key details:
//! - Base-expression effects precede the snapshot and always execute.
//! - Intermediate segment values may survive a later nullsafe short circuit.
//! - Distinct scalar alternatives use boxed storage; plain pointers keep their address word.

use crate::ir_lower::context::LoweringContext;
use crate::types::{PhpType, TypeEnv};

/// Read representations that can reach the chain's shared continuation.
pub(super) struct LocalViews {
    types: TypeEnv,
}

impl LocalViews {
    /// Captures existing PHP bindings after evaluating the chain's base expression.
    pub(super) fn new(ctx: &LoweringContext<'_, '_>) -> Self {
        Self { types: ctx.local_types_snapshot().into_iter().filter(|(name, _)|
            !crate::names::is_generated_local_name(name)).collect() }
    }

    /// Includes values that can remain after a later segment skips its operands.
    pub(super) fn observe(&mut self, ctx: &LoweringContext<'_, '_>) {
        for (name, previous) in &mut self.types {
            let Some(current) = ctx.local_types.get(name) else { continue; };
            if previous == current { continue; }
            *previous = if integer_or_null(previous) && integer_or_null(current) {
                PhpType::Union(vec![PhpType::Int, PhpType::Void])
            } else if ctx.local_is_plain_frame_slot(name) {
                crate::ir_lower::stmt::join_pointer_arm_types(&[previous.clone(), current.clone()])
                    .unwrap_or(PhpType::Mixed)
            } else { PhpType::Mixed };
        }
    }

    /// Uses the shared representation for reads after either the taken or skipped path.
    pub(super) fn publish(self, ctx: &mut LoweringContext<'_, '_>) {
        for (name, ty) in self.types {
            if ctx.has_local_slot(&name) && ctx.local_type(&name) != ty {
                ctx.set_local_type(&name, ty);
            }
        }
    }
}

/// Recognizes the semantic alternatives of the existing integer/null tagged ABI.
fn integer_or_null(ty: &PhpType) -> bool {
    match ty {
        PhpType::Int | PhpType::Void | PhpType::TaggedScalar => true,
        PhpType::Union(members) => members.iter().all(|member|
            matches!(member, PhpType::Int | PhpType::Void)),
        _ => false,
    }
}
