//! Purpose:
//! Materializes canonical borrowed numeric operands without changing their PHP value kinds.
//!
//! Called from:
//! - Checked arithmetic, updates and boxed unary operators.
//!
//! Key details:
//! - Existing Mixed cells stay borrowed; new canonical boxes own an independent value.
//! - Union storage is cloned rather than nested inside another PHP value box.

use super::context::{LoweredValue, LoweringContext};
use crate::{ir::{IrHeapKind, IrType, Op}, span::Span, types::PhpType};

/// Returns the original canonical cell or an independently owned box without consuming its source.
pub(super) fn box_value(ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, span: Span) -> LoweredValue {
    if value.ir_type == IrType::Heap(IrHeapKind::Mixed) { return value; }
    let op = if value.ir_type == IrType::Heap(IrHeapKind::Union) { Op::MixedClone } else { Op::MixedBox };
    ctx.emit_owned_value(op, vec![value.value], None, PhpType::Mixed, op.default_effects(), Some(span))
}
