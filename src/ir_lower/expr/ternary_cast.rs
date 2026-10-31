//! Purpose:
//! Ternary, cast, and scalar-coercion cleanup lowering.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.
//! - A string cast whose runtime value can carry the missing typed-array sentinel
//!   normalizes that sentinel into a real empty string. Computed string producers
//!   that cannot carry it stay unwrapped so their ownership remains visible.
//! - Whether a value can carry the sentinel is decided in `missing_string`, including
//!   stores a loop back-edge has not emitted yet.

use super::*;

/// Lowers a ternary expression with lazy branch evaluation.
pub(super) fn lower_ternary(
    ctx: &mut LoweringContext<'_, '_>,
    condition: &Expr,
    then_expr: &Expr,
    else_expr: &Expr,
    expr: &Expr,
) -> LoweredValue {
    lower_ternary_with_destination(ctx, condition, then_expr, else_expr, expr, None)
}

/// Lowers a ternary whose result is bound to `destination`, when one is known.
///
/// An array-literal branch is built against the destination, so a `[null]` default bound to
/// `array<ptr<T>|null>` (the fallback an associative unpack synthesizes for a missing key) holds
/// the null pointer word even when the merge temporary boxes it. An indexed pointer-array
/// destination also types the merge temporary itself and lowers every branch against it, so a
/// boxed local in either branch (`$b ? $a : [null]`) is converted to pointer words instead of
/// being merged through a `mixed` box.
pub(super) fn lower_ternary_with_destination(
    ctx: &mut LoweringContext<'_, '_>,
    condition: &Expr,
    then_expr: &Expr,
    else_expr: &Expr,
    expr: &Expr,
    destination: Option<&PhpType>,
) -> LoweredValue {
    let then_block = ctx.builder.create_named_block("ternary.then", Vec::new());
    let else_block = ctx.builder.create_named_block("ternary.else", Vec::new());
    lower_condition(ctx, condition, then_block, else_block);
    let pointer_array_destination =
        destination.filter(|ty| crate::types::is_pointer_element_indexed_array(ty));
    let result_type = match pointer_array_destination.map(PhpType::codegen_repr) {
        Some(PhpType::Array(elem)) => PhpType::Array(Box::new(elem.codegen_repr())),
        _ => branch_merge_result_type(ctx, then_expr, else_expr, expr),
    };
    let temp_name = ctx.declare_owned_hidden_temp(result_type.clone());
    let split_initialized = ctx.initialized_slots_snapshot();
    let merge = ctx.builder.create_named_block("ternary.merge", Vec::new());

    ctx.builder.position_at_end(then_block);
    ctx.restore_initialized_slots(split_initialized.clone());
    store_branch_into_temp(ctx, &temp_name, result_type.clone(), then_expr, expr.span, destination);
    let then_reachable = !ctx.builder.insertion_block_is_terminated();
    let then_initialized = ctx.initialized_slots_snapshot();
    branch_to(ctx, merge);

    ctx.builder.position_at_end(else_block);
    ctx.restore_initialized_slots(split_initialized.clone());
    store_branch_into_temp(ctx, &temp_name, result_type, else_expr, expr.span, destination);
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

/// Stores one ternary branch into the merge temporary, building an array literal against the
/// destination type when there is one, and any branch when the destination is a pointer array.
fn store_branch_into_temp(
    ctx: &mut LoweringContext<'_, '_>,
    temp_name: &str,
    temp_type: PhpType,
    branch: &Expr,
    span: crate::span::Span,
    destination: Option<&PhpType>,
) {
    match destination {
        Some(destination)
            if crate::types::is_pointer_element_indexed_array(destination)
                || matches!(branch.kind, ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_)) =>
        {
            let value = lower_expr_for_destination(ctx, branch, destination);
            store_value_into_temp(ctx, temp_name, temp_type, value, span);
        }
        _ => store_expr_into_temp(ctx, temp_name, temp_type, branch, span),
    }
}

