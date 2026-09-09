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

/// Retires owned operands and provisional string unboxes after the binary opcode has read them.
pub(super) fn release_binary_operand_temporary(
    ctx: &mut LoweringContext<'_, '_>,
    operand: LoweredValue,
    span: Span,
) {
    if ctx.value_needs_release_after_use(operand) {
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
