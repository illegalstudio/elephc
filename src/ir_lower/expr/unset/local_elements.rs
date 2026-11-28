//! Purpose:
//! Removes local array offsets after selecting the storage left by key evaluation.
//!
//! Called from:
//! - Boxed, hash and packed local unset entry points.
//!
//! Key details:
//! - One key evaluation and owner ledger spans validation, COW and removal.
//! - Current packed element metadata drives sparse conversion; boxed cells stay attached.
//! - Scalar/object replacements use PHP's boxed unset dispatch without array reinterpretation.

use super::*;

/// Evaluates the key once and removes from the current receiver with balanced exceptional owners.
pub(super) fn lower(
    ctx: &mut LoweringContext<'_, '_>, name: &str, array_span: Span, index: &Expr, expr: &Expr,
) -> LoweredValue {
    let key = lower_expr(ctx, index);
    let (key, key_owner) = root_owned_call_operand(ctx, key, index.span);
    let prepared = crate::ir_lower::offset_key_operands::PreparedOffsetKey::prepare(ctx, key, index.span);
    let key = prepared.value;
    let mut receiver = if ctx.local_type(name).codegen_repr() == PhpType::Mixed {
        crate::ir_lower::stmt::load_array_local_for_write(ctx, name, array_span)
    } else { ctx.load_local(name, Some(array_span)) };
    crate::ir_lower::array_key_guards::for_removal(ctx, receiver, key, index.span);
    if receiver.ir_type == IrType::Heap(IrHeapKind::Array) {
        let element = match ctx.builder.value_php_type(receiver.value).codegen_repr() {
            PhpType::Array(element) if *element != PhpType::Never => *element,
            _ => PhpType::Mixed,
        };
        let ty = PhpType::AssocArray { key: Box::new(PhpType::Int), value: Box::new(element) };
        let hash = ctx.emit_value(Op::ArrayToHash, vec![receiver.value], None,
            ty.clone(), Op::ArrayToHash.default_effects(), Some(array_span));
        ctx.store_mutated_local(name, hash, ty, Some(array_span));
        receiver = ctx.load_local(name, Some(array_span));
    }
    let mut receiver_owner = None;
    let op = match receiver.ir_type {
        IrType::Heap(IrHeapKind::Hash) => Op::HashUnset,
        IrType::Heap(IrHeapKind::Mixed | IrHeapKind::Union) => Op::OffsetUnset,
        _ => {
            let boxed = ctx.box_value_as_mixed(receiver, PhpType::Mixed, Some(array_span));
            let (boxed, owner) = root_owned_call_operand(ctx, boxed, array_span);
            receiver = boxed;
            receiver_owner = owner;
            Op::OffsetUnset
        }
    };
    let removed = ctx.emit_value(op, vec![receiver.value, key.value], None,
        PhpType::Bool, op.default_effects(), Some(expr.span));
    if let Some(owner) = receiver_owner { retire_owned_call_operand(ctx, owner, index.span); }
    prepared.finish(ctx, index.span);
    if let Some(owner) = key_owner { retire_owned_call_operand(ctx, owner, index.span); }
    removed
}
