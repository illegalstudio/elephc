//! Purpose:
//! Comparison lowering, temporary cleanup, and DateTime instant comparisons.
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
    if date_interval_compare_operator(op)
        && is_date_interval_family_value(ctx, lhs.value)
        && is_date_interval_family_value(ctx, rhs.value)
    {
        return lower_date_interval_uncomparable(ctx, lhs, op, rhs, expr);
    }
    // DateTime-family value comparison: PHP orders `DateTime`/`DateTimeImmutable` by their absolute
    // instant (timestamp seconds + microsecond), independent of the stored timezone. Compare the
    // two fields lexicographically so the signed-64-bit timestamp range never overflows. Identity
    // `===`/`!==` is deliberately excluded so it keeps comparing object references.
    if datetime_instant_compare_operator(op)
        && is_datetime_family_value(ctx, lhs.value)
        && is_datetime_family_value(ctx, rhs.value)
    {
        return lower_datetime_family_compare(ctx, lhs, op, rhs, expr);
    }
    if datetime_instant_compare_operator(op)
        && is_datetime_zone_family_value(ctx, lhs.value)
        && is_datetime_zone_family_value(ctx, rhs.value)
    {
        return lower_datetime_zone_compare(ctx, lhs, op, rhs, expr);
    }
    if let Some(result) = lower_nullable_date_period_property_compare(
        ctx, left, lhs, op, right, rhs, expr,
    ) {
        return result;
    }
    let uses_runtime_relational_compare = matches!(
        op,
        BinOp::Lt | BinOp::LtEq | BinOp::Gt | BinOp::GtEq
    )
        && (needs_runtime_ordering_dispatch(ctx, lhs.value)
            || needs_runtime_ordering_dispatch(ctx, rhs.value));
    let opcode = match op {
        BinOp::StrictEq => Op::StrictEq,
        BinOp::StrictNotEq => Op::StrictNotEq,
        BinOp::Eq => Op::LooseEq,
        BinOp::NotEq => Op::LooseNotEq,
        BinOp::Spaceship => Op::Spaceship,
        _ if uses_runtime_relational_compare => Op::PhpRelCmp,
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
    let immediate = if matches!(opcode, Op::ICmp | Op::FCmp | Op::StrCmp | Op::PhpRelCmp) {
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

/// Returns whether an operand carries a runtime tag that must select PHP's ordering rule.
fn needs_runtime_ordering_dispatch(ctx: &LoweringContext<'_, '_>, value: ValueId) -> bool {
    matches!(
        ctx.builder.value_php_type(value).codegen_repr(),
        PhpType::Mixed | PhpType::TaggedScalar
    )
}

/// Lowers equality between a nullable DatePeriod date property and a concrete date object.
///
/// The virtual `start`/`current`/`end` accessors use a boxed nullable interface
/// representation. Split the null arm before unboxing so end/count periods retain
/// PHP's `null == object` result while populated properties use DateTime's instant
/// comparison handler.
fn lower_nullable_date_period_property_compare(
    ctx: &mut LoweringContext<'_, '_>,
    left_expr: &Expr,
    lhs: LoweredValue,
    op: &BinOp,
    right_expr: &Expr,
    rhs: LoweredValue,
    expr: &Expr,
) -> Option<LoweredValue> {
    if !matches!(op, BinOp::Eq | BinOp::NotEq) {
        return None;
    }
    let lhs_ty = ctx.builder.value_php_type(lhs.value);
    let rhs_ty = ctx.builder.value_php_type(rhs.value);
    let (property_value, date_value, property_is_left) =
        if lhs_ty.codegen_repr() == PhpType::Mixed
            && datetime_family_type(ctx, &rhs_ty)
            && is_date_period_datetime_property_expr(ctx, left_expr)
        {
            (lhs, rhs, true)
        } else if rhs_ty.codegen_repr() == PhpType::Mixed
            && datetime_family_type(ctx, &lhs_ty)
            && is_date_period_datetime_property_expr(ctx, right_expr)
        {
            (rhs, lhs, false)
        } else {
            return None;
        };

    let is_null = ctx.emit_value(
        Op::IsNull,
        vec![property_value.value],
        None,
        PhpType::Bool,
        Op::IsNull.default_effects(),
        Some(expr.span),
    );
    let result_name = ctx.declare_hidden_temp(PhpType::Bool);
    let null_block = ctx
        .builder
        .create_named_block("dateperiod.compare.null", Vec::new());
    let object_block = ctx
        .builder
        .create_named_block("dateperiod.compare.object", Vec::new());
    let merge = ctx
        .builder
        .create_named_block("dateperiod.compare.merge", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: is_null.value,
        then_target: null_block,
        then_args: Vec::new(),
        else_target: object_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(null_block);
    let null_result = lower_bool_literal(ctx, matches!(op, BinOp::NotEq), expr);
    store_value_into_temp(
        ctx,
        &result_name,
        PhpType::Bool,
        null_result,
        expr.span,
    );
    release_binary_operand_temporary(ctx, property_value, expr.span);
    if date_value.value != property_value.value {
        release_binary_operand_temporary(ctx, date_value, expr.span);
    }
    branch_to(ctx, merge);

    ctx.builder.position_at_end(object_block);
    let unboxed = ctx.emit_value(
        Op::RuntimeCall,
        vec![property_value.value],
        None,
        PhpType::Object("DateTimeInterface".to_string()),
        effects_lookup::runtime_effects(),
        Some(expr.span),
    );
    if ctx.value_is_owning_temporary(property_value) {
        crate::ir_lower::ownership::release_if_owned(ctx, property_value, Some(expr.span));
    }
    let compared = if property_is_left {
        lower_datetime_family_compare(ctx, unboxed, op, date_value, expr)
    } else {
        lower_datetime_family_compare(ctx, date_value, op, unboxed, expr)
    };
    store_value_into_temp(ctx, &result_name, PhpType::Bool, compared, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    Some(ctx.load_local(&result_name, Some(expr.span)))
}

/// Returns true for one DatePeriod virtual date-property read with a known receiver class.
fn is_date_period_datetime_property_expr(
    ctx: &LoweringContext<'_, '_>,
    expr: &Expr,
) -> bool {
    let ExprKind::PropertyAccess { object, property } = &expr.kind else {
        return false;
    };
    if !matches!(property.as_str(), "start" | "current" | "end") {
        return false;
    }
    isset_object_expr_class(ctx, object).is_some_and(|(class_name, _)| {
        class_extends_class(ctx, &class_name, "DatePeriod")
    })
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
/// Returns whether `op` invokes php-src's non-strict DateInterval comparison handler.
fn date_interval_compare_operator(op: &BinOp) -> bool {
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

/// Returns whether `value` is a `DateInterval` instance or userland descendant.
fn is_date_interval_family_value(ctx: &LoweringContext<'_, '_>, value: ValueId) -> bool {
    let PhpType::Object(class_name) = ctx.builder.value_php_type(value) else {
        return false;
    };
    let class_name = class_name.trim_start_matches('\\');
    class_name == "DateInterval" || class_extends_class(ctx, class_name, "DateInterval")
}

/// Emits php-src's warning and fixed result for a non-strict DateInterval comparison.
fn lower_date_interval_uncomparable(
    ctx: &mut LoweringContext<'_, '_>,
    lhs: LoweredValue,
    op: &BinOp,
    rhs: LoweredValue,
    expr: &Expr,
) -> LoweredValue {
    let message_text = "\nWarning: Cannot compare DateInterval objects";
    let message_expr = Expr::new(
        ExprKind::StringLiteral(message_text.to_string()),
        expr.span,
    );
    let message = lower_string_literal(ctx, message_text, &message_expr);
    let line = emit_i64_at_span(ctx, expr.span.line as i64, expr.span);
    let level = emit_i64_at_span(ctx, 2, expr.span);
    let warning = emit_builtin_call_value(
        ctx,
        "__elephc_diag_warning",
        vec![message.value, line.value, level.value],
        PhpType::Void,
        expr.span,
        None,
    );
    let _ = warning;
    release_binary_operand_temporary(ctx, lhs, expr.span);
    if rhs.value != lhs.value {
        release_binary_operand_temporary(ctx, rhs, expr.span);
    }
    if matches!(op, BinOp::Spaceship) {
        emit_i64_at_span(ctx, 1, expr.span)
    } else {
        lower_bool_literal(ctx, matches!(op, BinOp::NotEq), expr)
    }
}

/// Returns whether an EIR value has a DateTime-interface family PHP type.
pub(super) fn is_datetime_family_value(ctx: &LoweringContext<'_, '_>, value: ValueId) -> bool {
    let ty = ctx.builder.value_php_type(value);
    datetime_family_type(ctx, &ty)
}

/// Returns true for a concrete or dynamically boxed DateTime-interface family type.
fn datetime_family_type(ctx: &LoweringContext<'_, '_>, ty: &PhpType) -> bool {
    match ty {
        PhpType::Object(name) => {
            let name = name.trim_start_matches('\\');
            name == "DateTimeInterface"
                || class_extends_class(ctx, name, "DateTime")
                || class_extends_class(ctx, name, "DateTimeImmutable")
        }
        PhpType::Union(members) => members.iter().all(|member| {
            matches!(member, PhpType::False | PhpType::Void)
                || datetime_family_type(ctx, member)
        }),
        _ => false,
    }
}

/// Returns the sole DateTime-family object arm carried by a concrete or sentinel union type.
fn datetime_family_object_class<'a>(
    ctx: &LoweringContext<'_, '_>,
    ty: &'a PhpType,
) -> Option<&'a str> {
    match ty {
        PhpType::Object(name) if datetime_family_type(ctx, ty) => Some(name.as_str()),
        PhpType::Union(members) if datetime_family_type(ctx, ty) => members.iter().find_map(
            |member| match member {
                PhpType::Object(name) => Some(name.as_str()),
                _ => None,
            },
        ),
        _ => None,
    }
}

/// Unboxes a DateTime-family sentinel union before calling its interface methods.
fn unbox_datetime_comparison_operand(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Span,
) -> LoweredValue {
    let ty = ctx.builder.value_php_type(value.value);
    if !matches!(ty.codegen_repr(), PhpType::Mixed) {
        return value;
    }
    let Some(class_name) = datetime_family_object_class(ctx, &ty).map(str::to_string) else {
        return value;
    };
    let unboxed = ctx.emit_value(
        Op::RuntimeCall,
        vec![value.value],
        None,
        PhpType::Object(class_name),
        effects_lookup::runtime_effects(),
        Some(span),
    );
    if ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    }
    unboxed
}

/// Returns true when `value` is a non-nullable `DateTimeZone` or descendant.
pub(super) fn is_datetime_zone_family_value(ctx: &LoweringContext<'_, '_>, value: ValueId) -> bool {
    let ty = ctx.builder.value_php_type(value);
    matches!(
        singular_object_class(&ty),
        Some((name, false)) if class_extends_class(ctx, name, "DateTimeZone")
    )
}

/// Lowers php-src's `DateTimeZone` comparison handler and maps its uncomparable sentinel.
///
/// The hidden method returns `0` for equal zones and `1` for unequal same-kind zones, while
/// throwing for uninitialized or different-kind operands. php-src makes every relational
/// comparison of unequal same-kind zones false; `<=>` exposes the sentinel as `1`.
fn lower_datetime_zone_compare(
    ctx: &mut LoweringContext<'_, '_>,
    lhs: LoweredValue,
    op: &BinOp,
    rhs: LoweredValue,
    expr: &Expr,
) -> LoweredValue {
    let method = ctx.intern_string("__elephc_compare");
    let comparison = ctx.emit_value(
        Op::MethodCall,
        vec![lhs.value, rhs.value],
        Some(Immediate::Data(method)),
        PhpType::Int,
        Op::MethodCall.default_effects(),
        Some(expr.span),
    );
    release_binary_operand_temporary(ctx, lhs, expr.span);
    if rhs.value != lhs.value {
        release_binary_operand_temporary(ctx, rhs, expr.span);
    }
    if matches!(op, BinOp::Spaceship) {
        return comparison;
    }
    let zero = lower_int_literal(ctx, 0, expr);
    let predicate = match op {
        BinOp::Eq | BinOp::LtEq | BinOp::GtEq => CmpPredicate::Eq,
        BinOp::NotEq => CmpPredicate::Ne,
        BinOp::Lt | BinOp::Gt => {
            return ctx.emit_value(
                Op::ICmp,
                vec![comparison.value, comparison.value],
                Some(Immediate::CmpPredicate(CmpPredicate::Ne)),
                PhpType::Bool,
                Op::ICmp.default_effects(),
                Some(expr.span),
            );
        }
        _ => unreachable!("DateTimeZone comparison lowering excludes identity operators"),
    };
    ctx.emit_value(
        Op::ICmp,
        vec![comparison.value, zero.value],
        Some(Immediate::CmpPredicate(predicate)),
        PhpType::Bool,
        Op::ICmp.default_effects(),
        Some(expr.span),
    )
}

/// Lowers php-src's overflow-safe `DateTime`/`DateTimeImmutable` instant comparator.
///
/// The hidden method compares timestamp seconds first and microseconds only on a tie, avoiding the
/// overflow that a `timestamp * 1_000_000 + microsecond` key would cause for PHP's large years.
fn lower_datetime_family_compare(
    ctx: &mut LoweringContext<'_, '_>,
    lhs: LoweredValue,
    op: &BinOp,
    rhs: LoweredValue,
    expr: &Expr,
) -> LoweredValue {
    let lhs = unbox_datetime_comparison_operand(ctx, lhs, expr.span);
    let rhs = unbox_datetime_comparison_operand(ctx, rhs, expr.span);
    let guard = ctx.intern_string("__elephc_assert_comparable");
    ctx.emit_void(
        Op::MethodCall,
        vec![lhs.value],
        Some(Immediate::Data(guard)),
        Op::MethodCall.default_effects(),
        Some(expr.span),
    );
    ctx.emit_void(
        Op::MethodCall,
        vec![rhs.value],
        Some(Immediate::Data(guard)),
        Op::MethodCall.default_effects(),
        Some(expr.span),
    );
    let timestamp_method = ctx.intern_string("getTimestamp");
    let microsecond_method = ctx.intern_string("getMicrosecond");
    let left_timestamp = ctx.emit_value(
        Op::MethodCall,
        vec![lhs.value],
        Some(Immediate::Data(timestamp_method)),
        PhpType::Int,
        Op::MethodCall.default_effects(),
        Some(expr.span),
    );
    let right_timestamp = ctx.emit_value(
        Op::MethodCall,
        vec![rhs.value],
        Some(Immediate::Data(timestamp_method)),
        PhpType::Int,
        Op::MethodCall.default_effects(),
        Some(expr.span),
    );
    let left_microsecond = ctx.emit_value(
        Op::MethodCall,
        vec![lhs.value],
        Some(Immediate::Data(microsecond_method)),
        PhpType::Int,
        Op::MethodCall.default_effects(),
        Some(expr.span),
    );
    let right_microsecond = ctx.emit_value(
        Op::MethodCall,
        vec![rhs.value],
        Some(Immediate::Data(microsecond_method)),
        PhpType::Int,
        Op::MethodCall.default_effects(),
        Some(expr.span),
    );
    let seconds_equal = emit_datetime_field_compare(
        ctx,
        left_timestamp,
        right_timestamp,
        CmpPredicate::Eq,
        expr,
    );
    let micros_equal = emit_datetime_field_compare(
        ctx,
        left_microsecond,
        right_microsecond,
        CmpPredicate::Eq,
        expr,
    );
    let equal = emit_datetime_bool_combine(ctx, Op::IBitAnd, seconds_equal, micros_equal, expr);
    let result = match op {
        BinOp::Eq => equal,
        BinOp::NotEq => {
            let zero = lower_bool_literal(ctx, false, expr);
            emit_datetime_field_compare(ctx, equal, zero, CmpPredicate::Eq, expr)
        }
        BinOp::Lt | BinOp::LtEq | BinOp::Gt | BinOp::GtEq | BinOp::Spaceship => {
            let seconds_less = emit_datetime_field_compare(
                ctx,
                left_timestamp,
                right_timestamp,
                CmpPredicate::Slt,
                expr,
            );
            let micros_less = emit_datetime_field_compare(
                ctx,
                left_microsecond,
                right_microsecond,
                CmpPredicate::Slt,
                expr,
            );
            let less_on_tie =
                emit_datetime_bool_combine(ctx, Op::IBitAnd, seconds_equal, micros_less, expr);
            let less =
                emit_datetime_bool_combine(ctx, Op::IBitOr, seconds_less, less_on_tie, expr);
            let seconds_greater = emit_datetime_field_compare(
                ctx,
                left_timestamp,
                right_timestamp,
                CmpPredicate::Sgt,
                expr,
            );
            let micros_greater = emit_datetime_field_compare(
                ctx,
                left_microsecond,
                right_microsecond,
                CmpPredicate::Sgt,
                expr,
            );
            let greater_on_tie =
                emit_datetime_bool_combine(ctx, Op::IBitAnd, seconds_equal, micros_greater, expr);
            let greater =
                emit_datetime_bool_combine(ctx, Op::IBitOr, seconds_greater, greater_on_tie, expr);
            match op {
                BinOp::Lt => less,
                BinOp::LtEq => emit_datetime_bool_combine(ctx, Op::IBitOr, less, equal, expr),
                BinOp::Gt => greater,
                BinOp::GtEq => {
                    emit_datetime_bool_combine(ctx, Op::IBitOr, greater, equal, expr)
                }
                BinOp::Spaceship => ctx.emit_value(
                    Op::ISub,
                    vec![greater.value, less.value],
                    None,
                    PhpType::Int,
                    Op::ISub.default_effects(),
                    Some(expr.span),
                ),
                _ => unreachable!(),
            }
        }
        _ => unreachable!("DateTime instant comparison excludes identity operators"),
    };
    release_binary_operand_temporary(ctx, lhs, expr.span);
    if rhs.value != lhs.value {
        release_binary_operand_temporary(ctx, rhs, expr.span);
    }
    result
}

/// Emits one signed integer comparison for a DateTime timestamp or microsecond field.
fn emit_datetime_field_compare(
    ctx: &mut LoweringContext<'_, '_>,
    left: LoweredValue,
    right: LoweredValue,
    predicate: CmpPredicate,
    expr: &Expr,
) -> LoweredValue {
    ctx.emit_value(
        Op::ICmp,
        vec![left.value, right.value],
        Some(Immediate::CmpPredicate(predicate)),
        PhpType::Bool,
        Op::ICmp.default_effects(),
        Some(expr.span),
    )
}

/// Combines two canonical boolean comparison results with an integer bit operation.
fn emit_datetime_bool_combine(
    ctx: &mut LoweringContext<'_, '_>,
    opcode: Op,
    left: LoweredValue,
    right: LoweredValue,
    expr: &Expr,
) -> LoweredValue {
    ctx.emit_value(
        opcode,
        vec![left.value, right.value],
        None,
        PhpType::Bool,
        opcode.default_effects(),
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
