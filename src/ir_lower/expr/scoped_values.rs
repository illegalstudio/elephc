//! Purpose:
//! First-class callable creation, buffers, and scoped constant lowering.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Lowers first-class callable creation.
pub(super) fn lower_first_class_callable(ctx: &mut LoweringContext<'_, '_>, target: &CallableTarget, expr: &Expr) -> LoweredValue {
    let operands = if let CallableTarget::Method { object, .. } = target {
        vec![lower_expr(ctx, object).value]
    } else {
        Vec::new()
    };
    let data = ctx.intern_string(&callable_target_name(target));
    ctx.emit_value(
        Op::FirstClassCallableNew,
        operands,
        Some(Immediate::ProfiledData {
            data,
            strict_php: crate::strict_php::is_enabled(),
        }),
        PhpType::Callable,
        Op::FirstClassCallableNew.default_effects(),
        Some(expr.span),
    )
}

/// Returns the strict-PHP visibility profile attached to runtime callable selection.
pub(super) fn callable_profile_immediate() -> Option<Immediate> {
    Some(Immediate::Bool(crate::strict_php::is_enabled()))
}

/// Lowers a pointer cast.
pub(super) fn lower_ptr_cast(ctx: &mut LoweringContext<'_, '_>, target_type: &str, inner: &Expr, expr: &Expr) -> LoweredValue {
    let value = lower_expr(ctx, inner);
    let data = ctx.intern_string(target_type);
    ctx.emit_value(
        Op::PtrCast,
        vec![value.value],
        Some(Immediate::Data(data)),
        PhpType::Pointer(Some(target_type.to_string())),
        Op::PtrCast.default_effects(),
        Some(expr.span),
    )
}

/// Lowers buffer allocation.
pub(super) fn lower_buffer_new(
    ctx: &mut LoweringContext<'_, '_>,
    element_type: &TypeExpr,
    len: &Expr,
    expr: &Expr,
) -> LoweredValue {
    let len_value = lower_expr(ctx, len);
    let php_type = PhpType::Buffer(Box::new(ctx.type_expr_to_php_type_for_value(element_type)));
    ctx.emit_value(
        Op::BufferNew,
        vec![len_value.value],
        None,
        php_type,
        Op::BufferNew.default_effects(),
        Some(expr.span),
    )
}

/// Lowers `::class`.
pub(super) fn lower_class_constant(ctx: &mut LoweringContext<'_, '_>, receiver: &StaticReceiver, expr: &Expr) -> LoweredValue {
    let name = match receiver {
        StaticReceiver::Static => receiver_name(receiver),
        _ => static_receiver_class_name(ctx, receiver).unwrap_or_else(|| receiver_name(receiver)),
    };
    let data = ctx.intern_class_name(&name);
    ctx.emit_value(
        Op::ConstClassName,
        Vec::new(),
        Some(Immediate::Data(data)),
        PhpType::Str,
        Op::ConstClassName.default_effects(),
        Some(expr.span),
    )
}

/// Lowers an object-valued `::class` receiver through the runtime class-name lookup.
pub(super) fn lower_object_class_name(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    expr: &Expr,
) -> LoweredValue {
    let object = lower_expr(ctx, object);
    emit_builtin_call_value(
        ctx,
        "get_class",
        vec![object.value],
        PhpType::Str,
        expr.span,
        None,
    )
}

/// Lowers a scoped constant read.
pub(super) fn lower_scoped_constant(ctx: &mut LoweringContext<'_, '_>, receiver: &StaticReceiver, name: &str, expr: &Expr) -> LoweredValue {
    let class_name = scoped_constant_receiver_name(ctx, receiver);
    let normalized_class_name = class_name.trim_start_matches('\\');
    if ctx
        .enums
        .get(normalized_class_name)
        .is_some_and(|enum_info| enum_info.cases.iter().any(|case| case.name == name))
    {
        let key = format!("{}::{}", normalized_class_name, name);
        let data = ctx.intern_string(&key);
        return ctx.emit_value(
            Op::ScopedConstantGet,
            Vec::new(),
            Some(Immediate::Data(data)),
            PhpType::Object(normalized_class_name.to_string()),
            Op::ScopedConstantGet.default_effects(),
            Some(expr.span),
        );
    }
    if matches!(receiver, StaticReceiver::Static) {
        return lower_late_static_scoped_constant(ctx, name, expr);
    }
    if let Some(value) = ctx.scoped_constant_value(&class_name, name) {
        return lower_expr(ctx, &value);
    }
    let key = format!("{}::{}", class_name, name);
    let data = ctx.intern_string(&key);
    ctx.emit_value(
        Op::ScopedConstantGet,
        Vec::new(),
        Some(Immediate::Data(data)),
        PhpType::Mixed,
        Op::ScopedConstantGet.default_effects(),
        Some(expr.span),
    )
}

