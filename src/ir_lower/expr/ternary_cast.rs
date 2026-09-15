//! Purpose:
//! Ternary, cast, and scalar-coercion cleanup lowering.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Lowers a ternary expression with lazy branch evaluation.
pub(super) fn lower_ternary(
    ctx: &mut LoweringContext<'_, '_>,
    condition: &Expr,
    then_expr: &Expr,
    else_expr: &Expr,
    expr: &Expr,
) -> LoweredValue {
    let cond = lower_expr(ctx, condition);
    let cond = ctx.truthy_consuming(cond, Some(condition.span));
    let result_type = branch_merge_result_type(ctx, then_expr, else_expr, expr);
    let temp_name = ctx.declare_owned_hidden_temp(result_type.clone());
    let split_initialized = ctx.initialized_slots_snapshot();
    let then_block = ctx.builder.create_named_block("ternary.then", Vec::new());
    let else_block = ctx.builder.create_named_block("ternary.else", Vec::new());
    let merge = ctx.builder.create_named_block("ternary.merge", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: cond.value,
        then_target: then_block,
        then_args: Vec::new(),
        else_target: else_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(then_block);
    ctx.restore_initialized_slots(split_initialized.clone());
    store_expr_into_temp(ctx, &temp_name, result_type.clone(), then_expr, expr.span);
    let then_reachable = !ctx.builder.insertion_block_is_terminated();
    let then_initialized = ctx.initialized_slots_snapshot();
    branch_to(ctx, merge);

    ctx.builder.position_at_end(else_block);
    ctx.restore_initialized_slots(split_initialized.clone());
    store_expr_into_temp(ctx, &temp_name, result_type, else_expr, expr.span);
    let else_reachable = !ctx.builder.insertion_block_is_terminated();
    let else_initialized = ctx.initialized_slots_snapshot();
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    ctx.restore_initialized_slots(merge_initialized_slots_for_expr(
        &split_initialized,
        then_initialized,
        then_reachable,
        else_initialized,
        else_reachable,
    ));
    take_owned_temp(ctx, &temp_name, expr.span)
}

/// Lowers a cast expression.
pub(super) fn lower_cast(ctx: &mut LoweringContext<'_, '_>, target: &CastType, inner: &Expr, expr: &Expr) -> LoweredValue {
    if matches!(target, CastType::Object) {
        return lower_object_cast(ctx, inner, expr);
    }
    let value = lower_expr(ctx, inner);
    // Keep the original producer visible for a no-op string cast. Wrapping an
    // owned string temporary in `Cast(Str)` would hide its ownership from the
    // retaining store/call cleanup and leak the detached string allocation.
    if matches!(target, CastType::String) && value.ir_type == IrType::Str {
        return value;
    }
    let source_type = ctx.builder.value_php_type(value.value);
    let php_type = cast_php_type(target, &source_type);
    let result = ctx.emit_value(
        Op::Cast,
        vec![value.value],
        Some(Immediate::CastTarget(value_ir_type(&php_type))),
        php_type,
        Op::Cast.default_effects(),
        Some(expr.span),
    );
    if matches!(target, CastType::String) {
        release_coerced_source_if_owned(ctx, value, Some(expr.span));
    } else if matches!(target, CastType::Int | CastType::Float | CastType::Bool | CastType::Array)
        && ctx.value_is_owning_temporary(value)
    {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(expr.span));
    }
    result
}

