//! Purpose:
//! Exposes scalar numeric builtin contracts from independently owned boxed results.
//!
//! Called from:
//! - Numeric builtin EIR homes sharing scalar and dynamic runtime paths.
//!
//! Key details:
//! - Physical storage normalization never changes the selected PHP numeric kind.
//! - Retires the temporary box after unboxing without consuming borrowed arguments.

use super::{BuiltinLoweringContext, LoweredBuiltinValue};
use crate::ir::Op;
use crate::span::Span;
use crate::types::PhpType;

/// Matches a boxed numeric implementation result to its checker-facing scalar result contract.
pub fn normalize_numeric_result(
    ctx: &mut dyn BuiltinLoweringContext, result: LoweredBuiltinValue, expected: &PhpType, span: Span,
) -> LoweredBuiltinValue {
    if !matches!(expected.codegen_repr(), PhpType::Int | PhpType::Float)
        || !matches!(ctx.value_php_type(result.value).codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        return result;
    }
    let scalar = ctx.emit_value(Op::MixedUnbox, vec![result.value], None,
        expected.clone(), Op::mixed_unbox_effects(expected), Some(span));
    ctx.emit_void(Op::Release, vec![result.value], None, Op::Release.default_effects(), Some(span));
    scalar
}