/// Returns the class name to use for a scoped constant lookup.
pub(super) fn scoped_constant_receiver_name(ctx: &LoweringContext<'_, '_>, receiver: &StaticReceiver) -> String {
    match receiver {
        StaticReceiver::Static => receiver_name(receiver),
        _ => static_receiver_class_name(ctx, receiver).unwrap_or_else(|| receiver_name(receiver)),
    }
}

/// Lowers `static::CONST` using late static binding: emits a runtime dispatch over the
/// called-class id so that each descendant class that overrides the constant contributes
/// its own value. Falls back to the lexical (declaring-class) constant value.
pub(super) fn lower_late_static_scoped_constant(ctx: &mut LoweringContext<'_, '_>, name: &str, expr: &Expr) -> LoweredValue {
    let Some(base_class) = ctx.current_class.clone() else {
        return lower_scoped_constant_fallback(ctx, "static", name, expr);
    };
    let fallback_value = ctx.scoped_constant_value(&base_class, name);
    let result_type = fallback_expr_type(expr);
    let candidates = late_static_constant_candidates(ctx, &base_class, name);
    if candidates.is_empty() {
        if let Some(value) = fallback_value {
            return lower_expr(ctx, &value);
        }
        return lower_scoped_constant_fallback(ctx, "static", name, expr);
    }
    let temp_name = ctx.declare_owned_hidden_temp(result_type.clone());
    let split_initialized = ctx.initialized_slots_snapshot();
    let merge = ctx.builder.create_named_block("static_const.merge", Vec::new());
    let called_class_id = ctx.emit_value(
        Op::LoadCalledClassId,
        Vec::new(),
        None,
        PhpType::Int,
        Op::LoadCalledClassId.default_effects(),
        Some(expr.span),
    );
    let mut branch_labels = Vec::new();
    for (class_name, class_id) in &candidates {
        let block = ctx.builder.create_named_block("static_const.branch", Vec::new());
        branch_labels.push((block, class_name.clone(), *class_id));
        let class_id_val = ctx.emit_value(
            Op::ConstI64,
            Vec::new(),
            Some(Immediate::I64(*class_id as i64)),
            PhpType::Int,
            Op::ConstI64.default_effects(),
            Some(expr.span),
        );
        let eq_result = ctx.emit_value(
            Op::ICmp,
            vec![called_class_id.value, class_id_val.value],
            Some(Immediate::CmpPredicate(CmpPredicate::Eq)),
            PhpType::Bool,
            Op::ICmp.default_effects(),
            Some(expr.span),
        );
        let skip_block = ctx.builder.create_named_block("static_const.skip", Vec::new());
        ctx.builder.terminate(Terminator::CondBr {
            cond: eq_result.value,
            then_target: block,
            then_args: Vec::new(),
            else_target: skip_block,
            else_args: Vec::new(),
        });
        ctx.builder.position_at_end(skip_block);
    }
    let fallback_expr = fallback_value
        .as_ref()
        .map(|v| v.clone())
        .unwrap_or_else(|| Expr::new(ExprKind::Null, expr.span));
    store_expr_into_temp(ctx, &temp_name, result_type.clone(), &fallback_expr, expr.span);
    branch_to(ctx, merge);
    for (block, class_name, _class_id) in branch_labels {
        ctx.builder.position_at_end(block);
        ctx.restore_initialized_slots(split_initialized.clone());
        let value = ctx.scoped_constant_value(&class_name, name)
            .unwrap_or_else(|| fallback_expr.clone());
        store_expr_into_temp(ctx, &temp_name, result_type.clone(), &value, expr.span);
        branch_to(ctx, merge);
    }
    ctx.builder.position_at_end(merge);
    let _ = split_initialized;
    take_owned_temp(ctx, &temp_name, expr.span)
}

