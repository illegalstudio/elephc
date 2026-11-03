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
    // An `ArrayAccess` object (the checker admits no other object) is read through the boxed
    // reader, which dispatches `offsetGet()` per position. Boxing moves the owned reference into
    // the box, which becomes the source this statement releases.
    let source = if matches!(source.ir_type, IrType::Heap(crate::ir::IrHeapKind::Object)) {
        ctx.box_value_as_mixed(source, PhpType::Mixed, Some(span))
    } else {
        source
    };
    let item_type = list_unpack_item_type(ctx, source.value);
    let get_op = list_unpack_get_op(source.ir_type);
    // Boxed `Mixed` sources read through `__rt_mixed_array_get`, which takes an explicit
    // warn-on-missing flag. Destructuring is an ordinary read, so a short source reports PHP's
    // undefined-key warning like `$src[$i]` would -- but destructuring NULL is silent in PHP,
    // every target simply becomes null. `PhpDumper::addServices()` destructures each
    // `$services[$id]`, null for a synthetic service, and printed "Trying to access array offset
    // on null" for each. The flag is therefore "the source is not null", decided at run time.
    let warning_flag = matches!(get_op, Op::RuntimeCall).then(|| {
        let is_null = ctx.emit_value(
            Op::IsNull,
            vec![source.value],
            None,
            PhpType::Bool,
            Op::IsNull.default_effects(),
            Some(span),
        );
        let zero = ctx.emit_value(
            Op::ConstI64,
            Vec::new(),
            Some(Immediate::I64(0)),
            PhpType::Int,
            Op::ConstI64.default_effects(),
            Some(span),
        );
        ctx.emit_value(
            Op::ICmp,
            vec![is_null.value, zero.value],
            Some(Immediate::CmpPredicate(crate::ir::CmpPredicate::Eq)),
            PhpType::Bool,
            Op::ICmp.default_effects(),
            Some(span),
        )
    });
    for (index, var) in vars.iter().enumerate() {
        let index_value = lower_list_unpack_index(ctx, index, span);
        let mut operands = vec![source.value, index_value.value];
        if let Some(warning_flag) = warning_flag {
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
    if ctx.value_is_owning_temporary(source) {
        crate::ir_lower::ownership::release_if_owned(ctx, source, Some(span));
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
pub(super) fn lower_global(ctx: &mut LoweringContext<'_, '_>, vars: &[String], span: Span) {
    for var in vars {
        ctx.bind_global_local(var, Some(span));
    }
}

/// Lowers `static $x = init;` for a static some by-reference use names
/// (`crate::ir_lower::static_refs`): php binds the local by reference to storage that outlives
/// the call, so `$x` is bound to the reference cell of a hidden global owned by this function
/// (every by-reference use then shares it), and the initializer runs once, guarded by a hidden
/// `bool` static flag.
fn lower_reference_bound_static_var(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    init: &Expr,
    span: Span,
) {
    let function = ctx.builder.function().name.clone();
    let mangled: String = function
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    // FNV-1a over the exact function name keeps two functions whose names mangle alike apart.
    let hash = function
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3));
    let storage = format!("__elephc_static_{hash:016x}_{mangled}__{name}");
    ctx.bind_static_reference_local(name, &storage, Some(span));

    let flag = format!("{name}#static_initialized");
    let flag_slot = ctx.declare_local_with_kind(&flag, PhpType::Bool, LocalKind::StaticLocal);
    let init_block = ctx.builder.create_named_block("static_ref_init", Vec::new());
    let after_block = ctx.builder.create_named_block("static_ref_after", Vec::new());
    let uninitialized = ctx
        .builder
        .emit_with_effects(
            Op::StaticLocalUninitialized,
            Vec::new(),
            Some(Immediate::LocalSlot(flag_slot)),
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

    ctx.builder.position_at_end(init_block);
    let value = lower_expr(ctx, init);
    let value_type = ctx.builder.value_php_type(value.value);
    ctx.store_local(name, value, value_type, Some(span));
    let initialized = ctx.emit_value(
        Op::ConstBool,
        Vec::new(),
        Some(Immediate::Bool(true)),
        PhpType::Bool,
        Op::ConstBool.default_effects(),
        Some(span),
    );
    ctx.builder.emit_with_effects(
        Op::InitStaticLocal,
        vec![initialized.value],
        Some(Immediate::LocalSlot(flag_slot)),
        IrType::Void,
        PhpType::Void,
        Ownership::NonHeap,
        Op::InitStaticLocal.default_effects(),
        Some(span),
    );
    branch_to(ctx, after_block);
    ctx.builder.position_at_end(after_block);
    ctx.set_local_type(name, PhpType::Mixed);
    ctx.clear_static_callable_locals();
}

/// Lowers a static local variable initialization.
///
/// PHP evaluates the initializer only until the static holds a value, so it is lowered in
/// its own block behind a `static_local_uninitialized` branch: `static $c = new C();` runs
/// the constructor once, not on every call. The initializer is lowered first, out of line,
/// because its type is what declares the slot the guard then names.
pub(super) fn lower_static_var(ctx: &mut LoweringContext<'_, '_>, name: &str, init: &Expr, span: Span) {
    if ctx.reference_bound_statics.contains(name) {
        lower_reference_bound_static_var(ctx, name, init, span);
        return;
    }
    let entry_block = ctx
        .builder
        .insertion_block()
        .expect("a static declaration is lowered inside a block");
    let init_block = ctx.builder.create_named_block("static_local_init", Vec::new());
    let after_block = ctx.builder.create_named_block("static_local_after", Vec::new());

    ctx.builder.position_at_end(init_block);
    let mut value = lower_expr(ctx, init);
    // The initializer runs once; subsequent calls may enter with an object or another value.
    // A null initializer therefore requires boxed persistent storage and no null-only flow fact.
    let initialized_type = ctx.builder.value_php_type(value.value);
    let storage_type = if initialized_type == PhpType::Void { PhpType::Mixed } else { initialized_type };
    // An array static the body later assigns boxed elements to is declared with the checker's
    // boxed element type from the start, so the initializer is converted below; the slot
    // widened only at that later store, after the raw-element initializer was already bound.
    let storage_type = match storage_type.codegen_repr() {
        PhpType::Array(elem)
            if ctx.reassigned_statics.contains(name)
                && !matches!(elem.codegen_repr(), PhpType::Mixed | PhpType::Never | PhpType::Void) =>
        {
            PhpType::Array(Box::new(PhpType::Mixed))
        }
        _ => storage_type,
    };
    let slot = ctx.declare_local_with_kind(name, storage_type.clone(), LocalKind::StaticLocal);
    // A string-keyed `static $m = [];` is declared as a hash (see
    // `required_static_local_storage_type`); the empty initializer becomes one to match. The
    // literal is an owned temporary, which the conversion consumes.
    let slot_ty = ctx.builder.local_php_type(slot);
    if matches!(slot_ty.codegen_repr(), PhpType::AssocArray { .. })
        && matches!(ctx.builder.value_php_type(value.value).codegen_repr(), PhpType::Array(_))
    {
        value = ctx.emit_value(
            Op::ArrayToHash,
            vec![value.value],
            None,
            slot_ty.clone(),
            Op::ArrayToHash.default_effects(),
            Some(span),
        );
    }
    // A static the body also assigns wider values (`static $last = [0, 0]; $last = $progress;`,
    // symfony/http-client's `NativeHttpClient`) is declared with boxed elements; the literal's
    // raw ones are boxed to match, as a return to an `array<mixed>` contract boxes them.
    let value_ty = ctx.builder.value_php_type(value.value).codegen_repr();
    let widen = match (&value_ty, &slot_ty.codegen_repr()) {
        (PhpType::Array(elem), PhpType::Array(slot_elem))
            if slot_elem.codegen_repr() == PhpType::Mixed
                && !matches!(elem.codegen_repr(), PhpType::Mixed | PhpType::Never | PhpType::Void) =>
        {
            Some(Op::ArrayToMixed)
        }
        (PhpType::AssocArray { value: elem, .. }, PhpType::AssocArray { value: slot_elem, .. })
            if slot_elem.codegen_repr() == PhpType::Mixed
                && !matches!(elem.codegen_repr(), PhpType::Mixed | PhpType::Never | PhpType::Void) =>
        {
            Some(Op::HashToMixed)
        }
        _ => None,
    };
    if let Some(op) = widen {
        value = ctx.emit_value(op, vec![value.value], None, slot_ty.clone(), op.default_effects(), Some(span));
    }
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
    // The slot's DECLARED type, not the initializer's: a string-keyed `static $m = [];` was
    // declared as a hash above, and an `array<never>` fact here made the next nested write
    // convert the slot's boxed value as if it were still a packed array.
    let declared_type = ctx.builder.local_php_type(slot);
    ctx.set_local_type(name, declared_type);
    ctx.clear_static_callable_locals();
}
