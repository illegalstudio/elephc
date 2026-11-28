//! Purpose:
//! Describes logical source evidence shared by bindings and evaluated call arguments.
//!
//! Called from:
//! - Scope source snapshots, literal builders and argument normalization.
//!
//! Key details:
//! - Nested entry proofs remain independent from inferred homogeneous array schemas.
//! - Metadata owns no PHP values and stores no reference cell identities.

/// A source producer snapshot whose write stamp matters only for a shared argument.
#[derive(Clone)]
pub(crate) struct SourceSnapshot {
    pub(crate) ty: crate::eval_ir::EvalParameterType,
    pub(crate) write_epoch: u64,
    pub(crate) entries: Vec<(crate::context::EvalArrayReferenceKey, Option<SourceSnapshot>)>,
}

impl SourceSnapshot {
    /// Captures declaration metadata without retaining or inspecting its runtime value.
    pub(crate) fn capture(ty: crate::eval_ir::EvalParameterType,
        context: &crate::context::ElephcEvalContext,
    ) -> Self { Self { ty, write_epoch: context.source_view_epoch(), entries: Vec::new() } }
}