/// Collects descendant classes that redefine a class constant, returning (class_name, class_id)
/// pairs sorted by class_id for deterministic dispatch.
pub(super) fn late_static_constant_candidates(
    ctx: &LoweringContext<'_, '_>,
    base_class: &str,
    const_name: &str,
) -> Vec<(String, u64)> {
    let base_value = ctx.scoped_constant_value(base_class, const_name);
    let mut candidates = Vec::new();
    for (class_name, class_info) in ctx.classes {
        if class_name == base_class {
            continue;
        }
        if !is_same_or_descendant_class(ctx, class_name, base_class) {
            continue;
        }
        let Some(value) = ctx.scoped_constant_value(class_name, const_name) else {
            continue;
        };
        if base_value.as_ref().is_some_and(|bv| expr_literals_equal(&value, bv)) {
            continue;
        }
        candidates.push((class_name.clone(), class_info.class_id));
    }
    candidates.sort_by_key(|(_, id)| *id);
    candidates
}

/// Returns true when `class_name` is `ancestor` or one of its descendants.
pub(super) fn is_same_or_descendant_class(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    ancestor: &str,
) -> bool {
    let mut cursor = Some(class_name);
    while let Some(name) = cursor {
        if name == ancestor {
            return true;
        }
        cursor = ctx
            .classes
            .get(name)
            .and_then(|info| info.parent.as_deref());
    }
    false
}

/// Compares two expressions for literal equality (used to skip redundant dispatch branches).
pub(super) fn expr_literals_equal(a: &Expr, b: &Expr) -> bool {
    match (&a.kind, &b.kind) {
        (ExprKind::IntLiteral(a), ExprKind::IntLiteral(b)) => a == b,
        (ExprKind::FloatLiteral(a), ExprKind::FloatLiteral(b)) => a == b,
        (ExprKind::StringLiteral(a), ExprKind::StringLiteral(b)) => a == b,
        (ExprKind::BoolLiteral(a), ExprKind::BoolLiteral(b)) => a == b,
        (ExprKind::Null, ExprKind::Null) => true,
        _ => false,
    }
}

/// Emits the fallback `Op::ScopedConstantGet` for unresolved scoped constants.
pub(super) fn lower_scoped_constant_fallback(ctx: &mut LoweringContext<'_, '_>, class_name: &str, name: &str, expr: &Expr) -> LoweredValue {
    let key = format!("{}::{}", class_name, name);
    let data = ctx.intern_string(&key);
    ctx.emit_value(
        Op::ScopedConstantGet,
        Vec::new(),
        Some(Immediate::Data(data)),
        fallback_expr_type(expr),
        Op::ScopedConstantGet.default_effects(),
        Some(expr.span),
    )
}

/// Lowers `new self`, `new static`, or `new parent`.
pub(super) fn lower_new_scoped_object(ctx: &mut LoweringContext<'_, '_>, receiver: &StaticReceiver, args: &[Expr], expr: &Expr) -> LoweredValue {
    if matches!(receiver, StaticReceiver::Static) {
        let fallback_class = ctx.current_class.clone().unwrap_or_else(|| receiver_name(receiver));
        let class_name = lower_class_constant(ctx, receiver, expr);
        let mut operands = vec![class_name.value];
        operands.extend(lower_args(ctx, args));
        let metadata = format!("{}|{}", fallback_class, fallback_class);
        let data = ctx.intern_class_name(&metadata);
        return ctx.emit_value(
            Op::DynamicObjectNew,
            operands,
            Some(Immediate::Data(data)),
            PhpType::Object(fallback_class),
            Op::DynamicObjectNew.default_effects(),
            Some(expr.span),
        );
    }
    let name = static_receiver_class_name(ctx, receiver).unwrap_or_else(|| receiver_name(receiver));
    let sig = constructor_signature(ctx, &Name::from(name.clone())).cloned();
    let operands = lower_args_with_signature(ctx, sig.as_ref(), args);
    emit_fixed_object_new(
        ctx,
        &name,
        operands,
        PhpType::Object(name.clone()),
        expr.span,
    )
}

/// Lowers a residual magic constant.
pub(super) fn lower_magic_constant(ctx: &mut LoweringContext<'_, '_>, kind: &MagicConstant, expr: &Expr) -> LoweredValue {
    let value = format!("__{:?}__", kind);
    lower_string_literal(ctx, &value, expr)
}
