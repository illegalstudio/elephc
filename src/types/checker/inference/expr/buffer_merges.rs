//! Purpose:
//! Types ternary, `?:`, `match` and `??` merges that involve a `buffer<T>` arm, and rejects the
//! ones that would turn the buffer into a value that is no longer a buffer.
//!
//! Called from:
//! - `crate::types::checker::inference::expr::basic` (ternary, `?:` and `match` result types).
//! - `crate::types::checker::inference::expr::calls_objects` (`??` result type).
//! - `crate::types::checker::inference::expr::effects` (the assignment-effects walk of a ternary
//!   and of `??`).
//!
//! Key details:
//! - A buffer value is an opaque handle with no null and no `mixed` form that can still be
//!   indexed: boxing it stores the handle as an integer, so an element read through the merged
//!   value answered null and a write through it was lost. Every arm that completes must
//!   therefore have the same `buffer<T>` type.
//! - A `throw` arm never completes, so it is skipped by its shape: `infer_type` types it `never`
//!   and the assignment-effects walk `null`. The EIR merge typing (`shared_buffer_merge_type` in
//!   `src/ir_lower/expr/merge_temps.rs`) skips the same arms and ends their branch, so the
//!   checker and the lowered merge agree. Any other arm, a call to a `never` function included,
//!   is an ordinary arm, since the lowered merge would store its result.

use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind};
use crate::span::Span;
use crate::types::PhpType;

/// Returns the buffer type a merge produces when one of its completing arms holds a buffer.
///
/// `construct` names the merge in the diagnostic (`a ternary`, `a match`, ...). Each arm comes
/// with the type the caller inferred for it; an arm that is a `throw` expression never completes
/// and is skipped, whatever type the caller gave it. Answers `Ok(None)` when no completing arm
/// holds a buffer, so the caller keeps its ordinary merge, `Ok(Some(buffer))` when every
/// completing arm has that one `buffer<T>` type, and an error at `span` when a buffer arm meets
/// an arm of any other type (another buffer type, `null`, `mixed`, a scalar) or a buffer that may
/// be null.
pub(super) fn buffer_merge_result_type(
    construct: &str,
    arms: &[(&Expr, PhpType)],
    span: Span,
) -> Result<Option<PhpType>, CompileError> {
    let completing: Vec<&PhpType> = arms
        .iter()
        .filter(|(arm, _)| !matches!(arm.kind, ExprKind::Throw(_)))
        .map(|(_, ty)| ty)
        .collect();
    let Some(buffer) = completing.iter().copied().find(|ty| holds_buffer(ty)) else {
        return Ok(None);
    };
    if !matches!(buffer, PhpType::Buffer(_)) {
        return Err(CompileError::new(
            span,
            &format!(
                "Type error: every arm of {construct} must have the same buffer type, and {buffer} \
                 can be null: a buffer has no null or mixed form"
            ),
        ));
    }
    if let Some(other) = completing.iter().copied().find(|ty| *ty != buffer) {
        return Err(CompileError::new(
            span,
            &format!(
                "Type error: every arm of {construct} must have the same buffer type: {buffer} \
                 cannot be merged with {other}, because a buffer has no null or mixed form"
            ),
        ));
    }
    Ok(Some(buffer.clone()))
}

/// Returns whether a checker type is a buffer or a union with a buffer member.
fn holds_buffer(ty: &PhpType) -> bool {
    match ty {
        PhpType::Buffer(_) => true,
        PhpType::Union(members) => members.iter().any(holds_buffer),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a placeholder arm expression that is not a `throw`.
    fn value_arm() -> Expr {
        Expr::new(ExprKind::Variable("b".to_string()), Span::dummy())
    }

    /// Builds a `throw` arm expression.
    fn throw_arm() -> Expr {
        Expr::new(
            ExprKind::Throw(Box::new(Expr::new(ExprKind::Null, Span::dummy()))),
            Span::dummy(),
        )
    }

    /// Returns the `buffer<float>` checker type.
    fn float_buffer() -> PhpType {
        PhpType::Buffer(Box::new(PhpType::Float))
    }

    /// Arms of one buffer type merge as that type, and arms without a buffer keep the ordinary
    /// merge.
    #[test]
    fn test_same_buffer_arms_merge_as_the_buffer() {
        let (a, b) = (value_arm(), value_arm());
        let arms = [(&a, float_buffer()), (&b, float_buffer())];
        let merged = buffer_merge_result_type("a ternary", &arms, Span::dummy());
        assert_eq!(merged.unwrap(), Some(float_buffer()));
        let arms = [(&a, PhpType::Int), (&b, PhpType::Void)];
        let scalars = buffer_merge_result_type("a ternary", &arms, Span::dummy());
        assert_eq!(scalars.unwrap(), None);
    }

    /// A `throw` arm does not widen the merge, whether it is typed `null` (the effects walk) or
    /// `never` (`infer_type`).
    #[test]
    fn test_throw_arms_are_skipped() {
        let (a, null_throw, never_throw) = (value_arm(), throw_arm(), throw_arm());
        let merged = buffer_merge_result_type(
            "a match",
            &[
                (&a, float_buffer()),
                (&null_throw, PhpType::Void),
                (&never_throw, PhpType::Never),
            ],
            Span::dummy(),
        );
        assert_eq!(merged.unwrap(), Some(float_buffer()));
    }

    /// A buffer arm beside another buffer type, `null`, `mixed`, a `never` call that the lowered
    /// merge would still store, or a buffer that may be null, is rejected.
    #[test]
    fn test_buffer_beside_another_type_is_rejected() {
        let (a, b) = (value_arm(), value_arm());
        for other in [
            PhpType::Buffer(Box::new(PhpType::Int)),
            PhpType::Void,
            PhpType::Mixed,
            PhpType::Never,
            PhpType::Union(vec![float_buffer(), PhpType::Void]),
        ] {
            let arms = [(&a, float_buffer()), (&b, other.clone())];
            let merged = buffer_merge_result_type("a ternary", &arms, Span::dummy());
            let message = merged
                .expect_err("a mixed buffer merge must be rejected")
                .message;
            assert!(
                message.contains("every arm of a ternary must have the same buffer type"),
                "{other}: {message}"
            );
        }
    }
}
