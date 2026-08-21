//! Purpose:
//! Comparison lowering, temporary cleanup, and DateTime/SimpleXML comparison semantics.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Lowers a comparison operation.
pub(super) fn lower_compare(
    ctx: &mut LoweringContext<'_, '_>,
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    expr: &Expr,
) -> LoweredValue {
    let mut lhs = lower_expr(ctx, left);
    let mut rhs = lower_expr(ctx, right);
    // DateTime-family value comparison: PHP orders `DateTime`/`DateTimeImmutable` by their absolute
    // instant (timestamp seconds + microsecond), independent of the stored timezone. Replace each
    // object operand with a monotonic integer instant key so `==`, `!=`, `<`, `<=`, `>`, `>=`, and
    // `<=>` reduce to ordinary integer comparison. Identity `===`/`!==` is deliberately excluded so
    // it keeps comparing object references.
    if datetime_instant_compare_operator(op)
        && is_datetime_family_value(ctx, lhs.value)
        && is_datetime_family_value(ctx, rhs.value)
    {
        let lhs_key = lower_datetime_instant_key(ctx, lhs, expr);
        let rhs_key = lower_datetime_instant_key(ctx, rhs, expr);
        release_binary_operand_temporary(ctx, lhs, expr.span);
        if rhs.value != lhs.value {
            release_binary_operand_temporary(ctx, rhs, expr.span);
        }
        lhs = lhs_key;
        rhs = rhs_key;
    }
    if let Some(result) = lower_simplexml_object_compare(ctx, lhs, op, rhs, expr) {
        return result;
    }
    (lhs, rhs) = coerce_simplexml_string_comparison_operands(ctx, lhs, op, rhs, expr.span);
    let opcode = match op {
        BinOp::StrictEq => Op::StrictEq,
        BinOp::StrictNotEq => Op::StrictNotEq,
        BinOp::Eq => Op::LooseEq,
        BinOp::NotEq => Op::LooseNotEq,
        BinOp::Spaceship => Op::Spaceship,
        _ if lhs.ir_type == IrType::F64 || rhs.ir_type == IrType::F64 => Op::FCmp,
        _ if lhs.ir_type == IrType::I64 && rhs.ir_type == IrType::I64 => Op::ICmp,
        _ if lhs.ir_type == IrType::Str && rhs.ir_type == IrType::Str => Op::StrCmp,
        _ => Op::ICmp,
    };
    if matches!(opcode, Op::FCmp) {
        lhs = coerce_to_float(ctx, lhs, left);
        rhs = coerce_to_float(ctx, rhs, right);
    } else if matches!(opcode, Op::ICmp) {
        lhs = coerce_to_int(ctx, lhs, left);
        rhs = coerce_to_int(ctx, rhs, right);
    }
    let immediate = if matches!(opcode, Op::ICmp | Op::FCmp | Op::StrCmp) {
        Some(Immediate::CmpPredicate(cmp_predicate(op)))
    } else {
        None
    };
    let php_type = if matches!(op, BinOp::Spaceship) { PhpType::Int } else { PhpType::Bool };
    let result = ctx.emit_value(
        opcode,
        vec![lhs.value, rhs.value],
        immediate,
        php_type,
        opcode.default_effects(),
        Some(expr.span),
    );
    release_binary_operand_temporary(ctx, lhs, expr.span);
    if rhs.value != lhs.value {
        release_binary_operand_temporary(ctx, rhs, expr.span);
    }
    result
}

/// Casts only the SimpleXML side of loose wrapper/string comparisons before dispatch.
fn coerce_simplexml_string_comparison_operands(
    ctx: &mut LoweringContext<'_, '_>,
    lhs: LoweredValue,
    op: &BinOp,
    rhs: LoweredValue,
    span: Span,
) -> (LoweredValue, LoweredValue) {
    if !matches!(op, BinOp::Eq | BinOp::NotEq) {
        return (lhs, rhs);
    }
    let lhs_type = ctx.builder.value_php_type(lhs.value);
    let rhs_type = ctx.builder.value_php_type(rhs.value);
    if crate::ir_lower::internal_extensions::simplexml_object_result_type(ctx, &lhs_type).is_some()
        && rhs_type == PhpType::Str
    {
        return (coerce_to_string_at_span(ctx, lhs, Some(span)), rhs);
    }
    if lhs_type == PhpType::Str
        && crate::ir_lower::internal_extensions::simplexml_object_result_type(ctx, &rhs_type)
            .is_some()
    {
        return (lhs, coerce_to_string_at_span(ctx, rhs, Some(span)));
    }
    (lhs, rhs)
}

