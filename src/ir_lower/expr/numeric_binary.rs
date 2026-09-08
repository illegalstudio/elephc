//! Purpose:
//! Numeric binary operations and PHP array-union type planning.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Lowers a binary operation.
pub(super) fn lower_binary(
    ctx: &mut LoweringContext<'_, '_>,
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    expr: &Expr,
) -> LoweredValue {
    match op {
        BinOp::Concat => lower_concat(ctx, left, right, expr),
        BinOp::Eq | BinOp::NotEq | BinOp::StrictEq | BinOp::StrictNotEq
        | BinOp::Lt | BinOp::Gt | BinOp::LtEq | BinOp::GtEq | BinOp::Spaceship => {
            lower_compare(ctx, left, op, right, expr)
        }
        BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod | BinOp::Pow
        | BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::ShiftLeft | BinOp::ShiftRight => {
            lower_numeric_binary(ctx, left, op, right, expr)
        }
        BinOp::And | BinOp::Or => lower_logical_binary(ctx, left, op, right, expr),
        BinOp::NullCoalesce => lower_null_coalesce(ctx, left, right, expr),
        BinOp::Xor => lower_logical_xor(ctx, left, right, expr),
    }
}

/// Lowers an integer or float binary operation.
pub(super) fn lower_numeric_binary(
    ctx: &mut LoweringContext<'_, '_>,
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    expr: &Expr,
) -> LoweredValue {
    if crate::parser::ast::is_synthetic_unary_plus(op, right) {
        return lower_unary_plus(ctx, left, expr);
    }
    let lhs = lower_expr(ctx, left);
    let rhs = lower_expr(ctx, right);
    if matches!(op, BinOp::Add) {
        if let Some((op, result_ty)) = array_union_plan(ctx, lhs.value, rhs.value) {
            return ctx.emit_value(
                op,
                vec![lhs.value, rhs.value],
                None,
                result_ty,
                op.default_effects(),
                Some(expr.span),
            );
        }
    }
    if matches!(op, BinOp::Pow) {
        // PHP's `**` is int-preserving (`2 ** 3` is `int(8)`), so an int/int power goes
        // through the checked helper that reproduces `zend_pow_function_base`: it keeps an
        // `i64` while the value fits and promotes to a double at the exact multiplication
        // that overflows, or immediately for a negative exponent. Only that case can be an
        // int, and the type checker marks it `Mixed` for the same reason it marks
        // overflow-capable `+`/`-`/`*` operands `Mixed`.
        if lhs.ir_type == IrType::I64
            && rhs.ir_type == IrType::I64
            && fallback_expr_type(expr) == PhpType::Mixed
        {
            return ctx.emit_value(
                Op::ICheckedPow,
                vec![lhs.value, rhs.value],
                None,
                PhpType::Mixed,
                Op::ICheckedPow.default_effects(),
                Some(expr.span),
            );
        }
        // A boxed operand (any non-`I64`/`F64` storage, typically an overflow-capable
        // `Mixed` int) keeps the int-preserving behavior through the runtime dispatcher,
        // which only takes the integer path when both payloads really are integers.
        if should_use_mixed_numeric_binop(lhs.ir_type, rhs.ir_type) {
            let result = lower_mixed_numeric_binary(ctx, lhs, rhs, MixedNumericOp::Pow, expr);
            release_binary_operand_temporary(ctx, lhs, expr.span);
            if rhs.value != lhs.value {
                release_binary_operand_temporary(ctx, rhs, expr.span);
            }
            return result;
        }
        let lhs = coerce_to_float(ctx, lhs, expr);
        let rhs = coerce_to_float(ctx, rhs, expr);
        return ctx.emit_value(
            Op::FPow,
            vec![lhs.value, rhs.value],
            None,
            PhpType::Float,
            Op::FPow.default_effects(),
            Some(expr.span),
        );
    }
    if matches!(op, BinOp::Mod) {
        let lhs = coerce_to_int(ctx, lhs, expr);
        let rhs = coerce_to_int(ctx, rhs, expr);
        return ctx.emit_value(
            Op::ISMod,
            vec![lhs.value, rhs.value],
            None,
            PhpType::Int,
            arithmetic_effects(Op::ISMod, right),
            Some(expr.span),
        );
    }
    if matches!(
        op,
        BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::ShiftLeft | BinOp::ShiftRight
    ) {
        let lhs = coerce_to_int(ctx, lhs, left);
        let rhs = coerce_to_int(ctx, rhs, right);
        let iop = match op {
            BinOp::BitAnd => Op::IBitAnd,
            BinOp::BitOr => Op::IBitOr,
            BinOp::BitXor => Op::IBitXor,
            BinOp::ShiftLeft => Op::IShl,
            BinOp::ShiftRight => Op::IShrA,
            _ => Op::RuntimeCall,
        };
        return ctx.emit_value(
            iop,
            vec![lhs.value, rhs.value],
            None,
            PhpType::Int,
            arithmetic_effects(iop, right),
            Some(expr.span),
        );
    }
    if let Some(mixed_op) = mixed_numeric_op(op) {
        if should_use_mixed_numeric_binop(lhs.ir_type, rhs.ir_type) {
            let result = lower_mixed_numeric_binary(ctx, lhs, rhs, mixed_op, expr);
            release_binary_operand_temporary(ctx, lhs, expr.span);
            if rhs.value != lhs.value {
                release_binary_operand_temporary(ctx, rhs, expr.span);
            }
            return result;
        }
    }
    if lhs.ir_type == IrType::F64 || rhs.ir_type == IrType::F64 {
        let lhs = coerce_to_float(ctx, lhs, expr);
        let rhs = coerce_to_float(ctx, rhs, expr);
        let fop = match op {
            BinOp::Add => Op::FAdd,
            BinOp::Sub => Op::FSub,
            BinOp::Mul => Op::FMul,
            BinOp::Div => Op::FDiv,
            _ => Op::RuntimeCall,
        };
        return ctx.emit_value(fop, vec![lhs.value, rhs.value], None, PhpType::Float, arithmetic_effects(fop, right), Some(expr.span));
    }
    if matches!(op, BinOp::Div) && (lhs.ir_type != IrType::I64 || rhs.ir_type != IrType::I64) {
        let lhs = coerce_to_float(ctx, lhs, left);
        let rhs = coerce_to_float(ctx, rhs, right);
        return ctx.emit_value(
            Op::FDiv,
            vec![lhs.value, rhs.value],
            None,
            PhpType::Float,
            arithmetic_effects(Op::FDiv, right),
            Some(expr.span),
        );
    }
    if lhs.ir_type == IrType::I64 && rhs.ir_type == IrType::I64 {
        // Check if the type checker promoted this to Mixed (non-constant int arithmetic
        // that can overflow to float). If so, emit a checked helper that returns a Mixed box.
        let result_php_type = fallback_expr_type(expr);
        if result_php_type == PhpType::Mixed && matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul) {
            // Identity shortcuts: x+0, x-0, 0+x, 0-x cannot overflow → keep plain Int.
            // x*1, 1*x cannot overflow → keep plain Int.
            // x*0, 0*x always yields 0 → keep plain Int.
            let lhs_is_zero = matches!(&left.kind, ExprKind::IntLiteral(0));
            let rhs_is_zero = matches!(&right.kind, ExprKind::IntLiteral(0));
            let lhs_is_one = matches!(&left.kind, ExprKind::IntLiteral(1));
            let rhs_is_one = matches!(&right.kind, ExprKind::IntLiteral(1));
            let is_identity = match op {
                BinOp::Add => lhs_is_zero || rhs_is_zero,
                BinOp::Sub => rhs_is_zero,
                BinOp::Mul => lhs_is_zero || rhs_is_zero || lhs_is_one || rhs_is_one,
                _ => false,
            };
            if !is_identity {
                let checked_op = match op {
                    BinOp::Add => Op::ICheckedAdd,
                    BinOp::Sub => Op::ICheckedSub,
                    BinOp::Mul => Op::ICheckedMul,
                    _ => unreachable!(),
                };
                return ctx.emit_value(
                    checked_op,
                    vec![lhs.value, rhs.value],
                    None,
                    PhpType::Mixed,
                    checked_op.default_effects(),
                    Some(expr.span),
                );
            }
        }
        let iop = match op {
            BinOp::Add => Op::IAdd,
            BinOp::Sub => Op::ISub,
            BinOp::Mul => Op::IMul,
            BinOp::Div => Op::IDiv,
            BinOp::Mod => Op::ISMod,
            BinOp::Pow => Op::IPow,
            BinOp::BitAnd => Op::IBitAnd,
            BinOp::BitOr => Op::IBitOr,
            BinOp::BitXor => Op::IBitXor,
            BinOp::ShiftLeft => Op::IShl,
            BinOp::ShiftRight => Op::IShrA,
            _ => Op::MixedNumericBinop,
        };
        let php_type = if matches!(op, BinOp::Div) { PhpType::Float } else { PhpType::Int };
        let result_type = if matches!(op, BinOp::Div) { IrType::F64 } else { IrType::I64 };
        let ownership = Ownership::for_php_type(&php_type);
        let value = ctx
            .builder
            .emit_with_effects(iop, vec![lhs.value, rhs.value], None, result_type, php_type, ownership, arithmetic_effects(iop, right), Some(expr.span))
            .expect("numeric binary produces a value");
        return LoweredValue { value, ir_type: result_type };
    }
    if let Some(mixed_op) = mixed_numeric_op(op) {
        let result = lower_mixed_numeric_binary(ctx, lhs, rhs, mixed_op, expr);
        release_binary_operand_temporary(ctx, lhs, expr.span);
        if rhs.value != lhs.value {
            release_binary_operand_temporary(ctx, rhs, expr.span);
        }
        return result;
    }
    ctx.emit_value(
        Op::RuntimeCall,
        vec![lhs.value, rhs.value],
        None,
        fallback_expr_type(expr),
        effects_lookup::runtime_effects(),
        Some(expr.span),
    )
}

