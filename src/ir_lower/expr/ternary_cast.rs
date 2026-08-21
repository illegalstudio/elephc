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
    let value = lower_expr(ctx, inner);
    if let Some(result) = lower_simplexml_scalar_cast(ctx, target, value, expr) {
        return result;
    }
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

/// Routes SimpleXML scalar casts through the native object handler.
fn lower_simplexml_scalar_cast(
    ctx: &mut LoweringContext<'_, '_>,
    target: &CastType,
    value: LoweredValue,
    expr: &Expr,
) -> Option<LoweredValue> {
    lower_simplexml_scalar_cast_at_span(ctx, target, value, expr.span)
}

/// Routes one already-lowered SimpleXML cast through identity or its native handler.
pub(super) fn lower_simplexml_scalar_cast_at_span(
    ctx: &mut LoweringContext<'_, '_>,
    target: &CastType,
    value: LoweredValue,
    span: Span,
) -> Option<LoweredValue> {
    let source_type = ctx.builder.value_php_type(value.value);
    let PhpType::Object(class_name) =
        crate::ir_lower::internal_extensions::simplexml_object_result_type(ctx, &source_type)?
    else {
        return None;
    };
    if matches!(target, CastType::Object) {
        return Some(value);
    }
    let (kind, result_type) = match target {
        CastType::Bool => (0, PhpType::Bool),
        CastType::Int => (1, PhpType::Int),
        CastType::Float => (2, PhpType::Float),
        CastType::String if !simplexml_descendant_declares_method(ctx, &class_name, "__toString") => {
            (3, PhpType::Str)
        }
        CastType::Array => (
            5,
            PhpType::AssocArray { key: Box::new(PhpType::Mixed), value: Box::new(PhpType::Mixed) },
        ),
        CastType::String | CastType::Object => return None,
    };
    let opcode = crate::ir_lower::internal_extensions::simplexml_object_handler_opcode_for_type(
        ctx,
        &source_type,
        "cast",
    )?;
    let discriminator = emit_i64_at_span(ctx, kind, span);
    let result = crate::ir_lower::internal_extensions::emit_call(
        ctx,
        opcode,
        crate::ir_lower::internal_extensions::FLAG_RECEIVER,
        vec![value.value, discriminator.value],
        result_type,
        span,
    );
    if ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    }
    Some(result)
}

/// Reports whether a userland SimpleXML descendant overrides one native conversion method.
fn simplexml_descendant_declares_method(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    method: &str,
) -> bool {
    let method = php_symbol_key(method);
    let mut current = class_name.trim_start_matches('\\').to_string();
    loop {
        if current.eq_ignore_ascii_case("SimpleXMLElement") {
            return false;
        }
        let Some(class_info) = ctx.classes.get(&current) else {
            return false;
        };
        if class_info.method_decls.iter().any(|declaration| {
            declaration.has_body && php_symbol_key(&declaration.name) == method
        }) {
            return true;
        }
        let Some(parent) = class_info.parent.as_deref() else {
            return false;
        };
        current = parent.trim_start_matches('\\').to_string();
    }
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
    }
}
