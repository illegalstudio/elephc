//! Purpose:
//! Constant, list, global, and static-local declarations.
//!
//! Called from:
//! - `crate::ir_lower::stmt`.
//!
//! Key details:
//! - Preserves statement ordering, CFG shape, EIR effects, and ownership contracts.

use super::*;

/// Lowers a global constant declaration.
pub(super) fn lower_const_decl(ctx: &mut LoweringContext<'_, '_>, name: &str, value: &Expr, span: Span) {
    let value = lower_expr(ctx, value);
    let data = ctx.intern_global_name(name);
    ctx.emit_void(
        Op::StoreGlobal,
        vec![value.value],
        Some(Immediate::GlobalName(data)),
        Op::StoreGlobal.default_effects(),
        Some(span),
    );
}

/// Lowers simple positional list destructuring into indexed reads plus local writes.
pub(super) fn lower_list_unpack(ctx: &mut LoweringContext<'_, '_>, vars: &[String], value: &Expr, span: Span) {
    let source = match &value.kind {
        ExprKind::Variable(name) => ctx
            .load_local_mixed_storage_for_runtime_read(name, Some(value.span))
            .unwrap_or_else(|| lower_expr(ctx, value)),
        _ => lower_expr(ctx, value),
    };
    let item_type = list_unpack_item_type(ctx, source.value);
    let get_op = list_unpack_get_op(source.ir_type);
    for (index, var) in vars.iter().enumerate() {
        let index_value = lower_list_unpack_index(ctx, index, span);
        let mut operands = vec![source.value, index_value.value];
        // Boxed `Mixed` sources read through `__rt_mixed_array_get`, which takes an
        // explicit warn-on-missing flag. Destructuring is an ordinary read, so a
        // short source reports PHP's undefined-key warning like `$src[$i]` would.
        if matches!(get_op, Op::RuntimeCall) {
            let warning_flag = crate::ir_lower::expr::emit_bool_literal(ctx, true, Some(span));
            operands.push(warning_flag.value);
        }
        let item = ctx.emit_value(
            get_op,
            operands,
            None,
            item_type.clone(),
            get_op.default_effects(),
            Some(span),
        );
        ctx.store_local(var, item, item_type.clone(), Some(span));
    }
}

/// Emits the positional integer key used to read one list-unpack element.
pub(super) fn lower_list_unpack_index(
    ctx: &mut LoweringContext<'_, '_>,
    index: usize,
    span: Span,
) -> LoweredValue {
    ctx.emit_value(
        Op::ConstI64,
        Vec::new(),
        Some(Immediate::I64(index as i64)),
        PhpType::Int,
        Op::ConstI64.default_effects(),
        Some(span),
    )
}

/// Returns the element-read opcode for a list-unpack source value.
pub(super) fn list_unpack_get_op(source_type: IrType) -> Op {
    match source_type {
        IrType::Heap(crate::ir::IrHeapKind::Array) => Op::ArrayGet,
        IrType::Heap(crate::ir::IrHeapKind::Hash) => Op::HashGet,
        _ => Op::RuntimeCall,
    }
}

/// Returns the PHP type assigned to each simple list-unpack destination.
///
/// Indexed-array reads use `Op::ArrayGet`, whose runtime OOB fallback produces a
/// null in the result shape (tagged scalar or sentinel). To preserve that null
/// for `??` and `IsNull`, the destination type is widened the same way as a
/// direct array index read (see `array_access_element_result_type`). Without
/// this widening an `Array(Int)` element would lower to `PhpType::Int`, whose
/// null fallback is the in-band `NULL_SENTINEL` i64, and `$b ?? 'n'` would see
/// a non-null integer instead of null for missing keys (#337).
pub(super) fn list_unpack_item_type(ctx: &LoweringContext<'_, '_>, source: crate::ir::ValueId) -> PhpType {
    let item_type = match ctx.builder.value_php_type(source).codegen_repr() {
        PhpType::Array(elem_ty) => array_access_element_result_type(elem_ty.codegen_repr()),
        PhpType::AssocArray { value, .. } => {
            array_access_element_result_type(value.codegen_repr())
        }
        _ => PhpType::Mixed,
    };
    normalize_materialized_element_type(item_type)
}

/// Normalizes non-materializable element metadata to the null sentinel.
pub(super) fn normalize_materialized_element_type(item_type: PhpType) -> PhpType {
    match item_type {
        PhpType::Never => PhpType::Void,
        other => other,
    }
}

/// Normalizes indexed-array write payloads to storage shapes Phase 04 can lower.
pub(super) fn normalize_array_write_element_type(item_type: PhpType) -> PhpType {
    let item_type = normalize_materialized_element_type(item_type);
    if item_type.is_refcounted() && !matches!(item_type, PhpType::Str) {
        PhpType::Mixed
    } else {
        item_type
    }
}

/// Declares global aliases in the local slot table.
pub(super) fn lower_global(ctx: &mut LoweringContext<'_, '_>, vars: &[String]) {
    for var in vars {
        let php_type = ctx.global_alias_type(var);
        ctx.declare_local_with_kind(var, php_type, LocalKind::GlobalAlias);
    }
}

/// Lowers a static local variable initialization.
pub(super) fn lower_static_var(ctx: &mut LoweringContext<'_, '_>, name: &str, init: &Expr, span: Span) {
    let value = lower_expr(ctx, init);
    let slot = ctx.declare_local_with_kind(
        name,
        ctx.builder.value_php_type(value.value),
        LocalKind::StaticLocal,
    );
    ctx.builder.emit_with_effects(
        Op::InitStaticLocal,
        vec![value.value],
        Some(Immediate::LocalSlot(slot)),
        IrType::Void,
        PhpType::Void,
        Ownership::NonHeap,
        Op::InitStaticLocal.default_effects(),
        Some(span),
    );
}
