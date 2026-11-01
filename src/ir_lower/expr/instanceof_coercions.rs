//! Purpose:
//! Instanceof and scalar coercion helpers.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Lowers `instanceof`.
///
/// Native and eval predicates borrow their operands. Retire independent owners,
/// including class-name strings detached from widened Mixed locals, after use.
/// Root an owning value operand before evaluating a dynamic target that may throw;
/// a local read can also own a detached payload independently of its source slot.
/// Borrowed objects and the scalar result keep their existing lifetimes.
pub(super) fn lower_instanceof(
    ctx: &mut LoweringContext<'_, '_>,
    value: &Expr,
    target: &InstanceOfTarget,
    expr: &Expr,
) -> LoweredValue {
    let mut value_operand = lower_expr(ctx, value);
    if statically_known_instanceof_result(ctx, expr) == Some(false) {
        if ctx.value_is_owning_temporary(value_operand) {
            crate::ir_lower::ownership::release_if_owned(ctx, value_operand, Some(expr.span));
        }
        return lower_bool_literal(ctx, false, expr);
    }
    let mut operands = vec![value_operand.value];
    // Set to the frame slot when the value operand is rooted across a dynamic
    // target evaluation that can unwind.
    let mut value_root: Option<crate::ir::LocalSlotId> = None;
    // Set to the lowered dynamic-target operand, which this predicate owns and
    // must retire after the op has read it.
    let mut dynamic_target: Option<LoweredValue> = None;
    let immediate = match target {
        // Lowering only ever sees the instantiated class, which is an ordinary `Name`.
        InstanceOfTarget::Generic(_) => unreachable!(
            "InstanceOfTarget::Generic must be instantiated by generics::classes"
        ),
        InstanceOfTarget::Name(name) => {
            if name.as_str().trim_start_matches('\\') == "static" && ctx.local_slots.contains_key("this") {
                // Apply the same storage-aware retirement to the implicit target
                // as to an explicit local read. Finalization preserves true borrows.
                let target_operand = ctx.load_local("this", Some(expr.span));
                operands.push(target_operand.value);
                dynamic_target = Some(target_operand);
                None
            } else if name.as_str().trim_start_matches('\\') == "static" && ctx.current_class.is_some() {
                // `$value instanceof static` in a STATIC method: there is no `$this` to take the
                // class from, and the literal name "static" matched nothing, so the test was
                // always false -- brick/math's `BigNumber::of()` never recognised a number already
                // of the requested class. The called class answers it, as `static::class` does.
                let called = Expr::new(
                    ExprKind::ClassConstant {
                        receiver: crate::parser::ast::StaticReceiver::Static,
                    },
                    expr.span,
                );
                operands.push(lower_expr(ctx, &called).value);
                None
            } else {
                Some(Immediate::Data(ctx.intern_class_name(&instanceof_target_name(ctx, name.as_str()))))
            }
        }
        InstanceOfTarget::Expr(target_expr) => {
            let (rooted, slot) = root_owned_call_operand(ctx, value_operand, expr.span);
            value_operand = rooted;
            operands[0] = rooted.value;
            value_root = slot;
            let target_operand = lower_expr(ctx, target_expr);
            operands.push(target_operand.value);
            dynamic_target = Some(target_operand);
            None
        }
    };
    let op = if immediate.is_some() { Op::InstanceOf } else { Op::InstanceOfDynamic };
    let result = ctx.emit_value(op, operands, immediate, PhpType::Bool, op.default_effects(), Some(expr.span));
    // Retire the dynamic target operand first, then the value operand, so an
    // independently owned temporary on either side is released exactly once.
    if let Some(target_operand) = dynamic_target {
        if target_operand.value != value_operand.value
            && ctx.value_needs_release_after_use(target_operand)
        {
            crate::ir_lower::ownership::release_if_owned(ctx, target_operand, Some(expr.span));
        }
    }
    match value_root {
        Some(slot) => retire_owned_call_operand(ctx, slot, expr.span),
        None => {
            if ctx.value_needs_release_after_use(value_operand) {
                crate::ir_lower::ownership::release_if_owned(ctx, value_operand, Some(expr.span));
            }
        }
    }
    result
}