/// Routes non-identity SimpleXML comparisons through php-src object or boolean handlers.
fn lower_simplexml_object_compare(
    ctx: &mut LoweringContext<'_, '_>,
    lhs: LoweredValue,
    op: &BinOp,
    rhs: LoweredValue,
    expr: &Expr,
) -> Option<LoweredValue> {
    if matches!(op, BinOp::StrictEq | BinOp::StrictNotEq) {
        return None;
    }
    let lhs_type = ctx.builder.value_php_type(lhs.value);
    let rhs_type = ctx.builder.value_php_type(rhs.value);
    let lhs_simplexml =
        crate::ir_lower::internal_extensions::simplexml_object_result_type(ctx, &lhs_type);
    let rhs_simplexml =
        crate::ir_lower::internal_extensions::simplexml_object_result_type(ctx, &rhs_type);
    if lhs_simplexml.is_some() && simplexml_boolean_comparison_type(&rhs_type) {
        return Some(lower_simplexml_boolean_compare(
            ctx, lhs, op, rhs, expr, &lhs_type, true,
        ));
    }
    if rhs_simplexml.is_some() && simplexml_boolean_comparison_type(&lhs_type) {
        return Some(lower_simplexml_boolean_compare(
            ctx, lhs, op, rhs, expr, &rhs_type, false,
        ));
    }
    let PhpType::Object(_) = lhs_simplexml? else {
        return None;
    };
    let PhpType::Object(_) = rhs_simplexml? else {
        return None;
    };
    if simplexml_type_may_fail(&lhs_type) || simplexml_type_may_fail(&rhs_type) {
        return Some(lower_fallible_simplexml_object_compare(
            ctx,
            lhs,
            op,
            rhs,
            expr,
            &lhs_type,
            &rhs_type,
        ));
    }
    let comparison = emit_simplexml_object_comparison_unreleased(
        ctx,
        lhs.value,
        rhs.value,
        &lhs_type,
        expr.span,
    );
    release_binary_operand_temporary(ctx, lhs, expr.span);
    if rhs.value != lhs.value {
        release_binary_operand_temporary(ctx, rhs, expr.span);
    }
    Some(simplexml_compare_result(ctx, comparison, op, expr.span))
}

/// Casts the SimpleXML side of an object/bool comparison before applying scalar ordering.
fn lower_simplexml_boolean_compare(
    ctx: &mut LoweringContext<'_, '_>,
    lhs: LoweredValue,
    op: &BinOp,
    rhs: LoweredValue,
    expr: &Expr,
    simplexml_type: &PhpType,
    simplexml_on_left: bool,
) -> LoweredValue {
    let simplexml_value = if simplexml_on_left { lhs.value } else { rhs.value };
    let simplexml_bool = simplexml_compare_truthiness_unreleased(
        ctx,
        simplexml_value,
        simplexml_type,
        expr.span,
    );
    let (lhs_bool, rhs_bool) = if simplexml_on_left {
        (simplexml_bool, rhs)
    } else {
        (lhs, simplexml_bool)
    };
    let result = emit_bool_comparison_result(ctx, lhs_bool, op, rhs_bool, expr.span);
    release_binary_operand_temporary(ctx, lhs, expr.span);
    if rhs.value != lhs.value {
        release_binary_operand_temporary(ctx, rhs, expr.span);
    }
    result
}

