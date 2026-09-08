//! Purpose:
//! Acquires stable string buffers for descriptor invoker by-value arguments.
//!
//! Called from:
//! - The indexed, associative, and default argument builders in the parent module.
//!
//! Key details:
//! - Every resulting string has one owner recorded by `InvokerArgumentOwners`.
//! - Boxed strings bypass the allocating cast so persistence happens exactly once.
//! - Other scalar casts are persisted before later arguments can overwrite shared scratch.

use super::{abi, DataSection, Emitter, InvokerEmitContext, PhpType};

/// Coerces a borrowed argument and detaches any resulting string for invocation cleanup.
pub(super) fn coerce(
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
    data: &mut DataSection,
    source_ty: &PhpType,
    target_ty: Option<&PhpType>,
) -> (PhpType, bool) {
    if source_ty.codegen_repr() == PhpType::Mixed
        && target_ty.is_some_and(|ty| ty.codegen_repr() == PhpType::Str)
    {
        let string_label = ctx.next_label("owned_string_argument");
        let persist_label = ctx.next_label("persist_string_argument");
        abi::emit_owned_mixed_string(emitter, &string_label, &persist_label);
        return (PhpType::Str, false);
    }
    let coerced = super::coerce_current_value_to_target(emitter, ctx, data, source_ty, target_ty);
    if coerced.0 == PhpType::Str {
        abi::emit_call_label(emitter, "__rt_str_persist");
    }
    coerced
}