/// Returns the closed-world result of a named `instanceof` whose target has no emitted metadata.
///
/// The left-hand value must still be evaluated by the caller. An earlier `eval()` disables the
/// proof because runtime code may have declared the target class or interface. Negation is folded
/// recursively so branch lowering can omit the unreachable arm without relying on IR passes.
pub(in crate::ir_lower) fn statically_known_instanceof_result(
    ctx: &LoweringContext<'_, '_>,
    expr: &Expr,
) -> Option<bool> {
    if let ExprKind::Not(inner) = &expr.kind {
        return statically_known_instanceof_result(ctx, inner).map(|value| !value);
    }
    let ExprKind::InstanceOf {
        target: InstanceOfTarget::Name(name),
        ..
    } = &expr.kind
    else {
        return None;
    };
    // `eval_executed()` alone is a FLOW fact inside ONE function: it is false in a function that
    // contains no `eval()` of its own, even when the program runs one elsewhere. The proof this
    // guard protects is a CLOSED-WORLD claim about the whole program, so it needs the
    // program-level answer too — the same one dynamic function resolution already consults.
    //
    // Symfony is the case: `FlattenException::createFromThrowable()` is compiled and asks
    // `$e instanceof HttpExceptionInterface`, while that interface is autoloaded at runtime. The
    // fold answered a hardcoded `false`, so every `NotFoundHttpException` lost its status code and
    // Symfony rendered 500 where php renders 404.
    if ctx.eval_executed() || ctx.can_resolve_runtime_dynamic_functions() {
        return None;
    }
    // `static` names the late-static class, which only the running call knows. It is no entry in
    // the class table, so looking it up here "proved" every `$x instanceof static` false.
    if name.as_str().trim_start_matches('\\').eq_ignore_ascii_case("static") {
        return None;
    }
    let target = instanceof_target_name(ctx, name.as_str());
    if target
        .trim_start_matches('\\')
        .eq_ignore_ascii_case("Closure")
    {
        // `Closure` is a PHP intrinsic represented by a callable descriptor rather than a
        // normal entry in the AOT class metadata registry. The backend has a dedicated tag-10
        // check, so it must see this expression even when no type declaration referenced
        // `Closure` while building the registry.
        return None;
    }
    let known = ctx.classes.contains_key(&target)
        || ctx.interfaces.contains_key(&target)
        || ctx.enums.contains_key(&target);
    (!known).then_some(false)
}

/// Resolves lexical `instanceof` target keywords to concrete class names when possible.
fn instanceof_target_name(
    ctx: &LoweringContext<'_, '_>,
    name: &str,
) -> String {
    match name.trim_start_matches('\\') {
        "self" => ctx.current_class.clone().unwrap_or_else(|| name.to_string()),
        "parent" => ctx
            .current_class
            .as_deref()
            .and_then(|class_name| ctx.classes.get(class_name))
            .and_then(|class_info| class_info.parent.clone())
            .unwrap_or_else(|| name.to_string()),
        _ => name.to_string(),
    }
}

/// Returns the positive nominal local-type fact proven by one static `instanceof` branch.
pub(in crate::ir_lower) fn instanceof_branch_local_type(
    ctx: &LoweringContext<'_, '_>,
    condition: &Expr,
    branch_matches: bool,
) -> Option<(String, PhpType)> {
    let (name, target) = positive_instanceof_local(condition, branch_matches)?;
    let class_name = instanceof_target_name(ctx, target);
    if class_name.trim_start_matches('\\') == "static" {
        return None;
    }
    // `instanceof` is an INTERSECTION: replacing the local's type with the target drops whatever
    // the other half declared. Taking the target on its own here typed `$this` as the bare
    // INTERFACE, and an interface has no properties -- the first `$this->context` read survived
    // only because the checker had recorded a type for that exact span, and the SECOND one, where
    // the checker's own narrowing had already lapsed, came back `Int` and was refused as a method
    // receiver. Both sides now ask one module the same question.
    let current_type = ctx.local_types.get(name).cloned().or_else(|| {
        (name == "this")
            .then(|| ctx.current_class.clone().map(PhpType::Object))
            .flatten()
    });
    let narrowed = match current_type {
        Some(PhpType::Object(current)) => crate::types::instanceof_intersection::
            narrow_object_to_instanceof(ctx.classes, ctx.interfaces, &current, &class_name),
        // A scalar or array local is never an instance of anything, so the branch is dead, and
        // retyping its slot as an object only makes the dead read unloadable (Twig's compiled
        // `($tmp = !$x) && $tmp instanceof Markup ? (string) $tmp : $tmp`). The checker keeps the
        // same type on that edge (`stmt_check::narrowing`).
        Some(
            PhpType::Int
            | PhpType::Float
            | PhpType::Bool
            | PhpType::False
            | PhpType::Str
            | PhpType::Array(_)
            | PhpType::AssocArray { .. },
        ) => return None,
        _ => None,
    };
    Some((
        name.to_string(),
        narrowed.unwrap_or(PhpType::Object(class_name)),
    ))
}