/// Guards loader-failure union arms before invoking SimpleXML's native comparison handler.
fn lower_fallible_simplexml_object_compare(
    ctx: &mut LoweringContext<'_, '_>,
    lhs: LoweredValue,
    op: &BinOp,
    rhs: LoweredValue,
    expr: &Expr,
    lhs_type: &PhpType,
    rhs_type: &PhpType,
) -> LoweredValue {
    let result_type = if matches!(op, BinOp::Spaceship) {
        PhpType::Int
    } else {
        PhpType::Bool
    };
    let temp_name = ctx.declare_hidden_temp(result_type.clone());
    let failure_block = ctx
        .builder
        .create_named_block("simplexml.compare.failure", Vec::new());
    let object_block = ctx
        .builder
        .create_named_block("simplexml.compare.object", Vec::new());
    let merge = ctx
        .builder
        .create_named_block("simplexml.compare.merge", Vec::new());
    let lhs_failure = simplexml_receiver_is_failure(ctx, lhs.value, expr.span);
    let rhs_failure = simplexml_receiver_is_failure(ctx, rhs.value, expr.span);
    let either_failure = ctx.emit_value(
        Op::IBitOr,
        vec![lhs_failure.value, rhs_failure.value],
        None,
        PhpType::Bool,
        Op::IBitOr.default_effects(),
        Some(expr.span),
    );
    ctx.builder.terminate(Terminator::CondBr {
        cond: either_failure.value,
        then_target: failure_block,
        then_args: Vec::new(),
        else_target: object_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(failure_block);
    let lhs_bool = lower_fallible_simplexml_compare_truthiness(
        ctx,
        lhs.value,
        lhs_failure,
        lhs_type,
        expr.span,
    );
    let rhs_bool = lower_fallible_simplexml_compare_truthiness(
        ctx,
        rhs.value,
        rhs_failure,
        rhs_type,
        expr.span,
    );
    let scalar_result = emit_bool_comparison_result(ctx, lhs_bool, op, rhs_bool, expr.span);
    store_value_into_temp(
        ctx,
        &temp_name,
        result_type.clone(),
        scalar_result,
        expr.span,
    );
    branch_to(ctx, merge);

    ctx.builder.position_at_end(object_block);
    let comparison = emit_simplexml_object_comparison_unreleased(
        ctx,
        lhs.value,
        rhs.value,
        lhs_type,
        expr.span,
    );
    let object_result = simplexml_compare_result(ctx, comparison, op, expr.span);
    store_value_into_temp(
        ctx,
        &temp_name,
        result_type,
        object_result,
        expr.span,
    );
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    release_binary_operand_temporary(ctx, lhs, expr.span);
    if rhs.value != lhs.value {
        release_binary_operand_temporary(ctx, rhs, expr.span);
    }
    ctx.load_local(&temp_name, Some(expr.span))
}

/// Emits the native comparison result for two runtime-proven SimpleXML wrappers.
fn emit_simplexml_object_comparison_unreleased(
    ctx: &mut LoweringContext<'_, '_>,
    lhs: crate::ir::ValueId,
    rhs: crate::ir::ValueId,
    lhs_type: &PhpType,
    span: Span,
) -> LoweredValue {
    let opcode =
        crate::ir_lower::internal_extensions::simplexml_object_handler_opcode_for_type(
            ctx,
            lhs_type,
            "compare",
        )
        .expect("SimpleXML comparison requires the locked compare handler");
    crate::ir_lower::internal_extensions::emit_call(
        ctx,
        opcode,
        crate::ir_lower::internal_extensions::FLAG_RECEIVER,
        vec![lhs, rhs],
        PhpType::Int,
        span,
    )
}

/// Converts one direct or loader-fallible SimpleXML value to bool without consuming it.
fn lower_fallible_simplexml_compare_truthiness(
    ctx: &mut LoweringContext<'_, '_>,
    value: crate::ir::ValueId,
    failure: LoweredValue,
    value_type: &PhpType,
    span: Span,
) -> LoweredValue {
    if !simplexml_type_may_fail(value_type) {
        return emit_simplexml_bool_cast_unreleased(ctx, value, value_type, span);
    }
    let temp_name = ctx.declare_hidden_temp(PhpType::Bool);
    let failure_block = ctx
        .builder
        .create_named_block("simplexml.compare.bool.failure", Vec::new());
    let object_block = ctx
        .builder
        .create_named_block("simplexml.compare.bool.object", Vec::new());
    let merge = ctx
        .builder
        .create_named_block("simplexml.compare.bool.merge", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: failure.value,
        then_target: failure_block,
        then_args: Vec::new(),
        else_target: object_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(failure_block);
    let false_value = emit_bool_at_span(ctx, false, span);
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, false_value, span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(object_block);
    let cast = emit_simplexml_bool_cast_unreleased(ctx, value, value_type, span);
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, cast, span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    ctx.load_local(&temp_name, Some(span))
}

/// Converts a direct or loader-fallible SimpleXML value to bool without consuming it.
fn simplexml_compare_truthiness_unreleased(
    ctx: &mut LoweringContext<'_, '_>,
    value: crate::ir::ValueId,
    value_type: &PhpType,
    span: Span,
) -> LoweredValue {
    if !simplexml_type_may_fail(value_type) {
        return emit_simplexml_bool_cast_unreleased(ctx, value, value_type, span);
    }
    let failure = simplexml_receiver_is_failure(ctx, value, span);
    lower_fallible_simplexml_compare_truthiness(ctx, value, failure, value_type, span)
}

/// Invokes SimpleXML's boolean cast handler for one runtime-proven live wrapper.
fn emit_simplexml_bool_cast_unreleased(
    ctx: &mut LoweringContext<'_, '_>,
    value: crate::ir::ValueId,
    value_type: &PhpType,
    span: Span,
) -> LoweredValue {
    let opcode =
        crate::ir_lower::internal_extensions::simplexml_object_handler_opcode_for_type(
            ctx,
            value_type,
            "cast",
        )
        .expect("SimpleXML truthiness requires the locked cast handler");
    let bool_kind = emit_i64_at_span(ctx, 0, span);
    crate::ir_lower::internal_extensions::emit_call(
        ctx,
        opcode,
        crate::ir_lower::internal_extensions::FLAG_RECEIVER,
        vec![value, bool_kind.value],
        PhpType::Bool,
        span,
    )
}

/// Compares two canonical booleans with PHP's scalar ordering semantics.
fn emit_bool_comparison_result(
    ctx: &mut LoweringContext<'_, '_>,
    lhs: LoweredValue,
    op: &BinOp,
    rhs: LoweredValue,
    span: Span,
) -> LoweredValue {
    if matches!(op, BinOp::Spaceship) {
        return ctx.emit_value(
            Op::ISub,
            vec![lhs.value, rhs.value],
            None,
            PhpType::Int,
            Op::ISub.default_effects(),
            Some(span),
        );
    }
    ctx.emit_value(
        Op::ICmp,
        vec![lhs.value, rhs.value],
        Some(Immediate::CmpPredicate(cmp_predicate(op))),
        PhpType::Bool,
        Op::ICmp.default_effects(),
        Some(span),
    )
}

/// Returns whether a SimpleXML result type can carry a loader failure at runtime.
fn simplexml_type_may_fail(php_type: &PhpType) -> bool {
    matches!(
        php_type,
        PhpType::Union(members)
            if members.iter().any(|member| {
                matches!(member, PhpType::False | PhpType::Void | PhpType::Never)
            })
    )
}

/// Tests a nullable or scalar-failure SimpleXML union without invoking wrapper handlers.
pub(super) fn simplexml_receiver_is_failure(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: crate::ir::ValueId,
    span: Span,
) -> LoweredValue {
    let receiver_type = ctx.builder.value_php_type(receiver);
    let has_scalar_failure = matches!(
        receiver_type,
        PhpType::Union(ref members) if members.iter().any(|member| matches!(member, PhpType::False))
    );
    if !has_scalar_failure {
        return ctx.emit_value(
            Op::IsNull,
            vec![receiver],
            None,
            PhpType::Bool,
            Op::IsNull.default_effects(),
            Some(span),
        );
    }
    let false_value = emit_bool_literal(ctx, false, Some(span));
    ctx.emit_value(
        Op::StrictEq,
        vec![receiver, false_value.value],
        None,
        PhpType::Bool,
        Op::StrictEq.default_effects(),
        Some(span),
    )
}

/// Reports whether PHP compares this scalar type to an object through boolean casting.
fn simplexml_boolean_comparison_type(php_type: &PhpType) -> bool {
    matches!(php_type.codegen_repr(), PhpType::Bool | PhpType::False)
}

/// Converts SimpleXML's zero-or-uncomparable handler result into one PHP comparison result.
fn simplexml_compare_result(
    ctx: &mut LoweringContext<'_, '_>,
    comparison: LoweredValue,
    op: &BinOp,
    span: Span,
) -> LoweredValue {
    if matches!(op, BinOp::Spaceship) {
        return comparison;
    }
    if matches!(op, BinOp::Lt | BinOp::Gt) {
        return emit_bool_at_span(ctx, false, span);
    }
    let zero = emit_i64_at_span(ctx, 0, span);
    let predicate = if matches!(op, BinOp::NotEq) {
        CmpPredicate::Ne
    } else {
        CmpPredicate::Eq
    };
    ctx.emit_value(
        Op::ICmp,
        vec![comparison.value, zero.value],
        Some(Immediate::CmpPredicate(predicate)),
        PhpType::Bool,
        Op::ICmp.default_effects(),
        Some(span),
    )
}

/// Emits a boolean constant at a specific source span.
fn emit_bool_at_span(
    ctx: &mut LoweringContext<'_, '_>,
    value: bool,
    span: crate::span::Span,
) -> LoweredValue {
    ctx.emit_value(
        Op::ConstBool,
        Vec::new(),
        Some(Immediate::Bool(value)),
        if value { PhpType::Bool } else { PhpType::False },
        Op::ConstBool.default_effects(),
        Some(span),
    )
}

/// Releases an owning binary-operator operand once the consuming opcode has read it.
pub(super) fn release_binary_operand_temporary(
    ctx: &mut LoweringContext<'_, '_>,
    operand: LoweredValue,
    span: Span,
) {
    if ctx.value_is_owning_temporary(operand) {
        crate::ir_lower::ownership::release_if_owned(ctx, operand, Some(span));
    }
}

/// Returns true for the comparison operators PHP evaluates against a `DateTime`'s instant.
///
/// Identity `===`/`!==` is excluded: PHP keeps those as object-reference comparisons, so they must
/// not be rewritten into the instant-key integer comparison.
pub(super) fn datetime_instant_compare_operator(op: &BinOp) -> bool {
    matches!(
        op,
        BinOp::Eq
            | BinOp::NotEq
            | BinOp::Lt
            | BinOp::LtEq
            | BinOp::Gt
            | BinOp::GtEq
            | BinOp::Spaceship
    )
}

/// Returns true when `value` is a non-nullable `DateTime`/`DateTimeImmutable` instance whose instant
/// can be compared through its `timestamp`/`microsecond` integer properties.
///
/// Nullable operands (`?DateTime`) are excluded: reading the `timestamp`/`microsecond` properties off
/// a possible `null` would be invalid, so those fall through to the normal comparison path where
/// PHP's null-vs-object ordering applies.
pub(super) fn is_datetime_family_value(ctx: &LoweringContext<'_, '_>, value: ValueId) -> bool {
    let ty = ctx.builder.value_php_type(value);
    matches!(
        singular_object_class(&ty),
        Some((name, false))
            if matches!(name.trim_start_matches('\\'), "DateTime" | "DateTimeImmutable")
    )
}

/// Lowers a `DateTime`/`DateTimeImmutable` object to a monotonic integer instant key,
/// `timestamp * 1_000_000 + microsecond`.
///
/// Both components are stored as `int` properties, so the key is an exact ordering of the absolute
/// instant including the sub-second part. Reducing each operand to this key lets the family's
/// comparison operators reuse ordinary signed-integer comparison without any object-aware codegen.
pub(super) fn lower_datetime_instant_key(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    expr: &Expr,
) -> LoweredValue {
    let timestamp = lower_property_get_from_value(ctx, object, "timestamp", Op::PropGet, expr);
    let microsecond = lower_property_get_from_value(ctx, object, "microsecond", Op::PropGet, expr);
    let million = lower_int_literal(ctx, 1_000_000, expr);
    let scaled = ctx.emit_value(
        Op::IMul,
        vec![timestamp.value, million.value],
        None,
        PhpType::Int,
        Op::IMul.default_effects(),
        Some(expr.span),
    );
    ctx.emit_value(
        Op::IAdd,
        vec![scaled.value, microsecond.value],
        None,
        PhpType::Int,
        Op::IAdd.default_effects(),
        Some(expr.span),
    )
}

/// Maps an AST comparison operator to an EIR predicate.
pub(super) fn cmp_predicate(op: &BinOp) -> CmpPredicate {
    match op {
        BinOp::Eq => CmpPredicate::Eq,
        BinOp::NotEq => CmpPredicate::Ne,
        BinOp::Lt => CmpPredicate::Slt,
        BinOp::LtEq => CmpPredicate::Sle,
        BinOp::Gt => CmpPredicate::Sgt,
        BinOp::GtEq => CmpPredicate::Sge,
        _ => CmpPredicate::Eq,
    }
}
