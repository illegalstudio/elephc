//! Purpose:
//! Stages original offset kinds that a generic array fallback cannot materialize directly.
//!
//! Called from:
//! - Boxed writes and array/property removal lowering.
//!
//! Key details:
//! - Boxing preserves the original PHP kind and borrows the caller's source lease.
//! - A new boxed owner remains published until the offset operation finishes.

use super::context::{LoweredValue, LoweringContext};
use crate::{ir::LocalSlotId, span::Span, types::PhpType};

/// One original key with an optional independently rooted representation box.
pub(super) struct PreparedOffsetKey {
    pub(super) value: LoweredValue,
    owner: Option<LocalSlotId>,
}

impl PreparedOffsetKey {
    /// Preserves unsupported raw key kinds in a Mixed cell before either receiver branch emits.
    pub(super) fn prepare(ctx: &mut LoweringContext<'_, '_>, key: LoweredValue, span: Span) -> Self {
        if !matches!(ctx.builder.value_php_type(key.value).codegen_repr(),
            PhpType::Object(_) | PhpType::Callable | PhpType::Array(_)
            | PhpType::AssocArray { .. } | PhpType::Iterable)
        { return Self { value: key, owner: None }; }
        ctx.with_independent_write_receiver(|ctx| {
            let key = super::ownership::borrow_value(ctx, key, Some(span));
            let key = ctx.box_value_as_mixed(key, PhpType::Mixed, Some(span));
            let (value, owner) = super::expr::root_owned_call_operand(ctx, key, span);
            Self { value, owner }
        })
    }

    /// Joins a writer's existing reverse-order owner ledger without retaining the source again.
    pub(super) fn into_value_with_owners(self, roots: &mut Vec<LocalSlotId>) -> LoweredValue {
        if let Some(owner) = self.owner { roots.push(owner); }
        self.value
    }

    /// Retires only this representation box, leaving normal source cleanup to its caller.
    pub(super) fn finish(self, ctx: &mut LoweringContext<'_, '_>, span: Span) {
        if let Some(owner) = self.owner { super::expr::retire_owned_call_operand(ctx, owner, span); }
    }
}
