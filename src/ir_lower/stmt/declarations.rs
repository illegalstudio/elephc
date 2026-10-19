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
    let source = lower_expr(ctx, value);
    // A destination can replace the RHS local before later elements are read.
    let source = if ctx.value_is_owning_temporary(source) {
        source
    } else {
        crate::ir_lower::ownership::acquire_if_refcounted(ctx, source, Some(span))
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
    crate::ir_lower::ownership::release_if_owned(ctx, source, Some(span));
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
///
/// PHP evaluates the initializer only until the static holds a value, so it is lowered in
/// its own block behind a `static_local_uninitialized` branch: `static $c = new C();` runs
/// the constructor once, not on every call. The initializer is lowered first, out of line,
/// because its type is what declares the slot the guard then names.
pub(super) fn lower_static_var(ctx: &mut LoweringContext<'_, '_>, name: &str, init: &Expr, span: Span) {
    let entry_block = ctx
        .builder
        .insertion_block()
        .expect("a static declaration is lowered inside a block");
    let init_block = ctx.builder.create_named_block("static_local_init", Vec::new());
    let after_block = ctx.builder.create_named_block("static_local_after", Vec::new());

    ctx.builder.position_at_end(init_block);
    let value = lower_expr(ctx, init);
    // Same declared type as the checker: an initializer that says nothing about the value a
    // previous call left (`static $x;`, `= null`, `= []`) gives boxed `Mixed` storage read at
    // run time, and `init_static_local` boxes the initializer into it.
    let declared_type =
        crate::ir_lower::expr::static_local_declared_type_for(&ctx.loop_storage_scope, name)
            .unwrap_or_else(|| {
                crate::types::checker::stmt_check::assignments::static_local_declared_type(
                    ctx.builder.value_php_type(value.value),
                    false,
                )
            });
    let boxed = declared_type != ctx.builder.value_php_type(value.value);
    let slot = ctx.declare_local_with_kind(name, declared_type.clone(), LocalKind::StaticLocal);
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
    branch_to(ctx, after_block);

    ctx.builder.position_at_end(entry_block);
    let uninitialized = ctx
        .builder
        .emit_with_effects(
            Op::StaticLocalUninitialized,
            Vec::new(),
            Some(Immediate::LocalSlot(slot)),
            IrType::I64,
            PhpType::Bool,
            Ownership::NonHeap,
            Op::StaticLocalUninitialized.default_effects(),
            Some(span),
        )
        .expect("static_local_uninitialized produces a branch condition");
    ctx.builder.terminate(Terminator::CondBr {
        cond: uninitialized,
        then_target: init_block,
        then_args: Vec::new(),
        else_target: after_block,
        else_args: Vec::new(),
    });
    ctx.builder.position_at_end(after_block);
    if boxed {
        ctx.set_local_type(name, declared_type);
    }
    ctx.clear_static_callable_locals();
}