/// Lowers a cast expression.
pub(super) fn lower_cast(ctx: &mut LoweringContext<'_, '_>, target: &CastType, inner: &Expr, expr: &Expr) -> LoweredValue {
    if matches!(target, CastType::Object) {
        return lower_object_cast(ctx, inner, expr);
    }
    if matches!(target, CastType::Array) {
        return lower_array_cast(ctx, inner, expr);
    }
    let value = lower_expr(ctx, inner);
    if matches!(target, CastType::Int | CastType::Float) {
        if let Some(result) = object_operators::lower_object_cast(ctx, value, target, expr) {
            if ctx.value_is_owning_temporary(value) {
                crate::ir_lower::ownership::release_if_owned(ctx, value, Some(expr.span));
            }
            return result;
        }
    }
    if matches!(target, CastType::Bool) {
        if let Some(result) = object_operators::lower_object_truthy(ctx, value, Some(expr.span)) {
            release_binary_operand_temporary(ctx, value, expr.span);
            return result;
        }
    }
    // Keep the original producer visible for a no-op string cast. Wrapping an
    // owned string temporary in `Cast(Str)` would hide its ownership from the
    // retaining store/call cleanup and leak the detached string allocation.
    // Missing typed-array elements are the exception: they stay string-typed
    // while carrying the shared null sentinel, including after a variable stores
    // them. The decision follows that runtime producer, not the cast operand's
    // immediate AST shape.
    if matches!(target, CastType::String) && value.ir_type == IrType::Str {
        if !super::missing_string::string_value_may_carry_missing_entry(ctx, value) {
            return value;
        }
        return normalize_missing_string_value(ctx, value, expr);
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
    } else if matches!(target, CastType::Int | CastType::Float | CastType::Bool)
        && ctx.value_needs_release_after_use(value)
    {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(expr.span));
    }
    result
}

/// Normalizes a string that may carry the missing-entry sentinel.
///
/// The cast starts borrowed: it aliases a real string or the canonical empty
/// string. A concrete `PhpLocal` or `StaticLocal` keeps that borrow and is not
/// copied: the slot still owns the bytes, and copying one on every call both
/// leaked and dominated the loop. A `??` temporary, and a load whose storage
/// no longer matches the string, is copied with `StrPersist` before it is
/// released. A local that widens only after this cast is borrowed on the first
/// pass and copied on the string-storage replay, once its finished slot is
/// known to free the load. Element reads that nobody owns stay borrowed.
fn normalize_missing_string_value(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    expr: &Expr,
) -> LoweredValue {
    let normalized = ctx.emit_value(
        Op::Cast,
        vec![value.value],
        Some(Immediate::CastTarget(value_ir_type(&PhpType::Str))),
        PhpType::Str,
        Op::Cast.default_effects(),
        Some(expr.span),
    );
    ctx.builder
        .set_value_ownership(normalized.value, Ownership::Borrowed);
    if !string_cast_source_is_released(ctx, value) {
        if let Some(name) = php_string_local_load_name(ctx, value) {
            ctx.provisional_borrowed_string_casts.insert(name);
        }
        return normalized;
    }
    // `Bool(true)` marks the copy of a normalized cast so a later release can
    // find it without freeing unrelated persisted strings.
    let owned = ctx.emit_owned_value(
        Op::StrPersist,
        vec![normalized.value],
        Some(Immediate::Bool(true)),
        PhpType::Str,
        Op::StrPersist.default_effects(),
        Some(expr.span),
    );
    crate::ir_lower::ownership::release_if_owned(ctx, value, Some(expr.span));
    owned
}

/// Returns whether freeing `value` after the cast would free bytes the cast aliases.
///
/// A concrete string local only lends its slot, so the cast stays borrowed and
/// nothing is released. A hidden or owned temporary (`??`) and a load that
/// already had to allocate a detached string are released, after the cast has
/// been copied. A local named by the string-storage replay is copied even when
/// this cast still sees the pre-widening slot type.
fn string_cast_source_is_released(ctx: &LoweringContext<'_, '_>, value: LoweredValue) -> bool {
    if ctx.value_is_owning_temporary(value) {
        return true;
    }
    let Some(inst) = ctx.builder.value_defining_instruction(value.value) else {
        return false;
    };
    if !matches!(inst.op, Op::LoadLocal | Op::LoadStaticLocal) {
        return false;
    }
    let Some(Immediate::LocalSlot(slot)) = inst.immediate else {
        return false;
    };
    if !matches!(
        ctx.builder.local_kind(slot),
        crate::ir::LocalKind::PhpLocal | crate::ir::LocalKind::StaticLocal
    ) {
        return true;
    }
    if php_string_local_load_name(ctx, value)
        .is_some_and(|name| ctx.owned_string_cast_locals.contains(&name))
    {
        return true;
    }
    let storage = ctx.builder.local_php_type(slot);
    let result = ctx.builder.value_php_type(value.value);
    crate::ir::local_load_coercion_owns_result(&storage, &result)
}