/// Extracts a local and named target when the selected branch proves an `instanceof` condition.
fn positive_instanceof_local(condition: &Expr, branch_matches: bool) -> Option<(&str, &str)> {
    if let ExprKind::Not(inner) = &condition.kind {
        return positive_instanceof_local(inner, !branch_matches);
    }
    // `A && B` is true only when A is, so the TRUE branch of the whole condition carries every
    // narrowing A proves. Without this, `if ($r instanceof StreamedResponse && $r->getCallback())`
    // narrowed the right operand (see `lower_logical_binary`) but not the BODY, and the first call
    // on the proven receiver inside the block was refused as an unknown method — Symfony's
    // `HttpKernel::handle` calls `setCallback` there.
    //
    // Only the true side, and only for `&&`: a false `A && B` says nothing about A, and `||` on
    // its true side says nothing either.
    if let ExprKind::BinaryOp {
        left,
        op: crate::parser::ast::BinOp::And,
        right,
    } = &condition.kind
    {
        if branch_matches {
            return positive_instanceof_local(left, true)
                .or_else(|| positive_instanceof_local(right, true));
        }
        return None;
    }
    if !branch_matches {
        return None;
    }
    let ExprKind::InstanceOf {
        value,
        target: InstanceOfTarget::Name(target),
    } = &condition.kind
    else {
        return None;
    };
    let name = match &value.kind {
        ExprKind::Variable(name) => name.as_str(),
        ExprKind::This => "this",
        _ => return None,
    };
    Some((name, target.as_str()))
}

/// Coerces a value to integer storage before integer-only operations.
pub(super) fn coerce_to_int(ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, expr: &Expr) -> LoweredValue {
    coerce_to_int_at_span(ctx, value, Some(expr.span))
}

/// Coerces a value to integer storage using an explicit source span.
pub(crate) fn coerce_to_int_at_span(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<crate::span::Span>,
) -> LoweredValue {
    match value.ir_type {
        IrType::I64
            if matches!(
                ctx.builder.value_php_type(value.value).codegen_repr(),
                PhpType::Mixed | PhpType::Union(_)
            ) =>
        {
            ctx.emit_value(
                Op::Move,
                vec![value.value],
                None,
                PhpType::Int,
                Op::Move.default_effects(),
                span,
            )
        }
        IrType::I64 => value,
        IrType::F64 => ctx.emit_value(Op::FToI, vec![value.value], None, PhpType::Int, Op::FToI.default_effects(), span),
        IrType::Str => {
            let result = ctx.emit_value(Op::StrToI, vec![value.value], None, PhpType::Int, Op::StrToI.default_effects(), span);
            // The integer cannot alias a string copied to capture an operand.
            if ctx.value_needs_release_after_use(value) {
                crate::ir_lower::ownership::release_if_owned(ctx, value, span);
            }
            result
        }
        _ => {
            let result = ctx.emit_value(
                Op::Cast,
                vec![value.value],
                Some(Immediate::CastTarget(IrType::I64)),
                PhpType::Int,
                Op::Cast.default_effects(),
                span,
            );
            // The cast lowers to `__rt_mixed_cast_int`, which returns a raw
            // scalar that never aliases the source box. Dropping the owning
            // reference here leaked one checked-arithmetic Mixed cell per
            // evaluation for `%`, bitops, comparisons, and coerced array
            // indexes with a compound operand (issue #500).
            release_coerced_source_if_owned(ctx, value, span);
            result
        }
    }
}