/// Lowers PHP unary plus while preserving its runtime numeric-string and TypeError semantics.
fn lower_unary_plus(
    ctx: &mut LoweringContext<'_, '_>,
    operand: &Expr,
    expr: &Expr,
) -> LoweredValue {
    let value = lower_expr(ctx, operand);
    let php_type = ctx.builder.value_php_type(value.value);
    match php_type {
        PhpType::Int | PhpType::Float => value,
        PhpType::Bool | PhpType::False | PhpType::Void | PhpType::Never | PhpType::TaggedScalar => {
            coerce_to_int(ctx, value, operand)
        }
        _ => {
            let result = ctx.emit_value(
                Op::MixedNumericBinop,
                vec![value.value],
                Some(Immediate::MixedNumericOp(MixedNumericOp::UnaryPlus)),
                PhpType::Mixed,
                Op::MixedNumericBinop.default_effects(),
                Some(expr.span),
            );
            release_binary_operand_temporary(ctx, value, expr.span);
            result
        }
    }
}

/// Returns the EIR opcode and result type for PHP array union operands.
pub(super) fn array_union_plan(
    ctx: &LoweringContext<'_, '_>,
    lhs: ValueId,
    rhs: ValueId,
) -> Option<(Op, PhpType)> {
    let lhs_ty = ctx.builder.value_php_type(lhs).codegen_repr();
    let rhs_ty = ctx.builder.value_php_type(rhs).codegen_repr();
    match (&lhs_ty, &rhs_ty) {
        (PhpType::Array(left_elem), PhpType::Array(right_elem)) => {
            indexed_array_union_element_type(left_elem, right_elem)
                .map(|elem_ty| (Op::ArrayUnion, PhpType::Array(Box::new(elem_ty))))
        }
        (
            PhpType::AssocArray {
                key: left_key,
                value: left_value,
            },
            PhpType::AssocArray {
                key: right_key,
                value: right_value,
            },
        ) => Some((
            Op::HashUnion,
            PhpType::AssocArray {
                key: Box::new(assoc_union_key_type(left_key, right_key)),
                value: Box::new(array_union_value_type(left_value, right_value)),
            },
        )),
        (PhpType::Array(left_elem), PhpType::AssocArray { key, value }) => {
            Some((
                Op::ArrayHashUnion,
                PhpType::AssocArray {
                    key: Box::new(merge_array_key_types(PhpType::Int, key.codegen_repr())),
                    value: Box::new(array_union_value_type(left_elem, value)),
                },
            ))
        }
        (PhpType::AssocArray { key, value }, PhpType::Array(right_elem)) => {
            Some((
                Op::HashArrayUnion,
                PhpType::AssocArray {
                    key: Box::new(merge_array_key_types(key.codegen_repr(), PhpType::Int)),
                    value: Box::new(array_union_value_type(value, right_elem)),
                },
            ))
        }
        _ => None,
    }
}