/// Returns the PHP name of a concrete string local load, when `value` is one.
fn php_string_local_load_name(ctx: &LoweringContext<'_, '_>, value: LoweredValue) -> Option<String> {
    let inst = ctx.builder.value_defining_instruction(value.value)?;
    if !matches!(inst.op, Op::LoadLocal | Op::LoadStaticLocal) {
        return None;
    }
    let Some(Immediate::LocalSlot(slot)) = inst.immediate else {
        return None;
    };
    if !matches!(
        ctx.builder.local_kind(slot),
        crate::ir::LocalKind::PhpLocal | crate::ir::LocalKind::StaticLocal
    ) {
        return None;
    }
    let name = ctx
        .builder
        .function()
        .locals
        .get(slot.as_raw() as usize)?
        .name
        .as_deref()?;
    Some(name.strip_suffix("#cow").unwrap_or(name).to_string())
}

/// Lowers PHP's `(array)` cast by the source classification it shares with the checker.
///
/// An array is returned UNCHANGED (PHP's `(array)` is the identity on an array), `null` becomes
/// a fresh empty array, and a scalar becomes a fresh one-element packed array holding it at
/// key 0. An object is projected to its property map by the backend's hash cast, and every
/// runtime-typed source is boxed when it is not already and dispatched on its runtime tag by
/// `__rt_mixed_cast_array`, which applies the same four rules.
fn lower_array_cast(ctx: &mut LoweringContext<'_, '_>, inner: &Expr, expr: &Expr) -> LoweredValue {
    let value = lower_expr(ctx, inner);
    let source_type = ctx.builder.value_php_type(value.value);
    let cast = crate::types::ArrayCast::for_source(&source_type);
    let result_type = cast.result_type(&source_type);
    match cast {
        crate::types::ArrayCast::Identity => value,
        crate::types::ArrayCast::Empty => ctx.emit_value(
            Op::ArrayNew,
            Vec::new(),
            Some(Immediate::Capacity(0)),
            result_type,
            Op::ArrayNew.default_effects(),
            Some(expr.span),
        ),
        crate::types::ArrayCast::WrapScalar(element_type) => {
            let array = ctx.emit_value(
                Op::ArrayNew,
                Vec::new(),
                Some(Immediate::Capacity(1)),
                result_type,
                Op::ArrayNew.default_effects(),
                Some(expr.span),
            );
            ctx.emit_void(
                Op::ArrayPush,
                vec![array.value, value.value],
                None,
                Op::ArrayPush.default_effects(),
                Some(inner.span),
            );
            crate::ir_lower::stmt::release_indexed_array_write_operand(
                ctx,
                Some(&element_type),
                value,
                inner.span,
            );
            array
        }
        crate::types::ArrayCast::ObjectProperties | crate::types::ArrayCast::Dynamic => {
            let source = if matches!(cast, crate::types::ArrayCast::Dynamic)
                && !matches!(value.ir_type, IrType::Heap(IrHeapKind::Mixed | IrHeapKind::Union))
            {
                ctx.box_value_as_mixed(value, PhpType::Mixed, Some(inner.span))
            } else {
                value
            };
            let result = ctx.emit_value(
                Op::Cast,
                vec![source.value],
                Some(Immediate::CastTarget(value_ir_type(&result_type))),
                result_type,
                Op::Cast.default_effects(),
                Some(expr.span),
            );
            if ctx.value_is_owning_temporary(source) {
                crate::ir_lower::ownership::release_if_owned(ctx, source, Some(expr.span));
            }
            result
        }
    }
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

/// Releases independent scalar-coercion sources, including boxes materialized by local loads.
pub(super) fn release_coerced_source_if_owned(
    ctx: &mut LoweringContext<'_, '_>,
    source: LoweredValue,
    span: Option<crate::span::Span>,
) {
    if !ctx.value_needs_release_after_use(source) {
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
///
/// Shared with array-literal source typing so an `(array)` element is classified from the
/// same type this lowering attaches to the value.
pub(super) fn cast_php_type(target: &CastType, source_type: &PhpType) -> PhpType {
    crate::types::php_cast_result_type(target, source_type)
}