/// Lowers PHP's `(object)` cast.
///
/// An object source is returned UNCHANGED — PHP's `(object)` is the identity on an object,
/// so no copy is made and `(object) $o === $o` holds. Every other source is converted by the
/// elephc-PHP helpers `object_cast_prelude` injects, which is what keeps the conversion
/// (array keys become property names, `null` becomes an empty stdClass, a scalar becomes a
/// `scalar` property) correct on every supported target with no per-target assembly.
///
/// The source is lowered ONCE into a synthetic local, and the helper call then names that
/// local — the same rewrite `ref_place_args` uses — so the ordinary user-call path handles
/// argument lowering, the return type, and owned-temporary release, and the source's side
/// effects happen exactly once.
fn lower_object_cast(
    ctx: &mut LoweringContext<'_, '_>,
    inner: &Expr,
    expr: &Expr,
) -> LoweredValue {
    let value = lower_expr(ctx, inner);
    let source_type = ctx.builder.value_php_type(value.value);
    if matches!(source_type.codegen_repr(), PhpType::Object(_)) {
        return value;
    }
    let helper = if source_type.is_php_array() {
        crate::object_cast_prelude::CAST_HELPER
    } else if matches!(source_type.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        crate::object_cast_prelude::DYNAMIC_CAST_HELPER
    } else {
        crate::object_cast_prelude::CAST_HELPER
    };
    let local_type = normalize_value_php_type(source_type);
    let temp = ctx.declare_synthetic_php_local(local_type.clone());
    ctx.store_local(&temp, value, local_type, Some(inner.span));
    let argument = Expr::new(ExprKind::Variable(temp), inner.span);
    let name = Name::from(helper.to_string());
    lower_function_call(ctx, &name, std::slice::from_ref(&argument), expr)
}

/// Releases an owning temporary when a scalar coercion cannot alias its source storage.
pub(super) fn release_coerced_source_if_owned(
    ctx: &mut LoweringContext<'_, '_>,
    source: LoweredValue,
    span: Option<crate::span::Span>,
) {
    if !ctx.value_is_owning_temporary(source) {
        return;
    }
    if !coerced_source_repr_is_releasable(&ctx.builder.value_php_type(source.value)) {
        return;
    }
    crate::ir_lower::ownership::release_if_owned(ctx, source, span);
}

/// Returns true when a coerced source's codegen repr is a heap shape the scalar
/// coercion casts never alias, so the coercers can release it internally.
///
/// Boxed Mixed sources are safe to release: the backend lowers
/// `cast Mixed -> Str/I64/F64` through `__rt_mixed_cast_string` /
/// `__rt_mixed_cast_int` / `__rt_mixed_cast_float`. String payloads are
/// persisted into an independent allocation; scalar and null payloads return
/// source-independent conversion storage or raw scalars. The produced value
/// therefore never aliases the released Mixed cell. Skipping Mixed leaked
/// every owned boxed temporary that flowed into a string coercion — e.g.
/// `echo $row[1] . "\n"` inside a by-value `foreach` leaked the `$row[1]`
/// element box each iteration (issue #527) — and every checked-arithmetic
/// box consumed directly by `%`, bitops, comparisons, or array indexes
/// (issue #500). `release_if_owned` only type-gates the EIR Release; backend
/// ownership filtering releases Owned/MaybeOwned and skips NonHeap, Borrowed,
/// Persistent, and Moved. Non-null unions such as int|string codegen-repr to
/// Mixed; tagged nullable-int unions bypass this predicate.
pub(super) fn coerced_source_repr_is_releasable(php_type: &PhpType) -> bool {
    matches!(
        php_type.codegen_repr(),
        PhpType::Object(_) | PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Mixed
    )
}

/// Returns the PHP type produced by a cast.
pub(super) fn cast_php_type(target: &CastType, source_type: &PhpType) -> PhpType {
    match target {
        CastType::Int => PhpType::Int,
        CastType::Float => PhpType::Float,
        CastType::String => PhpType::Str,
        CastType::Bool => PhpType::Bool,
        CastType::Array
            if matches!(source_type.codegen_repr(), PhpType::Object(_)) =>
        PhpType::AssocArray {
            key: Box::new(PhpType::Str),
            value: Box::new(PhpType::Mixed),
        },
        CastType::Array
            if matches!(
                source_type.codegen_repr(),
                PhpType::Mixed | PhpType::Union(_)
            ) => PhpType::Mixed,
        CastType::Array => PhpType::Array(Box::new(PhpType::Mixed)),
        // Mirrors the checker's `(object)` arms in
        // `types::checker::inference::expr::basic`: identity on an object, `mixed` for a
        // runtime-typed source that may already hold an unrelated class, stdClass otherwise.
        CastType::Object if matches!(source_type.codegen_repr(), PhpType::Object(_)) => {
            source_type.clone()
        }
        CastType::Object
            if matches!(
                source_type.codegen_repr(),
                PhpType::Mixed | PhpType::Union(_)
            ) =>
        {
            PhpType::Mixed
        }
        CastType::Object => PhpType::Object("stdClass".to_string()),
    }
}