/// Merges indexed-array element types supported by the current EIR storage model.
pub(super) fn indexed_array_union_element_type(left: &PhpType, right: &PhpType) -> Option<PhpType> {
    if left == right {
        return Some(left.clone());
    }
    if matches!(left, PhpType::Never) {
        return Some(right.codegen_repr());
    }
    if matches!(right, PhpType::Never) {
        return Some(left.codegen_repr());
    }
    let left = left.codegen_repr();
    let right = right.codegen_repr();
    if left == right {
        return Some(left);
    }
    None
}

/// Returns the merged key type for associative-array union operands.
pub(super) fn assoc_union_key_type(left: &PhpType, right: &PhpType) -> PhpType {
    let left = left.codegen_repr();
    let right = right.codegen_repr();
    if left == right {
        left
    } else {
        PhpType::Mixed
    }
}

/// Returns the merged value type for array union operands.
pub(super) fn array_union_value_type(left: &PhpType, right: &PhpType) -> PhpType {
    let left = left.codegen_repr();
    let right = right.codegen_repr();
    if left == right {
        left
    } else if matches!(left, PhpType::Never) {
        right
    } else if matches!(right, PhpType::Never) {
        left
    } else {
        PhpType::Mixed
    }
}

/// Returns true when runtime mixed numeric dispatch is needed before float coercion.
pub(super) fn should_use_mixed_numeric_binop(lhs: IrType, rhs: IrType) -> bool {
    !matches!(lhs, IrType::I64 | IrType::F64)
        || !matches!(rhs, IrType::I64 | IrType::F64)
}

/// Emits a mixed-numeric EIR opcode with the operation immediate required by the backend.
pub(super) fn lower_mixed_numeric_binary(
    ctx: &mut LoweringContext<'_, '_>,
    lhs: LoweredValue,
    rhs: LoweredValue,
    op: MixedNumericOp,
    expr: &Expr,
) -> LoweredValue {
    ctx.emit_value(
        Op::MixedNumericBinop,
        vec![lhs.value, rhs.value],
        Some(Immediate::MixedNumericOp(op)),
        PhpType::Mixed,
        Op::MixedNumericBinop.default_effects(),
        Some(expr.span),
    )
}

/// Maps AST arithmetic to the mixed-numeric runtime helper set currently available.
pub(super) fn mixed_numeric_op(op: &BinOp) -> Option<MixedNumericOp> {
    match op {
        BinOp::Add => Some(MixedNumericOp::Add),
        BinOp::Sub => Some(MixedNumericOp::Sub),
        BinOp::Mul => Some(MixedNumericOp::Mul),
        BinOp::Pow => Some(MixedNumericOp::Pow),
        _ => None,
    }
}