/// Converts a packed-array float key with PHP diagnostics unless its read already did so.
pub(crate) fn coerce_array_key_to_int_at_span(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<crate::span::Span>,
    already_diagnosed: bool,
) -> LoweredValue {
    if value.ir_type == IrType::F64 && !already_diagnosed {
        ctx.emit_value(
            Op::FToI,
            vec![value.value],
            Some(Immediate::FloatKeyDiagnostic),
            PhpType::Int,
            Op::FToI.default_effects() | Effects::MAY_WARN,
            span,
        )
    } else {
        coerce_to_int_at_span(ctx, value, span)
    }
}

/// Coerces a value to float when the storage type allows a direct conversion.
pub(super) fn coerce_to_float(ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, expr: &Expr) -> LoweredValue {
    coerce_to_float_at_span(ctx, value, Some(expr.span))
}

/// Coerces a value to float storage using an explicit source span.
pub(super) fn coerce_to_float_at_span(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<crate::span::Span>,
) -> LoweredValue {
    match value.ir_type {
        IrType::F64 => value,
        IrType::I64 => ctx.emit_value(Op::IToF, vec![value.value], None, PhpType::Float, Op::IToF.default_effects(), span),
        _ => {
            let result = ctx.emit_value(
                Op::Cast,
                vec![value.value],
                Some(Immediate::CastTarget(IrType::F64)),
                PhpType::Float,
                Op::Cast.default_effects(),
                span,
            );
            // Mirror of the int coercion above: `__rt_mixed_cast_float`
            // returns a raw scalar, so the owning source box (e.g. a checked
            // `pow` operand, issue #500) must be released here.
            release_coerced_source_if_owned(ctx, value, span);
            result
        }
    }
}

/// Coerces a value to string when possible.
pub(super) fn coerce_to_string(ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, expr: &Expr) -> LoweredValue {
    coerce_to_string_at_span(ctx, value, Some(expr.span))
}

/// Coerces a value to string storage using an explicit source span.
pub(crate) fn coerce_to_string_at_span(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<crate::span::Span>,
) -> LoweredValue {
    if matches!(ctx.builder.value_php_type(value.value), PhpType::Resource(_)) {
        return ctx.emit_value(
            Op::ResourceToStr,
            vec![value.value],
            None,
            PhpType::Str,
            Op::ResourceToStr.default_effects(),
            span,
        );
    }
    // A boxed gradual value is physically one integer-sized cell even when its runtime payload
    // is a string. Inspecting only `ir_type` would route that cell through `IToStr` and stringify
    // its address. The PHP representation is authoritative at this boundary, so unwrap/cast the
    // Mixed cell before considering the physical scalar register class.
    if matches!(
        ctx.builder.value_php_type(value.value).codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    ) {
        let result = ctx.emit_value(
            Op::Cast,
            vec![value.value],
            Some(Immediate::CastTarget(IrType::Str)),
            PhpType::Str,
            Op::Cast.default_effects(),
            span,
        );
        release_coerced_source_if_owned(ctx, value, span);
        return result;
    }
    match value.ir_type {
        IrType::Str => value,
        IrType::I64 | IrType::TaggedScalar => ctx.emit_value(Op::IToStr, vec![value.value], None, PhpType::Str, Op::IToStr.default_effects(), span),
        IrType::F64 => ctx.emit_value(Op::FToStr, vec![value.value], None, PhpType::Str, Op::FToStr.default_effects(), span),
        _ => {
            let result = ctx.emit_value(
                Op::Cast,
                vec![value.value],
                Some(Immediate::CastTarget(IrType::Str)),
                PhpType::Str,
                Op::Cast.default_effects(),
                span,
            );
            release_coerced_source_if_owned(ctx, value, span);
            result
        }
    }
}
