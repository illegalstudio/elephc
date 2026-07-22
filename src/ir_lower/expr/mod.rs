//! Purpose:
//! Dispatches AST expression nodes into EIR values while preserving source-order
//! evaluation.
//!
//! Called from:
//! - `crate::ir_lower::stmt` and nested expression lowering.
//!
//! Key details:
//! - Simple scalar operations lower to concrete EIR arithmetic/string opcodes.
//! - Complex PHP runtime behavior lowers to high-level EIR opcodes with
//!   conservative effects until Phase 04 gives them target-specific meaning.

use crate::ir::{
    BlockId, CmpPredicate, Effects, Immediate, IrHeapKind, IrType, LocalKind, LocalSlotId,
    MixedNumericOp, Op, Ownership, Terminator, ValueId,
};
use crate::ir_lower::context::{
    value_ir_type, ClosureCapture, LoweredValue, LoweringContext, StaticCallableBinding,
};
use crate::ir_lower::effects_lookup;
use crate::ir_lower::function;
use crate::names::{php_symbol_key, property_hook_get_method, property_hook_set_method, Name};
use crate::parser::ast::{
    is_compound_assignment_self_read, BinOp, CallableTarget, CastType, Expr, ExprKind,
    InstanceOfTarget, MagicConstant, StaticReceiver, Stmt, StmtKind, TypeExpr, Visibility,
};
use crate::span::Span;
use crate::types::checker::builtins::canonical_builtin_function_name;
use crate::types::{
    checker::infer_expr_type_syntactic, merge_array_key_types, normalized_array_key_type,
    ExternFunctionSig, FunctionSig, PhpType, ReturnArgAlias, ThrowAccessKind,
};
use std::collections::HashSet;

mod constants;
mod nullsafe_chain;

/// Lowers an expression and returns its EIR value.
pub(crate) fn lower_expr(ctx: &mut LoweringContext<'_, '_>, expr: &Expr) -> LoweredValue {
    if let Some(value) = nullsafe_chain::lower(ctx, expr) {
        return value;
    }

    match &expr.kind {
        // `IncludeValue` is a transient parser node fully expanded by the resolver;
        // it can never reach this pass.
        ExprKind::IncludeValue { .. } => unreachable!(
            "ExprKind::IncludeValue must be expanded by the resolver"
        ),
        ExprKind::StringLiteral(value) => lower_string_literal(ctx, value, expr),
        ExprKind::IntLiteral(value) => lower_int_literal(ctx, *value, expr),
        ExprKind::FloatLiteral(value) => lower_float_literal(ctx, *value, expr),
        ExprKind::BoolLiteral(value) => lower_bool_literal(ctx, *value, expr),
        ExprKind::Null => lower_null(ctx, expr),
        ExprKind::Variable(name) => ctx.load_local(name, Some(expr.span)),
        ExprKind::BinaryOp { left, op, right } => lower_binary(ctx, left, op, right, expr),
        ExprKind::InstanceOf { value, target } => lower_instanceof(ctx, value, target, expr),
        ExprKind::Negate(inner) => lower_numeric_unary(ctx, inner, Op::INeg, Op::FNeg, expr),
        ExprKind::Not(inner) => lower_not(ctx, inner, expr),
        ExprKind::BitNot(inner) => lower_int_unary(ctx, inner, Op::IBitNot, expr),
        ExprKind::Throw(inner) => lower_throw_expr(ctx, inner, expr),
        ExprKind::ErrorSuppress(inner) => lower_error_suppress(ctx, inner, expr),
        ExprKind::Print(inner) => lower_print(ctx, inner, expr),
        ExprKind::NullCoalesce { value, default } => {
            lower_null_coalesce(ctx, value, default, expr)
        }
        ExprKind::Pipe { value, callable } => lower_pipe(ctx, value, callable, expr),
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            conditional_value_temp,
        } => lower_assignment_expr(
            ctx,
            target,
            value,
            result_target.as_deref(),
            prelude,
            conditional_value_temp.as_deref(),
            expr,
        ),
        ExprKind::PreIncrement(name) => lower_inc_dec(ctx, name, true, false, expr),
        ExprKind::PostIncrement(name) => lower_inc_dec(ctx, name, true, true, expr),
        ExprKind::PreDecrement(name) => lower_inc_dec(ctx, name, false, false, expr),
        ExprKind::PostDecrement(name) => lower_inc_dec(ctx, name, false, true, expr),
        ExprKind::FunctionCall { name, args } => lower_function_call(ctx, name, args, expr),
        ExprKind::ArrayLiteral(items) => lower_array_literal(ctx, items, expr),
        ExprKind::ArrayLiteralAssoc(pairs) => lower_assoc_array_literal(ctx, pairs, expr),
        ExprKind::Match { subject, arms, default } => lower_match(ctx, subject, arms, default.as_deref(), expr),
        ExprKind::ArrayAccess { array, index } => lower_array_access(ctx, array, index, expr),
        ExprKind::Ternary { condition, then_expr, else_expr } => {
            lower_ternary(ctx, condition, then_expr, else_expr, expr)
        }
        ExprKind::ShortTernary { value, default } => {
            lower_short_ternary(ctx, value, default, expr)
        }
        ExprKind::Cast { target, expr: inner } => lower_cast(ctx, target, inner, expr),
        ExprKind::Closure {
            params,
            variadic,
            variadic_by_ref,
            return_type,
            body,
            captures,
            capture_refs,
            is_static,
            ..
        } => lower_closure(
            ctx,
            params,
            variadic.as_deref(),
            *variadic_by_ref,
            return_type.as_ref(),
            body,
            captures,
            capture_refs,
            expr,
            *is_static,
        ),
        ExprKind::NamedArg { value, .. } => lower_expr(ctx, value),
        ExprKind::Spread(inner) => lower_expr(ctx, inner),
        ExprKind::ClosureCall { var, args } => lower_closure_call(ctx, var, args, expr),
        ExprKind::ExprCall { callee, args } => lower_expr_call(ctx, callee, args, expr),
        ExprKind::ConstRef(name) => constants::lower_const_ref(ctx, name, expr),
        ExprKind::NewObject { class_name, args } => lower_new_object(ctx, class_name, args, expr),
        ExprKind::Clone(inner) => lower_clone(ctx, inner, expr),
        ExprKind::NewDynamic { name_expr, args } => {
            lower_new_dynamic(ctx, name_expr, args, expr)
        }
        ExprKind::NewDynamicObject { class_name, fallback_class, required_parent, args } => {
            lower_new_dynamic_object(ctx, class_name, fallback_class, required_parent, args, expr)
        }
        ExprKind::PropertyAccess { object, property } => lower_property_get(ctx, object, property, Op::PropGet, expr),
        ExprKind::DynamicPropertyAccess { object, property } => lower_dynamic_property_get(ctx, object, property, expr),
        ExprKind::NullsafePropertyAccess { object, property } => {
            lower_property_get(ctx, object, property, Op::NullsafePropGet, expr)
        }
        ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            lower_dynamic_property_get(ctx, object, property, expr)
        }
        ExprKind::StaticPropertyAccess { receiver, property } => {
            lower_static_property_get(ctx, receiver, property, expr)
        }
        ExprKind::MethodCall {
            object,
            method,
            args,
        } => lower_method_call(ctx, object, method, args, Op::MethodCall, expr),
        ExprKind::NullsafeMethodCall {
            object,
            method,
            args,
        } => lower_nullsafe_method_call(ctx, object, method, args, expr),
        ExprKind::NullsafeDynamicMethodCall { .. } => {
            unreachable!("nullsafe dynamic method calls are lowered as a nullsafe postfix chain")
        }
        ExprKind::StaticMethodCall {
            receiver,
            method,
            args,
        } => lower_static_method_call(ctx, receiver, method, args, expr),
        ExprKind::FirstClassCallable(target) => lower_first_class_callable(ctx, target, expr),
        ExprKind::This => ctx.load_local("this", Some(expr.span)),
        ExprKind::PtrCast { target_type, expr: inner } => lower_ptr_cast(ctx, target_type, inner, expr),
        ExprKind::BufferNew { element_type, len } => lower_buffer_new(ctx, element_type, len, expr),
        ExprKind::ClassConstant { receiver } => lower_class_constant(ctx, receiver, expr),
        ExprKind::ObjectClassName { object } => lower_object_class_name(ctx, object, expr),
        ExprKind::ScopedConstantAccess { receiver, name } => {
            lower_scoped_constant(ctx, receiver, name, expr)
        }
        ExprKind::NewScopedObject { receiver, args } => lower_new_scoped_object(ctx, receiver, args, expr),
        ExprKind::MagicConstant(kind) => lower_magic_constant(ctx, kind, expr),
        ExprKind::Yield { key, value } => lower_yield(ctx, key.as_deref(), value.as_deref(), expr),
        ExprKind::YieldFrom(inner) => lower_yield_from(ctx, inner, expr),
    }
}

/// Lowers a string literal.
fn lower_string_literal(ctx: &mut LoweringContext<'_, '_>, value: &str, expr: &Expr) -> LoweredValue {
    let data = ctx.intern_string(value);
    let value = ctx
        .builder
        .emit_with_effects(
            Op::ConstStr,
            Vec::new(),
            Some(Immediate::Data(data)),
            IrType::Str,
            PhpType::Str,
            Ownership::Persistent,
            Op::ConstStr.default_effects(),
            Some(expr.span),
        )
        .expect("const_str produces a value");
    LoweredValue { value, ir_type: IrType::Str }
}

/// Lowers an integer literal.
fn lower_int_literal(ctx: &mut LoweringContext<'_, '_>, value: i64, expr: &Expr) -> LoweredValue {
    let value = ctx
        .builder
        .emit_with_effects(
            Op::ConstI64,
            Vec::new(),
            Some(Immediate::I64(value)),
            IrType::I64,
            PhpType::Int,
            Ownership::NonHeap,
            Op::ConstI64.default_effects(),
            Some(expr.span),
        )
        .expect("const_i64 produces a value");
    LoweredValue { value, ir_type: IrType::I64 }
}

/// Lowers a float literal.
fn lower_float_literal(ctx: &mut LoweringContext<'_, '_>, value: f64, expr: &Expr) -> LoweredValue {
    let value = ctx
        .builder
        .emit_with_effects(
            Op::ConstF64,
            Vec::new(),
            Some(Immediate::F64(value)),
            IrType::F64,
            PhpType::Float,
            Ownership::NonHeap,
            Op::ConstF64.default_effects(),
            Some(expr.span),
        )
        .expect("const_f64 produces a value");
    LoweredValue { value, ir_type: IrType::F64 }
}

/// Lowers a boolean literal.
fn lower_bool_literal(ctx: &mut LoweringContext<'_, '_>, value: bool, expr: &Expr) -> LoweredValue {
    let value = ctx
        .builder
        .emit_with_effects(
            Op::ConstBool,
            Vec::new(),
            Some(Immediate::Bool(value)),
            IrType::I64,
            if value { PhpType::Bool } else { PhpType::False },
            Ownership::NonHeap,
            Op::ConstBool.default_effects(),
            Some(expr.span),
        )
        .expect("const_bool produces a value");
    LoweredValue { value, ir_type: IrType::I64 }
}

/// Lowers PHP null.
fn lower_null(ctx: &mut LoweringContext<'_, '_>, expr: &Expr) -> LoweredValue {
    let value = ctx
        .builder
        .emit_with_effects(
            Op::ConstNull,
            Vec::new(),
            None,
            IrType::I64,
            PhpType::Void,
            Ownership::NonHeap,
            Op::ConstNull.default_effects(),
            Some(expr.span),
        )
        .expect("const_null produces a value");
    LoweredValue { value, ir_type: IrType::I64 }
}

/// Lowers a nullsafe expression that is known to short-circuit to PHP null.
fn lower_boxed_null(ctx: &mut LoweringContext<'_, '_>, expr: &Expr) -> LoweredValue {
    let null = lower_null(ctx, expr);
    ctx.box_value_as_mixed(null, PhpType::Mixed, Some(expr.span))
}

/// Lowers a binary operation.
fn lower_binary(
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
fn lower_numeric_binary(
    ctx: &mut LoweringContext<'_, '_>,
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    expr: &Expr,
) -> LoweredValue {
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
            Op::ISMod.default_effects(),
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
            iop.default_effects(),
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
        return ctx.emit_value(fop, vec![lhs.value, rhs.value], None, PhpType::Float, fop.default_effects(), Some(expr.span));
    }
    if matches!(op, BinOp::Div) && (lhs.ir_type != IrType::I64 || rhs.ir_type != IrType::I64) {
        let lhs = coerce_to_float(ctx, lhs, left);
        let rhs = coerce_to_float(ctx, rhs, right);
        return ctx.emit_value(
            Op::FDiv,
            vec![lhs.value, rhs.value],
            None,
            PhpType::Float,
            Op::FDiv.default_effects(),
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
            .emit_with_effects(iop, vec![lhs.value, rhs.value], None, result_type, php_type, ownership, iop.default_effects(), Some(expr.span))
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

/// Returns the EIR opcode and result type for PHP array union operands.
fn array_union_plan(
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
fn indexed_array_union_element_type(left: &PhpType, right: &PhpType) -> Option<PhpType> {
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
fn assoc_union_key_type(left: &PhpType, right: &PhpType) -> PhpType {
    let left = left.codegen_repr();
    let right = right.codegen_repr();
    if left == right {
        left
    } else {
        PhpType::Mixed
    }
}

/// Returns the merged value type for array union operands.
fn array_union_value_type(left: &PhpType, right: &PhpType) -> PhpType {
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
fn should_use_mixed_numeric_binop(lhs: IrType, rhs: IrType) -> bool {
    !matches!(lhs, IrType::I64 | IrType::F64)
        || !matches!(rhs, IrType::I64 | IrType::F64)
}

/// Emits a mixed-numeric EIR opcode with the operation immediate required by the backend.
fn lower_mixed_numeric_binary(
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
fn mixed_numeric_op(op: &BinOp) -> Option<MixedNumericOp> {
    match op {
        BinOp::Add => Some(MixedNumericOp::Add),
        BinOp::Sub => Some(MixedNumericOp::Sub),
        BinOp::Mul => Some(MixedNumericOp::Mul),
        _ => None,
    }
}

/// Lowers string concatenation.
fn lower_concat(ctx: &mut LoweringContext<'_, '_>, left: &Expr, right: &Expr, expr: &Expr) -> LoweredValue {
    let lhs = lower_expr(ctx, left);
    let lhs = coerce_to_string(ctx, lhs, expr);
    let lhs = persist_concat_lhs_if_rhs_can_reset(ctx, lhs, right, expr.span);
    let rhs = lower_expr(ctx, right);
    let rhs = coerce_to_string(ctx, rhs, expr);
    if lhs.ir_type == IrType::Str && rhs.ir_type == IrType::Str {
        let result = ctx.emit_value(
            Op::StrConcat,
            vec![lhs.value, rhs.value],
            None,
            PhpType::Str,
            Op::StrConcat.default_effects(),
            Some(expr.span),
        );
        release_binary_operand_temporary(ctx, lhs, expr.span);
        if rhs.value != lhs.value {
            release_binary_operand_temporary(ctx, rhs, expr.span);
        }
        return result;
    }
    let result = ctx.emit_value(
        Op::RuntimeCall,
        vec![lhs.value, rhs.value],
        None,
        PhpType::Str,
        effects_lookup::runtime_effects(),
        Some(expr.span),
    );
    release_binary_operand_temporary(ctx, lhs, expr.span);
    if rhs.value != lhs.value {
        release_binary_operand_temporary(ctx, rhs, expr.span);
    }
    result
}

/// Persists scratch-backed concat LHS values before a call-like RHS can reset concat storage.
fn persist_concat_lhs_if_rhs_can_reset(
    ctx: &mut LoweringContext<'_, '_>,
    lhs: LoweredValue,
    rhs: &Expr,
    span: Span,
) -> LoweredValue {
    if lhs.ir_type != IrType::Str {
        return lhs;
    }
    let Some(op) = ctx.builder.value_defining_op(lhs.value) else {
        return lhs;
    };
    if !string_op_uses_scratch_storage(op) || !expr_can_reset_concat_storage(rhs) {
        return lhs;
    }
    ctx.emit_value(
        Op::StrPersist,
        vec![lhs.value],
        None,
        PhpType::Str,
        Op::StrPersist.default_effects(),
        Some(span),
    )
}

/// Returns whether a string-producing opcode exposes scratch or borrowed string storage.
pub(crate) fn string_op_uses_scratch_storage(op: Op) -> bool {
    matches!(
        op,
        Op::IToStr
            | Op::FToStr
            | Op::BoolToStr
            | Op::ResourceToStr
            | Op::MixedCastString
            | Op::StrConcat
            | Op::StrCharAt
            | Op::StrInterpolate
            | Op::RuntimeCall
    )
}

/// Returns whether evaluating an expression can reset the caller's concat scratch storage.
fn expr_can_reset_concat_storage(expr: &Expr) -> bool {
    match &expr.kind {
        // `IncludeValue` is a transient parser node fully expanded by the resolver;
        // it can never reach this pass.
        ExprKind::IncludeValue { .. } => unreachable!(
            "ExprKind::IncludeValue must be expanded by the resolver"
        ),
        ExprKind::FunctionCall { .. }
        | ExprKind::ClosureCall { .. }
        | ExprKind::ExprCall { .. }
        | ExprKind::MethodCall { .. }
        | ExprKind::NullsafeMethodCall { .. }
        | ExprKind::NullsafeDynamicMethodCall { .. }
        | ExprKind::StaticMethodCall { .. }
        | ExprKind::NewObject { .. }
        | ExprKind::NewDynamic { .. }
        | ExprKind::NewDynamicObject { .. }
        | ExprKind::NewScopedObject { .. }
        | ExprKind::Clone(_)
        | ExprKind::Pipe { .. }
        | ExprKind::Yield { .. }
        | ExprKind::YieldFrom(_) => true,
        ExprKind::BinaryOp { left, right, .. } => {
            expr_can_reset_concat_storage(left) || expr_can_reset_concat_storage(right)
        }
        ExprKind::InstanceOf { value, target } => {
            expr_can_reset_concat_storage(value)
                || matches!(target, InstanceOfTarget::Expr(inner) if expr_can_reset_concat_storage(inner))
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Cast { expr: inner, .. }
        | ExprKind::NamedArg { value: inner, .. }
        | ExprKind::Spread(inner)
        | ExprKind::PtrCast { expr: inner, .. }
        | ExprKind::BufferNew { len: inner, .. }
        | ExprKind::ObjectClassName { object: inner } => {
            expr_can_reset_concat_storage(inner)
        }
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default } => {
            expr_can_reset_concat_storage(value) || expr_can_reset_concat_storage(default)
        }
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            ..
        } => {
            !prelude.is_empty()
                || expr_can_reset_concat_storage(target)
                || expr_can_reset_concat_storage(value)
                || result_target
                    .as_ref()
                    .is_some_and(|target| expr_can_reset_concat_storage(target))
        }
        ExprKind::ArrayLiteral(items) => items.iter().any(expr_can_reset_concat_storage),
        ExprKind::ArrayLiteralAssoc(items) => items
            .iter()
            .any(|(key, value)| expr_can_reset_concat_storage(key) || expr_can_reset_concat_storage(value)),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            expr_can_reset_concat_storage(subject)
                || arms.iter().any(|(conditions, result)| {
                    conditions.iter().any(expr_can_reset_concat_storage)
                        || expr_can_reset_concat_storage(result)
                })
                || default
                    .as_ref()
                    .is_some_and(|default| expr_can_reset_concat_storage(default))
        }
        ExprKind::ArrayAccess { array, index } => {
            expr_can_reset_concat_storage(array) || expr_can_reset_concat_storage(index)
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            expr_can_reset_concat_storage(condition)
                || expr_can_reset_concat_storage(then_expr)
                || expr_can_reset_concat_storage(else_expr)
        }
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            expr_can_reset_concat_storage(object) || expr_can_reset_concat_storage(property)
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => {
            expr_can_reset_concat_storage(object)
        }
        ExprKind::FirstClassCallable(target) => callable_target_can_reset_concat_storage(target),
        ExprKind::Closure { .. }
        | ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::Variable(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::PreIncrement(_)
        | ExprKind::PostIncrement(_)
        | ExprKind::PreDecrement(_)
        | ExprKind::PostDecrement(_)
        | ExprKind::ConstRef(_)
        | ExprKind::StaticPropertyAccess { .. }
        | ExprKind::This
        | ExprKind::ClassConstant { .. }
        | ExprKind::ScopedConstantAccess { .. }
        | ExprKind::MagicConstant(_) => false,
    }
}

/// Returns whether constructing a callable target evaluates an expression that can reset concat.
fn callable_target_can_reset_concat_storage(target: &CallableTarget) -> bool {
    match target {
        CallableTarget::Function(_) | CallableTarget::StaticMethod { .. } => false,
        CallableTarget::Method { object, .. } => expr_can_reset_concat_storage(object),
    }
}

/// Lowers a comparison operation.
fn lower_compare(
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

/// Releases an owning binary-operator operand once the consuming opcode has read it.
fn release_binary_operand_temporary(
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
fn datetime_instant_compare_operator(op: &BinOp) -> bool {
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
fn is_datetime_family_value(ctx: &LoweringContext<'_, '_>, value: ValueId) -> bool {
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
fn lower_datetime_instant_key(
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
fn cmp_predicate(op: &BinOp) -> CmpPredicate {
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

/// Lowers a numeric unary operation.
fn lower_numeric_unary(
    ctx: &mut LoweringContext<'_, '_>,
    inner: &Expr,
    int_op: Op,
    float_op: Op,
    expr: &Expr,
) -> LoweredValue {
    let value = lower_expr(ctx, inner);
    match value.ir_type {
        IrType::F64 => ctx.emit_value(float_op, vec![value.value], None, PhpType::Float, float_op.default_effects(), Some(expr.span)),
        IrType::I64 => {
            // Check if the type checker promoted this to Mixed (non-constant int negate
            // can overflow PHP_INT_MIN to float).
            let result_php_type = fallback_expr_type(expr);
            if result_php_type == PhpType::Mixed && int_op == Op::INeg {
                // Emit a checked negate via the mixed numeric sub helper: 0 - value
                let zero = lower_int_literal(ctx, 0, expr);
                return lower_mixed_numeric_binary(ctx, zero, value, MixedNumericOp::Sub, expr);
            }
            ctx.emit_value(int_op, vec![value.value], None, PhpType::Int, int_op.default_effects(), Some(expr.span))
        }
        IrType::TaggedScalar => {
            let narrowed = lower_tagged_scalar_to_int(ctx, value, Some(expr.span));
            ctx.emit_value(int_op, vec![narrowed.value], None, PhpType::Int, int_op.default_effects(), Some(expr.span))
        }
        _ if int_op == Op::INeg => {
            let zero = lower_int_literal(ctx, 0, expr);
            let result = lower_mixed_numeric_binary(ctx, zero, value, MixedNumericOp::Sub, expr);
            // Mirror the binary mixed-op path: an owning boxed operand (e.g.
            // `-($i * 7 + 1)`, issue #500) must be released once consumed.
            release_binary_operand_temporary(ctx, value, expr.span);
            result
        }
        _ => ctx.emit_value(Op::RuntimeCall, vec![value.value], None, PhpType::Mixed, Effects::all(), Some(expr.span)),
    }
}

/// Lowers an integer unary operation.
fn lower_int_unary(ctx: &mut LoweringContext<'_, '_>, inner: &Expr, op: Op, expr: &Expr) -> LoweredValue {
    let value = lower_expr(ctx, inner);
    if value.ir_type == IrType::I64 {
        ctx.emit_value(op, vec![value.value], None, PhpType::Int, op.default_effects(), Some(expr.span))
    } else if value.ir_type == IrType::TaggedScalar {
        let narrowed = lower_tagged_scalar_to_int(ctx, value, Some(expr.span));
        ctx.emit_value(op, vec![narrowed.value], None, PhpType::Int, op.default_effects(), Some(expr.span))
    } else {
        ctx.emit_value(Op::RuntimeCall, vec![value.value], None, PhpType::Mixed, Effects::all(), Some(expr.span))
    }
}

/// Lowers a tagged scalar into PHP int semantics, coercing null to zero.
fn lower_tagged_scalar_to_int(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<Span>,
) -> LoweredValue {
    ctx.emit_value(
        Op::Cast,
        vec![value.value],
        Some(Immediate::CastTarget(IrType::I64)),
        PhpType::Int,
        Op::Cast.default_effects(),
        span,
    )
}

/// Lowers logical negation.
fn lower_not(ctx: &mut LoweringContext<'_, '_>, inner: &Expr, expr: &Expr) -> LoweredValue {
    let value = lower_expr(ctx, inner);
    let value = ctx.truthy(value, Some(expr.span));
    let zero = lower_int_literal(ctx, 0, expr);
    ctx.emit_value(
        Op::ICmp,
        vec![value.value, zero.value],
        Some(Immediate::CmpPredicate(CmpPredicate::Eq)),
        PhpType::Bool,
        Op::ICmp.default_effects(),
        Some(expr.span),
    )
}

/// Lowers throw used as an expression and returns a placeholder null value.
fn lower_throw_expr(ctx: &mut LoweringContext<'_, '_>, inner: &Expr, expr: &Expr) -> LoweredValue {
    let value = lower_expr(ctx, inner);
    // Match statement-form `throw`: transfer owning temps, but retain loads that
    // leave a local slot as owner (e.g. `true ? throw $e : 0` after a catch bind).
    let transferable = ctx.value_is_owning_temporary(value)
        && !ctx.value_is_owned_unboxed_local_load(value.value);
    let value = if transferable {
        value
    } else {
        crate::ir_lower::ownership::acquire_if_refcounted(ctx, value, Some(inner.span))
    };
    ctx.emit_void(
        Op::ThrowException,
        vec![value.value],
        None,
        Op::ThrowException.default_effects(),
        Some(expr.span),
    );
    lower_null(ctx, expr)
}

/// Lowers an error-suppressed expression.
fn lower_error_suppress(ctx: &mut LoweringContext<'_, '_>, inner: &Expr, expr: &Expr) -> LoweredValue {
    ctx.emit_void(Op::ErrorSuppressBegin, Vec::new(), None, Op::ErrorSuppressBegin.default_effects(), Some(expr.span));
    let value = lower_expr(ctx, inner);
    ctx.emit_void(Op::ErrorSuppressEnd, Vec::new(), None, Op::ErrorSuppressEnd.default_effects(), Some(expr.span));
    value
}

/// Lowers `print`.
fn lower_print(ctx: &mut LoweringContext<'_, '_>, inner: &Expr, expr: &Expr) -> LoweredValue {
    let value = lower_expr(ctx, inner);
    ctx.emit_void(Op::PrintValue, vec![value.value], None, Op::PrintValue.default_effects(), Some(expr.span));
    lower_int_literal(ctx, 1, expr)
}

/// Lowers short-circuiting logical `&&` and `||`.
fn lower_logical_binary(
    ctx: &mut LoweringContext<'_, '_>,
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    expr: &Expr,
) -> LoweredValue {
    let lhs = lower_expr(ctx, left);
    let lhs = ctx.truthy(lhs, Some(left.span));
    let temp_name = ctx.declare_hidden_temp(PhpType::Bool);
    let rhs_block = ctx.builder.create_named_block("logical.rhs", Vec::new());
    let const_block = ctx.builder.create_named_block("logical.const", Vec::new());
    let merge = ctx.builder.create_named_block("logical.merge", Vec::new());
    let (then_target, else_target) = match op {
        BinOp::And => (rhs_block, const_block),
        BinOp::Or => (const_block, rhs_block),
        _ => unreachable!("only short-circuit logical operators reach this lowering"),
    };
    ctx.builder.terminate(Terminator::CondBr {
        cond: lhs.value,
        then_target,
        then_args: Vec::new(),
        else_target,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(rhs_block);
    let rhs = lower_expr(ctx, right);
    let rhs = ctx.truthy(rhs, Some(right.span));
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, rhs, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(const_block);
    let const_value = emit_bool_literal(ctx, matches!(op, BinOp::Or), Some(expr.span));
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, const_value, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    take_owned_temp(ctx, &temp_name, expr.span)
}

/// Lowers non-short-circuiting PHP logical `xor`.
fn lower_logical_xor(
    ctx: &mut LoweringContext<'_, '_>,
    left: &Expr,
    right: &Expr,
    expr: &Expr,
) -> LoweredValue {
    let lhs = lower_expr(ctx, left);
    let lhs = lower_truthy_bool(ctx, lhs, Some(left.span));
    let rhs = lower_expr(ctx, right);
    let rhs = lower_truthy_bool(ctx, rhs, Some(right.span));
    ctx.emit_value(
        Op::ICmp,
        vec![lhs.value, rhs.value],
        Some(Immediate::CmpPredicate(CmpPredicate::Ne)),
        PhpType::Bool,
        Op::ICmp.default_effects(),
        Some(expr.span),
    )
}

/// Converts a lowered PHP value into a canonical boolean value for value-level logical ops.
fn lower_truthy_bool(
    ctx: &mut LoweringContext<'_, '_>,
    input: LoweredValue,
    span: Option<crate::span::Span>,
) -> LoweredValue {
    match ctx.builder.value_php_type(input.value).codegen_repr() {
        PhpType::Bool => input,
        PhpType::Int => {
            let zero = ctx
                .builder
                .emit_with_effects(
                    Op::ConstI64,
                    Vec::new(),
                    Some(Immediate::I64(0)),
                    IrType::I64,
                    PhpType::Int,
                    Ownership::NonHeap,
                    Op::ConstI64.default_effects(),
                    span,
                )
                .expect("const_i64 produces a value");
            ctx.emit_value(
                Op::ICmp,
                vec![input.value, zero],
                Some(Immediate::CmpPredicate(CmpPredicate::Ne)),
                PhpType::Bool,
                Op::ICmp.default_effects(),
                span,
            )
        }
        PhpType::Void | PhpType::Never => emit_bool_literal(ctx, false, span),
        _ => ctx.emit_value(
            Op::IsTruthy,
            vec![input.value],
            None,
            PhpType::Bool,
            Op::IsTruthy.default_effects(),
            span,
        ),
    }
}

/// Lowers null coalesce so the default expression is evaluated only for null values.
fn lower_null_coalesce(
    ctx: &mut LoweringContext<'_, '_>,
    value: &Expr,
    default: &Expr,
    expr: &Expr,
) -> LoweredValue {
    let value = lower_null_coalesce_value(ctx, value);
    let is_null = ctx.emit_value(
        Op::IsNull,
        vec![value.value],
        None,
        PhpType::Bool,
        Op::IsNull.default_effects(),
        Some(expr.span),
    );
    let result_type = null_coalesce_result_type(ctx, value.value, default);
    let temp_name = ctx.declare_owned_hidden_temp(result_type.clone());
    let split_initialized = ctx.initialized_slots_snapshot();
    let default_block = ctx
        .builder
        .create_named_block("coalesce.default", Vec::new());
    let value_block = ctx.builder.create_named_block("coalesce.value", Vec::new());
    let merge = ctx.builder.create_named_block("coalesce.merge", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: is_null.value,
        then_target: default_block,
        then_args: Vec::new(),
        else_target: value_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(default_block);
    ctx.restore_initialized_slots(split_initialized.clone());
    store_expr_into_temp(ctx, &temp_name, result_type.clone(), default, expr.span);
    release_discarded_branch_value(ctx, value, expr.span);
    let default_reachable = !ctx.builder.insertion_block_is_terminated();
    let default_initialized = ctx.initialized_slots_snapshot();
    branch_to(ctx, merge);

    ctx.builder.position_at_end(value_block);
    ctx.restore_initialized_slots(split_initialized.clone());
    store_value_into_temp(ctx, &temp_name, result_type, value, expr.span);
    let value_reachable = !ctx.builder.insertion_block_is_terminated();
    let value_initialized = ctx.initialized_slots_snapshot();
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    ctx.restore_initialized_slots(merge_initialized_slots_for_expr(
        &split_initialized,
        default_initialized,
        default_reachable,
        value_initialized,
        value_reachable,
    ));
    take_owned_temp(ctx, &temp_name, expr.span)
}

/// Lowers the value side of `??`, suppressing undefined-offset warnings from
/// native array reads while preserving nullsafe-chain lazy evaluation.
fn lower_null_coalesce_value(ctx: &mut LoweringContext<'_, '_>, value: &Expr) -> LoweredValue {
    if let Some(value) = nullsafe_chain::lower_with_missing_warning(ctx, value, false) {
        return value;
    }
    if let ExprKind::ArrayAccess { array, index } = &value.kind {
        return lower_array_access_with_missing_warning(ctx, array, index, value, false);
    }
    lower_expr(ctx, value)
}

/// Returns the materialized result type for a null-coalesce merge.
fn null_coalesce_result_type(
    ctx: &LoweringContext<'_, '_>,
    value: ValueId,
    default: &Expr,
) -> PhpType {
    let value_ty = strip_void_from_union(ctx.builder.value_php_type(value)).codegen_repr();
    let default_ty = materialized_expr_type_for_merge(ctx, default).codegen_repr();
    wider_type_for_merge(&value_ty, &default_ty)
}

/// Chooses the wider materialized type for branch-local merge storage.
fn wider_type_for_merge(left: &PhpType, right: &PhpType) -> PhpType {
    let left = left.codegen_repr();
    let right = right.codegen_repr();
    if left == right {
        return left;
    }
    if matches!(left, PhpType::Void | PhpType::Never) {
        return right;
    }
    if matches!(right, PhpType::Void | PhpType::Never) {
        return left;
    }
    match (&left, &right) {
        // Mismatched element types must widen elementwise (issue #549): letting
        // one side win wholesale relabels the other side's runtime slots, so
        // typed reads through the merged type misinterpret the payload bytes.
        (PhpType::Array(left_elem), PhpType::Array(right_elem)) => {
            PhpType::Array(Box::new(merge_ir_indexed_element_type(
                left_elem.codegen_repr(),
                right_elem.codegen_repr(),
            )))
        }
        (
            PhpType::AssocArray { key: left_key, value: left_value },
            PhpType::AssocArray { key: right_key, value: right_value },
        ) => PhpType::AssocArray {
            key: Box::new(merge_ir_assoc_value_type(
                left_key.codegen_repr(),
                right_key.codegen_repr(),
            )),
            value: Box::new(merge_ir_assoc_value_type(
                left_value.codegen_repr(),
                right_value.codegen_repr(),
            )),
        },
        (
            PhpType::Int | PhpType::Bool | PhpType::Void | PhpType::Never,
            PhpType::Int | PhpType::Bool | PhpType::Void | PhpType::Never,
        ) => right.clone(),
        _ => PhpType::Mixed,
    }
}

/// Removes the null sentinel type from nullable unions after a successful `??` value branch.
fn strip_void_from_union(php_type: PhpType) -> PhpType {
    let PhpType::Union(members) = php_type else {
        return php_type;
    };
    let mut non_void = members
        .into_iter()
        .filter(|member| !matches!(member, PhpType::Void))
        .collect::<Vec<_>>();
    if non_void.is_empty() {
        PhpType::Void
    } else if non_void.len() == 1 {
        non_void.remove(0)
    } else {
        PhpType::Union(non_void)
    }
}

/// Lowers `expr ?: default`, preserving single evaluation of the first expression.
fn lower_short_ternary(
    ctx: &mut LoweringContext<'_, '_>,
    value: &Expr,
    default: &Expr,
    expr: &Expr,
) -> LoweredValue {
    let condition_span = value.span;
    let result_type = short_ternary_merge_result_type(ctx, value, default);
    let value = lower_expr(ctx, value);
    let cond = ctx.truthy(value, Some(condition_span));
    let temp_name = ctx.declare_owned_hidden_temp(result_type.clone());
    let split_initialized = ctx.initialized_slots_snapshot();
    let value_block = ctx
        .builder
        .create_named_block("short_ternary.value", Vec::new());
    let default_block = ctx
        .builder
        .create_named_block("short_ternary.default", Vec::new());
    let merge = ctx
        .builder
        .create_named_block("short_ternary.merge", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: cond.value,
        then_target: value_block,
        then_args: Vec::new(),
        else_target: default_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(value_block);
    ctx.restore_initialized_slots(split_initialized.clone());
    store_value_into_temp(ctx, &temp_name, result_type.clone(), value, expr.span);
    let value_reachable = !ctx.builder.insertion_block_is_terminated();
    let value_initialized = ctx.initialized_slots_snapshot();
    branch_to(ctx, merge);

    ctx.builder.position_at_end(default_block);
    ctx.restore_initialized_slots(split_initialized.clone());
    store_expr_into_temp(ctx, &temp_name, result_type, default, expr.span);
    release_discarded_branch_value(ctx, value, expr.span);
    let default_reachable = !ctx.builder.insertion_block_is_terminated();
    let default_initialized = ctx.initialized_slots_snapshot();
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    ctx.restore_initialized_slots(merge_initialized_slots_for_expr(
        &split_initialized,
        value_initialized,
        value_reachable,
        default_initialized,
        default_reachable,
    ));
    take_owned_temp(ctx, &temp_name, expr.span)
}

/// Releases a lowered value that a lazy branch tested but did not forward.
fn release_discarded_branch_value(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Span,
) {
    if ctx.value_needs_release_after_retaining_store(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    }
}

/// Lowers a pipe operation.
fn lower_pipe(ctx: &mut LoweringContext<'_, '_>, value: &Expr, callable: &Expr, expr: &Expr) -> LoweredValue {
    match &callable.kind {
        ExprKind::FirstClassCallable(CallableTarget::Function(name)) => {
            let arg = lower_pipe_value_temp(ctx, value, expr);
            let synthetic = Expr::new(
                ExprKind::FunctionCall {
                    name: name.clone(),
                    args: vec![arg],
                },
                expr.span,
            );
            lower_expr(ctx, &synthetic)
        }
        ExprKind::FirstClassCallable(CallableTarget::StaticMethod { receiver, method }) => {
            let arg = lower_pipe_value_temp(ctx, value, expr);
            let synthetic = Expr::new(
                ExprKind::StaticMethodCall {
                    receiver: receiver.clone(),
                    method: method.clone(),
                    args: vec![arg],
                },
                expr.span,
            );
            lower_expr(ctx, &synthetic)
        }
        ExprKind::FirstClassCallable(CallableTarget::Method { object, method }) => {
            let arg = lower_pipe_value_temp(ctx, value, expr);
            let synthetic = Expr::new(
                ExprKind::MethodCall {
                    object: object.clone(),
                    method: method.clone(),
                    args: vec![arg],
                },
                expr.span,
            );
            lower_expr(ctx, &synthetic)
        }
        ExprKind::Variable(name) => lower_pipe_callable_variable(ctx, value, name, expr),
        _ => lower_pipe_runtime_call(ctx, value, callable, expr),
    }
}

/// Lowers `value |> $callable` when the local still has straight-line callable metadata.
fn lower_pipe_callable_variable(
    ctx: &mut LoweringContext<'_, '_>,
    value: &Expr,
    name: &str,
    expr: &Expr,
) -> LoweredValue {
    let arg = lower_pipe_value_temp(ctx, value, expr);
    let callable = Expr::new(ExprKind::Variable(name.to_string()), expr.span);
    let Some(target) = ctx.static_callable_local(name) else {
        return lower_pipe_runtime_call(ctx, &arg, &callable, expr);
    };
    if matches!(target, StaticCallableBinding::InstanceMethod { .. }) {
        emit_backend_comment_marker(ctx, &format!("call descriptor variable ${}()", name), expr.span);
        return lower_pipe_runtime_call(ctx, &arg, &callable, expr);
    }
    emit_backend_comment_marker(
        ctx,
        &format!("uninvoked FCC wrapper ${} (stubbed by EIR direct pipe call)", name),
        expr.span,
    );
    let fallback_arg = arg.clone();
    lower_static_callable_call(ctx, target, &[arg], expr).unwrap_or_else(|| {
        lower_pipe_runtime_call(ctx, &fallback_arg, &callable, expr)
    })
}

/// Emits a backend-only comment marker using a void EIR NOP instruction.
fn emit_backend_comment_marker(ctx: &mut LoweringContext<'_, '_>, message: &str, span: Span) {
    let data = ctx.intern_string(message);
    ctx.emit_void(
        Op::Nop,
        Vec::new(),
        Some(Immediate::Data(data)),
        Op::Nop.default_effects(),
        Some(span),
    );
}

/// Lowers the pipe input once, stores it in a hidden local, and returns a temp argument expression.
fn lower_pipe_value_temp(ctx: &mut LoweringContext<'_, '_>, value: &Expr, expr: &Expr) -> Expr {
    let value = lower_expr(ctx, value);
    let temp_type = ctx.builder.value_php_type(value.value);
    let temp_name = ctx.declare_hidden_temp(temp_type.clone());
    store_value_into_temp(ctx, &temp_name, temp_type, value, expr.span);
    Expr::new(ExprKind::Variable(temp_name), expr.span)
}

/// Lowers pipe shapes that still need a dynamic callable invocation backend path.
fn lower_pipe_runtime_call(
    ctx: &mut LoweringContext<'_, '_>,
    value: &Expr,
    callable: &Expr,
    expr: &Expr,
) -> LoweredValue {
    let result_type = pipe_runtime_result_type(ctx, callable, expr);
    let value = lower_expr(ctx, value);
    let callable = lower_expr(ctx, callable);
    ctx.emit_value(
        Op::PipeCall,
        vec![value.value, callable.value],
        None,
        result_type,
        Op::PipeCall.default_effects(),
        Some(expr.span),
    )
}

/// Returns the best known result type for a runtime-lowered pipe call.
fn pipe_runtime_result_type(
    ctx: &LoweringContext<'_, '_>,
    callable: &Expr,
    expr: &Expr,
) -> PhpType {
    match &callable.kind {
        ExprKind::Variable(name) => ctx
            .static_callable_local(name)
            .map(|target| static_callable_return_type(ctx, &target))
            .unwrap_or_else(|| fallback_expr_type(expr)),
        _ => fallback_expr_type(expr),
    }
}

/// Lowers an assignment expression.
fn lower_assignment_expr(
    ctx: &mut LoweringContext<'_, '_>,
    target: &Expr,
    value: &Expr,
    result_target: Option<&Expr>,
    prelude: &[crate::parser::ast::Stmt],
    conditional_value_temp: Option<&str>,
    expr: &Expr,
) -> LoweredValue {
    for stmt in prelude {
        crate::ir_lower::stmt::lower_stmt(ctx, stmt);
    }
    if let Some(temp_name) = conditional_value_temp {
        if let Some(result) = lower_conditional_non_local_null_coalesce_assignment(
            ctx,
            temp_name,
            target,
            value,
            result_target,
            expr,
        ) {
            return result;
        }
    }
    let assigned_name = match &target.kind {
        ExprKind::Variable(name) => Some(name.as_str()),
        _ => None,
    };
    if let Some(name) = assigned_name {
        if is_compound_assignment_self_read(value, name, expr.span) && !ctx.has_local_slot(name) {
            let null_value = ctx.builder.emit_const_null();
            let null_lowered = LoweredValue { value: null_value, ir_type: IrType::I64 };
            ctx.store_local(name, null_lowered, PhpType::Void, Some(expr.span));
            ctx.mark_local_initialized(name);
        }
    }
    let static_callable = assigned_name.and_then(|_| static_callable_binding_for_expr(ctx, value));
    let reflected_class = assigned_name.and_then(|_| reflection_class_binding_for_expr(ctx, value));
    let reflected_function =
        assigned_name.and_then(|_| reflection_function_binding_for_expr(ctx, value));
    let reflected_property =
        assigned_name.and_then(|_| reflection_property_binding_for_expr(ctx, value));
    let reflected_method =
        assigned_name.and_then(|_| reflection_method_binding_for_expr(ctx, value));
    let reflected_args = assigned_name.and_then(|_| reflection_arg_array_binding_for_expr(value));
    let fiber_start_sig =
        assigned_name.and_then(|_| crate::ir_lower::fibers::start_sig_for_expr(ctx, value));
    let callable_array = assigned_name
        .and_then(|_| lower_callable_array_for_assignment(ctx, value, static_callable.as_ref()));
    let lowered = assigned_name
        .and_then(|_| callable_array.as_ref().map(|assignment| assignment.value))
        .or_else(|| assigned_name.and_then(|name| lower_closure_for_assignment(ctx, name, value)))
        .unwrap_or_else(|| lower_expr(ctx, value));
    let mut result = lowered;
    if let ExprKind::Variable(name) = &target.kind {
        // For static locals and ref-bound locals, keep the declared type to
        // avoid widening Int→Mixed. The codegen narrows Mixed→Int when the slot
        // is Int-typed. Without this, ref cells would hold Mixed boxes instead
        // of raw ints, breaking the ref cell ownership model.
        let value_php_type = ctx.builder.value_php_type(lowered.value);
        let is_static = matches!(
            ctx.local_kinds.get(name).copied(),
            Some(crate::ir::LocalKind::StaticLocal)
        );
        let is_ref_bound = ctx.is_ref_bound_local(name);
        let existing_type = ctx.local_types.get(name).cloned();
        let php_type = if is_static || is_ref_bound {
            existing_type.unwrap_or(value_php_type)
        } else {
            value_php_type
        };
        ctx.store_local(name, lowered, php_type, Some(expr.span));
        result = ctx.load_local(name, Some(expr.span));
        let static_callable = callable_array
            .map(|assignment| assignment.target)
            .or(static_callable);
        if let Some(target) = static_callable {
            ctx.bind_static_callable_local(name, target);
        }
        if let Some(reflected_class) = reflected_class {
            ctx.bind_reflection_class_local(name, reflected_class);
        }
        if let Some(reflected_function) = reflected_function {
            ctx.bind_reflection_function_local(name, reflected_function);
        }
        if let Some((reflected_class, reflected_property)) = reflected_property {
            ctx.bind_reflection_property_local(name, reflected_class, reflected_property);
        }
        if let Some((reflected_class, reflected_method)) = reflected_method {
            ctx.bind_reflection_method_local(name, reflected_class, reflected_method);
        }
        if let Some(reflected_args) = reflected_args {
            ctx.bind_reflection_arg_array_local(name, reflected_args);
        }
        if let Some(sig) = fiber_start_sig {
            ctx.bind_fiber_start_sig(name, sig);
        }
    } else {
        lower_non_local_assignment_write(ctx, target, value, expr.span);
    }
    if let Some(result_target) = result_target {
        return lower_expr(ctx, result_target);
    }
    result
}

/// Lowers a non-local `??=` assignment expression with lazy RHS evaluation.
fn lower_conditional_non_local_null_coalesce_assignment(
    ctx: &mut LoweringContext<'_, '_>,
    temp_name: &str,
    target: &Expr,
    value: &Expr,
    _result_target: Option<&Expr>,
    expr: &Expr,
) -> Option<LoweredValue> {
    let ExprKind::NullCoalesce {
        value: current,
        default,
    } = &value.kind
    else {
        return None;
    };
    let current = lower_expr(ctx, current);
    let is_null = ctx.emit_value(
        Op::IsNull,
        vec![current.value],
        None,
        PhpType::Bool,
        Op::IsNull.default_effects(),
        Some(expr.span),
    );
    let result_type = null_coalesce_result_type(ctx, current.value, default);
    ctx.declare_owned_hidden_temp_with_name(temp_name, result_type.clone());
    let assign_block = ctx.builder.create_named_block("coalesce_assign.default", Vec::new());
    let keep_block = ctx.builder.create_named_block("coalesce_assign.value", Vec::new());
    let merge = ctx.builder.create_named_block("coalesce_assign.merge", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: is_null.value,
        then_target: assign_block,
        then_args: Vec::new(),
        else_target: keep_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(assign_block);
    store_expr_into_temp(ctx, temp_name, result_type.clone(), default, expr.span);
    let temp_value = Expr::new(ExprKind::Variable(temp_name.to_string()), expr.span);
    lower_non_local_assignment_write(ctx, target, &temp_value, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(keep_block);
    store_value_into_temp(ctx, temp_name, result_type, current, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    Some(ctx.load_local(temp_name, Some(expr.span)))
}

/// Emits the write side of an assignment expression whose target is not a local variable.
fn lower_non_local_assignment_write(
    ctx: &mut LoweringContext<'_, '_>,
    target: &Expr,
    value: &Expr,
    span: Span,
) {
    if let ExprKind::DynamicPropertyAccess { object, property } = &target.kind {
        lower_dynamic_property_assign(ctx, object, property, value, span);
        return;
    }
    let Some(kind) = non_local_assignment_stmt_kind(target, value) else {
        lower_expr(ctx, value);
        return;
    };
    crate::ir_lower::stmt::lower_stmt(ctx, &Stmt::new(kind, span));
}

/// Builds the statement form that already owns lowering for non-local writes.
fn non_local_assignment_stmt_kind(target: &Expr, value: &Expr) -> Option<StmtKind> {
    match &target.kind {
        ExprKind::ArrayAccess { array, index } => match &array.kind {
            ExprKind::Variable(array) => Some(StmtKind::ArrayAssign {
                array: array.clone(),
                index: (**index).clone(),
                value: value.clone(),
            }),
            ExprKind::PropertyAccess { object, property } => Some(StmtKind::PropertyArrayAssign {
                object: object.clone(),
                property: property.clone(),
                index: (**index).clone(),
                value: value.clone(),
            }),
            ExprKind::StaticPropertyAccess { receiver, property } => {
                Some(StmtKind::StaticPropertyArrayAssign {
                    receiver: receiver.clone(),
                    property: property.clone(),
                    index: (**index).clone(),
                    value: value.clone(),
                })
            }
            _ => Some(StmtKind::NestedArrayAssign {
                target: target.clone(),
                value: value.clone(),
            }),
        },
        ExprKind::PropertyAccess { object, property } => Some(StmtKind::PropertyAssign {
            object: object.clone(),
            property: property.clone(),
            value: value.clone(),
        }),
        ExprKind::StaticPropertyAccess { receiver, property } => {
            Some(StmtKind::StaticPropertyAssign {
                receiver: receiver.clone(),
                property: property.clone(),
                value: value.clone(),
            })
        }
        _ => None,
    }
}

/// Lowers a runtime-name property write (`$object->{$property} = $value`).
fn lower_dynamic_property_assign(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    property: &Expr,
    value: &Expr,
    span: Span,
) {
    let object = lower_expr(ctx, object);
    let property = lower_expr(ctx, property);
    let value = lower_expr(ctx, value);
    ctx.emit_void(
        Op::DynamicPropSet,
        vec![object.value, property.value, value.value],
        None,
        Op::DynamicPropSet.default_effects(),
        Some(span),
    );
}

/// Lowers pre/post increment and decrement expressions.
///
/// PHP integer overflow promotion applies: `PHP_INT_MAX + 1` becomes float.
/// The result is typed Mixed and emitted through a checked helper that
/// returns a boxed Mixed value (int or float) at runtime.
fn lower_inc_dec(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    increment: bool,
    post: bool,
    expr: &Expr,
) -> LoweredValue {
    let old = ctx.load_local(name, Some(expr.span));
    let existing_type = ctx.local_type(name);
    if matches!(existing_type.codegen_repr(), PhpType::Mixed) {
        let return_old = if post {
            crate::ir_lower::ownership::acquire_if_refcounted(ctx, old, Some(expr.span))
        } else {
            old
        };
        let one = lower_int_literal(ctx, 1, expr);
        let op = if increment {
            MixedNumericOp::Add
        } else {
            MixedNumericOp::Sub
        };
        let new = lower_mixed_numeric_binary(ctx, old, one, op, expr);
        ctx.store_local(name, new, PhpType::Mixed, Some(expr.span));
        return if post {
            return_old
        } else {
            ctx.load_local(name, Some(expr.span))
        };
    }
    let one = lower_int_literal(ctx, 1, expr);
    let operand = coerce_to_int(ctx, old, expr);
    let checked_int_local = matches!(existing_type.codegen_repr(), PhpType::Int);
    let iop = match (increment, checked_int_local) {
        (true, true) => Op::ICheckedAdd,
        (false, true) => Op::ICheckedSub,
        (true, false) => Op::IAdd,
        (false, false) => Op::ISub,
    };
    let result_php_type = if checked_int_local { PhpType::Mixed } else { PhpType::Int };
    let result_ir_type = if checked_int_local {
        IrType::Heap(IrHeapKind::Mixed)
    } else {
        IrType::I64
    };
    let new = ctx
        .builder
        .emit_with_effects(
            iop,
            vec![operand.value, one.value],
            None,
            result_ir_type,
            result_php_type.clone(),
            Ownership::for_php_type(&result_php_type),
            iop.default_effects(),
            Some(expr.span),
        )
        .expect("integer inc/dec produces a value");
    let new = LoweredValue { value: new, ir_type: result_ir_type };
    ctx.store_local(name, new, result_php_type, Some(expr.span));
    if post {
        old
    } else {
        ctx.load_local(name, Some(expr.span))
    }
}

/// Lowers a direct function, builtin, or extern call.
fn lower_function_call(ctx: &mut LoweringContext<'_, '_>, name: &Name, args: &[Expr], expr: &Expr) -> LoweredValue {
    constants::register_static_define_call(ctx, name, args);
    if let Some(value) = constants::lower_static_defined_call(ctx, name, args, expr) {
        return value;
    }
    let canonical = name.as_str();
    if let Some(value) = lower_lazy_isset(ctx, canonical, args, expr) {
        return value;
    }
    if let Some(value) = lower_lazy_empty(ctx, canonical, args, expr) {
        return value;
    }
    if let Some(value) = lower_desugared_dynamic_method_call(ctx, canonical, args, expr) {
        return value;
    }
    if let Some(value) = lower_static_call_user_func(ctx, canonical, args, expr) {
        return value;
    }
    if let Some(value) = lower_dynamic_call_user_func(ctx, canonical, args, expr) {
        return value;
    }
    if let Some(value) = lower_dynamic_call_user_func_array(ctx, canonical, args, expr) {
        return value;
    }
    if let Some(value) = lower_static_array_map(ctx, canonical, args, expr) {
        return value;
    }
    if let Some(value) = lower_static_array_reduce(ctx, canonical, args, expr) {
        return value;
    }
    if let Some(value) = lower_static_array_walk(ctx, canonical, args, expr) {
        return value;
    }
    if php_symbol_key(canonical.trim_start_matches('\\')) == "unset" {
        if let Some(value) = lower_unset_locals(ctx, args, expr) {
            return value;
        }
    }
    if let Some(value) = lower_static_settype(ctx, canonical, args, expr) {
        return value;
    }
    if let Some(value) = lower_static_array_push(ctx, canonical, args, expr) {
        return value;
    }
    if let Some(value) = lower_static_is_callable(ctx, canonical, args, expr) {
        return value;
    }
    if let Some(value) = lower_eval_function_probe(ctx, canonical, args, expr) {
        return value;
    }
    if let Some(value) = lower_eval_class_probe(ctx, canonical, args, expr) {
        return value;
    }
    let sig = call_signature(ctx, canonical);
    let is_extern = ctx.extern_functions.contains_key(canonical);
    let is_user_function = ctx.functions.contains_key(canonical);
    let operands = if is_extern || is_user_function {
        lower_args_with_signature(ctx, sig.as_ref(), args)
    } else {
        lower_builtin_call_args(ctx, canonical, sig.as_ref(), args)
    };
    let php_type = if is_extern || is_user_function {
        call_return_type(ctx, canonical, &operands)
    } else if let Some(php_type) =
        registry_builtin_result_type(ctx, canonical, args, &operands, expr.span)
    {
        php_type
    } else {
        call_return_type(ctx, canonical, &operands)
    };
    if is_extern {
        let data = ctx.intern_function_name(canonical);
        let call = ctx.emit_value(
            Op::ExternCall,
            operands.clone(),
            Some(Immediate::Data(data)),
            php_type,
            Op::ExternCall.default_effects(),
            Some(expr.span),
        );
        // Plain extern calls release owned argument temporaries the same way method
        // and builtin calls do, so a fresh owned temporary passed as an argument is
        // not leaked once per call. The alias guard keeps a pass-through result alive.
        release_owned_call_arg_temporaries(
            ctx,
            &operands,
            Some(call.value),
            &ReturnArgAlias::Unknown,
            expr.span,
        );
        return call;
    }
    if is_user_function {
        let data = ctx.intern_function_name(canonical);
        let call = ctx.emit_value(
            Op::Call,
            operands.clone(),
            Some(Immediate::Data(data)),
            php_type,
            effects_lookup::user_call_effects(canonical),
            Some(expr.span),
        );
        // Plain user calls release owned argument temporaries the same way method and
        // builtin calls do. The alias guard keeps a passthrough result (e.g. a function
        // that returns its own array argument typed `iterable`) from being freed.
        let return_alias = ctx
            .return_alias_summaries
            .function(canonical)
            .cloned()
            .unwrap_or(ReturnArgAlias::Unknown);
        release_owned_call_arg_temporaries(
            ctx,
            &operands,
            Some(call.value),
            &return_alias,
            expr.span,
        );
        return call;
    }
    if ctx.has_eval_barrier()
        && plain_positional_call_args(args)
        && canonical_builtin_function_name(canonical).is_none()
    {
        let dynamic_name = php_symbol_key(canonical.trim_start_matches('\\'));
        let data = ctx.intern_function_name(&dynamic_name);
        return ctx.emit_value(
            Op::EvalFunctionCall,
            operands,
            Some(Immediate::Data(data)),
            PhpType::Mixed,
            Op::EvalFunctionCall.default_effects(),
            Some(expr.span),
        );
    }
    let eval_literal = eval_literal_fragment(canonical, args);
    emit_builtin_call_value(ctx, canonical, operands, php_type, expr.span, eval_literal)
}

/// Emits a builtin call and releases owned temporary arguments after the call consumes them.
fn emit_builtin_call_value(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    operands: Vec<crate::ir::ValueId>,
    php_type: PhpType,
    span: Span,
    eval_literal: Option<&str>,
) -> LoweredValue {
    if eval_literal.is_none() {
        if let Some(def) = crate::builtins::registry::lookup(name) {
            let lowered = crate::builtins::semantics::lower_registry_call(
                ctx,
                def,
                &operands,
                &php_type,
                span,
            )
            .unwrap_or_else(|error| {
                panic!(
                    "checked builtin {} failed backend-neutral EIR lowering at {}:{}: {}",
                    def.name,
                    span.line,
                    span.col,
                    error,
                )
            });
            let call = LoweredValue {
                value: lowered.value,
                ir_type: ctx.builder.value_type(lowered.value),
            };
            let return_alias = match def.spec.semantics.result_ownership {
                crate::builtins::semantics::BuiltinResultOwnership::NonHeap
                | crate::builtins::semantics::BuiltinResultOwnership::Fresh
                | crate::builtins::semantics::BuiltinResultOwnership::Independent => {
                    ReturnArgAlias::None
                }
                crate::builtins::semantics::BuiltinResultOwnership::Aliases(indexes) => {
                    ReturnArgAlias::Parameters(indexes.iter().copied().collect())
                }
                crate::builtins::semantics::BuiltinResultOwnership::Borrowed
                | crate::builtins::semantics::BuiltinResultOwnership::MayAliasArguments => {
                    ReturnArgAlias::Unknown
                }
            };
            release_owned_call_arg_temporaries(
                ctx,
                &operands,
                Some(call.value),
                &return_alias,
                span,
            );
            return call;
        }
    }
    let (op, immediate, effects) = if let Some(fragment) = eval_literal {
        (
            Op::EvalLiteralCall,
            Some(Immediate::Data(ctx.intern_string(fragment))),
            Op::EvalLiteralCall.default_effects(),
        )
    } else {
        (
            Op::LanguageConstructCall,
            Some(Immediate::Data(ctx.intern_function_name(name))),
            effects_lookup::language_construct_effects(name),
        )
    };
    let call = ctx.emit_value(
        op,
        operands.clone(),
        immediate,
        php_type,
        effects,
        Some(span),
    );
    release_owned_call_arg_temporaries(
        ctx,
        &operands,
        Some(call.value),
        &ReturnArgAlias::Unknown,
        span,
    );
    let eval_needs_barrier = match eval_literal {
        Some(fragment) => eval_literal_needs_barrier(ctx, fragment),
        None => true,
    };
    if php_symbol_key(name.trim_start_matches('\\')) == "eval" {
        ctx.mark_eval_executed();
        if eval_needs_barrier {
            ctx.apply_eval_barrier();
        } else if let Some(write_names) = eval_literal
            .and_then(|fragment| eval_literal_scope_barrier_writes(ctx, fragment))
        {
            ctx.apply_eval_scope_barrier(&write_names);
        }
    }
    call
}

/// Resolves a migrated registry builtin's result type from the same descriptor as the checker.
fn registry_builtin_result_type(
    ctx: &LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    operands: &[crate::ir::ValueId],
    span: Span,
) -> Option<PhpType> {
    let def = crate::builtins::registry::lookup(name)?;
    let arg_types = operands
        .iter()
        .map(|operand| ctx.builder.value_php_type(*operand))
        .collect::<Vec<_>>();
    let input = crate::builtins::semantics::BuiltinSemanticInput {
        name: def.name,
        args,
        arg_types: &arg_types,
        span,
    };
    let resolved = match def.spec.semantics.result_type {
        crate::builtins::semantics::BuiltinResultType::Checked => {
            // Synthetic builtin-class and prelude AST nodes share the dummy 0:0
            // span, so the checker map cannot identify an individual call there.
            // Use the typed runtime target's representation-safe fallback instead
            // of accepting whichever synthetic call last occupied that key.
            if span.line != 0 {
                if let Some(checked) = ctx.builtin_call_types.get(&span) {
                    return Some(normalize_value_php_type(checked.clone()));
                }
            }
            let crate::builtins::semantics::BuiltinLowering::Runtime(
                crate::ir::RuntimeCallTarget::Function(target),
            ) = def.spec.semantics.lowering
            else {
                return None;
            };
            target.fallback_result_type(&arg_types, &def.return_type)
        }
        crate::builtins::semantics::BuiltinResultType::Declared => def.return_type.clone(),
        crate::builtins::semantics::BuiltinResultType::Shared(resolve) => resolve(&input),
    };
    Some(normalize_value_php_type(resolved))
}

/// Returns true when a literal `eval` call may still need runtime scope/interpreter state.
fn eval_literal_needs_barrier(ctx: &LoweringContext<'_, '_>, fragment: &str) -> bool {
    let static_call_supported = |name: &str, args: &[Expr]| {
        eval_literal_static_function_supported_by_lowering(ctx, name, args)
    };
    let plan = crate::eval_aot::plan_literal_fragment_with_source_path_and_static_and_method_calls(
        fragment,
        ctx.source_path(),
        static_call_supported,
        |receiver, method, args| {
            eval_literal_static_method_supported_by_lowering(ctx, receiver, method, args)
        },
    );
    if plan.is_fully_static_no_bridge() {
        return false;
    }
    if plan.uses_scope_read_params()
        && eval_literal_scope_read_params_supported_by_lowering(
            ctx,
            plan.reads(),
            plan.array_read_constraints(),
            plan.assoc_array_read_constraints(),
            plan.float_predicate_read_constraints(),
        )
    {
        return false;
    }
    if plan.requires_runtime_eval_scope()
        && eval_literal_scope_constraints_supported_by_lowering(
            ctx,
            plan.array_read_constraints(),
            plan.assoc_array_read_constraints(),
            plan.float_predicate_read_constraints(),
        )
    {
        return false;
    }
    true
}

/// Returns the caller locals written by an EIR literal `eval` that only needs scope state.
fn eval_literal_scope_barrier_writes(
    ctx: &LoweringContext<'_, '_>,
    fragment: &str,
) -> Option<std::collections::BTreeSet<String>> {
    let static_call_supported = |name: &str, args: &[Expr]| {
        eval_literal_static_function_supported_by_lowering(ctx, name, args)
    };
    let plan = crate::eval_aot::plan_literal_fragment_with_source_path_and_static_and_method_calls(
        fragment,
        ctx.source_path(),
        static_call_supported,
        |receiver, method, args| {
            eval_literal_static_method_supported_by_lowering(ctx, receiver, method, args)
        },
    );
    (plan.requires_runtime_eval_scope()
        && eval_literal_scope_constraints_supported_by_lowering(
            ctx,
            plan.array_read_constraints(),
            plan.assoc_array_read_constraints(),
            plan.float_predicate_read_constraints(),
        ))
    .then(|| plan.writes().clone())
}

/// Returns true when all scope-read variables can be passed as direct Mixed params.
fn eval_literal_scope_read_params_supported_by_lowering(
    ctx: &LoweringContext<'_, '_>,
    read_names: &std::collections::BTreeSet<String>,
    array_read_constraints: &std::collections::BTreeSet<String>,
    assoc_array_read_constraints: &std::collections::BTreeSet<String>,
    float_predicate_read_constraints: &std::collections::BTreeSet<String>,
) -> bool {
    read_names
        .iter()
        .all(|name| eval_literal_scope_read_param_supported_by_lowering(ctx, name))
        && array_read_constraints
            .iter()
            .all(|name| eval_literal_scope_read_array_param_supported_by_lowering(ctx, name))
        && assoc_array_read_constraints
            .iter()
            .all(|name| eval_literal_scope_read_assoc_array_param_supported_by_lowering(ctx, name))
        && float_predicate_read_constraints.iter().all(|name| {
            eval_literal_scope_read_float_predicate_param_supported_by_lowering(ctx, name)
        })
}

/// Returns true when all constrained scope reads fit caller local types.
fn eval_literal_scope_constraints_supported_by_lowering(
    ctx: &LoweringContext<'_, '_>,
    array_read_constraints: &std::collections::BTreeSet<String>,
    assoc_array_read_constraints: &std::collections::BTreeSet<String>,
    float_predicate_read_constraints: &std::collections::BTreeSet<String>,
) -> bool {
    array_read_constraints
        .iter()
        .all(|name| eval_literal_scope_read_array_param_supported_by_lowering(ctx, name))
        && assoc_array_read_constraints
            .iter()
            .all(|name| eval_literal_scope_read_assoc_array_param_supported_by_lowering(ctx, name))
        && float_predicate_read_constraints.iter().all(|name| {
            eval_literal_scope_read_float_predicate_param_supported_by_lowering(ctx, name)
        })
}

/// Returns true when one read variable has no eval runtime state dependency.
fn eval_literal_scope_read_param_supported_by_lowering(
    ctx: &LoweringContext<'_, '_>,
    name: &str,
) -> bool {
    if crate::superglobals::is_superglobal(name)
        || (ctx.in_main && ctx.all_global_var_names.contains(name))
    {
        return false;
    }
    let Some(slot) = ctx.local_slots.get(name) else {
        return true;
    };
    if ctx.is_ref_bound_local(name) {
        return false;
    }
    if ctx.local_kinds.get(name).copied() != Some(LocalKind::PhpLocal) {
        return false;
    }
    let Some(ty) = ctx.local_types.get(name) else {
        return false;
    };
    eval_literal_scope_read_param_type_supported(ty)
        && ctx.initialized_slots_snapshot().contains(slot)
}

/// Returns true when one direct read-param is statically known to be array-like.
fn eval_literal_scope_read_array_param_supported_by_lowering(
    ctx: &LoweringContext<'_, '_>,
    name: &str,
) -> bool {
    if crate::superglobals::is_superglobal(name)
        || (ctx.in_main && ctx.all_global_var_names.contains(name))
    {
        return false;
    }
    let Some(slot) = ctx.local_slots.get(name) else {
        return false;
    };
    if ctx.is_ref_bound_local(name) {
        return false;
    }
    if ctx.local_kinds.get(name).copied() != Some(LocalKind::PhpLocal) {
        return false;
    }
    let Some(ty) = ctx.local_types.get(name) else {
        return false;
    };
    eval_literal_scope_read_array_param_type_supported(ty)
        && ctx.initialized_slots_snapshot().contains(slot)
}

/// Returns true when one direct read-param is statically known to be associative-array-like.
fn eval_literal_scope_read_assoc_array_param_supported_by_lowering(
    ctx: &LoweringContext<'_, '_>,
    name: &str,
) -> bool {
    if crate::superglobals::is_superglobal(name)
        || (ctx.in_main && ctx.all_global_var_names.contains(name))
    {
        return false;
    }
    let Some(slot) = ctx.local_slots.get(name) else {
        return false;
    };
    if ctx.is_ref_bound_local(name) {
        return false;
    }
    if ctx.local_kinds.get(name).copied() != Some(LocalKind::PhpLocal) {
        return false;
    }
    let Some(ty) = ctx.local_types.get(name) else {
        return false;
    };
    eval_literal_scope_read_assoc_array_param_type_supported(ty)
        && ctx.initialized_slots_snapshot().contains(slot)
}

/// Returns true when one direct read-param can feed float predicate builtins safely.
fn eval_literal_scope_read_float_predicate_param_supported_by_lowering(
    ctx: &LoweringContext<'_, '_>,
    name: &str,
) -> bool {
    if crate::superglobals::is_superglobal(name)
        || (ctx.in_main && ctx.all_global_var_names.contains(name))
    {
        return false;
    }
    let Some(slot) = ctx.local_slots.get(name) else {
        return false;
    };
    if ctx.is_ref_bound_local(name) {
        return false;
    }
    if ctx.local_kinds.get(name).copied() != Some(LocalKind::PhpLocal) {
        return false;
    }
    let Some(ty) = ctx.local_types.get(name) else {
        return false;
    };
    eval_literal_scope_read_float_predicate_param_type_supported(ty)
        && ctx.initialized_slots_snapshot().contains(slot)
}

/// Returns true when a local type can be boxed to the param-mode Mixed ABI.
fn eval_literal_scope_read_param_type_supported(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Int
            | PhpType::Bool
            | PhpType::Float
            | PhpType::Str
            | PhpType::Void
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Object(_)
            | PhpType::Mixed
            | PhpType::Union(_)
    )
}

/// Returns true when a local type satisfies array-only direct read-param semantics.
fn eval_literal_scope_read_array_param_type_supported(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Array(_) | PhpType::AssocArray { .. }
    )
}

/// Returns true when a local type satisfies associative-array direct read-param semantics.
fn eval_literal_scope_read_assoc_array_param_type_supported(ty: &PhpType) -> bool {
    matches!(ty.codegen_repr(), PhpType::AssocArray { .. })
}

/// Returns true when a local type can reach IEEE float predicates without TypeError.
fn eval_literal_scope_read_float_predicate_param_type_supported(ty: &PhpType) -> bool {
    matches!(ty.codegen_repr(), PhpType::Int | PhpType::Float)
}

/// Returns the literal eval fragment when the call is a simple `eval('...')`.
fn eval_literal_fragment<'a>(name: &str, args: &'a [Expr]) -> Option<&'a str> {
    if php_symbol_key(name.trim_start_matches('\\')) != "eval"
        || args.len() != 1
        || crate::types::call_args::has_named_args(args)
        || args.iter().any(is_spread_arg)
    {
        return None;
    }
    match &args[0].kind {
        ExprKind::StringLiteral(fragment) => Some(fragment.as_str()),
        _ => None,
    }
}

/// Returns true when a literal-eval static function call can avoid the eval barrier.
fn eval_literal_static_function_supported_by_lowering(
    ctx: &LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
) -> bool {
    if args.len() > 6 {
        return false;
    }
    let key = php_symbol_key(name.trim_start_matches('\\'));
    let Some(signature) = ctx
        .functions
        .iter()
        .find(|(function_name, _)| php_symbol_key(function_name.trim_start_matches('\\')) == key)
        .map(|(_, signature)| signature)
    else {
        return false;
    };
    crate::eval_aot::static_function_signature_supported(signature, args)
}

/// Returns true when a literal-eval static method call can avoid the eval barrier.
fn eval_literal_static_method_supported_by_lowering(
    ctx: &LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    method: &str,
    args: &[Expr],
) -> bool {
    if args.len() > 6 || !matches!(receiver, StaticReceiver::Named(_)) {
        return false;
    }
    let Some(class_name) = static_receiver_class_name(ctx, receiver) else {
        return false;
    };
    let method_key = php_symbol_key(method);
    let Some(class_info) = ctx.classes.get(class_name.as_str()) else {
        return false;
    };
    if class_info
        .static_method_visibilities
        .get(&method_key)
        .unwrap_or(&Visibility::Public)
        != &Visibility::Public
    {
        return false;
    }
    let Some(signature) = static_method_implementation_signature(ctx, receiver, method) else {
        return false;
    };
    crate::eval_aot::static_function_signature_supported(signature, args)
}

/// Returns true when a dynamic eval fallback can preserve simple positional call semantics.
fn plain_positional_call_args(args: &[Expr]) -> bool {
    !crate::types::call_args::has_named_args(args)
        && !args.iter().any(is_spread_arg)
}

/// Lowers post-eval function-name probes through the eval context's dynamic table.
fn lower_eval_function_probe(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let probe_name = php_symbol_key(name.trim_start_matches('\\'));
    if probe_name != "function_exists" && probe_name != "is_callable" {
        return None;
    }
    if !ctx.has_eval_barrier()
        || args.len() != 1
        || crate::types::call_args::has_named_args(args)
        || args.iter().any(is_spread_arg)
    {
        return None;
    }
    let ExprKind::StringLiteral(function_name) = &args[0].kind else {
        return None;
    };
    if function_name.contains("::")
        || resolve_static_string_callable(ctx, function_name).is_some()
    {
        return None;
    }
    let dynamic_name = php_symbol_key(function_name.trim_start_matches('\\'));
    let data = ctx.intern_function_name(&dynamic_name);
    Some(ctx.emit_value(
        Op::EvalFunctionExists,
        Vec::new(),
        Some(Immediate::Data(data)),
        PhpType::Bool,
        Op::EvalFunctionExists.default_effects(),
        Some(expr.span),
    ))
}

/// Lowers post-eval class-name probes through the eval context's dynamic class table.
fn lower_eval_class_probe(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let probe_name = php_symbol_key(name.trim_start_matches('\\'));
    if probe_name != "class_exists" {
        return None;
    }
    if !ctx.has_eval_barrier()
        || args.is_empty()
        || args.len() > 2
        || crate::types::call_args::has_named_args(args)
        || args.iter().any(is_spread_arg)
    {
        return None;
    }
    let ExprKind::StringLiteral(class_name) = &args[0].kind else {
        return None;
    };
    if aot_class_exists_for_eval_probe(ctx, class_name) {
        return None;
    }
    if let Some(autoload) = args.get(1) {
        lower_expr(ctx, autoload);
    }
    let data = ctx.intern_class_name(class_name);
    Some(ctx.emit_value(
        Op::EvalClassExists,
        Vec::new(),
        Some(Immediate::Data(data)),
        PhpType::Bool,
        Op::EvalClassExists.default_effects(),
        Some(expr.span),
    ))
}

/// Returns true when an AOT class already satisfies a native class_exists probe.
fn aot_class_exists_for_eval_probe(ctx: &LoweringContext<'_, '_>, class_name: &str) -> bool {
    let key = php_symbol_key(class_name.trim_start_matches('\\'));
    ctx.classes
        .keys()
        .any(|candidate| php_symbol_key(candidate.trim_start_matches('\\')) == key)
}

/// Lowers `isset()` as a lazy language construct instead of an eager builtin call.
fn lower_lazy_isset(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if php_symbol_key(name.trim_start_matches('\\')) != "isset" {
        return None;
    }
    if crate::types::call_args::has_named_args(args) || args.iter().any(is_spread_arg) {
        return None;
    }
    if args.is_empty() {
        return Some(lower_bool_literal(ctx, false, expr));
    }

    let temp_name = ctx.declare_hidden_temp(PhpType::Bool);
    let false_block = ctx.builder.create_named_block("isset.lazy_false", Vec::new());
    let merge = ctx.builder.create_named_block("isset.lazy_merge", Vec::new());
    for (idx, arg) in args.iter().enumerate() {
        let checked = lower_lazy_isset_operand(ctx, arg).unwrap_or_else(|| {
            // `isset()` never emits undefined-offset warnings, so eager array
            // operands must be lowered with the silent read variants.
            let value = if let ExprKind::ArrayAccess { array, index } = &arg.kind {
                lower_array_access_with_missing_warning(ctx, array, index, arg, false)
            } else {
                lower_expr(ctx, arg)
            };
            emit_builtin_call_value(ctx, name, vec![value.value], PhpType::Int, arg.span, None)
        });
        let then_target = if idx + 1 == args.len() {
            ctx.builder.create_named_block("isset.lazy_true", Vec::new())
        } else {
            ctx.builder.create_named_block("isset.lazy_next", Vec::new())
        };
        ctx.builder.terminate(Terminator::CondBr {
            cond: checked.value,
            then_target,
            then_args: Vec::new(),
            else_target: false_block,
            else_args: Vec::new(),
        });
        ctx.builder.position_at_end(then_target);
    }

    let true_value = lower_bool_literal(ctx, true, expr);
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, true_value, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(false_block);
    let false_value = lower_bool_literal(ctx, false, expr);
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, false_value, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    Some(take_owned_temp(ctx, &temp_name, expr.span))
}

/// Lowers a single `isset()` operand that has special lazy PHP semantics.
fn lower_lazy_isset_operand(
    ctx: &mut LoweringContext<'_, '_>,
    arg: &Expr,
) -> Option<LoweredValue> {
    match &arg.kind {
        ExprKind::ArrayAccess { array, index } => {
            if array_access_expr_satisfies_array_access(ctx, array) {
                let synthetic = Expr::new(
                    ExprKind::MethodCall {
                        object: array.clone(),
                        method: "offsetExists".to_string(),
                        args: vec![(**index).clone()],
                    },
                    arg.span,
                );
                return Some(lower_expr(ctx, &synthetic));
            }
            if !array_access_expr_supports_native_isset_probe(ctx, array) {
                return None;
            }
            Some(lower_native_isset_offset_probe(ctx, array, index, arg))
        }
        ExprKind::PropertyAccess { object, property }
        | ExprKind::NullsafePropertyAccess { object, property } => {
            lower_lazy_property_isset_operand(ctx, object, property, arg)
        }
        // `isset($this)` inside a static closure always evaluates to `false`
        // because static closures have no `$this` binding. PHP allows this
        // probe and returns false; elephc must not try to load a missing slot.
        ExprKind::This if !ctx.local_slots.contains_key("this") => {
            Some(lower_bool_literal(ctx, false, arg))
        }
        _ => None,
    }
}

/// Lowers `empty($obj->magicProp)` with PHP's overloaded-property semantics:
/// `empty` consults `__isset` first and only evaluates `__get` when `__isset`
/// is truthy, so an unset virtual property is empty without ever reading it.
/// Returns `None` for operands the eager `empty` builtin already handles (plain
/// variables, declared properties, array elements), letting that path run.
fn lower_lazy_empty(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if php_symbol_key(name.trim_start_matches('\\')) != "empty" {
        return None;
    }
    if args.len() != 1
        || crate::types::call_args::has_named_args(args)
        || args.iter().any(is_spread_arg)
    {
        return None;
    }
    if let ExprKind::ArrayAccess { array, index } = &args[0].kind {
        let value = lower_array_access_with_missing_warning(ctx, array, index, &args[0], false);
        return Some(emit_builtin_call_value(
            ctx,
            name,
            vec![value.value],
            PhpType::Bool,
            expr.span,
            None,
        ));
    }
    let (exists_call, get_call) = lazy_empty_magic_property_calls(ctx, &args[0])?;

    let temp_name = ctx.declare_hidden_temp(PhpType::Bool);
    let present_block = ctx.builder.create_named_block("empty.present", Vec::new());
    let absent_block = ctx.builder.create_named_block("empty.absent", Vec::new());
    let merge = ctx.builder.create_named_block("empty.merge", Vec::new());

    // `__isset(prop)` decides whether the property is considered set at all.
    let exists = lower_expr(ctx, &exists_call);
    ctx.builder.terminate(Terminator::CondBr {
        cond: exists.value,
        then_target: present_block,
        then_args: Vec::new(),
        else_target: absent_block,
        else_args: Vec::new(),
    });

    // Set: empty is the emptiness of the `__get` value (reuses the eager builtin).
    ctx.builder.position_at_end(present_block);
    let get_value = lower_expr(ctx, &get_call);
    let empty_name = ctx.intern_function_name(name);
    let empty_value = ctx.emit_value(
        Op::LanguageConstructCall,
        vec![get_value.value],
        Some(Immediate::Data(empty_name)),
        PhpType::Bool,
        effects_lookup::language_construct_effects(name),
        Some(expr.span),
    );
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, empty_value, expr.span);
    branch_to(ctx, merge);

    // Not set: empty is true and `__get` is never called.
    ctx.builder.position_at_end(absent_block);
    let true_value = lower_bool_literal(ctx, true, expr);
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, true_value, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    Some(ctx.load_local(&temp_name, Some(expr.span)))
}

/// For an `empty()` operand that is an overloaded (magic) property access,
/// returns the `(__isset, __get)` synthetic call expressions PHP would evaluate.
/// The property name is a string literal, so reusing it for both calls is
/// side-effect free. Returns `None` for any other operand shape.
fn lazy_empty_magic_property_calls(
    ctx: &LoweringContext<'_, '_>,
    arg: &Expr,
) -> Option<(Expr, Expr)> {
    match &arg.kind {
        ExprKind::PropertyAccess { object, property } => {
            property_existence_magic_class(ctx, object, property, "__isset")?;
            let key = Expr::new(ExprKind::StringLiteral(property.clone()), arg.span);
            let exists = Expr::new(
                ExprKind::MethodCall {
                    object: object.clone(),
                    method: "__isset".to_string(),
                    args: vec![key.clone()],
                },
                arg.span,
            );
            let get = Expr::new(
                ExprKind::MethodCall {
                    object: object.clone(),
                    method: "__get".to_string(),
                    args: vec![key],
                },
                arg.span,
            );
            Some((exists, get))
        }
        ExprKind::NullsafePropertyAccess { object, property } => {
            property_existence_magic_class(ctx, object, property, "__isset")?;
            let key = Expr::new(ExprKind::StringLiteral(property.clone()), arg.span);
            let exists = Expr::new(
                ExprKind::NullsafeMethodCall {
                    object: object.clone(),
                    method: "__isset".to_string(),
                    args: vec![key.clone()],
                },
                arg.span,
            );
            let get = Expr::new(
                ExprKind::NullsafeMethodCall {
                    object: object.clone(),
                    method: "__get".to_string(),
                    args: vec![key],
                },
                arg.span,
            );
            Some((exists, get))
        }
        _ => None,
    }
}

/// Returns the class whose `magic` method (`__isset`/`__unset`) should handle
/// property existence/removal: a property that cannot be accessed normally on an
/// object whose class declares the magic method.
fn property_existence_magic_class(
    ctx: &LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
    magic: &str,
) -> Option<String> {
    let class_name = instance_callable_object_class(ctx, object)?;
    let class_info = ctx.classes.get(&class_name)?;
    if property_is_accessible_for_ir(ctx, &class_name, class_info, property) {
        return None;
    }
    class_method_signature(ctx, &class_name, &php_symbol_key(magic)).map(|_| class_name)
}

/// Lowers native array/hash `isset($array[$key])` without reading the element value.
fn lower_native_isset_offset_probe(
    ctx: &mut LoweringContext<'_, '_>,
    array: &Expr,
    index: &Expr,
    expr: &Expr,
) -> LoweredValue {
    let array_value = lower_subscript_receiver_silently(ctx, array);
    if value_is_nullable(ctx, array_value.value) {
        return lower_nullable_native_isset_offset_probe(ctx, array_value, index, expr);
    }
    lower_native_isset_offset_probe_from_value(ctx, array_value, index, expr)
}

/// Lowers nullable native array/hash `isset` without evaluating the offset on null receivers.
fn lower_nullable_native_isset_offset_probe(
    ctx: &mut LoweringContext<'_, '_>,
    array_value: LoweredValue,
    index: &Expr,
    expr: &Expr,
) -> LoweredValue {
    let is_null = ctx.emit_value(
        Op::IsNull,
        vec![array_value.value],
        None,
        PhpType::Bool,
        Op::IsNull.default_effects(),
        Some(expr.span),
    );
    let temp_name = ctx.declare_hidden_temp(PhpType::Bool);
    let null_block = ctx
        .builder
        .create_named_block("isset.native.null", Vec::new());
    let probe_block = ctx
        .builder
        .create_named_block("isset.native.probe", Vec::new());
    let merge = ctx
        .builder
        .create_named_block("isset.native.merge", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: is_null.value,
        then_target: null_block,
        then_args: Vec::new(),
        else_target: probe_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(null_block);
    let false_value = emit_bool_literal(ctx, false, Some(expr.span));
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, false_value, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(probe_block);
    let checked = lower_native_isset_offset_probe_from_value(ctx, array_value, index, expr);
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, checked, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    take_owned_temp(ctx, &temp_name, expr.span)
}

/// Lowers native array/hash `isset` once the receiver has already been evaluated.
fn lower_native_isset_offset_probe_from_value(
    ctx: &mut LoweringContext<'_, '_>,
    array_value: LoweredValue,
    index: &Expr,
    expr: &Expr,
) -> LoweredValue {
    match array_value.ir_type {
        IrType::Heap(IrHeapKind::Array) => {
            let mut index_value = lower_expr(ctx, index);
            let index_ty = index_expr_key_type(ctx, index);
            if index_ty == PhpType::Int {
                index_value = coerce_to_int_at_span(ctx, index_value, Some(index.span));
                ctx.emit_value(
                    Op::ArrayIsset,
                    vec![array_value.value, index_value.value],
                    None,
                    PhpType::Bool,
                    Op::ArrayIsset.default_effects(),
                    Some(expr.span),
                )
            } else {
                // String or mixed key on indexed storage: read through the
                // mixed-key runtime path and check if the result is null.
                let read_value = ctx.emit_value(
                    Op::ArrayGetMixedKey,
                    vec![array_value.value, index_value.value],
                    None,
                    PhpType::Mixed,
                    Op::ArrayGetMixedKey.default_effects(),
                    Some(expr.span),
                );
                let is_null = ctx.emit_value(
                    Op::IsNull,
                    vec![read_value.value],
                    None,
                    PhpType::Bool,
                    Op::IsNull.default_effects(),
                    Some(expr.span),
                );
                let zero = ctx.emit_value(
                    Op::ConstI64,
                    Vec::new(),
                    Some(Immediate::I64(0)),
                    PhpType::Int,
                    Op::ConstI64.default_effects(),
                    Some(expr.span),
                );
                ctx.emit_value(
                    Op::ICmp,
                    vec![is_null.value, zero.value],
                    Some(Immediate::CmpPredicate(crate::ir::CmpPredicate::Eq)),
                    PhpType::Bool,
                    Op::ICmp.default_effects(),
                    Some(expr.span),
                )
            }
        }
        IrType::Heap(IrHeapKind::Hash) => {
            let index_value = lower_expr(ctx, index);
            ctx.emit_value(
                Op::HashIsset,
                vec![array_value.value, index_value.value],
                None,
                PhpType::Bool,
                Op::HashIsset.default_effects(),
                Some(expr.span),
            )
        }
        _ => {
            let read_value = lower_array_access_from_value(ctx, array_value, index, expr, false);
            emit_builtin_call_value(
                ctx,
                "isset",
                vec![read_value.value],
                PhpType::Int,
                expr.span,
                None,
            )
        }
    }
}

/// Returns whether a syntactic array receiver can use a non-materializing native `isset` probe.
fn array_access_expr_supports_native_isset_probe(
    ctx: &LoweringContext<'_, '_>,
    array: &Expr,
) -> bool {
    let ty = match &array.kind {
        ExprKind::Variable(name) => ctx
            .local_types
            .get(name)
            .cloned()
            .unwrap_or_else(|| infer_expr_type_syntactic(array)),
        ExprKind::PropertyAccess { object, property } => {
            property_access_expr_type_for_ir(ctx, object, property)
                .unwrap_or_else(|| infer_expr_type_syntactic(array))
        }
        ExprKind::ArrayLiteral(items) => array_literal_type_for_ir(ctx, items, array),
        ExprKind::ArrayLiteralAssoc(pairs) => assoc_array_literal_type_for_ir(ctx, pairs, array),
        _ => infer_expr_type_syntactic(array),
    }
    .codegen_repr();
    matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. })
}

/// Lowers `isset($object->property)` without performing a normal property read first.
fn lower_lazy_property_isset_operand(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
    arg: &Expr,
) -> Option<LoweredValue> {
    match property_isset_action(ctx, object, property)? {
        IssetPropertyAction::Fallback => None,
        IssetPropertyAction::Magic => {
            let object = lower_expr(ctx, object);
            Some(lower_magic_property_isset(ctx, object, property, arg))
        }
        IssetPropertyAction::AlwaysFalse => {
            lower_expr(ctx, object);
            Some(emit_bool_literal(ctx, false, Some(arg.span)))
        }
    }
}

/// Describes how `isset($object->property)` should be lowered for a known receiver class.
enum IssetPropertyAction {
    Fallback,
    Magic,
    AlwaysFalse,
}

/// Selects the PHP-visible `isset()` behavior for a statically known object property operand.
fn property_isset_action(
    ctx: &LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
) -> Option<IssetPropertyAction> {
    let (class_name, _) = isset_object_expr_class(ctx, object)?;
    if is_builtin_stdclass_name(&class_name) {
        return Some(IssetPropertyAction::Fallback);
    }
    let class_info = ctx.classes.get(class_name.as_str())?;
    if class_info.allow_dynamic_properties {
        return Some(IssetPropertyAction::Fallback);
    }
    if property_is_accessible_for_ir(ctx, &class_name, class_info, property) {
        return Some(IssetPropertyAction::Fallback);
    }
    if class_method_signature(ctx, &class_name, &php_symbol_key("__isset")).is_some() {
        Some(IssetPropertyAction::Magic)
    } else {
        Some(IssetPropertyAction::AlwaysFalse)
    }
}

/// Returns the single receiver class and whether that receiver may be null.
fn isset_object_expr_class(ctx: &LoweringContext<'_, '_>, object: &Expr) -> Option<(String, bool)> {
    let ty = match &object.kind {
        ExprKind::Variable(name) => ctx.local_type(name),
        ExprKind::This => PhpType::Object(ctx.current_class.clone()?),
        ExprKind::NewObject { class_name, .. } => PhpType::Object(class_name.to_string()),
        ExprKind::NewDynamicObject { fallback_class, .. } => {
            PhpType::Object(fallback_class.to_string())
        }
        ExprKind::FunctionCall { name, .. } => ctx
            .functions
            .get(name.as_str())
            .map(|sig| sig.return_type.clone())
            .unwrap_or_else(|| infer_expr_type_syntactic(object)),
        _ => infer_expr_type_syntactic(object),
    };
    let (class_name, nullable) = singular_object_class(&ty)?;
    normalized_class_name(class_name).map(|name| (name, nullable))
}

/// Returns whether a named property can use normal `isset()` value probing.
fn property_is_accessible_for_ir(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    class_info: &crate::types::ClassInfo,
    property: &str,
) -> bool {
    if class_info.visible_property(property).is_none() {
        return false;
    }
    class_info
        .property_visibilities
        .get(property)
        .is_none_or(|visibility| {
            let declaring_class = class_info
                .property_declaring_classes
                .get(property)
                .map(String::as_str)
                .unwrap_or(class_name);
            ir_can_access_member(ctx, declaring_class, visibility)
        })
}

/// Checks PHP member visibility from the current lowering class scope.
fn ir_can_access_member(
    ctx: &LoweringContext<'_, '_>,
    declaring_class: &str,
    visibility: &Visibility,
) -> bool {
    match visibility {
        Visibility::Public => true,
        Visibility::Private => ctx
            .current_class
            .as_deref()
            .is_some_and(|current| same_php_class_name(current, declaring_class)),
        Visibility::Protected => ctx.current_class.as_deref().is_some_and(|current| {
            same_php_class_name(current, declaring_class)
                || class_extends_class(ctx, current, declaring_class)
        }),
    }
}

/// Returns true when two class metadata names match PHP's case-insensitive class lookup.
fn same_php_class_name(left: &str, right: &str) -> bool {
    php_symbol_key(left.trim_start_matches('\\')) == php_symbol_key(right.trim_start_matches('\\'))
}

/// Lowers a magic `__isset($name)` call and coerces the result to PHP boolean semantics.
fn lower_magic_property_isset(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    property: &str,
    arg: &Expr,
) -> LoweredValue {
    if value_is_nullable(ctx, object.value) {
        return lower_nullable_magic_property_isset(ctx, object, property, arg);
    }
    let args = vec![Expr::new(
        ExprKind::StringLiteral(property.to_string()),
        arg.span,
    )];
    let result =
        lower_method_call_with_receiver(ctx, object, "__isset", &args, Op::MethodCall, arg);
    ctx.truthy(result, Some(arg.span))
}

/// Lowers `__isset` for nullable receivers, returning false instead of calling on null.
fn lower_nullable_magic_property_isset(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    property: &str,
    arg: &Expr,
) -> LoweredValue {
    let temp_name = ctx.declare_hidden_temp(PhpType::Bool);
    let null_block = ctx
        .builder
        .create_named_block("isset.property.null", Vec::new());
    let call_block = ctx
        .builder
        .create_named_block("isset.property.call", Vec::new());
    let merge = ctx
        .builder
        .create_named_block("isset.property.merge", Vec::new());
    let is_null = ctx.emit_value(
        Op::IsNull,
        vec![object.value],
        None,
        PhpType::Bool,
        Op::IsNull.default_effects(),
        Some(arg.span),
    );
    ctx.builder.terminate(Terminator::CondBr {
        cond: is_null.value,
        then_target: null_block,
        then_args: Vec::new(),
        else_target: call_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(null_block);
    let false_value = emit_bool_literal(ctx, false, Some(arg.span));
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, false_value, arg.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(call_block);
    let args = vec![Expr::new(
        ExprKind::StringLiteral(property.to_string()),
        arg.span,
    )];
    let result =
        lower_method_call_with_receiver(ctx, object, "__isset", &args, Op::MethodCall, arg);
    let result = ctx.truthy(result, Some(arg.span));
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, result, arg.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    ctx.load_local(&temp_name, Some(arg.span))
}

/// Lowers direct function/static-method first-class callable probes for `is_callable()`.
fn lower_static_is_callable(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if php_symbol_key(name.trim_start_matches('\\')) != "is_callable" || args.len() != 1 {
        return None;
    }
    if crate::types::call_args::has_named_args(args) || args.iter().any(is_spread_arg) {
        return None;
    }
    // Eval can declare callable targets after static metadata has been built.
    if ctx.has_eval_barrier() {
        return None;
    }
    match &args[0].kind {
        ExprKind::FirstClassCallable(
            CallableTarget::Function(_) | CallableTarget::StaticMethod { .. },
        ) => Some(emit_bool_literal(ctx, true, Some(expr.span))),
        ExprKind::ArrayLiteral(items) => {
            let is_callable = static_array_callable_is_callable(ctx, items)?;
            Some(emit_bool_literal(ctx, is_callable, Some(expr.span)))
        }
        ExprKind::Variable(name) => ctx.static_callable_local(name).map(|target| {
            emit_bool_literal(
                ctx,
                static_callable_binding_is_callable(ctx, &target),
                Some(expr.span),
            )
        }),
        _ => None,
    }
}

/// Returns whether straight-line callable-local metadata represents a public callable.
fn static_callable_binding_is_callable(
    ctx: &LoweringContext<'_, '_>,
    target: &StaticCallableBinding,
) -> bool {
    match target {
        StaticCallableBinding::StaticMethod { receiver, method }
        | StaticCallableBinding::StaticMethodDescriptor { receiver, method } => {
            static_receiver_class_name(ctx, receiver)
                .is_some_and(|class_name| static_method_callback_is_callable(ctx, &class_name, method))
        }
        StaticCallableBinding::UserFunction(_)
        | StaticCallableBinding::ExternFunction(_)
        | StaticCallableBinding::Builtin(_)
        | StaticCallableBinding::Closure { .. }
        | StaticCallableBinding::InstanceMethod { .. } => true,
    }
}

/// Lowers static-string `call_user_func*` forms to direct call opcodes when possible.
fn lower_static_call_user_func(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    match php_symbol_key(name.trim_start_matches('\\')).as_str() {
        "call_user_func" => {
            let callback_expr = args.first()?;
            let callback_args = &args[1..];
            let signature = callable_descriptor_signature_for_expr(ctx, callback_expr);
            if call_user_func_should_use_descriptor(ctx, callback_expr, callback_args, signature.as_ref()) {
                return lower_call_user_func_descriptor_invoke(
                    ctx,
                    callback_expr,
                    callback_args,
                    signature.as_ref(),
                    expr,
                );
            }
            if let Some(callback) = instance_call_user_func_callback(ctx, callback_expr) {
                return lower_instance_callable_call_user_func(
                    ctx,
                    callback_expr,
                    callback,
                    callback_args,
                    expr,
                );
            }
            if let Some(callback) = static_call_user_func_callback(ctx, callback_expr) {
                return lower_static_callable_call(ctx, callback, callback_args, expr);
            }
            lower_eval_call_user_func_fallback(ctx, callback_expr, callback_args, expr)
        }
        "call_user_func_array" => {
            let [callback_arg, arg_array] = args else {
                return None;
            };
            if matches!(arg_array.kind, ExprKind::ArrayLiteralAssoc(_))
                && static_callable_binding_for_expr(ctx, callback_arg)
                    .is_some_and(|target| matches!(target, StaticCallableBinding::InstanceMethod { .. }))
            {
                return None;
            }
            if let Some(callback_args) = static_call_user_func_array_args(arg_array) {
                if let Some(callback) = instance_call_user_func_callback(ctx, callback_arg) {
                    return lower_instance_callable_call_user_func(
                        ctx,
                        callback_arg,
                        callback,
                        &callback_args,
                        expr,
                    );
                }
                if let Some(callback) = static_call_user_func_callback(ctx, callback_arg) {
                    return lower_static_callable_call(ctx, callback, &callback_args, expr);
                }
            }
            lower_eval_call_user_func_array_fallback(ctx, callback_arg, arg_array, expr)
        }
        _ => None,
    }
}

/// Lowers unresolved string callbacks after an eval barrier through the eval function table.
fn lower_eval_call_user_func_fallback(
    ctx: &mut LoweringContext<'_, '_>,
    callback_expr: &Expr,
    callback_args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if !ctx.has_eval_barrier() || !plain_positional_call_args(callback_args) {
        return None;
    }
    let ExprKind::StringLiteral(callback_name) = &callback_expr.kind else {
        return None;
    };
    if callback_name.contains("::")
        || resolve_static_string_callable(ctx, callback_name).is_some()
    {
        return None;
    }
    let dynamic_name = php_symbol_key(callback_name.trim_start_matches('\\'));
    let data = ctx.intern_function_name(&dynamic_name);
    let operands = lower_args(ctx, callback_args);
    Some(ctx.emit_value(
        Op::EvalFunctionCall,
        operands,
        Some(Immediate::Data(data)),
        PhpType::Mixed,
        Op::EvalFunctionCall.default_effects(),
        Some(expr.span),
    ))
}

/// Lowers unresolved `call_user_func_array()` string callbacks through the eval table.
fn lower_eval_call_user_func_array_fallback(
    ctx: &mut LoweringContext<'_, '_>,
    callback_expr: &Expr,
    arg_array: &Expr,
    expr: &Expr,
) -> Option<LoweredValue> {
    if !ctx.has_eval_barrier() {
        return None;
    }
    let ExprKind::StringLiteral(callback_name) = &callback_expr.kind else {
        return None;
    };
    if callback_name.contains("::")
        || resolve_static_string_callable(ctx, callback_name).is_some()
    {
        return None;
    }
    let dynamic_name = php_symbol_key(callback_name.trim_start_matches('\\'));
    let data = ctx.intern_function_name(&dynamic_name);
    let arg_array = lower_expr(ctx, arg_array);
    let arg_array = coerce_eval_function_arg_array(ctx, arg_array, expr.span);
    Some(ctx.emit_value(
        Op::EvalFunctionCallArray,
        vec![arg_array.value],
        Some(Immediate::Data(data)),
        PhpType::Mixed,
        Op::EvalFunctionCallArray.default_effects(),
        Some(expr.span),
    ))
}

/// Boxes a post-barrier dynamic-call argument container for the eval bridge ABI.
fn coerce_eval_function_arg_array(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Span,
) -> LoweredValue {
    if matches!(
        ctx.builder.value_php_type(value.value).codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    ) {
        return value;
    }
    ctx.emit_value(
        Op::MixedBox,
        vec![value.value],
        None,
        PhpType::Mixed,
        Op::MixedBox.default_effects(),
        Some(span),
    )
}

/// Lowers `call_user_func*` for receiver-bound first-class callables through `expr_call`.
fn lower_instance_callable_call_user_func(
    ctx: &mut LoweringContext<'_, '_>,
    callback_expr: &Expr,
    callback: StaticCallableBinding,
    callback_args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let result_type = static_callable_return_type(ctx, &callback);
    let signature = instance_callable_signature(&callback).cloned();
    let mut operands = vec![lower_expr(ctx, callback_expr).value];
    operands.extend(lower_args_with_signature(ctx, signature.as_ref(), callback_args));
    Some(ctx.emit_value(
        Op::ExprCall,
        operands,
        None,
        result_type,
        Op::ExprCall.default_effects(),
        Some(expr.span),
    ))
}

/// Lowers dynamic `call_user_func()` callbacks through descriptor invocation.
fn lower_dynamic_call_user_func(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if php_symbol_key(name.trim_start_matches('\\')) != "call_user_func" || args.is_empty() {
        return None;
    }
    if matches!(args[0].kind, ExprKind::NamedArg { .. } | ExprKind::Spread(_)) {
        return None;
    }
    let signature = callable_descriptor_signature_for_expr(ctx, &args[0]);
    let callback = lower_expr(ctx, &args[0]);
    if descriptor_callback_php_type_supported(&ctx.builder.value_php_type(callback.value).codegen_repr()) {
        return lower_call_user_func_descriptor_invoke_from_value(
            ctx,
            callback,
            &args[1..],
            signature.as_ref(),
            expr,
        );
    }
    if crate::types::call_args::has_named_args(&args[1..]) || args[1..].iter().any(is_spread_arg) {
        return None;
    }
    let mut operands = Vec::with_capacity(args.len());
    operands.push(callback.value);
    operands.extend(lower_args(ctx, &args[1..]));
    Some(ctx.emit_value(
        Op::ExprCall,
        operands,
        None,
        PhpType::Mixed,
        Op::ExprCall.default_effects(),
        Some(expr.span),
    ))
}

/// Lowers dynamic `call_user_func_array()` through the descriptor-invoker EIR path.
fn lower_dynamic_call_user_func_array(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if php_symbol_key(name.trim_start_matches('\\')) != "call_user_func_array" {
        return None;
    }
    let [callback_expr, arg_array_expr] = args else {
        return None;
    };
    if crate::types::call_args::has_named_args(args) || args.iter().any(is_spread_arg) {
        return None;
    }
    let signature = callable_descriptor_signature_for_expr(ctx, callback_expr);
    let callback = lower_expr(ctx, callback_expr);
    let arg_array = lower_descriptor_invoker_arg_array_for_call_user_func_array(
        ctx,
        arg_array_expr,
        signature.as_ref(),
    )
    .unwrap_or_else(|| lower_expr(ctx, arg_array_expr));
    Some(emit_callable_descriptor_invoke(
        ctx,
        callback,
        arg_array,
        PhpType::Mixed,
        expr.span,
    ))
}

/// Returns the callable signature available to descriptor-invoker argument lowering.
fn callable_descriptor_signature_for_expr(
    ctx: &LoweringContext<'_, '_>,
    callback: &Expr,
) -> Option<FunctionSig> {
    match &callback.kind {
        ExprKind::Ternary { then_expr, else_expr, .. } => {
            let left = callable_descriptor_signature_for_expr(ctx, then_expr)?;
            let right = callable_descriptor_signature_for_expr(ctx, else_expr)?;
            compatible_descriptor_signature(left, &right)
        }
        ExprKind::ShortTernary { value, default } => {
            let left = callable_descriptor_signature_for_expr(ctx, value)?;
            let right = callable_descriptor_signature_for_expr(ctx, default)?;
            compatible_descriptor_signature(left, &right)
        }
        ExprKind::Variable(name) => ctx
            .callable_param_signature(name)
            .cloned()
            .or_else(|| ctx.static_callable_local(name).and_then(|target| {
                signature_for_static_callable_binding(ctx, target)
            })),
        _ => static_callable_binding_for_expr(ctx, callback)
            .and_then(|target| signature_for_static_callable_binding(ctx, target))
            .or_else(|| invokable_object_signature_for_expr(ctx, callback)),
    }
}

/// Returns the `__invoke` signature for an invokable object callback expression.
fn invokable_object_signature_for_expr(
    ctx: &LoweringContext<'_, '_>,
    callback: &Expr,
) -> Option<FunctionSig> {
    let class_name = instance_callable_object_class(ctx, callback)?;
    class_method_signature(ctx, &class_name, "__invoke").cloned()
}

/// Keeps a descriptor signature only when two runtime branches have the same callable ABI.
fn compatible_descriptor_signature(left: FunctionSig, right: &FunctionSig) -> Option<FunctionSig> {
    (left == *right).then_some(left)
}

/// Extracts a callable signature from a statically understood callable binding.
fn signature_for_static_callable_binding(
    ctx: &LoweringContext<'_, '_>,
    target: StaticCallableBinding,
) -> Option<FunctionSig> {
    match target {
        StaticCallableBinding::UserFunction(name) => ctx.functions.get(&name).cloned(),
        StaticCallableBinding::ExternFunction(name) => ctx
            .extern_functions
            .get(&name)
            .map(function_sig_from_extern_for_descriptor),
        StaticCallableBinding::Builtin(_) => None,
        StaticCallableBinding::Closure { signature, .. } => Some(signature),
        StaticCallableBinding::StaticMethod { receiver, method }
        | StaticCallableBinding::StaticMethodDescriptor { receiver, method } => {
            static_method_implementation_signature(ctx, &receiver, &method).cloned()
        }
        StaticCallableBinding::InstanceMethod { signature, .. } => Some(signature),
    }
}

/// Converts an extern signature into the PHP-facing descriptor invoker signature.
fn function_sig_from_extern_for_descriptor(sig: &ExternFunctionSig) -> FunctionSig {
    FunctionSig {
        params: sig.params.clone(),
        param_type_exprs: vec![None; sig.params.len()],
        param_attributes: Vec::new(),
        defaults: vec![None; sig.params.len()],
        return_type: sig.return_type.clone(),
        declared_return: true,
        by_ref_return: false,
        ref_params: vec![false; sig.params.len()],
        declared_params: vec![true; sig.params.len()],
        variadic: None,
        deprecation: None,
    }
}

/// Builds an invoker argument array that preserves by-reference literal variables.
fn lower_descriptor_invoker_arg_array_for_call_user_func_array(
    ctx: &mut LoweringContext<'_, '_>,
    arg_array: &Expr,
    sig: Option<&FunctionSig>,
) -> Option<LoweredValue> {
    let ExprKind::ArrayLiteral(items) = &arg_array.kind else {
        return None;
    };
    if items.iter().any(is_spread_arg) || !items.iter().enumerate().any(|(index, item)| {
        invoker_ref_arg_variable(ctx, sig, index, item).is_some()
    }) {
        return None;
    }

    let elem_ty = PhpType::Mixed;
    let array_ty = PhpType::Array(Box::new(elem_ty.clone()));
    let array = ctx.emit_value(
        Op::ArrayNew,
        Vec::new(),
        Some(Immediate::Capacity(items.len() as u32)),
        array_ty.clone(),
        Op::ArrayNew.default_effects(),
        Some(arg_array.span),
    );
    for (index, item) in items.iter().enumerate() {
        let value = if let Some(var_name) = invoker_ref_arg_variable(ctx, sig, index, item) {
            lower_invoker_ref_arg_marker(ctx, var_name, item.span)
        } else {
            let value = lower_expr(ctx, item);
            coerce_variadic_tail_value(ctx, value, &array_ty, item.span)
        };
        ctx.emit_void(
            Op::ArrayPush,
            vec![array.value, value.value],
            None,
            Op::ArrayPush.default_effects(),
            Some(item.span),
        );
        super::stmt::release_indexed_array_write_operand(ctx, Some(&elem_ty), value, item.span);
    }
    Some(array)
}

/// Returns true when `call_user_func()` must keep runtime descriptor semantics.
fn call_user_func_should_use_descriptor(
    ctx: &LoweringContext<'_, '_>,
    callback: &Expr,
    args: &[Expr],
    sig: Option<&FunctionSig>,
) -> bool {
    let has_named_or_spread =
        crate::types::call_args::has_named_args(args) || args.iter().any(is_spread_arg);
    if has_named_or_spread {
        return true;
    }
    if call_user_func_has_incompatible_ref_marker_arg(ctx, args, sig) {
        return false;
    }
    if sig.is_some_and(|sig| sig.ref_params.iter().any(|is_ref| *is_ref)) {
        return true;
    }
    match &callback.kind {
        ExprKind::ArrayLiteral(_)
        | ExprKind::ArrayLiteralAssoc(_)
        | ExprKind::Closure { .. }
        | ExprKind::NewObject { .. }
        | ExprKind::NewDynamicObject { .. }
        | ExprKind::Ternary { .. }
        | ExprKind::ShortTernary { .. }
        | ExprKind::FirstClassCallable(CallableTarget::Method { .. }) => true,
        ExprKind::Variable(name) => {
            if let Some(target) = ctx.static_callable_local(name) {
                return matches!(
                    target,
                    StaticCallableBinding::Closure { .. }
                        | StaticCallableBinding::StaticMethodDescriptor { .. }
                        | StaticCallableBinding::InstanceMethod { .. }
                );
            }
            matches!(
                ctx.local_type(name).codegen_repr(),
                PhpType::Callable | PhpType::Array(_) | PhpType::Object(_)
            )
        }
        _ => false,
    }
}

/// Returns true when direct descriptor ref markers cannot represent an argument.
fn call_user_func_has_incompatible_ref_marker_arg(
    ctx: &LoweringContext<'_, '_>,
    args: &[Expr],
    sig: Option<&FunctionSig>,
) -> bool {
    let Some(sig) = sig else {
        return false;
    };
    args.iter().enumerate().any(|(index, arg)| {
        if !sig.ref_params.get(index).copied().unwrap_or(false) {
            return false;
        }
        let ExprKind::Variable(name) = &arg.kind else {
            return false;
        };
        !invoker_ref_arg_storage_compatible(ctx, sig, index, name)
    })
}

/// Lowers `call_user_func()` into a descriptor invoke when the callback value is supported.
fn lower_call_user_func_descriptor_invoke(
    ctx: &mut LoweringContext<'_, '_>,
    callback_expr: &Expr,
    args: &[Expr],
    sig: Option<&FunctionSig>,
    expr: &Expr,
) -> Option<LoweredValue> {
    let callback = lower_expr(ctx, callback_expr);
    if !descriptor_callback_php_type_supported(&ctx.builder.value_php_type(callback.value).codegen_repr()) {
        return None;
    }
    lower_call_user_func_descriptor_invoke_from_value(ctx, callback, args, sig, expr)
}

/// Emits `CallableDescriptorInvoke` for an already evaluated `call_user_func()` callback.
fn lower_call_user_func_descriptor_invoke_from_value(
    ctx: &mut LoweringContext<'_, '_>,
    callback: LoweredValue,
    args: &[Expr],
    sig: Option<&FunctionSig>,
    expr: &Expr,
) -> Option<LoweredValue> {
    let arg_container = lower_descriptor_invoker_arg_container_for_call_user_func(ctx, args, sig, expr.span)?;
    let result_type = sig
        .map(|sig| normalize_value_php_type(sig.return_type.codegen_repr()))
        .unwrap_or(PhpType::Mixed);
    Some(emit_callable_descriptor_invoke(
        ctx,
        callback,
        arg_container,
        result_type,
        expr.span,
    ))
}

/// Emits a descriptor invoke and releases an owned argument container after the call.
fn emit_callable_descriptor_invoke(
    ctx: &mut LoweringContext<'_, '_>,
    callback: LoweredValue,
    arg_container: LoweredValue,
    result_type: PhpType,
    span: Span,
) -> LoweredValue {
    let result = ctx.emit_value(
        Op::CallableDescriptorInvoke,
        vec![callback.value, arg_container.value],
        None,
        result_type,
        Op::CallableDescriptorInvoke.default_effects(),
        Some(span),
    );
    if ctx.value_is_owning_temporary(arg_container) {
        crate::ir_lower::ownership::release_if_owned(ctx, arg_container, Some(span));
    }
    result
}

/// Returns true when the EIR backend has descriptor dispatch for this callback type.
///
/// A `Mixed`/`Union` callback (e.g. a callable read back from an untyped property)
/// is routed here too: the codegen `callable_descriptor_invoke` unboxes it and
/// dispatches by runtime tag (string function name or closure descriptor), so the
/// robust descriptor path is preferred over the `Op::ExprCall` fallback, which has
/// no Mixed arm.
fn descriptor_callback_php_type_supported(php_type: &PhpType) -> bool {
    matches!(
        php_type,
        PhpType::Str
            | PhpType::Callable
            | PhpType::Array(_)
            | PhpType::Object(_)
            | PhpType::Mixed
            | PhpType::Union(_)
    )
}

/// Builds the descriptor-invoker argument container for `call_user_func()`.
fn lower_descriptor_invoker_arg_container_for_call_user_func(
    ctx: &mut LoweringContext<'_, '_>,
    args: &[Expr],
    sig: Option<&FunctionSig>,
    span: Span,
) -> Option<LoweredValue> {
    if crate::types::call_args::has_named_args(args) {
        if args.iter().any(is_spread_arg) {
            return None;
        }
        return Some(lower_named_descriptor_invoker_arg_container(ctx, args, sig, span));
    }
    Some(lower_indexed_descriptor_invoker_arg_array(ctx, args, sig, span))
}

/// Builds an indexed `array<mixed>` argument container, expanding positional spreads.
fn lower_indexed_descriptor_invoker_arg_array(
    ctx: &mut LoweringContext<'_, '_>,
    args: &[Expr],
    sig: Option<&FunctionSig>,
    span: Span,
) -> LoweredValue {
    let elem_ty = PhpType::Mixed;
    let array_ty = PhpType::Array(Box::new(elem_ty.clone()));
    let array = ctx.emit_value(
        Op::ArrayNew,
        Vec::new(),
        Some(Immediate::Capacity(args.len() as u32)),
        array_ty.clone(),
        Op::ArrayNew.default_effects(),
        Some(span),
    );
    let mut positional_index = 0usize;
    for arg in args {
        if let ExprKind::Spread(inner) = &arg.kind {
            let source = lower_expr(ctx, inner);
            lower_indexed_array_spread_into_array(ctx, array, source, Some(&elem_ty), arg.span);
            continue;
        }
        let value = if let Some(var_name) = invoker_ref_arg_variable(ctx, sig, positional_index, arg) {
            lower_invoker_ref_arg_marker(ctx, var_name, arg.span)
        } else {
            let value = lower_expr(ctx, arg);
            coerce_variadic_tail_value(ctx, value, &array_ty, arg.span)
        };
        ctx.emit_void(
            Op::ArrayPush,
            vec![array.value, value.value],
            None,
            Op::ArrayPush.default_effects(),
            Some(arg.span),
        );
        super::stmt::release_indexed_array_write_operand(ctx, Some(&elem_ty), value, arg.span);
        positional_index += 1;
    }
    array
}

/// Builds a boxed hash argument container for named `call_user_func()` args.
fn lower_named_descriptor_invoker_arg_container(
    ctx: &mut LoweringContext<'_, '_>,
    args: &[Expr],
    sig: Option<&FunctionSig>,
    span: Span,
) -> LoweredValue {
    let hash_ty = PhpType::AssocArray {
        key: Box::new(PhpType::Mixed),
        value: Box::new(PhpType::Mixed),
    };
    let hash = ctx.emit_value(
        Op::HashNew,
        Vec::new(),
        Some(Immediate::Capacity(args.len() as u32)),
        hash_ty,
        Op::HashNew.default_effects(),
        Some(span),
    );
    let mut next_positional_key = 0i64;
    for arg in args {
        match &arg.kind {
            ExprKind::NamedArg { name, value } => {
                let key = lower_string_literal(ctx, name, arg);
                let param_index = sig.and_then(|sig| {
                    let regular_param_count = crate::types::call_args::regular_param_count(sig);
                    crate::types::call_args::named_param_index(sig, regular_param_count, name)
                });
                let value = if let Some(index) = param_index {
                    invoker_ref_arg_variable(ctx, sig, index, value)
                        .map(|var_name| lower_invoker_ref_arg_marker(ctx, var_name, value.span))
                } else {
                    None
                }
                .unwrap_or_else(|| lower_expr(ctx, value));
                ctx.emit_void(
                    Op::HashSet,
                    vec![hash.value, key.value, value.value],
                    None,
                    Op::HashSet.default_effects(),
                    Some(arg.span),
                );
            }
            _ => {
                let key = emit_i64_at_span(ctx, next_positional_key, arg.span);
                let value = if let Some(var_name) =
                    invoker_ref_arg_variable(ctx, sig, next_positional_key as usize, arg)
                {
                    lower_invoker_ref_arg_marker(ctx, var_name, arg.span)
                } else {
                    lower_expr(ctx, arg)
                };
                next_positional_key += 1;
                ctx.emit_void(
                    Op::HashSet,
                    vec![hash.value, key.value, value.value],
                    None,
                    Op::HashSet.default_effects(),
                    Some(arg.span),
                );
            }
        }
    }
    ctx.box_value_as_mixed(hash, PhpType::Mixed, Some(span))
}

/// Returns the variable name when this literal argument should be passed by reference.
fn invoker_ref_arg_variable<'a>(
    _ctx: &LoweringContext<'_, '_>,
    sig: Option<&FunctionSig>,
    index: usize,
    item: &'a Expr,
) -> Option<&'a str> {
    let ExprKind::Variable(name) = &item.kind else {
        return None;
    };
    if let Some(sig) = sig {
        if !sig.ref_params.get(index).copied().unwrap_or(false) {
            return None;
        }
    }
    Some(name.as_str())
}

/// Returns true when a local slot can be passed directly to a descriptor ref param.
fn invoker_ref_arg_storage_compatible(
    ctx: &LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    var_name: &str,
) -> bool {
    let Some((_, param_ty)) = sig.params.get(index) else {
        return true;
    };
    value_ir_type(&param_ty.codegen_repr()) == value_ir_type(&ctx.local_type(var_name).codegen_repr())
}

/// Emits an invoker reference-cell marker for a local variable argument.
fn lower_invoker_ref_arg_marker(
    ctx: &mut LoweringContext<'_, '_>,
    var_name: &str,
    span: Span,
) -> LoweredValue {
    let php_type = ctx.local_type(var_name);
    let slot = ctx.declare_local(var_name, php_type);
    ctx.emit_value(
        Op::InvokerRefArg,
        Vec::new(),
        Some(Immediate::LocalSlot(slot)),
        PhpType::Mixed,
        Op::InvokerRefArg.default_effects(),
        Some(span),
    )
}

/// Lowers `array_map()` for a static callback and indexed array literal source.
fn lower_static_array_map(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if php_symbol_key(name.trim_start_matches('\\')) != "array_map" || args.len() != 2 {
        return None;
    }
    if crate::types::call_args::has_named_args(args) || args.iter().any(is_spread_arg) {
        return None;
    }
    if matches!(args[0].kind, ExprKind::Variable(_)) {
        return None;
    }
    let callback = static_call_user_func_callback(ctx, &args[0])?;
    let ExprKind::ArrayLiteral(items) = &args[1].kind else {
        return None;
    };
    let elem_type = static_callable_return_type(ctx, &callback);
    let array = ctx.emit_value(
        Op::ArrayNew,
        Vec::new(),
        Some(Immediate::Capacity(items.len() as u32)),
        PhpType::Array(Box::new(elem_type.clone())),
        Op::ArrayNew.default_effects(),
        Some(expr.span),
    );
    for item in items {
        let value = lower_static_callable_call(ctx, callback.clone(), std::slice::from_ref(item), expr)?;
        ctx.emit_void(
            Op::ArrayPush,
            vec![array.value, value.value],
            None,
            Op::ArrayPush.default_effects(),
            Some(item.span),
        );
        super::stmt::release_indexed_array_write_operand(ctx, Some(&elem_type), value, item.span);
    }
    Some(array)
}

/// Lowers `array_reduce()` for a static callback and immediate indexed-array literal.
fn lower_static_array_reduce(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if php_symbol_key(name.trim_start_matches('\\')) != "array_reduce" || args.len() != 3 {
        return None;
    }
    if crate::types::call_args::has_named_args(args) || args.iter().any(is_spread_arg) {
        return None;
    }
    if matches!(args[1].kind, ExprKind::Variable(_)) {
        return None;
    }
    let ExprKind::ArrayLiteral(items) = &args[0].kind else {
        return None;
    };
    if !items.iter().all(static_callback_array_item_can_inline) {
        return None;
    }
    let callback = static_call_user_func_callback(ctx, &args[1])?;
    let result_type = fallback_expr_type(expr);
    let temp_name = ctx.declare_owned_hidden_temp(result_type.clone());
    let initial = lower_expr(ctx, &args[2]);
    store_value_into_temp(ctx, &temp_name, result_type.clone(), initial, expr.span);
    for item in items {
        let carry = ctx.load_local(&temp_name, Some(expr.span));
        let item_value = lower_expr(ctx, item);
        let reduced = lower_static_callable_value_call(
            ctx,
            callback.clone(),
            vec![carry.value, item_value.value],
            expr,
        )?;
        store_value_into_temp(ctx, &temp_name, result_type.clone(), reduced, expr.span);
    }
    Some(take_owned_temp(ctx, &temp_name, expr.span))
}

/// Lowers `array_walk()` for a static callback and immediate indexed-array literal.
fn lower_static_array_walk(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if php_symbol_key(name.trim_start_matches('\\')) != "array_walk" || args.len() != 2 {
        return None;
    }
    if crate::types::call_args::has_named_args(args) || args.iter().any(is_spread_arg) {
        return None;
    }
    if matches!(args[1].kind, ExprKind::Variable(_)) {
        return None;
    }
    let ExprKind::ArrayLiteral(items) = &args[0].kind else {
        return None;
    };
    if !items.iter().all(static_callback_array_item_can_inline) {
        return None;
    }
    let callback = static_call_user_func_callback(ctx, &args[1])?;
    for item in items {
        let item_value = lower_expr(ctx, item);
        lower_static_callable_value_call(ctx, callback.clone(), vec![item_value.value], expr)?;
    }
    Some(lower_null(ctx, expr))
}

/// Returns whether a literal array element can be reordered around callback invocation safely.
fn static_callback_array_item_can_inline(item: &Expr) -> bool {
    matches!(
        item.kind,
        ExprKind::IntLiteral(_)
            | ExprKind::FloatLiteral(_)
            | ExprKind::BoolLiteral(_)
            | ExprKind::StringLiteral(_)
            | ExprKind::Null
    )
}

/// Returns the best known element type for a static callback used by `array_map()`.
fn static_callable_return_type(
    ctx: &LoweringContext<'_, '_>,
    target: &StaticCallableBinding,
) -> PhpType {
    match target {
        StaticCallableBinding::UserFunction(name)
        | StaticCallableBinding::ExternFunction(name)
        | StaticCallableBinding::Builtin(name) => call_return_type(ctx, name, &[]),
        StaticCallableBinding::Closure { signature, .. } => {
            normalize_value_php_type(signature.return_type.codegen_repr())
        }
        StaticCallableBinding::StaticMethod { receiver, method }
        | StaticCallableBinding::StaticMethodDescriptor { receiver, method } => {
            static_method_implementation_signature(ctx, receiver, method)
                .map(|signature| normalize_value_php_type(signature.return_type.codegen_repr()))
                .unwrap_or(PhpType::Mixed)
        }
        StaticCallableBinding::InstanceMethod { signature, .. } => {
            normalize_value_php_type(signature.return_type.codegen_repr())
        }
    }
}

/// Lowers one resolved static callable target using already-evaluated positional operands.
fn lower_static_callable_value_call(
    ctx: &mut LoweringContext<'_, '_>,
    target: StaticCallableBinding,
    operands: Vec<crate::ir::ValueId>,
    expr: &Expr,
) -> Option<LoweredValue> {
    match target {
        StaticCallableBinding::UserFunction(function_name) => {
            let php_type = call_return_type(ctx, &function_name, &operands);
            let data = ctx.intern_function_name(&function_name);
            Some(ctx.emit_value(
                Op::Call,
                operands,
                Some(Immediate::Data(data)),
                php_type,
                effects_lookup::user_call_effects(&function_name),
                Some(expr.span),
            ))
        }
        StaticCallableBinding::ExternFunction(function_name) => {
            let php_type = call_return_type(ctx, &function_name, &operands);
            let data = ctx.intern_function_name(&function_name);
            Some(ctx.emit_value(
                Op::ExternCall,
                operands,
                Some(Immediate::Data(data)),
                php_type,
                Op::ExternCall.default_effects(),
                Some(expr.span),
            ))
        }
        StaticCallableBinding::Builtin(function_name) => {
            let php_type = call_return_type(ctx, &function_name, &operands);
            Some(emit_builtin_call_value(
                ctx,
                &function_name,
                operands,
                php_type,
                expr.span,
                None,
            ))
        }
        StaticCallableBinding::Closure {
            name,
            signature,
            captures,
        } => {
            let mut operands = operands;
            append_closure_capture_operands(&mut operands, &captures);
            let php_type = normalize_value_php_type(signature.return_type.codegen_repr());
            let data = ctx.intern_function_name(&name);
            Some(ctx.emit_value(
                Op::Call,
                operands,
                Some(Immediate::Data(data)),
                php_type,
                effects_lookup::user_call_effects(&name),
                Some(expr.span),
            ))
        }
        StaticCallableBinding::StaticMethod { receiver, method } => {
            let sig = static_method_implementation_signature(ctx, &receiver, &method);
            let result_type = sig
                .map(|signature| normalize_value_php_type(signature.return_type.codegen_repr()))
                .unwrap_or_else(|| fallback_expr_type(expr));
            let name = format!("{}::{}", receiver_name(&receiver), method);
            let data = ctx.intern_string(&name);
            Some(ctx.emit_value(
                Op::StaticMethodCall,
                operands,
                Some(Immediate::Data(data)),
                result_type,
                Op::StaticMethodCall.default_effects(),
                Some(expr.span),
            ))
        }
        StaticCallableBinding::StaticMethodDescriptor { receiver, method } => {
            lower_static_method_descriptor_value_call(ctx, &receiver, &method, operands, expr)
        }
        StaticCallableBinding::InstanceMethod { .. } => None,
    }
}

/// Resolves a compile-time `call_user_func*` callback expression.
fn static_call_user_func_callback(
    ctx: &LoweringContext<'_, '_>,
    callback: &Expr,
) -> Option<StaticCallableBinding> {
    match &callback.kind {
        ExprKind::StringLiteral(name) => resolve_static_string_callable(ctx, name),
        ExprKind::FirstClassCallable(CallableTarget::Function(name)) => {
            resolve_static_string_callable(ctx, name.as_str())
        }
        ExprKind::FirstClassCallable(CallableTarget::StaticMethod { receiver, method }) => {
            resolve_static_method_callable(ctx, receiver.clone(), method.clone())
        }
        ExprKind::Variable(name) => ctx
            .static_callable_local(name)
            .and_then(direct_static_callable_binding),
        ExprKind::ArrayLiteral(items) => static_array_callable_descriptor_target(ctx, items)
            .or_else(|| instance_array_callable_target(ctx, items)),
        _ => None,
    }
}

/// Resolves `call_user_func*` callbacks that must keep descriptor receiver state.
fn instance_call_user_func_callback(
    ctx: &LoweringContext<'_, '_>,
    callback: &Expr,
) -> Option<StaticCallableBinding> {
    let target = match &callback.kind {
        ExprKind::FirstClassCallable(CallableTarget::Method { .. }) => {
            static_callable_binding_for_expr(ctx, callback)
        }
        ExprKind::Variable(name) => ctx.static_callable_local(name),
        _ => None,
    }?;
    if matches!(target, StaticCallableBinding::InstanceMethod { .. }) {
        Some(target)
    } else {
        None
    }
}

/// Returns signature metadata for receiver-bound callables that still need descriptor state.
fn instance_callable_signature(target: &StaticCallableBinding) -> Option<&FunctionSig> {
    match target {
        StaticCallableBinding::InstanceMethod { signature, .. } => Some(signature),
        _ => None,
    }
}

/// Resolves a literal first-class callable expression to a static local binding.
pub(crate) fn static_callable_binding_for_expr(
    ctx: &LoweringContext<'_, '_>,
    expr: &Expr,
) -> Option<StaticCallableBinding> {
    match &expr.kind {
        ExprKind::StringLiteral(name) => resolve_static_string_callable(ctx, name),
        ExprKind::FirstClassCallable(CallableTarget::Function(name)) => {
            resolve_static_string_callable(ctx, name.as_str())
        }
        ExprKind::FirstClassCallable(CallableTarget::StaticMethod { receiver, method }) => {
            resolve_static_method_callable(ctx, receiver.clone(), method.clone())
        }
        ExprKind::ArrayLiteral(items) => static_array_callable_descriptor_target(ctx, items)
            .or_else(|| instance_array_callable_target(ctx, items)),
        ExprKind::FirstClassCallable(CallableTarget::Method { object, method }) => {
            resolve_instance_method_callable(ctx, object, method.clone(), false)
        }
        ExprKind::Variable(name) => ctx.static_callable_local(name),
        _ => None,
    }
}

/// Returns the reflected class captured by a statically-known `ReflectionClass` expression.
pub(crate) fn reflection_class_binding_for_expr(
    ctx: &LoweringContext<'_, '_>,
    expr: &Expr,
) -> Option<String> {
    reflection_class_new_instance_reflected_class(ctx, expr)
}

/// Returns the reflected function captured by a statically-known `ReflectionFunction` expression.
pub(crate) fn reflection_function_binding_for_expr(
    ctx: &LoweringContext<'_, '_>,
    expr: &Expr,
) -> Option<String> {
    reflection_function_reflected_target(ctx, expr)
}

/// Returns the reflected property captured by a statically-known `ReflectionProperty` expression.
pub(crate) fn reflection_property_binding_for_expr(
    ctx: &LoweringContext<'_, '_>,
    expr: &Expr,
) -> Option<(String, String)> {
    reflection_property_reflected_target(ctx, expr)
}

/// Returns the reflected method captured by a statically-known `ReflectionMethod` expression.
pub(crate) fn reflection_method_binding_for_expr(
    ctx: &LoweringContext<'_, '_>,
    expr: &Expr,
) -> Option<(String, String)> {
    reflection_method_reflected_target(ctx, expr)
}

/// Returns a safe static argument array that can be replayed for reflection forwarding.
pub(crate) fn reflection_arg_array_binding_for_expr(expr: &Expr) -> Option<Vec<Expr>> {
    let args = reflection_class_new_instance_args_value_without_locals(expr)?;
    if args.iter().all(reflection_arg_expr_can_track) {
        Some(args)
    } else {
        None
    }
}

/// Returns true when replaying an argument expression cannot duplicate side effects.
fn reflection_arg_expr_can_track(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::ConstRef(_)
        | ExprKind::ClassConstant { .. }
        | ExprKind::ScopedConstantAccess { .. }
        | ExprKind::MagicConstant(_) => true,
        ExprKind::Negate(inner) => matches!(
            &inner.kind,
            ExprKind::IntLiteral(_) | ExprKind::FloatLiteral(_)
        ),
        ExprKind::NamedArg { value, .. } => reflection_arg_expr_can_track(value),
        ExprKind::ArrayLiteral(items) => items.iter().all(reflection_arg_expr_can_track),
        ExprKind::ArrayLiteralAssoc(entries) => entries.iter().all(|(key, value)| {
            reflection_arg_array_key_can_track(key) && reflection_arg_expr_can_track(value)
        }),
        _ => false,
    }
}

/// Returns true when an associative array key is stable enough for replay.
fn reflection_arg_array_key_can_track(expr: &Expr) -> bool {
    matches!(
        expr.kind,
        ExprKind::StringLiteral(_)
            | ExprKind::IntLiteral(_)
            | ExprKind::BoolLiteral(_)
            | ExprKind::FloatLiteral(_)
    )
}

/// EIR value and callable binding produced by a callable-array assignment.
pub(crate) struct LoweredCallableArrayAssignment {
    pub(crate) value: LoweredValue,
    pub(crate) target: StaticCallableBinding,
}

/// Lowers a callable-array assignment while preserving its PHP array value.
pub(crate) fn lower_callable_array_for_assignment(
    ctx: &mut LoweringContext<'_, '_>,
    value: &Expr,
    target: Option<&StaticCallableBinding>,
) -> Option<LoweredCallableArrayAssignment> {
    let ExprKind::ArrayLiteral(items) = &value.kind else {
        return None;
    };
    let StaticCallableBinding::InstanceMethod {
        object,
        method,
        signature,
        ..
    } = target? else {
        return None;
    };
    let receiver = lower_expr(ctx, object);
    let receiver_ty = ctx.builder.value_php_type(receiver.value);
    let hidden_name = ctx.declare_hidden_temp(receiver_ty.clone());
    let receiver = ctx.store_local(&hidden_name, receiver, receiver_ty, Some(object.span));
    let array = lower_callable_array_literal_with_receiver(ctx, items, value, receiver);
    let hidden_object = Expr::new(ExprKind::Variable(hidden_name), object.span);
    let target = StaticCallableBinding::InstanceMethod {
        object: Box::new(hidden_object),
        method: method.clone(),
        signature: signature.clone(),
        direct_call: true,
    };
    Some(LoweredCallableArrayAssignment { value: array, target })
}

/// Lowers a callable-array literal after its receiver has already been captured.
fn lower_callable_array_literal_with_receiver(
    ctx: &mut LoweringContext<'_, '_>,
    items: &[Expr],
    expr: &Expr,
    receiver: LoweredValue,
) -> LoweredValue {
    let array_ty = array_literal_type_for_ir(ctx, items, expr);
    let elem_ty = indexed_array_literal_element_type(&array_ty);
    let array = ctx.emit_value(
        Op::ArrayNew,
        Vec::new(),
        Some(Immediate::Capacity(items.len() as u32)),
        array_ty,
        Op::ArrayNew.default_effects(),
        Some(expr.span),
    );
    ctx.emit_void(
        Op::ArrayPush,
        vec![array.value, receiver.value],
        None,
        Op::ArrayPush.default_effects(),
        Some(expr.span),
    );
    super::stmt::release_indexed_array_write_operand(ctx, elem_ty.as_ref(), receiver, expr.span);
    for item in items.iter().skip(1) {
        let value = lower_expr(ctx, item);
        ctx.emit_void(
            Op::ArrayPush,
            vec![array.value, value.value],
            None,
            Op::ArrayPush.default_effects(),
            Some(item.span),
        );
        super::stmt::release_indexed_array_write_operand(ctx, elem_ty.as_ref(), value, item.span);
    }
    array
}

/// Resolves a static callable array literal as a descriptor-backed static method.
fn static_array_callable_descriptor_target(
    ctx: &LoweringContext<'_, '_>,
    items: &[Expr],
) -> Option<StaticCallableBinding> {
    static_array_callable_parts(ctx, items).map(|(receiver, method)| {
        StaticCallableBinding::StaticMethodDescriptor { receiver, method }
    })
}

/// Resolves a literal `[$object, "method"]` callable array as an instance method.
fn instance_array_callable_target(
    ctx: &LoweringContext<'_, '_>,
    items: &[Expr],
) -> Option<StaticCallableBinding> {
    let [object, method_expr] = items else {
        return None;
    };
    let ExprKind::StringLiteral(method) = &method_expr.kind else {
        return None;
    };
    resolve_instance_method_callable(ctx, object, method.clone(), true)
}

/// Resolves the named static receiver and method from a static callable array literal.
fn static_array_callable_parts(
    ctx: &LoweringContext<'_, '_>,
    items: &[Expr],
) -> Option<(StaticReceiver, String)> {
    let [class_expr, method_expr] = items else {
        return None;
    };
    let class_name = static_callable_class_name(ctx, class_expr)?;
    let ExprKind::StringLiteral(method) = &method_expr.kind else {
        return None;
    };
    let class_name = lookup_folded_name(ctx.classes.keys(), class_name.trim_start_matches('\\'))?;
    let receiver = StaticReceiver::Named(Name::from(class_name));
    static_method_implementation_signature(ctx, &receiver, method)?;
    Some((receiver, method.clone()))
}

/// Extracts a compile-time class name for a static callable array.
fn static_callable_class_name(
    ctx: &LoweringContext<'_, '_>,
    class_expr: &Expr,
) -> Option<String> {
    match &class_expr.kind {
        ExprKind::StringLiteral(name) => Some(name.clone()),
        ExprKind::ClassConstant { receiver } => static_receiver_class_name(ctx, receiver),
        _ => None,
    }
}

/// Returns the static `is_callable()` result for a literal static-method callback array.
fn static_array_callable_is_callable(
    ctx: &LoweringContext<'_, '_>,
    items: &[Expr],
) -> Option<bool> {
    let [class_expr, method_expr] = items else {
        return None;
    };
    let class_name = static_callable_class_name(ctx, class_expr)?;
    let ExprKind::StringLiteral(method) = &method_expr.kind else {
        return None;
    };
    Some(static_method_callback_is_callable(ctx, &class_name, method))
}

/// Returns true when a compile-time class/method pair names a public static method.
fn static_method_callback_is_callable(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    method: &str,
) -> bool {
    let Some(class_name) = lookup_folded_name(ctx.classes.keys(), class_name.trim_start_matches('\\')) else {
        return false;
    };
    let Some(class_info) = ctx.classes.get(&class_name) else {
        return false;
    };
    let method_key = php_symbol_key(method);
    if !class_info.static_methods.contains_key(&method_key) {
        return false;
    }
    class_info.static_method_visibilities.get(&method_key) == Some(&Visibility::Public)
}

/// Converts a static `call_user_func_array()` argument array into call arguments.
fn static_call_user_func_array_args(arg_array: &Expr) -> Option<Vec<Expr>> {
    match &arg_array.kind {
        ExprKind::ArrayLiteral(items) => Some(items.clone()),
        ExprKind::ArrayLiteralAssoc(pairs) => static_call_user_func_array_assoc_args(pairs),
        _ => None,
    }
}

/// Converts literal associative callback arrays into positional or named call args.
fn static_call_user_func_array_assoc_args(pairs: &[(Expr, Expr)]) -> Option<Vec<Expr>> {
    let mut args = Vec::with_capacity(pairs.len());
    for (key, value) in pairs {
        match &key.kind {
            ExprKind::StringLiteral(name) => {
                args.push(Expr::new(
                    ExprKind::NamedArg {
                        name: name.clone(),
                        value: Box::new(value.clone()),
                    },
                    value.span,
                ));
            }
            ExprKind::IntLiteral(_) => args.push(value.clone()),
            _ => return None,
        }
    }
    Some(args)
}

/// Lowers one resolved static callable target to the corresponding EIR call opcode.
fn lower_static_callable_call(
    ctx: &mut LoweringContext<'_, '_>,
    target: StaticCallableBinding,
    callback_args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    match target {
        StaticCallableBinding::UserFunction(function_name) => {
            let sig = ctx.functions.get(&function_name).cloned();
            let operands = lower_args_with_signature(ctx, sig.as_ref(), callback_args);
            let php_type = call_return_type(ctx, &function_name, &operands);
            let data = ctx.intern_function_name(&function_name);
            Some(ctx.emit_value(
                Op::Call,
                operands,
                Some(Immediate::Data(data)),
                php_type,
                effects_lookup::user_call_effects(&function_name),
                Some(expr.span),
            ))
        }
        StaticCallableBinding::ExternFunction(function_name) => {
            let sig = ctx
                .extern_functions
                .get(&function_name)
                .map(function_sig_from_extern_for_descriptor);
            let operands = lower_args_with_signature(ctx, sig.as_ref(), callback_args);
            let php_type = call_return_type(ctx, &function_name, &operands);
            let data = ctx.intern_function_name(&function_name);
            Some(ctx.emit_value(
                Op::ExternCall,
                operands,
                Some(Immediate::Data(data)),
                php_type,
                Op::ExternCall.default_effects(),
                Some(expr.span),
            ))
        }
        StaticCallableBinding::Builtin(function_name) => {
            let sig = call_signature(ctx, &function_name);
            let operands = lower_builtin_call_args(ctx, &function_name, sig.as_ref(), callback_args);
            let php_type = call_return_type(ctx, &function_name, &operands);
            Some(emit_builtin_call_value(
                ctx,
                &function_name,
                operands,
                php_type,
                expr.span,
                None,
            ))
        }
        StaticCallableBinding::Closure {
            name,
            signature,
            captures,
        } => {
            let mut operands = lower_args_with_signature(ctx, Some(&signature), callback_args);
            append_closure_capture_operands(&mut operands, &captures);
            let php_type = normalize_value_php_type(signature.return_type.codegen_repr());
            let data = ctx.intern_function_name(&name);
            Some(ctx.emit_value(
                Op::Call,
                operands,
                Some(Immediate::Data(data)),
                php_type,
                effects_lookup::user_call_effects(&name),
                Some(expr.span),
            ))
        }
        StaticCallableBinding::StaticMethod { receiver, method } => {
            Some(lower_static_method_call(ctx, &receiver, &method, callback_args, expr))
        }
        StaticCallableBinding::StaticMethodDescriptor { receiver, method } => {
            Some(lower_static_method_descriptor_call(
                ctx,
                &receiver,
                &method,
                callback_args,
                expr,
            ))
        }
        StaticCallableBinding::InstanceMethod {
            object,
            method,
            direct_call: true,
            ..
        } => Some(lower_method_call(ctx, &object, &method, callback_args, Op::MethodCall, expr)),
        StaticCallableBinding::InstanceMethod { .. } => None,
    }
}

/// Resolves a PHP string callback using case-insensitive function lookup rules.
fn resolve_static_string_callable(
    ctx: &LoweringContext<'_, '_>,
    callback: &str,
) -> Option<StaticCallableBinding> {
    let callback = callback.trim_start_matches('\\');
    if let Some((class_name, method)) = callback.rsplit_once("::") {
        let class_name = lookup_folded_name(ctx.classes.keys(), class_name.trim_start_matches('\\'))?;
        return resolve_static_method_callable(
            ctx,
            StaticReceiver::Named(Name::from(class_name)),
            method.to_string(),
        );
    }
    if let Some(function_name) = lookup_folded_name(ctx.extern_functions.keys(), callback) {
        return Some(StaticCallableBinding::ExternFunction(function_name));
    }
    if let Some(function_name) = canonical_builtin_function_name(callback) {
        return Some(StaticCallableBinding::Builtin(function_name));
    }
    if let Some(function_name) = lookup_folded_name(ctx.functions.keys(), callback) {
        return Some(StaticCallableBinding::UserFunction(function_name));
    }
    None
}

/// Appends captured closure values after caller-visible operands for hidden ABI params.
fn append_closure_capture_operands(operands: &mut Vec<ValueId>, captures: &[ClosureCapture]) {
    operands.extend(captures.iter().map(|capture| capture.value));
}

/// Resolves a static method callback when class and method are compile-time known.
fn resolve_static_method_callable(
    ctx: &LoweringContext<'_, '_>,
    receiver: StaticReceiver,
    method: String,
) -> Option<StaticCallableBinding> {
    static_method_implementation_signature(ctx, &receiver, &method)?;
    Some(StaticCallableBinding::StaticMethod { receiver, method })
}

/// Resolves a first-class instance-method callable to signature metadata only.
fn resolve_instance_method_callable(
    ctx: &LoweringContext<'_, '_>,
    object: &Expr,
    method: String,
    direct_call: bool,
) -> Option<StaticCallableBinding> {
    let class_name = instance_callable_object_class(ctx, object)?;
    let method_key = php_symbol_key(&method);
    let signature = class_method_signature(ctx, &class_name, &method_key)?.clone();
    Some(StaticCallableBinding::InstanceMethod {
        object: Box::new(object.clone()),
        method,
        signature,
        direct_call,
    })
}

/// Returns a static callable only when it can be lowered without descriptor state.
fn direct_static_callable_binding(target: StaticCallableBinding) -> Option<StaticCallableBinding> {
    if matches!(target, StaticCallableBinding::InstanceMethod { .. }) {
        None
    } else {
        Some(target)
    }
}

/// Resolves the concrete class for an object expression used in an instance FCC.
/// Returns the property type referenced by a `Closure::bind(fn () => $this->prop, $newThis, …)`
/// when the closure body is exactly `return $this->prop`: the bound object's property type.
/// The closure's own `$this` is Mixed (it is bound dynamically), so the type comes from the
/// bind's receiver argument.
fn closure_bind_property_return_type(
    ctx: &LoweringContext<'_, '_>,
    callee: &Expr,
) -> Option<PhpType> {
    let ExprKind::StaticMethodCall { receiver, method, args } = &callee.kind else {
        return None;
    };
    if !method.eq_ignore_ascii_case("bind") {
        return None;
    }
    let crate::parser::ast::StaticReceiver::Named(name) = receiver else {
        return None;
    };
    if !name
        .as_str()
        .trim_start_matches('\\')
        .eq_ignore_ascii_case("Closure")
    {
        return None;
    }
    let ExprKind::Closure { body, .. } = &args.first()?.kind else {
        return None;
    };
    let new_this_class = instance_callable_object_class(ctx, args.get(1)?)?;
    let [stmt] = body.as_slice() else {
        return None;
    };
    let StmtKind::Return(Some(ret)) = &stmt.kind else {
        return None;
    };
    let ExprKind::PropertyAccess { object, property } = &ret.kind else {
        return None;
    };
    if !matches!(object.kind, ExprKind::This) {
        return None;
    }
    let info = ctx.classes.get(new_this_class.trim_start_matches('\\'))?;
    info.properties
        .iter()
        .find(|(name, _)| name == property)
        .map(|(_, ty)| ty.clone())
}

/// Lowers `Closure::bind(fn &() => $this->prop, $newThis, scope)()` as a direct call to the
/// closure with `$newThis` boxed as its `$this` capture.
///
/// `Closure::bind` rebinds the closure's receiver; invoking the result through the generic
/// runtime descriptor invoker boxes the closure's return value as Mixed, which cannot carry a
/// by-reference property cell pointer. Calling the closure directly (as `$f()` does) passes the
/// cell pointer through. The call result is typed from the bound receiver's property so a
/// by-reference array return binds correctly. Only the auto-captured `$this` shape (the
/// `fn &() => $this->prop` form) is handled; other captures fall back to the generic path.
fn lower_bound_closure_immediate_call(
    ctx: &mut LoweringContext<'_, '_>,
    callee: &Expr,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let (bound, _closure_value) = build_bound_closure_binding(ctx, callee, expr)?;
    lower_static_callable_call(ctx, bound, args, expr)
}

/// Builds the static-callable binding for `Closure::bind(fn &() => $this->prop, $newThis, scope)`.
///
/// Lowers the closure literal (once), boxes `$newThis` as the closure's `$this` capture, and
/// overrides the binding's return type with the bound receiver's property type so a
/// by-reference return binds correctly. Returns the binding together with the lowered closure
/// descriptor value (the still-unbound `closure_new`), which callers may store in the assigned
/// variable. `None` unless the call is the single auto-captured `$this` shape — the only form
/// whose `$this` is fully known at compile time. Shared by the immediate-invoke path
/// (`Closure::bind(...)()`) and the variable-assignment path (`$b = Closure::bind(...)`).
fn build_bound_closure_binding(
    ctx: &mut LoweringContext<'_, '_>,
    callee: &Expr,
    expr: &Expr,
) -> Option<(StaticCallableBinding, LoweredValue)> {
    let result_type = closure_bind_property_return_type(ctx, callee)?;
    let ExprKind::StaticMethodCall { args: bind_args, .. } = &callee.kind else {
        return None;
    };
    let closure_lit = bind_args.first()?;
    if !matches!(closure_lit.kind, ExprKind::Closure { .. }) {
        return None;
    }
    let new_this = bind_args.get(1)?.clone();
    // Lower the closure literal to obtain its static binding (function name + captures).
    let closure_value = lower_expr(ctx, closure_lit);
    let Some(StaticCallableBinding::Closure {
        name,
        mut signature,
        captures,
    }) = ctx.take_pending_static_callable_result()
    else {
        return None;
    };
    // Only the single auto-captured `$this` shape is supported here.
    if captures.len() != 1 {
        return None;
    }
    let new_this_value = lower_expr(ctx, &new_this);
    let boxed_this = ctx.box_value_as_mixed(new_this_value, PhpType::Mixed, Some(expr.span));
    signature.return_type = result_type;
    let bound = StaticCallableBinding::Closure {
        name,
        signature,
        captures: vec![ClosureCapture {
            value: boxed_this.value,
        }],
    };
    Some((bound, closure_value))
}

/// Returns true when an assignment value is a by-reference `Closure::bind` of the auto-`$this`
/// shape, so the assignment should track the result as a static callable (routing a later
/// `$b()` through the direct-call path that carries the by-reference cell pointer).
///
/// Read-only structural check (no IR emitted) used to set `direct_closure` before lowering.
pub(crate) fn is_bound_closure_assignment_shape(ctx: &LoweringContext<'_, '_>, value: &Expr) -> bool {
    let ExprKind::StaticMethodCall { args, .. } = &value.kind else {
        return false;
    };
    let Some(ExprKind::Closure { by_ref_return, .. }) = args.first().map(|arg| &arg.kind) else {
        return false;
    };
    *by_ref_return && closure_bind_property_return_type(ctx, value).is_some()
}

/// Lowers `$b = Closure::bind(fn &() => $this->prop, $newThis, scope)` for assignment: builds
/// the bound-closure binding, publishes it as the pending static callable so the assignment
/// registers `$b` for later direct `$b()` calls, and returns the closure descriptor to store
/// in `$b`. `None` for any non-matching shape so normal assignment lowering applies.
pub(crate) fn lower_bound_closure_for_assignment(
    ctx: &mut LoweringContext<'_, '_>,
    value: &Expr,
) -> Option<LoweredValue> {
    let (bound, closure_value) = build_bound_closure_binding(ctx, value, value)?;
    ctx.set_pending_static_callable_result(bound);
    Some(closure_value)
}

/// Resolves the statically-known class name of an object expression used as the
/// receiver of an instance first-class callable (`$obj->m(...)`).
///
/// Returns the normalized class name for `$var` (from `local_types`), `$this`
/// (the current class), and `new` expressions; `None` when the receiver class
/// cannot be determined statically.
fn instance_callable_object_class(
    ctx: &LoweringContext<'_, '_>,
    object: &Expr,
) -> Option<String> {
    match &object.kind {
        ExprKind::Variable(name) => ctx
            .local_types
            .get(name)
            .and_then(class_name_from_php_type),
        ExprKind::This => ctx.current_class.as_deref().and_then(normalized_class_name),
        ExprKind::NewObject { class_name, .. } => normalized_class_name(class_name.as_str()),
        ExprKind::NewDynamicObject { fallback_class, .. } => {
            normalized_class_name(fallback_class.as_str())
        }
        ExprKind::FunctionCall { name, .. } => ctx
            .functions
            .get(name.as_str())
            .and_then(|sig| class_name_from_php_type(&sig.return_type)),
        _ => class_name_from_php_type(&infer_expr_type_syntactic(object)),
    }
}

/// Returns a non-empty normalized class name for an object PHP type.
fn class_name_from_php_type(ty: &PhpType) -> Option<String> {
    match ty.codegen_repr() {
        PhpType::Object(class_name) => normalized_class_name(&class_name),
        _ => None,
    }
}

/// Trims PHP's optional leading namespace separator from class metadata names.
fn normalized_class_name(class_name: &str) -> Option<String> {
    let class_name = class_name.trim_start_matches('\\');
    if class_name.is_empty() {
        None
    } else {
        Some(class_name.to_string())
    }
}

/// Looks up a PHP function name case-insensitively and returns the canonical candidate.
fn lookup_folded_name<'a, I>(names: I, requested: &str) -> Option<String>
where
    I: IntoIterator<Item = &'a String>,
{
    let requested = php_symbol_key(requested);
    names
        .into_iter()
        .find(|candidate| php_symbol_key(candidate) == requested)
        .cloned()
}

/// Returns the caller-visible signature used to normalize direct call operands.
fn call_signature(
    ctx: &LoweringContext<'_, '_>,
    name: &str,
) -> Option<FunctionSig> {
    if let Some(sig) = ctx.functions.get(name) {
        return Some(sig.clone());
    }
    if let Some(sig) = ctx.extern_functions.get(name) {
        return Some(function_sig_from_extern_for_descriptor(sig));
    }
    builtin_call_signature(name)
}

/// Looks up a PHP builtin call signature using the normalized global builtin name.
fn builtin_call_signature(name: &str) -> Option<FunctionSig> {
    crate::types::builtin_call_sig(&php_symbol_key(name.trim_start_matches('\\')))
}

/// Looks up precise first-class builtin metadata using the normalized global builtin name.
fn first_class_builtin_signature(name: &str) -> Option<FunctionSig> {
    crate::types::first_class_callable_builtin_sig(&php_symbol_key(name.trim_start_matches('\\')))
}

/// Lowers supported `unset(...)` targets without evaluating them as ordinary call args.
fn lower_unset_locals(
    ctx: &mut LoweringContext<'_, '_>,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if !args.iter().all(|arg| unset_target_supported(ctx, arg)) {
        return None;
    }
    let null = lower_null(ctx, expr);
    for arg in args {
        match &arg.kind {
            ExprKind::Variable(name) => {
                ctx.unset_local(name, null, Some(arg.span));
            }
            ExprKind::ArrayAccess { array, index } => {
                lower_unset_array_access(ctx, array, index, arg);
            }
            ExprKind::PropertyAccess { object, property }
            | ExprKind::NullsafePropertyAccess { object, property } => {
                lower_unset_property_access(ctx, object, property, arg);
            }
            _ => {}
        }
    }
    crate::ir_lower::ownership::collect_cycles(ctx, Some(expr.span));
    Some(null)
}

/// Returns true when an `unset(...)` target has direct EIR lowering.
fn unset_target_supported(ctx: &LoweringContext<'_, '_>, arg: &Expr) -> bool {
    match &arg.kind {
        ExprKind::Variable(_) => true,
        ExprKind::ArrayAccess { array, .. } => {
            unset_array_access_has_object_receiver(ctx, array)
                || unset_array_access_has_local_array_receiver(ctx, array)
        }
        ExprKind::PropertyAccess { object, property }
        | ExprKind::NullsafePropertyAccess { object, property } => {
            unset_property_access_has_direct_lowering(ctx, object, property)
        }
        _ => false,
    }
}

/// Returns true when an array-access unset receiver is a plain array/hash local whose element the
/// EIR backend can remove.
///
/// Associative arrays remove the element directly; packed indexed arrays are converted to a hash at
/// the unset site (PHP `unset()` leaves a sparse array). By-reference locals are excluded: their
/// storage is aliased to a caller whose static type would no longer match after a representation
/// change.
fn unset_array_access_has_local_array_receiver(
    ctx: &LoweringContext<'_, '_>,
    array: &Expr,
) -> bool {
    let ExprKind::Variable(name) = &array.kind else {
        return false;
    };
    if ctx.is_ref_bound_local(name) {
        return false;
    }
    matches!(
        ctx.local_type(name).codegen_repr(),
        PhpType::AssocArray { .. } | PhpType::Array(_)
    )
}

/// Returns true when an array-access unset receiver is a static ArrayAccess object.
fn unset_array_access_has_object_receiver(
    ctx: &LoweringContext<'_, '_>,
    array: &Expr,
) -> bool {
    let ty = match &array.kind {
        ExprKind::Variable(name) => ctx
            .local_types
            .get(name)
            .cloned()
            .unwrap_or_else(|| infer_expr_type_syntactic(array)),
        _ => infer_expr_type_syntactic(array),
    };
    type_satisfies_array_access_for_ir(ctx, &ty)
}

/// Lowers `unset($array[$key])`, dispatching on the receiver kind.
///
/// An associative-array local removes the element in place through `Op::HashUnset`. A packed
/// indexed-array local is first converted to a hash (PHP keeps the surviving keys without
/// renumbering) and then removed. An `ArrayAccess` object dispatches to its `offsetUnset($key)`
/// method like before. By-reference array locals fall through to the object path.
fn lower_unset_array_access(
    ctx: &mut LoweringContext<'_, '_>,
    array: &Expr,
    index: &Expr,
    expr: &Expr,
) {
    if let ExprKind::Variable(name) = &array.kind {
        if !ctx.is_ref_bound_local(name) {
            match ctx.local_type(name).codegen_repr() {
                PhpType::AssocArray { .. } => {
                    lower_unset_hash_element(ctx, name, array.span, index, expr);
                    return;
                }
                PhpType::Array(elem_ty) => {
                    let elem_ty = if *elem_ty == PhpType::Never {
                        PhpType::Mixed
                    } else {
                        *elem_ty
                    };
                    lower_unset_indexed_element(ctx, name, elem_ty, array.span, index, expr);
                    return;
                }
                _ => {}
            }
        }
    }
    let synthetic = Expr::new(
        ExprKind::MethodCall {
            object: Box::new(array.clone()),
            method: "offsetUnset".to_string(),
            args: vec![index.clone()],
        },
        expr.span,
    );
    lower_expr(ctx, &synthetic);
}

/// Lowers `unset($hash[$key])` for an associative-array local as a `HashUnset` instruction.
///
/// Loads the array local, lowers the key, and emits the removal. The backend (`lower_hash_unset`)
/// copy-on-write splits the table, releases the removed key/value payloads, and stores the unique
/// table pointer back into the local slot, so no explicit store-back is needed here.
fn lower_unset_hash_element(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    array_span: Span,
    index: &Expr,
    expr: &Expr,
) {
    let array_value = ctx.load_local(name, Some(array_span));
    let index_value = lower_expr(ctx, index);
    ctx.emit_void(
        Op::HashUnset,
        vec![array_value.value, index_value.value],
        None,
        Op::HashUnset.default_effects(),
        Some(expr.span),
    );
}

/// Lowers `unset($arr[$key])` for a packed indexed-array local.
///
/// PHP's `unset()` removes a key without renumbering, so the array can no longer be a contiguous
/// packed list (e.g. `unset([1,2,3][1])` leaves keys `0` and `2`). The local is converted to a hash
/// (`Op::ArrayToHash`) and retyped as `AssocArray<Int, T>`, after which the element is removed
/// through `HashUnset`. Subsequent uses of the local therefore see the associative representation.
fn lower_unset_indexed_element(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    elem_ty: PhpType,
    array_span: Span,
    index: &Expr,
    expr: &Expr,
) {
    let array_value = ctx.load_local(name, Some(array_span));
    let assoc_ty = PhpType::AssocArray {
        key: Box::new(PhpType::Int),
        value: Box::new(elem_ty),
    };
    let hash = ctx.emit_value(
        Op::ArrayToHash,
        vec![array_value.value],
        None,
        assoc_ty.clone(),
        Op::ArrayToHash.default_effects(),
        Some(array_span),
    );
    ctx.store_mutated_local(name, hash, assoc_ty, Some(array_span));
    lower_unset_hash_element(ctx, name, array_span, index, expr);
}

/// Returns true when a property unset target can be lowered without normal property storage support.
fn unset_property_access_has_direct_lowering(
    ctx: &LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
) -> bool {
    matches!(
        property_unset_action(ctx, object, property),
        Some(UnsetPropertyAction::Magic | UnsetPropertyAction::Noop)
    )
}

/// Lowers `unset($object->property)` for magic and no-op property targets.
fn lower_unset_property_access(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
    expr: &Expr,
) {
    match property_unset_action(ctx, object, property) {
        Some(UnsetPropertyAction::Magic) => {
            let object = lower_expr(ctx, object);
            lower_magic_property_unset(ctx, object, property, expr);
        }
        Some(UnsetPropertyAction::Noop) => {
            lower_expr(ctx, object);
        }
        Some(UnsetPropertyAction::Fallback) | None => {}
    }
}

/// Describes how `unset($object->property)` should be lowered for a known receiver class.
enum UnsetPropertyAction {
    Fallback,
    Magic,
    Noop,
}

/// Selects the PHP-visible `unset()` behavior for a statically known object property operand.
fn property_unset_action(
    ctx: &LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
) -> Option<UnsetPropertyAction> {
    let (class_name, _) = isset_object_expr_class(ctx, object)?;
    if is_builtin_stdclass_name(&class_name) {
        return Some(UnsetPropertyAction::Fallback);
    }
    let class_info = ctx.classes.get(class_name.as_str())?;
    if class_info.allow_dynamic_properties {
        return Some(UnsetPropertyAction::Fallback);
    }
    if property_is_accessible_for_ir(ctx, &class_name, class_info, property) {
        return Some(UnsetPropertyAction::Fallback);
    }
    if class_method_signature(ctx, &class_name, &php_symbol_key("__unset")).is_some() {
        Some(UnsetPropertyAction::Magic)
    } else {
        Some(UnsetPropertyAction::Noop)
    }
}

/// Lowers a magic `__unset($name)` call, guarding nullable receivers as a no-op.
fn lower_magic_property_unset(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    property: &str,
    expr: &Expr,
) {
    if value_is_nullable(ctx, object.value) {
        lower_nullable_magic_property_unset(ctx, object, property, expr);
        return;
    }
    let args = vec![Expr::new(
        ExprKind::StringLiteral(property.to_string()),
        expr.span,
    )];
    lower_method_call_with_receiver(ctx, object, "__unset", &args, Op::MethodCall, expr);
}

/// Lowers `__unset` for nullable receivers, doing nothing when the receiver is null.
fn lower_nullable_magic_property_unset(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    property: &str,
    expr: &Expr,
) {
    let null_block = ctx
        .builder
        .create_named_block("unset.property.null", Vec::new());
    let call_block = ctx
        .builder
        .create_named_block("unset.property.call", Vec::new());
    let merge = ctx
        .builder
        .create_named_block("unset.property.merge", Vec::new());
    let is_null = ctx.emit_value(
        Op::IsNull,
        vec![object.value],
        None,
        PhpType::Bool,
        Op::IsNull.default_effects(),
        Some(expr.span),
    );
    ctx.builder.terminate(Terminator::CondBr {
        cond: is_null.value,
        then_target: null_block,
        then_args: Vec::new(),
        else_target: call_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(null_block);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(call_block);
    let args = vec![Expr::new(
        ExprKind::StringLiteral(property.to_string()),
        expr.span,
    )];
    lower_method_call_with_receiver(ctx, object, "__unset", &args, Op::MethodCall, expr);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
}

/// Lowers `array_push($local, $value)` as a direct indexed-array mutation.
fn lower_static_array_push(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if php_symbol_key(name.trim_start_matches('\\')) != "array_push" || args.len() != 2 {
        return None;
    }
    if crate::types::call_args::has_named_args(args) || args.iter().any(is_spread_arg) {
        return None;
    }
    let ExprKind::Variable(array_name) = &args[0].kind else {
        return None;
    };
    if !matches!(ctx.local_type(array_name).codegen_repr(), PhpType::Array(_)) {
        return None;
    }
    let array_value = ctx.load_local(array_name, Some(args[0].span));
    if array_value.ir_type != IrType::Heap(IrHeapKind::Array) {
        return None;
    }
    let value = lower_expr(ctx, &args[1]);
    let (array_value, updated_ty, needs_storeback) =
        if super::stmt::ref_bound_mixed_indexed_array_write(ctx, array_name, value) {
            (array_value, Some(ctx.local_type(array_name)), true)
        } else {
            super::stmt::prepare_indexed_array_local_write(ctx, array_value, value, expr.span)
        };
    ctx.emit_void(
        Op::ArrayPush,
        vec![array_value.value, value.value],
        None,
        Op::ArrayPush.default_effects(),
        Some(expr.span),
    );
    let elem_ty =
        super::stmt::indexed_array_write_element_type(ctx, array_value, updated_ty.as_ref());
    super::stmt::finish_indexed_array_local_write(
        ctx,
        array_name,
        array_value,
        updated_ty,
        needs_storeback,
        expr.span,
    );
    super::stmt::release_indexed_array_write_operand(ctx, elem_ty.as_ref(), value, expr.span);
    Some(lower_null(ctx, expr))
}

/// Lowers builtin call operands, applying builtin-specific preservation where source order matters.
fn lower_builtin_call_args(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    sig: Option<&FunctionSig>,
    args: &[Expr],
) -> Vec<crate::ir::ValueId> {
    if is_empty_static_indexed_spread_arg(args) && zero_arity_call_signature(name, sig) {
        return Vec::new();
    }
    let canonical = php_symbol_key(name.trim_start_matches('\\'));
    if canonical == "eval" {
        return lower_eval_args(ctx, sig, args);
    }
    let argument_lowering = crate::builtins::registry::lookup(&canonical)
        .map(|def| def.spec.semantics.argument_lowering)
        .unwrap_or(crate::builtins::semantics::BuiltinArgumentLowering::Standard);
    match argument_lowering {
        crate::builtins::semantics::BuiltinArgumentLowering::Count => {
            lower_count_args(ctx, sig, args)
        }
        crate::builtins::semantics::BuiltinArgumentLowering::Date => {
            lower_date_args(ctx, sig, args)
        }
        crate::builtins::semantics::BuiltinArgumentLowering::JsonDecode => {
            lower_json_decode_args(ctx, sig, args)
        }
        crate::builtins::semantics::BuiltinArgumentLowering::PregReplaceCallback
            if !crate::types::call_args::has_named_args(args)
                && !args.iter().any(is_spread_arg) =>
        {
            lower_preg_replace_callback_args(ctx, sig, args)
        }
        crate::builtins::semantics::BuiltinArgumentLowering::PositionalRegex
            if !crate::types::call_args::has_named_args(args)
                && !args.iter().any(is_spread_arg) =>
        {
            lower_args(ctx, args)
        }
        crate::builtins::semantics::BuiltinArgumentLowering::UserValueSort
            if !crate::types::call_args::has_named_args(args)
                && !args.iter().any(is_spread_arg) =>
        {
            lower_user_value_sort_args(ctx, sig, args)
        }
        _ if !crate::types::call_args::has_named_args(args)
            && !args.iter().any(is_spread_arg) =>
        {
            lower_positional_builtin_args_with_signature(ctx, sig, args)
        }
        _ => lower_args_with_signature(ctx, sig, args),
    }
}

/// Lowers plain positional builtin operands without materializing omitted defaults or packing tails.
///
/// Runtime helpers consume the caller-provided arity, while the registry signature still supplies
/// by-reference handling and scalar storage coercions for every visible regular parameter.
fn lower_positional_builtin_args_with_signature(
    ctx: &mut LoweringContext<'_, '_>,
    sig: Option<&FunctionSig>,
    args: &[Expr],
) -> Vec<crate::ir::ValueId> {
    let Some(sig) = sig else {
        return lower_args(ctx, args);
    };
    let regular_param_count = crate::types::call_args::regular_param_count(sig);
    args.iter()
        .enumerate()
        .map(|(index, arg)| {
            if index < regular_param_count {
                lower_arg_with_signature(ctx, sig, index, arg)
            } else {
                lower_expr(ctx, arg).value
            }
        })
        .collect()
}

/// Lowers `count()` arguments, dropping a statically-default mode argument.
///
/// The EIR backend implements only `COUNT_NORMAL`; a literal `0` mode (named
/// or positional) is semantically a no-op and would otherwise trip the unary
/// count contract in codegen.
fn lower_count_args(
    ctx: &mut LoweringContext<'_, '_>,
    sig: Option<&FunctionSig>,
    args: &[Expr],
) -> Vec<crate::ir::ValueId> {
    let pruned: Vec<Expr> = args
        .iter()
        .enumerate()
        .filter(|(index, arg)| !count_arg_is_static_default_mode(*index, arg))
        .map(|(_, arg)| arg.clone())
        .collect();
    let mut operands = lower_args_with_signature(ctx, sig, &pruned);
    // Named and spread plans re-materialize the optional `mode` default even
    // after the AST prune; a trailing constant-zero mode stays a no-op for
    // the unary count contract, so drop the operand (DCE reclaims the const).
    if operands.len() == 2 {
        let trailing_zero_mode = ctx
            .builder
            .value_defining_instruction(operands[1])
            .is_some_and(|inst| {
                inst.op == Op::ConstI64
                    && matches!(inst.immediate, Some(crate::ir::Immediate::I64(0)))
            });
        if trailing_zero_mode {
            operands.pop();
        }
    }
    operands
}

/// Returns true when a `count()` argument is a statically-zero mode.
fn count_arg_is_static_default_mode(index: usize, arg: &Expr) -> bool {
    match &arg.kind {
        ExprKind::NamedArg { name, value } => {
            name == "mode" && matches!(value.kind, ExprKind::IntLiteral(0))
        }
        ExprKind::IntLiteral(0) => index == 1,
        _ => false,
    }
}

/// Lowers eval's code operand and coerces it through PHP string-conversion rules.
fn lower_eval_args(
    ctx: &mut LoweringContext<'_, '_>,
    sig: Option<&FunctionSig>,
    args: &[Expr],
) -> Vec<crate::ir::ValueId> {
    let operands = lower_args_with_signature(ctx, sig, args);
    let Some(code) = operands.first().copied() else {
        return operands;
    };
    let code_value = LoweredValue {
        value: code,
        ir_type: ctx.builder.value_type(code),
    };
    let span = args.first().map(|arg| arg.span);
    vec![coerce_to_string_at_span(ctx, code_value, span).value]
}

/// Lowers `usort`/`uasort` arguments, typing an unannotated comparator closure
/// against the array's object element type.
///
/// `usort`/`uasort` compare values, so a comparator over an array of objects must
/// see each element as the object handle — for `<=>` instant comparison and for
/// property/method access — not the raw pointer-sized integer the runtime stores
/// in each slot. The array operand is lowered exactly as the default positional
/// path would (positional builtin calls reach here with no signature); only an
/// unannotated closure comparator over an object-element array is specialized,
/// matching the element-type hint the checker applied to the comparator body.
fn lower_user_value_sort_args(
    ctx: &mut LoweringContext<'_, '_>,
    sig: Option<&FunctionSig>,
    args: &[Expr],
) -> Vec<crate::ir::ValueId> {
    if args.len() != 2 || !matches!(&args[1].kind, ExprKind::Closure { .. }) {
        return lower_args_with_signature(ctx, sig, args);
    }
    // The mutating sort keeps its by-reference local storeback in the EIR backend,
    // so the array operand only has to resolve to the array's value here.
    let array = match sig {
        Some(sig) => lower_arg_with_signature(ctx, sig, 0, &args[0]),
        None => lower_expr(ctx, &args[0]).value,
    };
    let elem_ty = match ctx.builder.value_php_type(array).codegen_repr() {
        PhpType::Array(elem) => elem.codegen_repr(),
        _ => PhpType::Int,
    };
    // Only an object-element array needs the comparator parameters re-typed; scalar
    // comparators already lower correctly through the default path.
    let callback = if matches!(elem_ty, PhpType::Object(_)) {
        lower_value_sort_comparator_closure(ctx, &args[1], elem_ty)
    } else {
        match sig {
            Some(sig) => lower_arg_with_signature(ctx, sig, 1, &args[1]),
            None => lower_expr(ctx, &args[1]).value,
        }
    };
    vec![array, callback]
}

/// Lowers a value-sort comparator closure with both parameters typed as the array element.
///
/// Falls back to the plain closure lowering for any non-closure callback operand,
/// though callers only reach this path with a closure comparator.
fn lower_value_sort_comparator_closure(
    ctx: &mut LoweringContext<'_, '_>,
    callback: &Expr,
    elem_ty: PhpType,
) -> crate::ir::ValueId {
    let ExprKind::Closure {
        params,
        variadic,
        variadic_by_ref,
        return_type,
        body,
        captures,
        capture_refs,
        is_static,
        ..
    } = &callback.kind
    else {
        return lower_expr(ctx, callback).value;
    };
    lower_closure_with_context(
        ctx,
        params,
        variadic.as_deref(),
        *variadic_by_ref,
        return_type.as_ref(),
        body,
        captures,
        capture_refs,
        callback,
        &[elem_ty.clone(), elem_ty],
        None,
        *is_static,
    )
    .value
}

/// Returns true when the call uses exactly one static empty indexed spread.
fn is_empty_static_indexed_spread_arg(args: &[Expr]) -> bool {
    let [arg] = args else {
        return false;
    };
    let ExprKind::Spread(inner) = &arg.kind else {
        return false;
    };
    matches!(&inner.kind, ExprKind::ArrayLiteral(items) if items.is_empty())
}

/// Returns true when the callable signature accepts no visible operands.
fn zero_arity_call_signature(name: &str, sig: Option<&FunctionSig>) -> bool {
    if let Some(sig) = sig {
        return is_zero_arity_signature(sig);
    }
    builtin_call_signature(name)
        .as_ref()
        .is_some_and(is_zero_arity_signature)
}

/// Returns true when a signature has no regular or variadic parameters.
fn is_zero_arity_signature(sig: &FunctionSig) -> bool {
    crate::types::call_args::regular_param_count(sig) == 0 && sig.variadic.is_none()
}

/// Lowers `settype($local, "type")` and updates subsequent local type facts.
fn lower_static_settype(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if php_symbol_key(name.trim_start_matches('\\')) != "settype" {
        return None;
    }
    let (var_arg, type_arg) = static_settype_arg_exprs(ctx, name, args)?;
    let ExprKind::Variable(local_name) = &var_arg.kind else {
        return None;
    };
    let target_ty = static_settype_target_type(&type_arg)?;
    let sig = call_signature(ctx, name);
    let operands = lower_builtin_call_args(ctx, name, sig.as_ref(), args);
    let result = emit_builtin_call_value(ctx, name, operands, PhpType::Bool, expr.span, None);
    ctx.set_local_type(local_name, target_ty);
    Some(result)
}

/// Returns canonical `settype()` argument expressions for static local mutation lowering.
fn static_settype_arg_exprs(
    ctx: &LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
) -> Option<(Expr, Expr)> {
    if args.len() != 2 || args.iter().any(is_spread_arg) {
        return None;
    }
    if !crate::types::call_args::has_named_args(args) {
        return Some((args[0].clone(), args[1].clone()));
    }
    let sig = call_signature(ctx, name)?;
    let call_span = args
        .first()
        .map(|arg| arg.span)
        .unwrap_or_else(crate::span::Span::dummy);
    let regular_param_count = crate::types::call_args::regular_param_count(&sig);
    let plan = crate::types::call_args::plan_call_args_with_regular_param_count_and_assoc_spreads(
        &sig,
        args,
        call_span,
        regular_param_count,
        false,
        true,
        &assoc_spread_sources(ctx, args),
    )
    .ok()?;
    if plan.has_spread_args() || plan.regular_args.len() != 2 {
        return None;
    }
    let var_arg = planned_regular_arg_expr(&plan.regular_args[0])?.clone();
    let type_arg = planned_regular_arg_expr(&plan.regular_args[1])?.clone();
    Some((var_arg, type_arg))
}

/// Returns the source expression assigned to a planned regular parameter.
fn planned_regular_arg_expr(
    arg: &crate::types::call_args::PlannedRegularArg,
) -> Option<&Expr> {
    match arg {
        crate::types::call_args::PlannedRegularArg::Source { expr, .. } => Some(expr),
        crate::types::call_args::PlannedRegularArg::Default(_)
        | crate::types::call_args::PlannedRegularArg::SpreadElement { .. } => None,
    }
}

/// Returns the PHP type named by a literal `settype()` second argument.
fn static_settype_target_type(arg: &Expr) -> Option<PhpType> {
    let ExprKind::StringLiteral(name) = &arg.kind else {
        return None;
    };
    match php_symbol_key(name).as_str() {
        "int" | "integer" => Some(PhpType::Int),
        "float" | "double" => Some(PhpType::Float),
        "string" => Some(PhpType::Str),
        "bool" | "boolean" => Some(PhpType::Bool),
        _ => None,
    }
}

/// Lowers static function callbacks for `preg_replace_callback()`.
fn lower_preg_replace_callback_args(
    ctx: &mut LoweringContext<'_, '_>,
    sig: Option<&FunctionSig>,
    args: &[Expr],
) -> Vec<crate::ir::ValueId> {
    if args.len() != 3 {
        return lower_args_with_signature(ctx, sig, args);
    }
    if matches!(&args[1].kind, ExprKind::Closure { .. }) {
        let pattern = lower_expr(ctx, &args[0]);
        let callback = lower_preg_replace_callback_closure(ctx, &args[1])
            .expect("preg_replace_callback closure check must match lowering");
        let subject = lower_expr(ctx, &args[2]);
        let subject = persist_call_arg_if_string(ctx, subject, args[2].span);
        return vec![pattern.value, callback.value, subject.value];
    }
    let Some(callback) = preg_replace_static_callback(ctx, &args[1]) else {
        return lower_args_with_signature(ctx, sig, args);
    };
    let pattern = lower_expr(ctx, &args[0]);
    let callback = lower_string_literal(ctx, &callback, &args[1]);
    let subject = lower_expr(ctx, &args[2]);
    let subject = persist_call_arg_if_string(ctx, subject, args[2].span);
    vec![pattern.value, callback.value, subject.value]
}

/// Lowers a `preg_replace_callback()` closure with match-array parameter context.
fn lower_preg_replace_callback_closure(
    ctx: &mut LoweringContext<'_, '_>,
    callback: &Expr,
) -> Option<LoweredValue> {
    let ExprKind::Closure {
        params,
        variadic,
        variadic_by_ref,
        return_type,
        body,
        captures,
        capture_refs,
        is_static,
        ..
    } = &callback.kind
    else {
        return None;
    };
    Some(lower_closure_with_context(
        ctx,
        params,
        variadic.as_deref(),
        *variadic_by_ref,
        return_type.as_ref(),
        body,
        captures,
        capture_refs,
        callback,
        &[PhpType::Array(Box::new(PhpType::Str))],
        None,
        *is_static,
    ))
}

/// Returns the userland callback name accepted by the current regex runtime helper.
fn preg_replace_static_callback(
    ctx: &LoweringContext<'_, '_>,
    callback: &Expr,
) -> Option<String> {
    match &callback.kind {
        ExprKind::FirstClassCallable(CallableTarget::Function(name)) => {
            Some(name.as_str().to_string())
        }
        ExprKind::Variable(name) => match ctx.static_callable_local(name)? {
            StaticCallableBinding::UserFunction(function_name) => Some(function_name),
            _ => None,
        },
        _ => None,
    }
}

/// Lowers simple positional `date` operands while stabilizing the format string before timestamp evaluation.
fn lower_date_args(
    ctx: &mut LoweringContext<'_, '_>,
    sig: Option<&FunctionSig>,
    args: &[Expr],
) -> Vec<crate::ir::ValueId> {
    if args.len() != 2
        || crate::types::call_args::has_named_args(args)
        || args.iter().any(is_spread_arg)
    {
        return lower_args_with_signature(ctx, sig, args);
    }
    let format = lower_expr(ctx, &args[0]);
    let format = persist_call_arg_if_string(ctx, format, args[0].span);
    vec![format.value, lower_expr(ctx, &args[1]).value]
}

/// Lowers simple positional `json_decode` operands while stabilizing string sources early.
fn lower_json_decode_args(
    ctx: &mut LoweringContext<'_, '_>,
    sig: Option<&FunctionSig>,
    args: &[Expr],
) -> Vec<crate::ir::ValueId> {
    if args.is_empty()
        || crate::types::call_args::has_named_args(args)
        || args.iter().any(is_spread_arg)
    {
        return lower_args_with_signature(ctx, sig, args);
    }
    let source = lower_expr(ctx, &args[0]);
    let source = persist_call_arg_if_string(ctx, source, args[0].span);
    let mut operands = Vec::with_capacity(args.len());
    operands.push(source.value);
    for arg in &args[1..] {
        operands.push(lower_expr(ctx, arg).value);
    }
    operands
}

/// Emits `StrPersist` for already-string call operands before later arguments can reuse string scratch storage.
fn persist_call_arg_if_string(
    ctx: &mut LoweringContext<'_, '_>,
    source: LoweredValue,
    span: crate::span::Span,
) -> LoweredValue {
    if source.ir_type != IrType::Str {
        return source;
    }
    ctx.emit_value(
        Op::StrPersist,
        vec![source.value],
        None,
        PhpType::Str,
        Op::StrPersist.default_effects(),
        Some(span),
    )
}

/// Lowers positional/named/spread call arguments in source order.
fn lower_args(ctx: &mut LoweringContext<'_, '_>, args: &[Expr]) -> Vec<crate::ir::ValueId> {
    args.iter().map(|arg| lower_expr(ctx, arg).value).collect()
}

/// Lowers one argument while applying by-reference storage normalization from a signature.
fn lower_arg_with_signature(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    arg: &Expr,
) -> crate::ir::ValueId {
    if let Some(value) = lower_by_ref_array_element_arg_with_signature(ctx, sig, index, arg) {
        return value;
    }
    if let Some(value) = lower_by_ref_array_arg_with_signature(ctx, sig, index, arg) {
        return value;
    }
    let lowered = lower_expr(ctx, arg);
    coerce_scalar_arg_to_param_storage(ctx, sig, index, lowered, arg).value
}

/// Coerces a positional argument to storage owned explicitly by EIR when required.
///
/// Integer-to-float conversion selects the callee's floating-point ABI class. Mixed-to-string
/// conversion is also explicit here because it allocates caller-owned storage whose lifetime
/// depends on the call's return/argument alias contract; leaving that conversion hidden in ABI
/// materialization would give EIR no value to transfer or release after the call.
fn coerce_scalar_arg_to_param_storage(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    value: LoweredValue,
    arg: &Expr,
) -> LoweredValue {
    let Some((_, param_ty)) = sig.params.get(index) else {
        return value;
    };
    let param_ty = param_ty.codegen_repr();
    if value.ir_type == IrType::I64 && param_ty == PhpType::Float {
        return coerce_to_float(ctx, value, arg);
    }
    let source_ty = ctx.builder.value_php_type(value.value).codegen_repr();
    if param_ty == PhpType::Str && matches!(source_ty, PhpType::Mixed | PhpType::Union(_)) {
        return coerce_to_string(ctx, value, arg);
    }
    value
}

/// Normalizes reordered call operands to their declared scalar parameter storage.
///
/// Named and spread arguments are evaluated in source order and then reordered, so their
/// int-to-float and Mixed-to-string conversions happen here in parameter order. By-reference
/// parameters and the variadic tail remain untouched. String conversions become owned EIR
/// values so normal alias-aware call cleanup can transfer or release them safely.
fn coerce_operands_to_params(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    mut operands: Vec<crate::ir::ValueId>,
) -> Vec<crate::ir::ValueId> {
    let regular_param_count = crate::types::call_args::regular_param_count(sig);
    let limit = operands.len().min(regular_param_count);
    for index in 0..limit {
        if sig.ref_params.get(index).copied().unwrap_or(false) {
            continue;
        }
        let Some((_, param_ty)) = sig.params.get(index) else {
            continue;
        };
        let value = operands[index];
        let operand_ty = ctx.builder.value_php_type(value).codegen_repr();
        let param_ty = param_ty.codegen_repr();
        if param_ty == PhpType::Float && matches!(operand_ty, PhpType::Int | PhpType::Bool) {
            let lowered = LoweredValue {
                value,
                ir_type: IrType::I64,
            };
            operands[index] = coerce_to_float_at_span(ctx, lowered, None).value;
        } else if param_ty == PhpType::Str
            && matches!(operand_ty, PhpType::Mixed | PhpType::Union(_))
        {
            let lowered = LoweredValue {
                value,
                ir_type: ctx.builder.value_type(value),
            };
            operands[index] = coerce_to_string_at_span(ctx, lowered, None).value;
        }
    }
    operands
}

/// Widens local indexed-array storage before passing it to an `array<mixed>` ref parameter.
fn lower_by_ref_array_arg_with_signature(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    arg: &Expr,
) -> Option<crate::ir::ValueId> {
    if !sig.ref_params.get(index).copied().unwrap_or(false) {
        return None;
    }
    let (_, param_ty) = sig.params.get(index)?;
    let ExprKind::Variable(name) = &arg.kind else {
        return None;
    };
    if !by_ref_array_arg_needs_mixed_storage(ctx, name, param_ty) {
        return None;
    }
    let array_ty = PhpType::Array(Box::new(PhpType::Mixed));
    let local = ctx.load_local(name, Some(arg.span));
    let converted = ctx.emit_value(
        Op::ArrayToMixed,
        vec![local.value],
        None,
        array_ty.clone(),
        Op::ArrayToMixed.default_effects(),
        Some(arg.span),
    );
    ctx.store_mutated_local(name, converted, array_ty, Some(arg.span));
    Some(ctx.load_local(name, Some(arg.span)).value)
}

/// Lowers `$array[$index]` as a direct by-reference argument cell address.
fn lower_by_ref_array_element_arg_with_signature(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    arg: &Expr,
) -> Option<crate::ir::ValueId> {
    if !sig.ref_params.get(index).copied().unwrap_or(false) {
        return None;
    }
    let ExprKind::ArrayAccess { array, index: element_index } = &arg.kind else {
        return None;
    };
    let ExprKind::Variable(array_name) = &array.kind else {
        return None;
    };
    let PhpType::Array(elem_ty) = ctx.local_type(array_name).codegen_repr() else {
        return None;
    };
    let (_, param_ty) = sig.params.get(index)?;
    let element_ty = match normalize_value_php_type(*elem_ty) {
        PhpType::Void => normalize_value_php_type(param_ty.codegen_repr()),
        other => other,
    };
    let array_value = ctx.load_local(array_name, Some(array.span));
    let element_index = lower_expr(ctx, element_index);
    let element_index = coerce_to_int_at_span(ctx, element_index, Some(arg.span));
    let value = ctx
        .builder
        .emit_with_effects(
            Op::ArrayElemAddr,
            vec![array_value.value, element_index.value],
            None,
            IrType::I64,
            element_ty,
            Ownership::NonHeap,
            Op::ArrayElemAddr.default_effects(),
            Some(arg.span),
        )
        .expect("array_elem_addr produces a value");
    Some(value)
}

/// Returns true when a local array must be converted before a by-reference call.
fn by_ref_array_arg_needs_mixed_storage(
    ctx: &LoweringContext<'_, '_>,
    name: &str,
    param_ty: &PhpType,
) -> bool {
    let PhpType::Array(param_elem) = param_ty.codegen_repr() else {
        return false;
    };
    if param_elem.codegen_repr() != PhpType::Mixed {
        return false;
    }
    let PhpType::Array(local_elem) = ctx.local_type(name).codegen_repr() else {
        return false;
    };
    local_elem.codegen_repr() != PhpType::Mixed
}

/// Lowers positional call arguments with omitted optional defaults and variadic tail packing.
fn lower_args_with_signature(
    ctx: &mut LoweringContext<'_, '_>,
    sig: Option<&FunctionSig>,
    args: &[Expr],
) -> Vec<crate::ir::ValueId> {
    let Some(sig) = sig else {
        return lower_args(ctx, args);
    };
    if crate::types::call_args::has_named_args(args) {
        let operands = lower_named_args_with_signature(ctx, sig, args);
        return coerce_operands_to_params(ctx, sig, operands);
    }
    if let Some(operands) = lower_positional_spread_args_with_signature(ctx, sig, args) {
        return coerce_operands_to_params(ctx, sig, operands);
    }
    let static_spread_args = if has_static_call_spread_args(args) {
        Some(expand_static_call_spread_args(args))
    } else {
        None
    };
    let args = static_spread_args.as_deref().unwrap_or(args);
    if let Some(operands) = lower_assoc_spread_only_args(ctx, sig, args) {
        return coerce_operands_to_params(ctx, sig, operands);
    }
    if args.iter().any(is_spread_arg) {
        return lower_args(ctx, args);
    }
    let regular_param_count = crate::types::call_args::regular_param_count(sig);
    let fixed_arg_count = if sig.variadic.is_some() {
        args.len().min(regular_param_count)
    } else {
        args.len()
    };
    if sig.variadic.is_none() && fixed_arg_count >= regular_param_count {
        let operands = args
            .iter()
            .enumerate()
            .map(|(index, arg)| lower_arg_with_signature(ctx, sig, index, arg))
            .collect();
        return coerce_operands_to_params(ctx, sig, operands);
    }
    let mut operands: Vec<crate::ir::ValueId> = args[..fixed_arg_count]
        .iter()
        .enumerate()
        .map(|(index, arg)| lower_arg_with_signature(ctx, sig, index, arg))
        .collect();
    for idx in fixed_arg_count..regular_param_count {
        let Some(Some(default)) = sig.defaults.get(idx) else {
            break;
        };
        operands.push(lower_expr(ctx, default).value);
    }
    if sig.variadic.is_some() {
        let tail = if args.len() > regular_param_count {
            &args[regular_param_count..]
        } else {
            &[]
        };
        operands.push(lower_variadic_tail_array(ctx, sig, tail).value);
    }
    coerce_operands_to_params(ctx, sig, operands)
}

/// Lowers one trailing indexed spread in a fixed-arity positional call.
fn lower_positional_spread_args_with_signature(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    args: &[Expr],
) -> Option<Vec<crate::ir::ValueId>> {
    if sig.variadic.is_some() {
        return None;
    }
    let spread_idx = single_trailing_indexed_spread_arg(ctx, args)?;
    let regular_param_count = crate::types::call_args::regular_param_count(sig);
    if spread_idx > regular_param_count {
        return None;
    }
    let first_spread_param_idx = spread_idx;
    let required_len = required_positional_spread_len(sig, first_spread_param_idx, regular_param_count);
    let ExprKind::Spread(inner) = &args[spread_idx].kind else {
        return None;
    };
    if static_indexed_spread_len(inner).is_some_and(|len| len >= required_len) {
        return None;
    }

    let mut operands = Vec::with_capacity(regular_param_count);
    for (index, arg) in args[..spread_idx].iter().enumerate() {
        operands.push(lower_arg_with_signature(ctx, sig, index, arg));
    }

    let spread_type = indexed_spread_source_type(ctx, inner)?;
    let spread = lower_expr(ctx, inner);
    let temp_name = ctx.declare_hidden_temp(spread_type.clone());
    store_value_into_temp(ctx, &temp_name, spread_type, spread, args[spread_idx].span);
    let spread_expr = Expr::new(ExprKind::Variable(temp_name), inner.span);
    let spread_value = lower_expr(ctx, &spread_expr);
    emit_positional_spread_min_len_guard(
        ctx,
        spread_value.value,
        required_len,
        args[spread_idx].span,
    );

    for param_idx in first_spread_param_idx..regular_param_count {
        let element_idx = param_idx - first_spread_param_idx;
        let default = sig.defaults.get(param_idx).and_then(|default| default.as_ref());
        let expr = if let Some(default) = default {
            if element_idx < required_len {
                spread_element_expr_for_ir(
                    &spread_expr,
                    element_idx,
                    None,
                    false,
                    args[spread_idx].span,
                )
            } else {
                spread_element_or_default_expr_for_ir(
                    &spread_expr,
                    element_idx,
                    None,
                    false,
                    default.clone(),
                    args[spread_idx].span,
                )
            }
        } else {
            spread_element_expr_for_ir(
                &spread_expr,
                element_idx,
                None,
                false,
                args[spread_idx].span,
            )
        };
        operands.push(lower_expr(ctx, &expr).value);
    }

    Some(operands)
}

/// Returns the element count for a statically-known indexed spread source.
fn static_indexed_spread_len(expr: &Expr) -> Option<usize> {
    match &expr.kind {
        ExprKind::ArrayLiteral(items) => Some(items.len()),
        _ => None,
    }
}

/// Returns the index of a single trailing positional spread that EIR can materialize.
fn single_trailing_indexed_spread_arg(
    ctx: &LoweringContext<'_, '_>,
    args: &[Expr],
) -> Option<usize> {
    let spread_indices = args
        .iter()
        .enumerate()
        .filter_map(|(idx, arg)| matches!(arg.kind, ExprKind::Spread(_)).then_some(idx))
        .collect::<Vec<_>>();
    let [spread_idx] = spread_indices.as_slice() else {
        return None;
    };
    if *spread_idx + 1 != args.len() {
        return None;
    }
    let ExprKind::Spread(inner) = &args[*spread_idx].kind else {
        return None;
    };
    indexed_spread_source_type(ctx, inner)?;
    Some(*spread_idx)
}

/// Returns the indexed-array source type for spread-only EIR lowering.
fn indexed_spread_source_type(
    ctx: &LoweringContext<'_, '_>,
    expr: &Expr,
) -> Option<PhpType> {
    let ty = match &expr.kind {
        ExprKind::Variable(name) => ctx.local_type(name),
        ExprKind::ArrayLiteral(items) => array_literal_type_for_ir(ctx, items, expr),
        _ => infer_expr_type_syntactic(expr),
    }
    .codegen_repr();
    if matches!(ty, PhpType::Array(_)) {
        Some(ty)
    } else {
        None
    }
}

/// Returns how many spread elements must exist to satisfy required parameters.
fn required_positional_spread_len(
    sig: &FunctionSig,
    start_param_idx: usize,
    regular_param_count: usize,
) -> usize {
    (start_param_idx..regular_param_count)
        .rfind(|idx| sig.defaults.get(*idx).and_then(|default| default.as_ref()).is_none())
        .map(|idx| idx - start_param_idx + 1)
        .unwrap_or(0)
}

/// Emits a fatal guard when a positional spread is shorter than required parameters.
fn emit_positional_spread_min_len_guard(
    ctx: &mut LoweringContext<'_, '_>,
    spread: crate::ir::ValueId,
    min_len: usize,
    span: crate::span::Span,
) {
    if min_len == 0 {
        return;
    }
    let len = ctx.emit_value(
        Op::ArrayLen,
        vec![spread],
        None,
        PhpType::Int,
        Op::ArrayLen.default_effects(),
        Some(span),
    );
    let min = emit_i64_at_span(ctx, min_len as i64, span);
    let has_required_args = ctx.emit_value(
        Op::ICmp,
        vec![len.value, min.value],
        Some(Immediate::CmpPredicate(CmpPredicate::Sge)),
        PhpType::Bool,
        Op::ICmp.default_effects(),
        Some(span),
    );
    let ok = ctx.builder.create_named_block("call.spread.len.ok", Vec::new());
    let fatal = ctx.builder.create_named_block("call.spread.len.fatal", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: has_required_args.value,
        then_target: ok,
        then_args: Vec::new(),
        else_target: fatal,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(fatal);
    let message = ctx.intern_string("Fatal error: too few arguments for spread call\n");
    ctx.builder.terminate(Terminator::Fatal { message });

    ctx.builder.position_at_end(ok);
}

/// Lowers named arguments in source order, then returns operands in signature order.
fn lower_named_args_with_signature(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    args: &[Expr],
) -> Vec<crate::ir::ValueId> {
    let call_span = args
        .first()
        .map(|arg| arg.span)
        .unwrap_or_else(crate::span::Span::dummy);
    let assoc_spread_sources = assoc_spread_sources(ctx, args);
    let regular_param_count = crate::types::call_args::regular_param_count(sig);
    let Ok(plan) = crate::types::call_args::plan_call_args_with_regular_param_count_and_assoc_spreads(
        sig,
        args,
        call_span,
        regular_param_count,
        false,
        true,
        &assoc_spread_sources,
    ) else {
        return lower_args(ctx, args);
    };
    if plan.has_spread_args() {
        if let Some(operands) = lower_named_args_with_spread_plan(ctx, sig, &plan, &assoc_spread_sources) {
            return operands;
        }
        if let Some(operands) = lower_dynamic_named_spread_variadic_args(ctx, sig, &plan) {
            return operands;
        }
        let normalized = plan.normalized_args();
        return lower_args(ctx, &normalized);
    }
    let mut source_values = Vec::with_capacity(plan.source_args.len());
    for source_arg in &plan.source_args {
        source_values.push(lower_call_source_arg(ctx, source_arg));
    }

    let mut operands = Vec::with_capacity(plan.regular_args.len() + usize::from(sig.variadic.is_some()));
    for arg in &plan.regular_args {
        match arg {
            crate::types::call_args::PlannedRegularArg::Source { source_index, .. } => {
                operands.push(source_values[*source_index]);
            }
            crate::types::call_args::PlannedRegularArg::Default(default) => {
                operands.push(lower_expr(ctx, default).value);
            }
            crate::types::call_args::PlannedRegularArg::SpreadElement { .. } => {
                return lower_args(ctx, args);
            }
        }
    }
    if sig.variadic.is_some() {
        operands.push(lower_named_variadic_tail_array(ctx, sig, &plan.source_values, &source_values).value);
    }
    operands
}

/// Lowers dynamic associative prefix spreads for variadic calls far enough to preserve duplicate fatals.
fn lower_dynamic_named_spread_variadic_args(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    plan: &crate::types::call_args::CallArgPlan,
) -> Option<Vec<crate::ir::ValueId>> {
    if sig.variadic.is_none() || !plan.prefix_has_dynamic_named_spread {
        return None;
    }
    let call_span = plan
        .source_args
        .first()
        .map(|arg| arg.span)
        .unwrap_or_else(crate::span::Span::dummy);
    let first_named_pos = plan.first_named_pos?;
    let prefix_expr = plan.positional_prefix_expr(call_span)?;
    let prefix = lower_expr(ctx, &prefix_expr);
    if !matches!(ctx.builder.value_php_type(prefix.value).codegen_repr(), PhpType::AssocArray { .. }) {
        return None;
    }
    let prefix_type = ctx.builder.value_php_type(prefix.value);
    let prefix_temp_name = ctx.declare_hidden_temp(prefix_type.clone());
    store_value_into_temp(ctx, &prefix_temp_name, prefix_type, prefix, prefix_expr.span);
    let prefix_temp = Expr::new(ExprKind::Variable(prefix_temp_name), prefix_expr.span);

    let mut source_values = vec![None; plan.source_args.len()];
    for (source_index, source_arg) in plan.source_args.iter().enumerate().skip(first_named_pos) {
        if matches!(source_arg.kind, ExprKind::Spread(_)) {
            return None;
        }
        source_values[source_index] = Some(lower_call_source_arg(ctx, source_arg));
    }
    emit_dynamic_named_prefix_duplicate_guards(ctx, sig, plan, &prefix_temp, first_named_pos);

    let mut operands = Vec::with_capacity(plan.regular_args.len() + 1);
    for arg in &plan.regular_args {
        match arg {
            crate::types::call_args::PlannedRegularArg::Source { source_index, .. } => {
                operands.push(source_values.get(*source_index).copied().flatten()?);
            }
            crate::types::call_args::PlannedRegularArg::Default(default) => {
                operands.push(lower_expr(ctx, default).value);
            }
            crate::types::call_args::PlannedRegularArg::SpreadElement {
                prefix_element_idx,
                param_name,
                prefer_named_key,
                default,
                guaranteed_present,
                spread_span,
                ..
            } => {
                let expr = if let Some(default) = default {
                    if *guaranteed_present {
                        spread_element_expr_for_ir(
                            &prefix_temp,
                            *prefix_element_idx,
                            param_name.as_deref(),
                            *prefer_named_key,
                            *spread_span,
                        )
                    } else {
                        spread_element_or_default_expr_for_ir(
                            &prefix_temp,
                            *prefix_element_idx,
                            param_name.as_deref(),
                            *prefer_named_key,
                            default.clone(),
                            *spread_span,
                        )
                    }
                } else {
                    spread_element_expr_for_ir(
                        &prefix_temp,
                        *prefix_element_idx,
                        param_name.as_deref(),
                        *prefer_named_key,
                        *spread_span,
                    )
                };
                operands.push(lower_expr(ctx, &expr).value);
            }
        }
    }
    operands.push(lower_variadic_tail_array(ctx, sig, &[]).value);
    Some(operands)
}

/// Emits duplicate checks for numeric prefix keys overwritten by later named parameters.
fn emit_dynamic_named_prefix_duplicate_guards(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    plan: &crate::types::call_args::CallArgPlan,
    prefix_temp: &Expr,
    first_named_pos: usize,
) {
    for source in &plan.source_values {
        if source.source_index() < first_named_pos {
            continue;
        }
        let Some(param_idx) = source.param_idx() else {
            continue;
        };
        let Some((param_name, _)) = sig.params.get(param_idx) else {
            continue;
        };
        emit_dynamic_named_prefix_duplicate_guard(
            ctx,
            prefix_temp,
            param_idx,
            param_name,
            source.expr().span,
        );
    }
}

/// Emits one duplicate guard for a numeric key in a dynamic associative prefix.
fn emit_dynamic_named_prefix_duplicate_guard(
    ctx: &mut LoweringContext<'_, '_>,
    prefix_temp: &Expr,
    param_idx: usize,
    param_name: &str,
    span: crate::span::Span,
) {
    let exists_expr = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("array_key_exists"),
            args: vec![
                Expr::new(ExprKind::IntLiteral(param_idx as i64), span),
                prefix_temp.clone(),
            ],
        },
        span,
    );
    let exists = lower_expr(ctx, &exists_expr);
    let ok = ctx.builder.create_named_block("call.dynamic_named_prefix.ok", Vec::new());
    let fatal = ctx.builder.create_named_block("call.dynamic_named_prefix.fatal", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: exists.value,
        then_target: fatal,
        then_args: Vec::new(),
        else_target: ok,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(fatal);
    let message = format!(
        "Fatal error: Named parameter ${} overwrites previous argument\n",
        param_name
    );
    let message = ctx.intern_string(&message);
    ctx.builder.terminate(Terminator::Fatal { message });

    ctx.builder.position_at_end(ok);
}

/// Lowers named/spread argument plans without re-evaluating dynamic spread expressions.
fn lower_named_args_with_spread_plan(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    plan: &crate::types::call_args::CallArgPlan,
    assoc_spread_sources: &[bool],
) -> Option<Vec<crate::ir::ValueId>> {
    if assoc_spread_sources.iter().any(|is_assoc| *is_assoc) {
        return None;
    }
    let call_span = plan
        .source_args
        .first()
        .map(|arg| arg.span)
        .unwrap_or_else(crate::span::Span::dummy);
    let first_named_pos = plan.first_named_pos?;
    let prefix_expr = plan.positional_prefix_expr(call_span)?;
    let static_variadic_prefix_len = static_indexed_variadic_prefix_len(&prefix_expr);
    if sig.variadic.is_some() && static_variadic_prefix_len.is_none() {
        return None;
    }
    let prefix = lower_expr(ctx, &prefix_expr);
    let prefix_type = ctx.builder.value_php_type(prefix.value);
    let prefix_temp_name = ctx.declare_hidden_temp(prefix_type.clone());
    store_value_into_temp(ctx, &prefix_temp_name, prefix_type, prefix, prefix_expr.span);
    let single_prefix_spread = !matches!(prefix_expr.kind, ExprKind::ArrayLiteral(_));
    let prefix_temp = Expr::new(ExprKind::Variable(prefix_temp_name), prefix_expr.span);

    let mut source_values = vec![None; plan.source_args.len()];
    for (source_index, source_arg) in plan.source_args.iter().enumerate().skip(first_named_pos) {
        if matches!(source_arg.kind, ExprKind::Spread(_)) {
            return None;
        }
        source_values[source_index] = Some(lower_call_source_arg(ctx, source_arg));
    }
    if single_prefix_spread {
        if let [check] = plan.spread_bounds_checks.as_slice() {
            let prefix_value = lower_expr(ctx, &prefix_temp);
            emit_named_spread_bounds_guard(ctx, prefix_value.value, check, call_span);
        }
    }

    let mut operands = Vec::with_capacity(plan.regular_args.len());
    for (param_idx, arg) in plan.regular_args.iter().enumerate() {
        match arg {
            crate::types::call_args::PlannedRegularArg::Source { source_index, .. } => {
                if *source_index < first_named_pos {
                    let expr = spread_element_expr_for_ir(
                        &prefix_temp,
                        param_idx,
                        None,
                        false,
                        plan.source_args.get(*source_index).map(|arg| arg.span).unwrap_or(call_span),
                    );
                    operands.push(lower_expr(ctx, &expr).value);
                } else {
                    operands.push(source_values.get(*source_index).copied().flatten()?);
                }
            }
            crate::types::call_args::PlannedRegularArg::Default(default) => {
                operands.push(lower_expr(ctx, default).value);
            }
            crate::types::call_args::PlannedRegularArg::SpreadElement {
                element_idx: _,
                prefix_element_idx,
                param_name,
                prefer_named_key,
                default,
                guaranteed_present,
                spread_span,
                ..
            } => {
                let element_idx = *prefix_element_idx;
                let expr = if let Some(default) = default {
                    if *guaranteed_present {
                        spread_element_expr_for_ir(
                            &prefix_temp,
                            element_idx,
                            param_name.as_deref(),
                            *prefer_named_key,
                            *spread_span,
                        )
                    } else {
                        spread_element_or_default_expr_for_ir(
                            &prefix_temp,
                            element_idx,
                            param_name.as_deref(),
                            *prefer_named_key,
                            default.clone(),
                            *spread_span,
                        )
                    }
                } else {
                    spread_element_expr_for_ir(
                        &prefix_temp,
                        element_idx,
                        param_name.as_deref(),
                        *prefer_named_key,
                        *spread_span,
                    )
                };
                operands.push(lower_expr(ctx, &expr).value);
            }
        }
    }
    if sig.variadic.is_some() {
        let regular_param_count = crate::types::call_args::regular_param_count(sig);
        let tail = lower_named_spread_static_variadic_tail_hash(
            ctx,
            sig,
            &prefix_temp,
            static_variadic_prefix_len.unwrap_or(regular_param_count),
            regular_param_count,
            plan,
            &source_values,
            first_named_pos,
            call_span,
        );
        operands.push(tail.value);
    }
    Some(operands)
}

/// Returns a static prefix length only for indexed array literals without nested spreads.
fn static_indexed_variadic_prefix_len(prefix_expr: &Expr) -> Option<usize> {
    let ExprKind::ArrayLiteral(items) = &prefix_expr.kind else {
        return None;
    };
    if items.iter().any(|item| matches!(item.kind, ExprKind::Spread(_))) {
        return None;
    }
    Some(items.len())
}

/// Builds a variadic tail hash from static spread overflow plus later named variadics.
#[allow(clippy::too_many_arguments)]
fn lower_named_spread_static_variadic_tail_hash(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    prefix_temp: &Expr,
    prefix_len: usize,
    regular_param_count: usize,
    plan: &crate::types::call_args::CallArgPlan,
    source_values: &[Option<crate::ir::ValueId>],
    first_named_pos: usize,
    span: crate::span::Span,
) -> LoweredValue {
    let value_ty = variadic_tail_value_type(sig);
    let prefix_tail_len = prefix_len.saturating_sub(regular_param_count);
    let named_tail_len = plan
        .source_values
        .iter()
        .filter(|source| source.source_index() >= first_named_pos && source.param_idx().is_none())
        .count();
    let hash_ty = PhpType::AssocArray {
        key: Box::new(PhpType::Mixed),
        value: Box::new(value_ty.clone()),
    };
    let hash = ctx.emit_value(
        Op::HashNew,
        Vec::new(),
        Some(Immediate::Capacity((prefix_tail_len + named_tail_len) as u32)),
        hash_ty,
        Op::HashNew.default_effects(),
        Some(span),
    );
    let array_ty = PhpType::Array(Box::new(value_ty.clone()));
    let mut next_positional_key = 0usize;
    for prefix_idx in regular_param_count..prefix_len {
        let key = emit_i64_at_span(ctx, next_positional_key as i64, span);
        next_positional_key += 1;
        let expr = spread_element_expr_for_ir(prefix_temp, prefix_idx, None, false, span);
        let value = lower_expr(ctx, &expr);
        let value = coerce_variadic_tail_value(ctx, value, &array_ty, span);
        ctx.emit_void(
            Op::HashSet,
            vec![hash.value, key.value, value.value],
            None,
            Op::HashSet.default_effects(),
            Some(span),
        );
    }
    for source in &plan.source_values {
        if source.source_index() < first_named_pos || source.param_idx().is_some() {
            continue;
        }
        let key = if let Some(key) = source.key() {
            lower_string_literal(ctx, key, source.expr())
        } else {
            let key = emit_i64_at_span(ctx, next_positional_key as i64, source.expr().span);
            next_positional_key += 1;
            key
        };
        let value = source_values[source.source_index()]
            .expect("named spread variadic source was not evaluated");
        let value = lowered_value_from_id(ctx, value);
        let value = coerce_variadic_tail_value(ctx, value, &array_ty, source.expr().span);
        ctx.emit_void(
            Op::HashSet,
            vec![hash.value, key.value, value.value],
            None,
            Op::HashSet.default_effects(),
            Some(source.expr().span),
        );
    }
    hash
}

/// Emits named-after-spread min/max checks against the already materialized prefix temp.
fn emit_named_spread_bounds_guard(
    ctx: &mut LoweringContext<'_, '_>,
    spread: crate::ir::ValueId,
    check: &crate::types::call_args::SpreadBoundsCheck,
    span: crate::span::Span,
) {
    if check.min_len == 0 && check.max_len.is_none() {
        return;
    }
    let len = ctx.emit_value(
        Op::ArrayLen,
        vec![spread],
        None,
        PhpType::Int,
        Op::ArrayLen.default_effects(),
        Some(span),
    );
    emit_named_spread_min_len_guard(ctx, len.value, check.min_len, span);
    emit_named_spread_max_len_guard(
        ctx,
        len.value,
        check.max_len,
        check.max_len_param_name.as_deref(),
        span,
    );
}

/// Emits the underflow branch for a named-after-spread bounds check.
fn emit_named_spread_min_len_guard(
    ctx: &mut LoweringContext<'_, '_>,
    len: crate::ir::ValueId,
    min_len: usize,
    span: crate::span::Span,
) {
    if min_len == 0 {
        return;
    }
    let min = emit_i64_at_span(ctx, min_len as i64, span);
    let has_required_args = ctx.emit_value(
        Op::ICmp,
        vec![len, min.value],
        Some(Immediate::CmpPredicate(CmpPredicate::Sge)),
        PhpType::Bool,
        Op::ICmp.default_effects(),
        Some(span),
    );
    let ok = ctx.builder.create_named_block("call.named_spread.min.ok", Vec::new());
    let fatal = ctx.builder.create_named_block("call.named_spread.min.fatal", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: has_required_args.value,
        then_target: ok,
        then_args: Vec::new(),
        else_target: fatal,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(fatal);
    let message = ctx.intern_string("Fatal error: named argument spread length mismatch\n");
    ctx.builder.terminate(Terminator::Fatal { message });

    ctx.builder.position_at_end(ok);
}

/// Emits the overflow branch for a named-after-spread bounds check.
fn emit_named_spread_max_len_guard(
    ctx: &mut LoweringContext<'_, '_>,
    len: crate::ir::ValueId,
    max_len: Option<usize>,
    param_name: Option<&str>,
    span: crate::span::Span,
) {
    let Some(max_len) = max_len else {
        return;
    };
    let max = emit_i64_at_span(ctx, max_len as i64, span);
    let within_bound = ctx.emit_value(
        Op::ICmp,
        vec![len, max.value],
        Some(Immediate::CmpPredicate(CmpPredicate::Sle)),
        PhpType::Bool,
        Op::ICmp.default_effects(),
        Some(span),
    );
    let ok = ctx.builder.create_named_block("call.named_spread.max.ok", Vec::new());
    let fatal = ctx.builder.create_named_block("call.named_spread.max.fatal", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: within_bound.value,
        then_target: ok,
        then_args: Vec::new(),
        else_target: fatal,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(fatal);
    let message = if let Some(param_name) = param_name {
        format!(
            "Fatal error: Named parameter ${} overwrites previous argument\n",
            param_name
        )
    } else {
        "Fatal error: named argument spread length mismatch\n".to_string()
    };
    let message = ctx.intern_string(&message);
    ctx.builder.terminate(Terminator::Fatal { message });

    ctx.builder.position_at_end(ok);
}

/// Lowers a single associative spread as named parameter reads by key.
fn lower_assoc_spread_only_args(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    args: &[Expr],
) -> Option<Vec<crate::ir::ValueId>> {
    let [arg] = args else {
        return None;
    };
    let ExprKind::Spread(inner) = &arg.kind else {
        return None;
    };
    if !is_assoc_spread_source(ctx, inner) || sig.variadic.is_some() {
        return None;
    }
    let spread = lower_expr(ctx, inner);
    let spread_type = ctx.builder.value_php_type(spread.value);
    let temp_name = ctx.declare_hidden_temp(spread_type.clone());
    store_value_into_temp(ctx, &temp_name, spread_type, spread, arg.span);
    let spread_expr = Expr::new(ExprKind::Variable(temp_name), inner.span);
    let mut operands = Vec::with_capacity(sig.params.len());
    for (idx, (param_name, _)) in sig.params.iter().enumerate() {
        let default = sig.defaults.get(idx).and_then(|default| default.as_ref());
        let param_expr = assoc_spread_param_expr(&spread_expr, param_name, default, arg.span);
        operands.push(lower_expr(ctx, &param_expr).value);
    }
    Some(operands)
}

/// Builds an expression that reads one named parameter from an associative spread.
fn assoc_spread_param_expr(
    spread_expr: &Expr,
    param_name: &str,
    default: Option<&Expr>,
    span: crate::span::Span,
) -> Expr {
    let key = Expr::new(ExprKind::StringLiteral(param_name.to_string()), span);
    let access = Expr::new(
        ExprKind::ArrayAccess {
            array: Box::new(spread_expr.clone()),
            index: Box::new(key.clone()),
        },
        span,
    );
    let Some(default) = default else {
        return access;
    };
    Expr::new(
        ExprKind::Ternary {
            condition: Box::new(Expr::new(
                ExprKind::FunctionCall {
                    name: Name::unqualified("array_key_exists"),
                    args: vec![key, spread_expr.clone()],
                },
                span,
            )),
            then_expr: Box::new(access),
            else_expr: Box::new(default.clone()),
        },
        span,
    )
}

/// Builds an expression that reads one materialized spread element from a hidden temp.
fn spread_element_expr_for_ir(
    spread_expr: &Expr,
    element_idx: usize,
    param_name: Option<&str>,
    prefer_named_key: bool,
    span: crate::span::Span,
) -> Expr {
    let index = if prefer_named_key {
        param_name
            .map(|name| Expr::new(ExprKind::StringLiteral(name.to_string()), span))
            .unwrap_or_else(|| Expr::new(ExprKind::IntLiteral(element_idx as i64), span))
    } else {
        Expr::new(ExprKind::IntLiteral(element_idx as i64), span)
    };
    Expr::new(
        ExprKind::ArrayAccess {
            array: Box::new(spread_expr.clone()),
            index: Box::new(index),
        },
        span,
    )
}

/// Builds an expression that falls back to a default when a spread element is absent.
fn spread_element_or_default_expr_for_ir(
    spread_expr: &Expr,
    element_idx: usize,
    param_name: Option<&str>,
    prefer_named_key: bool,
    default_expr: Expr,
    span: crate::span::Span,
) -> Expr {
    let condition = if prefer_named_key {
        if let Some(param_name) = param_name {
            Expr::new(
                ExprKind::FunctionCall {
                    name: Name::unqualified("array_key_exists"),
                    args: vec![
                        Expr::new(ExprKind::StringLiteral(param_name.to_string()), span),
                        spread_expr.clone(),
                    ],
                },
                span,
            )
        } else {
            spread_len_gt_expr_for_ir(spread_expr, element_idx, span)
        }
    } else {
        spread_len_gt_expr_for_ir(spread_expr, element_idx, span)
    };
    Expr::new(
        ExprKind::Ternary {
            condition: Box::new(condition),
            then_expr: Box::new(spread_element_expr_for_ir(
                spread_expr,
                element_idx,
                param_name,
                prefer_named_key,
                span,
            )),
            else_expr: Box::new(default_expr),
        },
        span,
    )
}

/// Builds `count($spread) > element_idx` for optional spread-slot defaults.
fn spread_len_gt_expr_for_ir(
    spread_expr: &Expr,
    element_idx: usize,
    span: crate::span::Span,
) -> Expr {
    Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(Expr::new(
                ExprKind::FunctionCall {
                    name: Name::unqualified("count"),
                    args: vec![spread_expr.clone()],
                },
                span,
            )),
            op: BinOp::Gt,
            right: Box::new(Expr::new(ExprKind::IntLiteral(element_idx as i64), span)),
        },
        span,
    )
}

/// Marks spread arguments whose source is known to be an associative array.
fn assoc_spread_sources(ctx: &LoweringContext<'_, '_>, args: &[Expr]) -> Vec<bool> {
    crate::types::call_args::expand_static_assoc_spread_args(args)
        .iter()
        .map(|arg| match &arg.kind {
            ExprKind::Spread(inner) => is_assoc_spread_source(ctx, inner),
            _ => false,
        })
        .collect()
}

/// Returns true when a spread expression should feed named parameters by key.
fn is_assoc_spread_source(ctx: &LoweringContext<'_, '_>, expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Variable(name) => matches!(ctx.local_types.get(name), Some(PhpType::AssocArray { .. })),
        ExprKind::ArrayLiteralAssoc(_) => true,
        _ => matches!(infer_expr_type_syntactic(expr), PhpType::AssocArray { .. }),
    }
}

/// Lowers one source call argument, unwrapping named syntax while preserving source position.
fn lower_call_source_arg(ctx: &mut LoweringContext<'_, '_>, arg: &Expr) -> crate::ir::ValueId {
    match &arg.kind {
        ExprKind::NamedArg { value, .. } => lower_expr(ctx, value).value,
        _ => lower_expr(ctx, arg).value,
    }
}

/// Builds the variadic tail array for a named-argument call plan.
fn lower_named_variadic_tail_array(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    tail: &[crate::types::call_args::PlannedSourceValue],
    source_values: &[crate::ir::ValueId],
) -> LoweredValue {
    if tail.iter().any(|source| source.key().is_some()) {
        return lower_named_variadic_tail_hash(ctx, sig, tail, source_values);
    }
    let span = tail
        .first()
        .map(|arg| arg.expr().span)
        .unwrap_or_else(crate::span::Span::dummy);
    let variadic_count = tail.iter().filter(|source| source.param_idx().is_none()).count();
    let array_ty = variadic_array_type(sig);
    let array = ctx.emit_value(
        Op::ArrayNew,
        Vec::new(),
        Some(Immediate::Capacity(variadic_count as u32)),
        array_ty.clone(),
        Op::ArrayNew.default_effects(),
        Some(span),
    );
    let elem_ty = indexed_array_literal_element_type(&array_ty);
    let by_ref_variadic = variadic_param_is_by_ref(sig);
    for source in tail {
        if source.param_idx().is_some() {
            continue;
        }
        let value = lower_variadic_tail_source_value(
            ctx,
            source.expr(),
            by_ref_variadic,
            Some(source_values[source.source_index()]),
            &array_ty,
        );
        ctx.emit_void(
            Op::ArrayPush,
            vec![array.value, value.value],
            None,
            Op::ArrayPush.default_effects(),
            Some(source.expr().span),
        );
        super::stmt::release_indexed_array_write_operand(
            ctx,
            elem_ty.as_ref(),
            value,
            source.expr().span,
        );
    }
    array
}

/// Builds an associative variadic tail when unknown named args must keep string keys.
fn lower_named_variadic_tail_hash(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    tail: &[crate::types::call_args::PlannedSourceValue],
    source_values: &[crate::ir::ValueId],
) -> LoweredValue {
    let span = tail
        .first()
        .map(|arg| arg.expr().span)
        .unwrap_or_else(crate::span::Span::dummy);
    let value_ty = variadic_tail_value_type(sig);
    let variadic_count = tail.iter().filter(|source| source.param_idx().is_none()).count();
    let hash_ty = PhpType::AssocArray {
        key: Box::new(PhpType::Mixed),
        value: Box::new(value_ty.clone()),
    };
    let hash = ctx.emit_value(
        Op::HashNew,
        Vec::new(),
        Some(Immediate::Capacity(variadic_count as u32)),
        hash_ty,
        Op::HashNew.default_effects(),
        Some(span),
    );
    let mut next_positional_key = 0usize;
    let by_ref_variadic = variadic_param_is_by_ref(sig);
    for source in tail {
        if source.param_idx().is_some() {
            continue;
        }
        let key = if let Some(key) = source.key() {
            lower_string_literal(ctx, key, source.expr())
        } else {
            let key = emit_i64_at_span(ctx, next_positional_key as i64, source.expr().span);
            next_positional_key += 1;
            key
        };
        let value = lower_variadic_tail_source_value(
            ctx,
            source.expr(),
            by_ref_variadic,
            Some(source_values[source.source_index()]),
            &PhpType::Array(Box::new(value_ty.clone())),
        );
        ctx.emit_void(
            Op::HashSet,
            vec![hash.value, key.value, value.value],
            None,
            Op::HashSet.default_effects(),
            Some(source.expr().span),
        );
    }
    hash
}

/// Rebuilds lowering metadata for an already emitted value.
fn lowered_value_from_id(
    ctx: &LoweringContext<'_, '_>,
    value: crate::ir::ValueId,
) -> LoweredValue {
    LoweredValue {
        value,
        ir_type: ctx.builder.value_type(value),
    }
}

/// Lowers the synthetic variadic tail array using the variadic parameter's storage type.
fn lower_variadic_tail_array(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    tail: &[Expr],
) -> LoweredValue {
    let span = tail
        .first()
        .map(|arg| arg.span)
        .unwrap_or_else(crate::span::Span::dummy);
    let array_ty = variadic_array_type(sig);
    let array = ctx.emit_value(
        Op::ArrayNew,
        Vec::new(),
        Some(Immediate::Capacity(tail.len() as u32)),
        array_ty.clone(),
        Op::ArrayNew.default_effects(),
        Some(span),
    );
    let elem_ty = indexed_array_literal_element_type(&array_ty);
    let by_ref_variadic = variadic_param_is_by_ref(sig);
    for item in tail {
        let value = lower_variadic_tail_source_value(ctx, item, by_ref_variadic, None, &array_ty);
        ctx.emit_void(
            Op::ArrayPush,
            vec![array.value, value.value],
            None,
            Op::ArrayPush.default_effects(),
            Some(item.span),
        );
        super::stmt::release_indexed_array_write_operand(ctx, elem_ty.as_ref(), value, item.span);
    }
    array
}

/// Lowers one value stored into a variadic tail container.
fn lower_variadic_tail_source_value(
    ctx: &mut LoweringContext<'_, '_>,
    expr: &Expr,
    by_ref_variadic: bool,
    prelowered: Option<crate::ir::ValueId>,
    array_ty: &PhpType,
) -> LoweredValue {
    if by_ref_variadic {
        if let ExprKind::Variable(name) = &expr.kind {
            return lower_invoker_ref_arg_marker(ctx, name, expr.span);
        }
    }
    let value = prelowered
        .map(|value| lowered_value_from_id(ctx, value))
        .unwrap_or_else(|| lower_expr(ctx, expr));
    coerce_variadic_tail_value(ctx, value, array_ty, expr.span)
}

/// Returns whether the synthetic variadic parameter slot is by-reference.
fn variadic_param_is_by_ref(sig: &FunctionSig) -> bool {
    let Some(variadic_name) = sig.variadic.as_ref() else {
        return false;
    };
    sig.params
        .iter()
        .position(|(name, _)| name == variadic_name)
        .and_then(|index| sig.ref_params.get(index))
        .copied()
        .unwrap_or(false)
}

/// Returns the element type expected inside a variadic tail container.
fn variadic_tail_value_type(sig: &FunctionSig) -> PhpType {
    if variadic_param_is_by_ref(sig) {
        return PhpType::Mixed;
    }
    let Some(variadic_name) = sig.variadic.as_ref() else {
        return PhpType::Mixed;
    };
    sig.params
        .iter()
        .find(|(name, _)| name == variadic_name)
        .map(|(_, ty)| match ty.codegen_repr() {
            PhpType::Array(elem_ty) => variadic_container_element_type(*elem_ty),
            other => variadic_container_element_type(other),
        })
        .unwrap_or(PhpType::Mixed)
}

/// Returns the runtime array type used for a variadic parameter slot.
fn variadic_array_type(sig: &FunctionSig) -> PhpType {
    if variadic_param_is_by_ref(sig) {
        return PhpType::Array(Box::new(PhpType::Mixed));
    }
    let Some(variadic_name) = sig.variadic.as_ref() else {
        return PhpType::Array(Box::new(PhpType::Mixed));
    };
    sig.params
        .iter()
        .find(|(name, _)| name == variadic_name)
        .map(|(_, ty)| match ty.codegen_repr() {
            PhpType::Array(elem_ty) => {
                PhpType::Array(Box::new(variadic_container_element_type(*elem_ty)))
            }
            other => PhpType::Array(Box::new(variadic_container_element_type(other))),
        })
        .unwrap_or_else(|| PhpType::Array(Box::new(PhpType::Mixed)))
}

/// Maps checker-only variadic container markers to their stored element type.
fn variadic_container_element_type(ty: PhpType) -> PhpType {
    if matches!(ty, PhpType::Iterable) {
        PhpType::Mixed
    } else {
        ty
    }
}

/// Boxes variadic tail values when the callee expects an `array<mixed>` slot.
fn coerce_variadic_tail_value(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    array_ty: &PhpType,
    span: crate::span::Span,
) -> LoweredValue {
    let PhpType::Array(elem_ty) = array_ty.codegen_repr() else {
        return value;
    };
    if elem_ty.codegen_repr() != PhpType::Mixed {
        return value;
    }
    if ctx.builder.value_php_type(value.value).codegen_repr() == PhpType::Mixed {
        return value;
    }
    ctx.box_value_as_mixed(value, PhpType::Mixed, Some(span))
}

/// Returns true when a call argument uses unpacking syntax.
fn is_spread_arg(arg: &Expr) -> bool {
    matches!(arg.kind, ExprKind::Spread(_))
}

/// Returns true when a call contains any static spread that EIR can flatten before lowering.
fn has_static_call_spread_args(args: &[Expr]) -> bool {
    has_static_indexed_spread_args(args) || has_static_assoc_spread_args(args)
}

/// Returns true when a call contains an indexed-array spread that EIR can flatten statically.
fn has_static_indexed_spread_args(args: &[Expr]) -> bool {
    args.iter().any(|arg| match &arg.kind {
        ExprKind::Spread(inner) => matches!(inner.kind, ExprKind::ArrayLiteral(_)),
        _ => false,
    })
}

/// Returns true when a call contains an associative-array spread literal that can be flattened.
fn has_static_assoc_spread_args(args: &[Expr]) -> bool {
    args.iter().any(|arg| match &arg.kind {
        ExprKind::Spread(inner) => matches!(inner.kind, ExprKind::ArrayLiteralAssoc(_)),
        _ => false,
    })
}

/// Flattens every statically-known call spread before EIR operand materialization.
fn expand_static_call_spread_args(args: &[Expr]) -> Vec<Expr> {
    let assoc_expanded = crate::types::call_args::expand_static_assoc_spread_args(args);
    expand_static_indexed_spread_args(&assoc_expanded)
}

/// Flattens static indexed array spreads into positional call arguments.
fn expand_static_indexed_spread_args(args: &[Expr]) -> Vec<Expr> {
    let mut expanded = Vec::new();
    for arg in args {
        match &arg.kind {
            ExprKind::Spread(inner) => {
                if let ExprKind::ArrayLiteral(items) = &inner.kind {
                    expanded.extend(items.iter().map(|value| {
                        Expr::new(value.kind.clone(), arg.span)
                    }));
                } else {
                    expanded.push(arg.clone());
                }
            }
            _ => expanded.push(arg.clone()),
        }
    }
    expanded
}

/// Returns the best available return type for a function-like call.
pub(super) fn call_return_type(
    ctx: &LoweringContext<'_, '_>,
    name: &str,
    _operands: &[crate::ir::ValueId],
) -> PhpType {
    let php_type = if let Some(sig) = ctx.functions.get(name) {
        eir_user_function_return_type(sig)
    } else if let Some(sig) = ctx.extern_functions.get(name) {
        sig.return_type.clone()
    } else if let Some(sig) = builtin_call_signature(name) {
        sig.return_type
    } else {
        PhpType::Mixed
    };
    normalize_value_php_type(php_type)
}

/// Returns the caller-visible EIR return type for a user function signature.
fn eir_user_function_return_type(signature: &FunctionSig) -> PhpType {
    if signature.declared_return || !signature_has_dynamic_untyped_param(signature) {
        return signature.return_type.clone();
    }
    dynamic_param_container_return_type(&signature.return_type)
}

/// Returns true when a PHP signature has params that EIR must receive as Mixed.
fn signature_has_dynamic_untyped_param(signature: &FunctionSig) -> bool {
    signature.params.iter().enumerate().any(|(index, (name, _))| {
        let declared = signature.declared_params.get(index).copied().unwrap_or(false);
        let by_ref = signature.ref_params.get(index).copied().unwrap_or(false);
        let variadic = signature.variadic.as_deref() == Some(name.as_str());
        !declared && !by_ref && !variadic
    })
}

/// Widens inferred container return elements that may be built from dynamic params.
fn dynamic_param_container_return_type(return_type: &PhpType) -> PhpType {
    match return_type.codegen_repr() {
        PhpType::Array(_) => PhpType::Array(Box::new(PhpType::Mixed)),
        PhpType::AssocArray { key, .. } => PhpType::AssocArray {
            key,
            value: Box::new(PhpType::Mixed),
        },
        PhpType::Union(members) => PhpType::Union(
            members
                .iter()
                .map(dynamic_param_container_return_type)
                .collect(),
        ),
        other => other,
    }
}

/// Distinguishes pre-lowered array-literal items between plain elements and spread operands.
enum SpreadItem {
    Element(LoweredValue),
    Spread(LoweredValue),
}

/// Lowers an indexed array literal.
fn lower_array_literal(ctx: &mut LoweringContext<'_, '_>, items: &[Expr], expr: &Expr) -> LoweredValue {
    // Fast path: literals without any spread keep the original dest-first lowering so the
    // common `[1, 2, 3]` form does not reorder allocation relative to element evaluation.
    if !items.iter().any(|item| matches!(item.kind, ExprKind::Spread(_))) {
        let array_ty = array_literal_type_for_ir(ctx, items, expr);
        return lower_array_literal_without_spread(ctx, items, expr, array_ty);
    }
    // Spread-containing literals: lower every item value in source order first so PHP-visible side
    // effects happen in order, then inspect each spread source's actual IR type to decide whether
    // the destination must be associative (hash) storage. Dest allocation is pure, so emitting it
    // after source evaluation preserves observable behavior.
    let mut lowered: Vec<SpreadItem> = Vec::with_capacity(items.len());
    let mut any_assoc_spread = false;
    for item in items {
        match &item.kind {
            ExprKind::Spread(inner) => {
                let source = lower_expr(ctx, inner);
                if matches!(
                    ctx.builder.value_php_type(source.value).codegen_repr(),
                    PhpType::AssocArray { .. }
                ) {
                    any_assoc_spread = true;
                }
                lowered.push(SpreadItem::Spread(source));
            }
            _ => {
                let value = lower_expr(ctx, item);
                lowered.push(SpreadItem::Element(value));
            }
        }
    }
    if any_assoc_spread {
        lower_array_literal_as_hash_from_lowered(ctx, items, &lowered, expr)
    } else {
        lower_array_literal_as_indexed_from_lowered(ctx, items, &lowered, expr)
    }
}

/// Lowers an indexed array literal using a contextual element storage type.
pub(crate) fn lower_array_literal_with_expected_type(
    ctx: &mut LoweringContext<'_, '_>,
    expr: &Expr,
    elem_ty: PhpType,
) -> LoweredValue {
    let ExprKind::ArrayLiteral(items) = &expr.kind else {
        return lower_expr(ctx, expr);
    };
    if items.iter().any(|item| matches!(item.kind, ExprKind::Spread(_))) {
        return lower_array_literal(ctx, items, expr);
    }
    let array_ty = expected_indexed_array_literal_type(elem_ty);
    lower_array_literal_without_spread(ctx, items, expr, array_ty)
}

/// Returns an indexed-array type for contextual literal lowering.
fn expected_indexed_array_literal_type(elem_ty: PhpType) -> PhpType {
    PhpType::Array(Box::new(elem_ty.codegen_repr()))
}

/// Lowers a no-spread indexed array literal into the requested array storage type.
fn lower_array_literal_without_spread(
    ctx: &mut LoweringContext<'_, '_>,
    items: &[Expr],
    expr: &Expr,
    array_ty: PhpType,
) -> LoweredValue {
    let elem_ty = indexed_array_literal_element_type(&array_ty);
    let array = ctx.emit_value(
        Op::ArrayNew,
        Vec::new(),
        Some(Immediate::Capacity(items.len() as u32)),
        array_ty,
        Op::ArrayNew.default_effects(),
        Some(expr.span),
    );
    for item in items {
        let value = lower_expr(ctx, item);
        let value = coerce_array_literal_element_to_storage_type(ctx, value, elem_ty.as_ref(), item);
        ctx.emit_void(
            Op::ArrayPush,
            vec![array.value, value.value],
            None,
            Op::ArrayPush.default_effects(),
            Some(item.span),
        );
        super::stmt::release_indexed_array_write_operand(ctx, elem_ty.as_ref(), value, item.span);
    }
    array
}

/// Coerces an array literal element to the contextual storage type when needed.
fn coerce_array_literal_element_to_storage_type(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    elem_ty: Option<&PhpType>,
    expr: &Expr,
) -> LoweredValue {
    let Some(elem_ty) = elem_ty else {
        return value;
    };
    let coerced = match elem_ty.codegen_repr() {
        PhpType::Int | PhpType::Bool if value.ir_type != IrType::I64 => {
            coerce_to_int(ctx, value, expr)
        }
        PhpType::Float if value.ir_type != IrType::F64 => coerce_to_float(ctx, value, expr),
        PhpType::Str if value.ir_type != IrType::Str => coerce_to_string(ctx, value, expr),
        _ => value,
    };
    // The scalar coercers release owning heap-repr sources internally (see
    // `release_coerced_source_if_owned`); releasing those here again would
    // double-free the element box. This caller-side release only covers the
    // remaining reprs (e.g. an owned string temp narrowed through `StrToI`).
    if coerced.value != value.value
        && !coerced_source_repr_is_releasable(&ctx.builder.value_php_type(value.value))
        && ctx.value_is_owning_temporary(value)
    {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(expr.span));
    }
    coerced
}

/// Lowers a spread-containing indexed-array literal whose spread sources are all indexed arrays.
fn lower_array_literal_as_indexed_from_lowered(
    ctx: &mut LoweringContext<'_, '_>,
    items: &[Expr],
    lowered: &[SpreadItem],
    expr: &Expr,
) -> LoweredValue {
    let array_ty = array_literal_type_for_ir(ctx, items, expr);
    let elem_ty = indexed_array_literal_element_type(&array_ty);
    let array = ctx.emit_value(
        Op::ArrayNew,
        Vec::new(),
        Some(Immediate::Capacity(items.len() as u32)),
        array_ty,
        Op::ArrayNew.default_effects(),
        Some(expr.span),
    );
    for (item, value) in items.iter().zip(lowered.iter()) {
        match value {
            SpreadItem::Spread(source) => {
                lower_indexed_array_spread_into_array(ctx, array, *source, elem_ty.as_ref(), item.span);
            }
            SpreadItem::Element(value) => {
                ctx.emit_void(Op::ArrayPush, vec![array.value, value.value], None, Op::ArrayPush.default_effects(), Some(item.span));
                super::stmt::release_indexed_array_write_operand(ctx, elem_ty.as_ref(), *value, item.span);
            }
        }
    }
    array
}

/// Lowers a spread-containing array literal with at least one associative spread as a hash.
fn lower_array_literal_as_hash_from_lowered(
    ctx: &mut LoweringContext<'_, '_>,
    items: &[Expr],
    lowered: &[SpreadItem],
    expr: &Expr,
) -> LoweredValue {
    let hash_ty = assoc_array_literal_type_from_spreads(ctx, items, expr);
    let value_ty = match hash_ty.codegen_repr() {
        PhpType::AssocArray { value, .. } => value.codegen_repr(),
        _ => PhpType::Mixed,
    };
    let hash = ctx.emit_value(
        Op::HashNew,
        Vec::new(),
        Some(Immediate::Capacity(items.len() as u32)),
        hash_ty,
        Op::HashNew.default_effects(),
        Some(expr.span),
    );
    for (item, value) in items.iter().zip(lowered.iter()) {
        match value {
            SpreadItem::Spread(source) => {
                lower_hash_spread_into_hash_from_value(ctx, hash, *source, item.span);
            }
            SpreadItem::Element(value) => {
                ctx.emit_void(
                    Op::RuntimeCall,
                    vec![hash.value, value.value],
                    None,
                    effects_lookup::runtime_effects(),
                    Some(item.span),
                );
                release_value_after_retaining_insert(ctx, Some(&value_ty), *value, item.span);
            }
        }
    }
    hash
}

/// Lowers a single already-lowered spread operand into a hash destination, handling both
/// associative and indexed source storage. Associative sources flatten directly through
/// `__rt_hash_spread`; indexed sources are first promoted to hash storage so the same
/// reindexing path applies.
fn lower_hash_spread_into_hash_from_value(
    ctx: &mut LoweringContext<'_, '_>,
    hash: LoweredValue,
    source: LoweredValue,
    span: crate::span::Span,
) {
    let source_is_hash = matches!(
        ctx.builder.value_php_type(source.value).codegen_repr(),
        PhpType::AssocArray { .. }
    );
    let spread_source = if source_is_hash {
        source
    } else {
        let promoted = ctx.emit_value(
            Op::ArrayToHash,
            vec![source.value],
            None,
            PhpType::AssocArray {
                key: Box::new(PhpType::Int),
                value: Box::new(PhpType::Mixed),
            },
            Op::ArrayToHash.default_effects(),
            Some(span),
        );
        LoweredValue {
            value: promoted.value,
            ir_type: IrType::Heap(IrHeapKind::Hash),
        }
    };
    ctx.emit_void(
        Op::HashSpread,
        vec![hash.value, spread_source.value],
        None,
        Op::HashSpread.default_effects(),
        Some(span),
    );
    if ctx.value_is_owning_temporary(spread_source) {
        crate::ir_lower::ownership::release_if_owned(ctx, spread_source, Some(span));
    }
}

/// Lowers an indexed-array spread by appending each source element to the destination.
fn lower_indexed_array_spread_into_array(
    ctx: &mut LoweringContext<'_, '_>,
    array: LoweredValue,
    source: LoweredValue,
    container_elem_ty: Option<&PhpType>,
    span: crate::span::Span,
) {
    let source_elem_ty = match ctx.builder.value_php_type(source.value).codegen_repr() {
        PhpType::Array(elem_ty) => elem_ty.codegen_repr(),
        _ => PhpType::Mixed,
    };
    let len = ctx.emit_value(
        Op::ArrayLen,
        vec![source.value],
        None,
        PhpType::Int,
        Op::ArrayLen.default_effects(),
        Some(span),
    );
    let zero = emit_i64_at_span(ctx, 0, span);
    let header = ctx.builder.create_named_block("array.spread.next", vec![(IrType::I64, PhpType::Int)]);
    let body = ctx.builder.create_named_block("array.spread.body", Vec::new());
    let exit = ctx.builder.create_named_block("array.spread.exit", Vec::new());
    ctx.builder.terminate(Terminator::Br { target: header, args: vec![zero.value] });

    ctx.builder.position_at_end(header);
    let index = ctx.builder.block_param(header, 0);
    let has_next = ctx.emit_value(
        Op::ICmp,
        vec![index, len.value],
        Some(Immediate::CmpPredicate(CmpPredicate::Slt)),
        PhpType::Bool,
        Op::ICmp.default_effects(),
        Some(span),
    );
    ctx.builder.terminate(Terminator::CondBr {
        cond: has_next.value,
        then_target: body,
        then_args: Vec::new(),
        else_target: exit,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(body);
    let value = ctx.emit_value(
        Op::ArrayGet,
        vec![source.value, index],
        None,
        source_elem_ty,
        Op::ArrayGet.default_effects(),
        Some(span),
    );
    ctx.emit_void(
        Op::ArrayPush,
        vec![array.value, value.value],
        None,
        Op::ArrayPush.default_effects(),
        Some(span),
    );
    super::stmt::release_indexed_array_write_operand(ctx, container_elem_ty, value, span);
    let one = emit_i64_at_span(ctx, 1, span);
    let next = ctx.emit_value(
        Op::IAdd,
        vec![index, one.value],
        None,
        PhpType::Int,
        Op::IAdd.default_effects(),
        Some(span),
    );
    ctx.builder.terminate(Terminator::Br { target: header, args: vec![next.value] });

    ctx.builder.position_at_end(exit);
    if ctx.value_is_owning_temporary(source) {
        crate::ir_lower::ownership::release_if_owned(ctx, source, Some(span));
    }
}

/// Emits an integer constant at a specific source span.
fn emit_i64_at_span(
    ctx: &mut LoweringContext<'_, '_>,
    value: i64,
    span: crate::span::Span,
) -> LoweredValue {
    ctx.emit_value(
        Op::ConstI64,
        Vec::new(),
        Some(Immediate::I64(value)),
        PhpType::Int,
        Op::ConstI64.default_effects(),
        Some(span),
    )
}

/// Returns the element type from an indexed-array literal type.
fn indexed_array_literal_element_type(array_ty: &PhpType) -> Option<PhpType> {
    match array_ty.codegen_repr() {
        PhpType::Array(elem) => Some(elem.codegen_repr()),
        _ => None,
    }
}

/// Releases an inserted temporary when the container retained or copied its payload.
/// Callable arrays keep raw descriptor pointers today, so the inserted owner stays alive.
fn release_value_after_retaining_insert(
    ctx: &mut LoweringContext<'_, '_>,
    container_elem_ty: Option<&PhpType>,
    value: LoweredValue,
    span: crate::span::Span,
) {
    if matches!(
        container_elem_ty.map(PhpType::codegen_repr),
        Some(PhpType::Mixed | PhpType::Callable)
    ) {
        return;
    }
    if ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    }
}

/// Returns the indexed-array type that the EIR backend can faithfully materialize.
fn array_literal_type_for_ir(
    ctx: &LoweringContext<'_, '_>,
    items: &[Expr],
    expr: &Expr,
) -> PhpType {
    if items.is_empty() {
        return fallback_expr_type(expr);
    }
    let mut elem_ty = array_literal_element_type_for_ir(ctx, &items[0]);
    for item in items.iter().skip(1) {
        elem_ty = merge_ir_indexed_element_type(
            elem_ty,
            array_literal_element_type_for_ir(ctx, item),
        );
    }
    PhpType::Array(Box::new(elem_ty))
}

/// Returns the best EIR storage element type for one indexed-array literal item.
fn array_literal_element_type_for_ir(
    ctx: &LoweringContext<'_, '_>,
    item: &Expr,
) -> PhpType {
    match &item.kind {
        ExprKind::Null => PhpType::Mixed,
        ExprKind::Spread(inner) => match array_literal_element_type_for_ir(ctx, inner).codegen_repr() {
            PhpType::Array(elem) => elem.codegen_repr(),
            _ => PhpType::Mixed,
        },
        ExprKind::ArrayLiteral(items) => array_literal_type_for_ir(ctx, items, item).codegen_repr(),
        ExprKind::ArrayLiteralAssoc(pairs) => assoc_array_literal_type_for_ir(ctx, pairs, item),
        ExprKind::ConstRef(name) => ctx
            .constant_value(name.as_str())
            .map(|(_, ty)| ir_array_storage_type(ty))
            .unwrap_or_else(|| ir_array_storage_type(infer_expr_type_syntactic(item))),
        ExprKind::Variable(name) => ir_array_storage_type(
            ctx.local_types
                .get(name)
                .cloned()
                .unwrap_or_else(|| infer_expr_type_syntactic(item)),
        ),
        ExprKind::FunctionCall { name, .. } => {
            let canonical = name.as_str();
            if let Some(sig) = ctx.functions.get(canonical) {
                return ir_array_storage_type(sig.return_type.clone());
            }
            if let Some(sig) = ctx.extern_functions.get(canonical) {
                return ir_array_storage_type(sig.return_type.clone());
            }
            ir_array_storage_type(infer_expr_type_syntactic(item))
        }
        ExprKind::ArrayAccess { array, .. } => array_access_expr_value_type_for_ir(ctx, array)
            .unwrap_or_else(|| ir_array_storage_type(infer_expr_type_syntactic(item))),
        ExprKind::PropertyAccess { object, property } => property_access_expr_type_for_ir(
            ctx,
            object,
            property,
        )
        .unwrap_or_else(|| ir_array_storage_type(infer_expr_type_syntactic(item))),
        _ => ir_array_storage_type(infer_expr_type_syntactic(item)),
    }
}

/// Returns the EIR array storage metadata type, preserving PHP resources.
fn ir_array_storage_type(php_type: PhpType) -> PhpType {
    let php_type = normalize_value_php_type(php_type);
    if matches!(php_type, PhpType::Resource(_)) {
        php_type
    } else {
        php_type.codegen_repr()
    }
}

/// Merges indexed-array element types for EIR storage metadata.
fn merge_ir_indexed_element_type(left: PhpType, right: PhpType) -> PhpType {
    if left == right {
        return left;
    }
    if matches!(left.codegen_repr(), PhpType::Void | PhpType::Never) {
        return right;
    }
    if matches!(right.codegen_repr(), PhpType::Void | PhpType::Never) {
        return left;
    }
    PhpType::Mixed
}

/// Lowers an associative array literal.
fn lower_assoc_array_literal(ctx: &mut LoweringContext<'_, '_>, pairs: &[(Expr, Expr)], expr: &Expr) -> LoweredValue {
    let hash = ctx.emit_value(
        Op::HashNew,
        Vec::new(),
        Some(Immediate::Capacity(pairs.len() as u32)),
        assoc_array_literal_type_for_ir(ctx, pairs, expr),
        Op::HashNew.default_effects(),
        Some(expr.span),
    );
    for (key, value) in pairs {
        let key = lower_expr(ctx, key);
        let value = lower_expr(ctx, value);
        ctx.emit_void(Op::HashSet, vec![hash.value, key.value, value.value], None, Op::HashSet.default_effects(), Some(expr.span));
    }
    hash
}

/// Returns the associative-array type for a literal that contains at least one associative
/// spread. Mirrors the type checker's `assoc_spread_literal_value_type` so EIR storage matches
/// the value types actually lowered into the hash.
fn assoc_array_literal_type_from_spreads(
    ctx: &LoweringContext<'_, '_>,
    items: &[Expr],
    expr: &Expr,
) -> PhpType {
    let mut value_ty = PhpType::Never;
    for item in items {
        let next = match &item.kind {
            ExprKind::Spread(inner) => match infer_expr_type_syntactic(inner).codegen_repr() {
                PhpType::Array(elem) => elem.codegen_repr(),
                PhpType::AssocArray { value, .. } => value.codegen_repr(),
                _ => PhpType::Mixed,
            },
            _ => array_literal_element_type_for_ir(ctx, item).codegen_repr(),
        };
        value_ty = merge_ir_assoc_value_type(value_ty, next);
    }
    if matches!(value_ty, PhpType::Never) {
        return fallback_expr_type(expr);
    }
    PhpType::AssocArray {
        key: Box::new(PhpType::Mixed),
        value: Box::new(value_ty),
    }
}

/// Returns the associative-array type that the EIR backend can faithfully materialize.
fn assoc_array_literal_type_for_ir(
    ctx: &LoweringContext<'_, '_>,
    pairs: &[(Expr, Expr)],
    expr: &Expr,
) -> PhpType {
    if pairs.is_empty() {
        return fallback_expr_type(expr);
    }
    let mut key_ty = normalized_array_key_type(
        &pairs[0].0,
        infer_expr_type_syntactic(&pairs[0].0),
    );
    let mut value_ty = assoc_array_literal_value_type_for_ir(ctx, &pairs[0].1);
    for (key, value) in pairs.iter().skip(1) {
        key_ty = merge_array_key_types(
            key_ty,
            normalized_array_key_type(key, infer_expr_type_syntactic(key)),
        );
        value_ty = merge_ir_assoc_value_type(
            value_ty,
            assoc_array_literal_value_type_for_ir(ctx, value),
        );
    }
    PhpType::AssocArray {
        key: Box::new(key_ty),
        value: Box::new(value_ty),
    }
}

/// Returns the best EIR storage value type for one associative-array literal value.
fn assoc_array_literal_value_type_for_ir(
    ctx: &LoweringContext<'_, '_>,
    value: &Expr,
) -> PhpType {
    match &value.kind {
        ExprKind::Null => PhpType::Mixed,
        ExprKind::ConstRef(name) => ctx
            .constant_value(name.as_str())
            .map(|(_, ty)| ir_array_storage_type(ty))
            .unwrap_or_else(|| ir_array_storage_type(infer_expr_type_syntactic(value))),
        // A class constant or enum case must be typed the way `lower_scoped_constant`
        // resolves it, not by the syntactic `::class`-is-string default, or the hash
        // value-type stamp would diverge from the lowered value and corrupt reads.
        ExprKind::ScopedConstantAccess { receiver, name } => {
            scoped_constant_value_type_for_ir(ctx, receiver, name, value)
        }
        ExprKind::Variable(name) => ir_array_storage_type(
            ctx.local_types
                .get(name)
                .cloned()
                .unwrap_or_else(|| infer_expr_type_syntactic(value)),
        ),
        ExprKind::FunctionCall { name, .. } => {
            let canonical = name.as_str();
            if let Some(sig) = ctx.functions.get(canonical) {
                return ir_array_storage_type(sig.return_type.clone());
            }
            if let Some(sig) = ctx.extern_functions.get(canonical) {
                return ir_array_storage_type(sig.return_type.clone());
            }
            ir_array_storage_type(infer_expr_type_syntactic(value))
        }
        ExprKind::ArrayAccess { array, .. } => array_access_expr_value_type_for_ir(ctx, array)
            .unwrap_or_else(|| ir_array_storage_type(infer_expr_type_syntactic(value))),
        ExprKind::PropertyAccess { object, property } => property_access_expr_type_for_ir(
            ctx,
            object,
            property,
        )
        .unwrap_or_else(|| ir_array_storage_type(infer_expr_type_syntactic(value))),
        _ => ir_array_storage_type(infer_expr_type_syntactic(value)),
    }
}

/// Returns the EIR storage value type for a scoped-constant array value,
/// resolving a class/interface constant the same way `lower_scoped_constant`
/// lowers it so the hash value-type stamp matches the value actually stored
/// (rather than the syntactic `::class`-is-string default). Falls back to the
/// syntactic guess when the constant cannot be resolved.
fn scoped_constant_value_type_for_ir(
    ctx: &LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    member: &str,
    value: &Expr,
) -> PhpType {
    let class_name = scoped_constant_receiver_name(ctx, receiver);
    let normalized = class_name.trim_start_matches('\\');
    // An enum case lowers to the case *object* singleton (see `lower_scoped_constant`),
    // so the hash must box it as a Mixed cell — stamp the value type Mixed to match.
    if ctx
        .enums
        .get(normalized)
        .is_some_and(|enum_info| enum_info.cases.iter().any(|case| case.name == member))
    {
        return PhpType::Mixed;
    }
    if let Some(const_expr) = ctx.scoped_constant_value(&class_name, member) {
        return ir_array_storage_type(infer_expr_type_syntactic(&const_expr));
    }
    ir_array_storage_type(infer_expr_type_syntactic(value))
}

/// Returns the element/value type for an array-access expression used inside a literal.
pub(super) fn array_access_expr_value_type_for_ir(
    ctx: &LoweringContext<'_, '_>,
    array: &Expr,
) -> Option<PhpType> {
    let array_ty = match &array.kind {
        ExprKind::Variable(name) => ctx.local_types.get(name).cloned(),
        ExprKind::PropertyAccess { object, property } => {
            property_access_expr_type_for_ir(ctx, object, property)
        }
        ExprKind::ArrayLiteral(items) => Some(array_literal_type_for_ir(ctx, items, array)),
        ExprKind::ArrayLiteralAssoc(pairs) => Some(assoc_array_literal_type_for_ir(ctx, pairs, array)),
        _ => None,
    }?
    .codegen_repr();
    match array_ty {
        PhpType::Array(elem_ty) => {
            Some(array_access_element_result_type(normalize_value_php_type(*elem_ty).codegen_repr()))
        }
        PhpType::AssocArray { value, .. } => {
            Some(array_access_element_result_type(normalize_value_php_type(*value).codegen_repr()))
        }
        PhpType::Str => Some(PhpType::Str),
        PhpType::Mixed | PhpType::Union(_) => Some(PhpType::Mixed),
        _ => None,
    }
}

/// Returns the declared type for an object property expression used inside a literal.
pub(super) fn property_access_expr_type_for_ir(
    ctx: &LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
) -> Option<PhpType> {
    let class_name = instance_callable_object_class(ctx, object)?;
    let normalized = class_name.trim_start_matches('\\');
    if is_builtin_stdclass_name(normalized) {
        return Some(PhpType::Mixed);
    }
    if let Some(property_ty) = runtime_property_type_override(ctx, normalized, property) {
        return Some(normalize_value_php_type(property_ty));
    }
    let class_info = ctx.classes.get(normalized)?;
    class_info
        .properties
        .iter()
        .find(|(name, _)| name == property)
        .map(|(_, ty)| normalize_value_php_type(ty.codegen_repr()))
}

/// Returns the declared result type for an instance method call before its receiver is lowered.
pub(super) fn method_call_expr_type_for_ir(
    ctx: &LoweringContext<'_, '_>,
    object: &Expr,
    method: &str,
) -> Option<PhpType> {
    let class_name = instance_callable_object_class(ctx, object)?;
    let method_key = php_symbol_key(method);
    class_method_signature(ctx, &class_name, &method_key)
        .map(|signature| normalize_value_php_type(signature.return_type.codegen_repr()))
}

/// Merges associative-array value types for EIR storage metadata.
fn merge_ir_assoc_value_type(left: PhpType, right: PhpType) -> PhpType {
    if left == right {
        return left;
    }
    if matches!(left, PhpType::Never) {
        return right;
    }
    if matches!(right, PhpType::Never) {
        return left;
    }
    PhpType::Mixed
}

/// Lowers a match expression with lazy arm-result evaluation.
fn lower_match(
    ctx: &mut LoweringContext<'_, '_>,
    subject: &Expr,
    arms: &[(Vec<Expr>, Expr)],
    default: Option<&Expr>,
    expr: &Expr,
) -> LoweredValue {
    let subject = lower_expr(ctx, subject);
    let result_type = match_merge_result_type(ctx, arms, default, expr);
    let temp_name = ctx.declare_owned_hidden_temp(result_type.clone());
    let merge = ctx.builder.create_named_block("match.merge", Vec::new());

    for (conditions, result) in arms {
        let result_block = ctx.builder.create_named_block("match.result", Vec::new());
        let mut fallthrough = ctx.builder.insertion_block();
        for condition in conditions {
            let next_test = ctx.builder.create_named_block("match.next", Vec::new());
            let condition = lower_expr(ctx, condition);
            let matched = ctx.emit_value(
                Op::StrictEq,
                vec![subject.value, condition.value],
                None,
                PhpType::Bool,
                Op::StrictEq.default_effects(),
                Some(expr.span),
            );
            ctx.builder.terminate(Terminator::CondBr {
                cond: matched.value,
                then_target: result_block,
                then_args: Vec::new(),
                else_target: next_test,
                else_args: Vec::new(),
            });
            ctx.builder.position_at_end(next_test);
            fallthrough = Some(next_test);
        }
        ctx.builder.position_at_end(result_block);
        store_expr_into_temp(ctx, &temp_name, result_type.clone(), result, expr.span);
        branch_to(ctx, merge);
        if let Some(fallthrough) = fallthrough {
            ctx.builder.position_at_end(fallthrough);
        }
    }
    if let Some(default) = default {
        store_expr_into_temp(ctx, &temp_name, result_type.clone(), default, expr.span);
        branch_to(ctx, merge);
    } else if !ctx.builder.insertion_block_is_terminated() {
        let message = ctx.intern_string("Fatal error: unhandled match case\n");
        ctx.builder.terminate(Terminator::Fatal { message });
    }
    ctx.builder.position_at_end(merge);
    take_owned_temp(ctx, &temp_name, expr.span)
}

/// Lowers array, hash, string, or ArrayAccess indexing.
fn lower_array_access(
    ctx: &mut LoweringContext<'_, '_>,
    array: &Expr,
    index: &Expr,
    expr: &Expr,
) -> LoweredValue {
    lower_array_access_with_missing_warning(ctx, array, index, expr, true)
}

/// Lowers array, hash, string, or ArrayAccess indexing with configurable
/// undefined-offset warning behavior for native indexed-array reads. Suppressed
/// warnings propagate through the whole subscript chain: PHP's `isset()` and `??`
/// are silent for every level of `$a[1][2][3]`, not just the outermost read.
fn lower_array_access_with_missing_warning(
    ctx: &mut LoweringContext<'_, '_>,
    array: &Expr,
    index: &Expr,
    expr: &Expr,
    warn_on_missing: bool,
) -> LoweredValue {
    let array_value = if warn_on_missing {
        lower_expr(ctx, array)
    } else {
        lower_subscript_receiver_silently(ctx, array)
    };
    if value_is_nullable(ctx, array_value.value) {
        return lower_nullable_array_access(ctx, array_value, index, expr, warn_on_missing);
    }
    lower_array_access_from_value(ctx, array_value, index, expr, warn_on_missing)
}

/// Lowers a subscript-chain receiver with undefined-offset warnings suppressed on
/// nested array reads, so `isset()`/`??` stay silent across chained subscripts.
fn lower_subscript_receiver_silently(
    ctx: &mut LoweringContext<'_, '_>,
    array: &Expr,
) -> LoweredValue {
    if let ExprKind::ArrayAccess { array: inner_array, index: inner_index } = &array.kind {
        return lower_array_access_with_missing_warning(ctx, inner_array, inner_index, array, false);
    }
    lower_expr(ctx, array)
}

/// Lowers array access once the receiver is already evaluated.
fn lower_array_access_from_value(
    ctx: &mut LoweringContext<'_, '_>,
    array_value: LoweredValue,
    index: &Expr,
    expr: &Expr,
    warn_on_missing: bool,
) -> LoweredValue {
    let mut index_value = lower_expr(ctx, index);
    let op = match array_value.ir_type {
        IrType::Heap(IrHeapKind::Array) => {
            let index_ty = index_expr_key_type(ctx, index);
            if index_ty == PhpType::Int {
                index_value = coerce_to_int_at_span(ctx, index_value, Some(index.span));
                if warn_on_missing {
                    Op::ArrayGet
                } else {
                    Op::ArrayGetSilent
                }
            } else {
                // String or Mixed key on indexed storage: use the mixed-key
                // runtime read path (mirrors Op::ArraySetMixedKey for writes).
                if warn_on_missing {
                    Op::ArrayGetMixedKey
                } else {
                    Op::ArrayGetMixedKeySilent
                }
            }
        }
        IrType::Heap(IrHeapKind::Hash) => {
            if warn_on_missing {
                Op::HashGet
            } else {
                Op::HashGetSilent
            }
        }
        IrType::Heap(IrHeapKind::Buffer) => Op::BufferGet,
        IrType::Str => {
            index_value = coerce_to_int_at_span(ctx, index_value, Some(index.span));
            Op::StrCharAt
        }
        _ => Op::RuntimeCall,
    };
    let result_type = array_access_result_type(ctx, array_value.value, op, expr);
    let result = ctx.emit_value(
        op,
        vec![array_value.value, index_value.value],
        None,
        result_type,
        op.default_effects(),
        Some(expr.span),
    );
    // An owning boxed index temporary (e.g. `$B[$i + 1]` on the mixed-key read
    // path) is consumed by the read without any runtime refcount operation on
    // the key, and the result is freshly allocated storage that never aliases
    // it — release it here or it leaks per read (issue #500). Int-coerced
    // index paths rebound `index_value` to a non-owning raw cast, so the
    // owning-temporary gate makes this a no-op for them.
    release_coerced_source_if_owned(ctx, index_value, Some(index.span));
    // Array access consumes an owning receiver produced by an earlier read,
    // call, or one-shot temp. Preserve borrowed string/callable payloads before
    // dropping that receiver; boxed and retained container reads are already
    // independent and must not be acquired twice.
    stabilize_borrowed_result_and_release_receiver(ctx, array_value, result, expr.span)
}

/// Lowers nullable receiver indexing without evaluating the index on a null receiver.
fn lower_nullable_array_access(
    ctx: &mut LoweringContext<'_, '_>,
    array_value: LoweredValue,
    index: &Expr,
    expr: &Expr,
    warn_on_missing: bool,
) -> LoweredValue {
    let is_null = ctx.emit_value(
        Op::IsNull,
        vec![array_value.value],
        None,
        PhpType::Bool,
        Op::IsNull.default_effects(),
        Some(expr.span),
    );
    let result_type = PhpType::Mixed;
    let temp_name = ctx.declare_owned_hidden_temp(result_type.clone());
    let null_block = ctx
        .builder
        .create_named_block("nullable.index.null", Vec::new());
    let read_block = ctx
        .builder
        .create_named_block("nullable.index.read", Vec::new());
    let merge = ctx
        .builder
        .create_named_block("nullable.index.merge", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: is_null.value,
        then_target: null_block,
        then_args: Vec::new(),
        else_target: read_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(null_block);
    let null_value = lower_boxed_null(ctx, expr);
    store_value_into_temp(ctx, &temp_name, result_type.clone(), null_value, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(read_block);
    let read_value = lower_array_access_from_value(ctx, array_value, index, expr, warn_on_missing);
    store_value_into_temp(ctx, &temp_name, result_type, read_value, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    take_owned_temp(ctx, &temp_name, expr.span)
}

/// Returns the statically-known key type for an array index expression.
/// Used to decide between Op::ArrayGet (int key) and Op::ArrayGetMixedKey.
fn index_expr_key_type(_ctx: &LoweringContext<'_, '_>, index: &Expr) -> PhpType {
    let ty = infer_expr_type_syntactic(index);
    normalized_array_key_type(index, ty)
}

/// Returns the best PHP result type for a lowered array/string/hash access.
fn array_access_result_type(
    ctx: &LoweringContext<'_, '_>,
    array: crate::ir::ValueId,
    op: Op,
    expr: &Expr,
) -> PhpType {
    match op {
        Op::StrCharAt => PhpType::Str,
        Op::ArrayGet | Op::ArrayGetSilent => match ctx.builder.value_php_type(array).codegen_repr() {
            PhpType::Array(elem_ty) => {
                array_access_element_result_type(normalize_value_php_type(*elem_ty))
            }
            _ => fallback_expr_type(expr),
        },
        Op::HashGet | Op::HashGetSilent => match ctx.builder.value_php_type(array).codegen_repr() {
            PhpType::AssocArray { value, .. } => {
                array_access_element_result_type(normalize_value_php_type(*value))
            }
            _ => fallback_expr_type(expr),
        },
        Op::BufferGet => match ctx.builder.value_php_type(array).codegen_repr() {
            PhpType::Buffer(elem_ty) => normalize_value_php_type(*elem_ty),
            _ => fallback_expr_type(expr),
        },
        Op::RuntimeCall => array_access_runtime_call_result_type(ctx, array, expr),
        Op::ArrayGetMixedKey | Op::ArrayGetMixedKeySilent => PhpType::Mixed,
        _ => match ctx.builder.value_php_type(array).codegen_repr() {
            PhpType::Mixed | PhpType::Union(_) => PhpType::Mixed,
            _ => fallback_expr_type(expr),
        },
    }
}

/// Returns the materialized result type for a PHP array read, including miss-capable int reads.
pub(crate) fn array_access_element_result_type(element_ty: PhpType) -> PhpType {
    if crate::codegen::sentinels::null_repr_is_tagged() && matches!(element_ty, PhpType::Int) {
        PhpType::TaggedScalar
    } else {
        element_ty
    }
}

/// Returns the EIR result type for object indexing routed through `ArrayAccess::offsetGet`.
fn array_access_runtime_call_result_type(
    ctx: &LoweringContext<'_, '_>,
    array: crate::ir::ValueId,
    expr: &Expr,
) -> PhpType {
    match ctx.builder.value_php_type(array).codegen_repr() {
        PhpType::Object(class_name) => array_access_offset_get_return_type(ctx, &class_name)
            .unwrap_or_else(|| fallback_expr_type(expr)),
        PhpType::Mixed => PhpType::Mixed,
        _ => fallback_expr_type(expr),
    }
}

/// Looks up the effective `offsetGet` return type for an ArrayAccess class.
fn array_access_offset_get_return_type(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
) -> Option<PhpType> {
    if !object_name_satisfies_interface_for_ir(ctx, class_name, "ArrayAccess") {
        return None;
    }
    let method_key = php_symbol_key("offsetGet");
    class_method_return_type_for_ir(ctx, class_name, &method_key)
        .or_else(|| interface_method_return_type_for_ir(ctx, "ArrayAccess", &method_key))
        .map(normalize_value_php_type)
}

/// Returns true when a syntactic array receiver is statically known as `ArrayAccess`.
fn array_access_expr_satisfies_array_access(
    ctx: &LoweringContext<'_, '_>,
    array: &Expr,
) -> bool {
    let ty = match &array.kind {
        ExprKind::Variable(name) => ctx
            .local_types
            .get(name)
            .cloned()
            .unwrap_or_else(|| infer_expr_type_syntactic(array)),
        _ => infer_expr_type_syntactic(array),
    };
    type_satisfies_array_access_for_ir(ctx, &ty)
}

/// Returns true when every possible object arm satisfies PHP's `ArrayAccess` interface.
pub(crate) fn type_satisfies_array_access_for_ir(
    ctx: &LoweringContext<'_, '_>,
    ty: &PhpType,
) -> bool {
    match ty {
        PhpType::Object(class_name) => {
            object_name_satisfies_interface_for_ir(ctx, class_name, "ArrayAccess")
        }
        PhpType::Union(members) => {
            let mut saw_object = false;
            for member in members {
                match member {
                    PhpType::Void | PhpType::Never => {}
                    other if type_satisfies_array_access_for_ir(ctx, other) => {
                        saw_object = true;
                    }
                    _ => return false,
                }
            }
            saw_object
        }
        _ => false,
    }
}

/// Returns true when a class or interface name satisfies the requested interface.
fn object_name_satisfies_interface_for_ir(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    interface_name: &str,
) -> bool {
    let normalized = class_name.trim_start_matches('\\');
    if php_symbol_key(normalized) == php_symbol_key(interface_name.trim_start_matches('\\')) {
        return true;
    }
    if ctx.interfaces.contains_key(normalized) {
        return interface_extends_interface_for_ir(ctx, normalized, interface_name);
    }
    class_implements_interface_for_ir(ctx, normalized, interface_name)
}

/// Returns whether a lowered class implements an interface, following parents.
fn class_implements_interface_for_ir(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    interface_name: &str,
) -> bool {
    let interface_key = php_symbol_key(interface_name.trim_start_matches('\\'));
    let mut current = Some(class_name.trim_start_matches('\\'));
    while let Some(candidate) = current {
        let Some(info) = ctx.classes.get(candidate) else {
            return false;
        };
        if info
            .interfaces
            .iter()
            .any(|interface| {
                let interface = interface.trim_start_matches('\\');
                php_symbol_key(interface) == interface_key
                    || interface_extends_interface_for_ir(ctx, interface, interface_name)
            })
        {
            return true;
        }
        current = info.parent.as_deref();
    }
    false
}

/// Returns true when an interface extends the requested ancestor interface.
fn interface_extends_interface_for_ir(
    ctx: &LoweringContext<'_, '_>,
    interface_name: &str,
    ancestor_name: &str,
) -> bool {
    if php_symbol_key(interface_name.trim_start_matches('\\'))
        == php_symbol_key(ancestor_name.trim_start_matches('\\'))
    {
        return true;
    }
    let Some(info) = ctx.interfaces.get(interface_name.trim_start_matches('\\')) else {
        return false;
    };
    info.parents.iter().any(|parent| {
        let parent = parent.trim_start_matches('\\');
        php_symbol_key(parent) == php_symbol_key(ancestor_name.trim_start_matches('\\'))
            || interface_extends_interface_for_ir(ctx, parent, ancestor_name)
    })
}

/// Returns a method return type from class metadata, following parent classes.
fn class_method_return_type_for_ir(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    method_key: &str,
) -> Option<PhpType> {
    let mut current = Some(class_name.trim_start_matches('\\'));
    while let Some(candidate) = current {
        let info = ctx.classes.get(candidate)?;
        if let Some(sig) = info.methods.get(method_key) {
            return Some(sig.return_type.clone());
        }
        current = info.parent.as_deref();
    }
    None
}

/// Returns a method return type from interface metadata, following interface parents.
fn interface_method_return_type_for_ir(
    ctx: &LoweringContext<'_, '_>,
    interface_name: &str,
    method_key: &str,
) -> Option<PhpType> {
    let mut visited = std::collections::HashSet::new();
    let mut queue = vec![interface_name.trim_start_matches('\\').to_string()];
    while let Some(name) = queue.pop() {
        if !visited.insert(name.clone()) {
            continue;
        }
        let Some(info) = ctx.interfaces.get(&name) else {
            continue;
        };
        if let Some(sig) = info.methods.get(method_key) {
            return Some(sig.return_type.clone());
        }
        queue.extend(info.parents.iter().cloned());
    }
    None
}

/// Lowers a ternary expression with lazy branch evaluation.
fn lower_ternary(
    ctx: &mut LoweringContext<'_, '_>,
    condition: &Expr,
    then_expr: &Expr,
    else_expr: &Expr,
    expr: &Expr,
) -> LoweredValue {
    let cond = lower_expr(ctx, condition);
    let cond = ctx.truthy(cond, Some(condition.span));
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
fn lower_cast(ctx: &mut LoweringContext<'_, '_>, target: &CastType, inner: &Expr, expr: &Expr) -> LoweredValue {
    let value = lower_expr(ctx, inner);
    // Keep the original producer visible for a no-op string cast. Wrapping an
    // owned string temporary in `Cast(Str)` would hide its ownership from the
    // retaining store/call cleanup and leak the detached string allocation.
    if matches!(target, CastType::String) && value.ir_type == IrType::Str {
        return value;
    }
    let php_type = cast_php_type(target);
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
        && ctx.value_is_owning_temporary(value)
    {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(expr.span));
    }
    result
}

/// Releases an owning temporary when a scalar coercion cannot alias its source storage.
fn release_coerced_source_if_owned(
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
fn coerced_source_repr_is_releasable(php_type: &PhpType) -> bool {
    matches!(
        php_type.codegen_repr(),
        PhpType::Object(_) | PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Mixed
    )
}

/// Returns the PHP type produced by a cast.
fn cast_php_type(target: &CastType) -> PhpType {
    match target {
        CastType::Int => PhpType::Int,
        CastType::Float => PhpType::Float,
        CastType::String => PhpType::Str,
        CastType::Bool => PhpType::Bool,
        CastType::Array => PhpType::Array(Box::new(PhpType::Mixed)),
    }
}

/// Lowers a closure expression into a callable descriptor backed by an EIR closure function.
fn lower_closure(
    ctx: &mut LoweringContext<'_, '_>,
    params: &[(String, Option<TypeExpr>, Option<Expr>, bool)],
    variadic: Option<&str>,
    variadic_by_ref: bool,
    return_type: Option<&TypeExpr>,
    body: &[crate::parser::ast::Stmt],
    captures: &[String],
    capture_refs: &[String],
    expr: &Expr,
    is_static: bool,
) -> LoweredValue {
    lower_closure_with_context(
        ctx,
        params,
        variadic,
        variadic_by_ref,
        return_type,
        body,
        captures,
        capture_refs,
        expr,
        &[],
        None,
        is_static,
    )
}

/// Lowers a closure assigned to a local and specializes self by-reference captures as callable.
pub(crate) fn lower_closure_for_assignment(
    ctx: &mut LoweringContext<'_, '_>,
    assigned_name: &str,
    value: &Expr,
) -> Option<LoweredValue> {
    let ExprKind::Closure {
        params,
        variadic,
        variadic_by_ref,
        return_type,
        body,
        captures,
        capture_refs,
        is_static,
        ..
    } = &value.kind
    else {
        return None;
    };
    if !capture_refs.iter().any(|capture| capture == assigned_name) {
        return None;
    }
    Some(lower_closure_with_context(
        ctx,
        params,
        variadic.as_deref(),
        *variadic_by_ref,
        return_type.as_ref(),
        body,
        captures,
        capture_refs,
        value,
        &[],
        Some(assigned_name),
        *is_static,
    ))
}

/// Lowers a closure expression, applying contextual types to unannotated parameters.
fn lower_closure_with_context(
    ctx: &mut LoweringContext<'_, '_>,
    params: &[(String, Option<TypeExpr>, Option<Expr>, bool)],
    variadic: Option<&str>,
    variadic_by_ref: bool,
    return_type: Option<&TypeExpr>,
    body: &[crate::parser::ast::Stmt],
    captures: &[String],
    capture_refs: &[String],
    expr: &Expr,
    contextual_arg_types: &[PhpType],
    self_ref_callable_capture: Option<&str>,
    is_static: bool,
) -> LoweredValue {
    // PHP auto-binds `$this` to non-static closures (including arrow functions)
    // defined inside an instance method, with no `use($this)` needed. The parser
    // never lists `$this` as a capture, so thread it through the existing capture
    // machinery here: load the enclosing `this` and append it to the captures so
    // the closure body gets a `this` local. Only capture when the body actually
    // references `$this` (directly or in a nested closure) — adding an unused
    // capture would push otherwise capture-free closures through capture-only
    // runtime paths. Nested closures compose: each level captures `this` from the
    // level above.
    // A method-defined closure loads the enclosing `this`; a top-level closure
    // that uses `$this` (bound later via `Closure::bind`) gets a null `this`
    // slot the bind fills, typed `Mixed` for runtime-dispatched member access.
    let with_this;
    let captures: &[String] = if !is_static
        && !captures.iter().any(|name| name == "this")
        && crate::types::checker::closure_body_uses_this(body)
    {
        with_this = captures
            .iter()
            .cloned()
            .chain(std::iter::once("this".to_string()))
            .collect::<Vec<_>>();
        &with_this
    } else {
        captures
    };
    let body_contains_eval = body_contains_eval_call(body);
    let mut captured_values = Vec::with_capacity(captures.len());
    let mut capture_params = Vec::with_capacity(captures.len());
    for capture in captures {
        let by_ref = capture_refs.iter().any(|name| name == capture);
        let (captured, php_type) = if capture == "this" && !ctx.local_slots.contains_key("this") {
            // Top-level closure: no enclosing `$this`. Start with a null receiver
            // that `Closure::bind` overwrites; `Mixed` so members dispatch at
            // runtime against the bound object's class.
            (lower_null(ctx, expr), PhpType::Mixed)
        } else {
            let php_type_override = if by_ref && self_ref_callable_capture == Some(capture.as_str()) {
                Some(PhpType::Callable)
            } else if by_ref && body_contains_eval {
                ctx.set_local_type(capture, PhpType::Mixed);
                Some(PhpType::Mixed)
            } else {
                None
            };
            let captured = ctx.load_local(capture, Some(expr.span));
            let php_type = php_type_override
                .unwrap_or_else(|| ctx.builder.value_php_type(captured.value));
            (captured, php_type)
        };
        let immediate = by_ref.then_some(Immediate::I64(1));
        ctx.emit_void(Op::ClosureCapture, vec![captured.value], immediate, Op::ClosureCapture.default_effects(), Some(expr.span));
        if by_ref {
            ctx.mark_ref_bound_local(capture);
        }
        captured_values.push(ClosureCapture { value: captured.value });
        capture_params.push((capture.clone(), php_type, by_ref));
    }
    let name = ctx.next_closure_name();
    let by_ref_return = matches!(&expr.kind, ExprKind::Closure { by_ref_return: true, .. });
    let signature = if contextual_arg_types.is_empty() {
        function::lower_closure_function(
            ctx,
            &name,
            params,
            variadic,
            variadic_by_ref,
            return_type,
            body,
            &capture_params,
            self_ref_callable_capture,
            by_ref_return,
        )
    } else {
        function::lower_closure_function_with_context(
            ctx,
            &name,
            params,
            variadic,
            variadic_by_ref,
            return_type,
            body,
            &capture_params,
            contextual_arg_types,
            self_ref_callable_capture,
            by_ref_return,
        )
    };
    let data = ctx.intern_string(&name);
    let closure_operands = captured_values
        .iter()
        .map(|capture| capture.value)
        .collect::<Vec<_>>();
    ctx.set_pending_static_callable_result(StaticCallableBinding::Closure {
        name,
        signature,
        captures: captured_values,
    });
    let closure = ctx.emit_value(
        Op::ClosureNew,
        closure_operands,
        Some(Immediate::Data(data)),
        PhpType::Callable,
        Op::ClosureNew.default_effects(),
        Some(expr.span),
    );
    if let Some(capture) = self_ref_callable_capture {
        ctx.set_local_logical_type(capture, PhpType::Callable);
    }
    closure
}

/// Returns true when a statement body contains an `eval(...)` call.
fn body_contains_eval_call(body: &[Stmt]) -> bool {
    body.iter().any(stmt_contains_eval_call)
}

/// Returns true when a statement or nested statement body contains an `eval(...)` call.
fn stmt_contains_eval_call(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::Echo(expr)
        | StmtKind::Throw(expr)
        | StmtKind::ExprStmt(expr)
        | StmtKind::ConstDecl { value: expr, .. }
        | StmtKind::ListUnpack { value: expr, .. }
        | StmtKind::StaticVar { init: expr, .. }
        | StmtKind::Assign { value: expr, .. }
        | StmtKind::TypedAssign { value: expr, .. }
        | StmtKind::ArrayPush { value: expr, .. }
        | StmtKind::StaticPropertyAssign { value: expr, .. }
        | StmtKind::StaticPropertyArrayPush { value: expr, .. } => expr_contains_eval_call(expr),
        StmtKind::Return(expr) => expr.as_ref().is_some_and(expr_contains_eval_call),
        StmtKind::ArrayAssign { index, value, .. }
        | StmtKind::StaticPropertyArrayAssign { index, value, .. }
        | StmtKind::PropertyArrayAssign { index, value, .. } => {
            expr_contains_eval_call(index) || expr_contains_eval_call(value)
        }
        StmtKind::NestedArrayAssign { target, value } => {
            expr_contains_eval_call(target) || expr_contains_eval_call(value)
        }
        StmtKind::PropertyAssign { object, value, .. }
        | StmtKind::PropertyArrayPush { object, value, .. } => {
            expr_contains_eval_call(object) || expr_contains_eval_call(value)
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            expr_contains_eval_call(condition)
                || body_contains_eval_call(then_body)
                || elseif_clauses.iter().any(|(condition, body)| {
                    expr_contains_eval_call(condition) || body_contains_eval_call(body)
                })
                || else_body.as_ref().is_some_and(|body| body_contains_eval_call(body))
        }
        StmtKind::IfDef { then_body, else_body, .. } => {
            body_contains_eval_call(then_body)
                || else_body.as_ref().is_some_and(|body| body_contains_eval_call(body))
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
            expr_contains_eval_call(condition) || body_contains_eval_call(body)
        }
        StmtKind::For { init, condition, update, body } => {
            init.as_deref().is_some_and(stmt_contains_eval_call)
                || condition.as_ref().is_some_and(expr_contains_eval_call)
                || update.as_deref().is_some_and(stmt_contains_eval_call)
                || body_contains_eval_call(body)
        }
        StmtKind::Foreach { array, body, .. } => {
            expr_contains_eval_call(array) || body_contains_eval_call(body)
        }
        StmtKind::Switch { subject, cases, default } => {
            expr_contains_eval_call(subject)
                || cases.iter().any(|(patterns, body)| {
                    patterns.iter().any(expr_contains_eval_call) || body_contains_eval_call(body)
                })
                || default.as_ref().is_some_and(|body| body_contains_eval_call(body))
        }
        StmtKind::Include { path, .. } => expr_contains_eval_call(path),
        StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. } => body_contains_eval_call(body),
        StmtKind::FunctionDecl { params, body, .. } => {
            params
                .iter()
                .any(|(_, _, default, _)| default.as_ref().is_some_and(expr_contains_eval_call))
                || body_contains_eval_call(body)
        }
        StmtKind::ClassDecl { properties, methods, constants, .. }
        | StmtKind::TraitDecl { properties, methods, constants, .. }
        | StmtKind::InterfaceDecl { properties, methods, constants, .. } => {
            properties.iter().any(|property| {
                property.default.as_ref().is_some_and(expr_contains_eval_call)
            }) || constants
                .iter()
                .any(|constant| expr_contains_eval_call(&constant.value))
                || methods.iter().any(|method| {
                    method.params.iter().any(|(_, _, default, _)| {
                        default.as_ref().is_some_and(expr_contains_eval_call)
                    }) || body_contains_eval_call(&method.body)
                })
        }
        StmtKind::Try { try_body, catches, finally_body } => {
            body_contains_eval_call(try_body)
                || catches.iter().any(|catch_clause| body_contains_eval_call(&catch_clause.body))
                || finally_body.as_ref().is_some_and(|body| body_contains_eval_call(body))
        }
        StmtKind::EnumDecl { cases, .. } => cases
            .iter()
            .any(|case| case.value.as_ref().is_some_and(expr_contains_eval_call)),
        StmtKind::RefAssign { .. }
        | StmtKind::Break(_)
        | StmtKind::Continue(_)
        | StmtKind::NamespaceDecl { .. }
        | StmtKind::UseDecl { .. }
        | StmtKind::FunctionVariantGroup { .. }
        | StmtKind::FunctionVariantMark { .. }
        | StmtKind::IncludeOnceMark { .. }
        | StmtKind::Global { .. }
        | StmtKind::PackedClassDecl { .. }
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. } => false,
    }
}

/// Returns true when an expression contains an `eval(...)` call.
fn expr_contains_eval_call(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::FunctionCall { name, args } => {
            is_eval_call_name(name) || args.iter().any(expr_contains_eval_call)
        }
        ExprKind::BinaryOp { left, right, .. } => {
            expr_contains_eval_call(left) || expr_contains_eval_call(right)
        }
        ExprKind::InstanceOf { value, target } => {
            expr_contains_eval_call(value) || instance_of_target_contains_eval_call(target)
        }
        ExprKind::Negate(expr)
        | ExprKind::Not(expr)
        | ExprKind::BitNot(expr)
        | ExprKind::Throw(expr)
        | ExprKind::Clone(expr)
        | ExprKind::ErrorSuppress(expr)
        | ExprKind::Print(expr)
        | ExprKind::Spread(expr)
        | ExprKind::Cast { expr, .. }
        | ExprKind::PtrCast { expr, .. }
        | ExprKind::BufferNew { len: expr, .. }
        | ExprKind::ObjectClassName { object: expr }
        | ExprKind::YieldFrom(expr) => expr_contains_eval_call(expr),
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default }
        | ExprKind::Pipe { value, callable: default }
        | ExprKind::ArrayAccess { array: value, index: default } => {
            expr_contains_eval_call(value) || expr_contains_eval_call(default)
        }
        ExprKind::Assignment { target, value, result_target, prelude, .. } => {
            expr_contains_eval_call(target)
                || expr_contains_eval_call(value)
                || result_target.as_ref().is_some_and(|target| expr_contains_eval_call(target))
                || body_contains_eval_call(prelude)
        }
        ExprKind::ArrayLiteral(items) => items.iter().any(expr_contains_eval_call),
        ExprKind::ArrayLiteralAssoc(entries) => entries
            .iter()
            .any(|(key, value)| expr_contains_eval_call(key) || expr_contains_eval_call(value)),
        ExprKind::Match { subject, arms, default } => {
            expr_contains_eval_call(subject)
                || arms.iter().any(|(patterns, value)| {
                    patterns.iter().any(expr_contains_eval_call) || expr_contains_eval_call(value)
                })
                || default.as_ref().is_some_and(|default| expr_contains_eval_call(default))
        }
        ExprKind::Ternary { condition, then_expr, else_expr } => {
            expr_contains_eval_call(condition)
                || expr_contains_eval_call(then_expr)
                || expr_contains_eval_call(else_expr)
        }
        ExprKind::Closure { params, body, .. } => {
            params
                .iter()
                .any(|(_, _, default, _)| default.as_ref().is_some_and(expr_contains_eval_call))
                || body_contains_eval_call(body)
        }
        ExprKind::NamedArg { value, .. } => expr_contains_eval_call(value),
        ExprKind::ClosureCall { args, .. }
        | ExprKind::StaticMethodCall { args, .. }
        | ExprKind::NewObject { args, .. }
        | ExprKind::NewScopedObject { args, .. } => args.iter().any(expr_contains_eval_call),
        ExprKind::ExprCall { callee, args } => {
            expr_contains_eval_call(callee) || args.iter().any(expr_contains_eval_call)
        }
        ExprKind::NewDynamic { name_expr, args } => {
            expr_contains_eval_call(name_expr) || args.iter().any(expr_contains_eval_call)
        }
        ExprKind::NewDynamicObject { class_name, args, .. } => {
            expr_contains_eval_call(class_name) || args.iter().any(expr_contains_eval_call)
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => expr_contains_eval_call(object),
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            expr_contains_eval_call(object) || expr_contains_eval_call(property)
        }
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            expr_contains_eval_call(object) || args.iter().any(expr_contains_eval_call)
        }
        ExprKind::NullsafeDynamicMethodCall {
            object,
            method,
            args,
        } => {
            expr_contains_eval_call(object)
                || expr_contains_eval_call(method)
                || args.iter().any(expr_contains_eval_call)
        }
        ExprKind::FirstClassCallable(target) => callable_target_contains_eval_call(target),
        ExprKind::Yield { key, value } => {
            key.as_ref().is_some_and(|key| expr_contains_eval_call(key))
                || value.as_ref().is_some_and(|value| expr_contains_eval_call(value))
        }
        ExprKind::IncludeValue { path, .. } => expr_contains_eval_call(path),
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::Variable(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::PreIncrement(_)
        | ExprKind::PostIncrement(_)
        | ExprKind::PreDecrement(_)
        | ExprKind::PostDecrement(_)
        | ExprKind::ConstRef(_)
        | ExprKind::StaticPropertyAccess { .. }
        | ExprKind::This
        | ExprKind::ClassConstant { .. }
        | ExprKind::ScopedConstantAccess { .. }
        | ExprKind::MagicConstant(_) => false,
    }
}

/// Returns true when an `instanceof` target expression contains an `eval(...)` call.
fn instance_of_target_contains_eval_call(target: &InstanceOfTarget) -> bool {
    match target {
        InstanceOfTarget::Name(_) => false,
        InstanceOfTarget::Expr(expr) => expr_contains_eval_call(expr),
    }
}

/// Returns true when a first-class callable target contains an `eval(...)` call.
fn callable_target_contains_eval_call(target: &CallableTarget) -> bool {
    match target {
        CallableTarget::Function(_) | CallableTarget::StaticMethod { .. } => false,
        CallableTarget::Method { object, .. } => expr_contains_eval_call(object),
    }
}

/// Returns true when a function call name resolves to PHP's `eval` construct.
fn is_eval_call_name(name: &Name) -> bool {
    php_symbol_key(name.as_str().trim_start_matches('\\')) == "eval"
}

/// Lowers a closure variable call.
fn lower_closure_call(ctx: &mut LoweringContext<'_, '_>, var: &str, args: &[Expr], expr: &Expr) -> LoweredValue {
    if let Some(value) = lower_invokable_object_variable_call(ctx, var, args, expr) {
        return value;
    }
    let mut result_type = None;
    let mut instance_signature = None;
    if let Some(target) = ctx.static_callable_local(var) {
        result_type = Some(static_callable_return_type(ctx, &target));
        instance_signature = instance_callable_signature(&target).cloned();
        if let Some(value) = lower_static_callable_call(ctx, target, args, expr) {
            return value;
        }
    }
    let callable = ctx.load_local(var, Some(expr.span));
    let result_type = result_type.unwrap_or_else(|| dynamic_callable_result_type(ctx, callable.value, expr));
    if instance_signature.is_none() {
        if let Some(arg_container) =
            lower_untyped_descriptor_invoker_arg_container(ctx, args, expr.span)
        {
            return emit_callable_descriptor_invoke(
                ctx,
                callable,
                arg_container,
                result_type,
                expr.span,
            );
        }
    }
    let mut operands = vec![callable.value];
    operands.extend(lower_args_with_signature(ctx, instance_signature.as_ref(), args));
    ctx.emit_value(Op::ClosureCall, operands, None, result_type, Op::ClosureCall.default_effects(), Some(expr.span))
}

/// Lowers `$object(...)` when the local object has an `__invoke` method.
fn lower_invokable_object_variable_call(
    ctx: &mut LoweringContext<'_, '_>,
    var: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let object = Expr::new(ExprKind::Variable(var.to_string()), expr.span);
    lower_invokable_object_expr_call(ctx, &object, args, expr)
}

/// Lowers invokable object calls through the normal method-call path.
fn lower_invokable_object_expr_call(
    ctx: &mut LoweringContext<'_, '_>,
    callee: &Expr,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if !is_invokable_object_expr(ctx, callee) {
        return None;
    }
    Some(lower_method_call(ctx, callee, "__invoke", args, Op::MethodCall, expr))
}

/// Returns true when an expression is known to evaluate to an object with `__invoke`.
fn is_invokable_object_expr(
    ctx: &LoweringContext<'_, '_>,
    callee: &Expr,
) -> bool {
    instance_callable_object_class(ctx, callee)
        .and_then(|class_name| class_method_signature(ctx, &class_name, "__invoke"))
        .is_some()
}

/// Lowers an expression call.
fn lower_expr_call(ctx: &mut LoweringContext<'_, '_>, callee: &Expr, args: &[Expr], expr: &Expr) -> LoweredValue {
    if let Some(value) = lower_invokable_object_expr_call(ctx, callee, args, expr) {
        return value;
    }
    if let Some(value) = lower_first_class_callable_expr_call(ctx, callee, args, expr) {
        return value;
    }
    if let Some(value) = lower_literal_callable_array_expr_call(ctx, callee, args, expr) {
        return value;
    }
    if let Some(callback) = static_call_user_func_callback(ctx, callee) {
        if let Some(value) = lower_static_callable_call(ctx, callback, args, expr) {
            return value;
        }
    }
    if let Some(callback) = static_assignment_callable_target(ctx, callee) {
        lower_expr(ctx, callee);
        if let Some(value) = lower_static_callable_call(ctx, callback, args, expr) {
            return value;
        }
    }
    // `Closure::bind(fn &() => $this->prop, $obj, $obj)()` invokes the bound closure. Lower it
    // as a direct call to the closure with `$obj` boxed as its `$this` capture, so a
    // by-reference return passes the property's ref-cell pointer through (the generic runtime
    // descriptor invoker boxes results and cannot).
    if let Some(value) = lower_bound_closure_immediate_call(ctx, callee, args, expr) {
        return value;
    }
    let lowered_callee = lower_expr(ctx, callee);
    // An immediately-invoked closure literal (`(fn &() => …)()`) registers its static
    // callable binding while lowering. Call it directly through the static-callable path
    // (as `$f()` does) so the closure body's signature — including a by-reference return —
    // drives the call instead of the generic descriptor-invoke path, which cannot return
    // every result type.
    if let Some(target) = ctx.take_pending_static_callable_result() {
        if let Some(value) = lower_static_callable_call(ctx, target, args, expr) {
            return value;
        }
    }
    let result_type = dynamic_callable_result_type(ctx, lowered_callee.value, expr);
    if let Some(arg_container) =
        lower_untyped_descriptor_invoker_arg_container(ctx, args, expr.span)
    {
        return emit_callable_descriptor_invoke(
            ctx,
            lowered_callee,
            arg_container,
            result_type,
            expr.span,
        );
    }
    let mut operands = vec![lowered_callee.value];
    operands.extend(lower_args(ctx, args));
    ctx.emit_value(Op::ExprCall, operands, None, result_type, Op::ExprCall.default_effects(), Some(expr.span))
}

/// Recognizes the parser's internal `call_user_func([$object, $method], ...)`
/// desugaring for ordinary dynamic method syntax without changing explicit calls.
fn lower_desugared_dynamic_method_call(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if php_symbol_key(name.trim_start_matches('\\')) != "call_user_func" {
        return None;
    }
    let callback = args.first()?;
    if callback.span != expr.span {
        return None;
    }
    let ExprKind::ArrayLiteral(items) = &callback.kind else {
        return None;
    };
    let [object, method] = items.as_slice() else {
        return None;
    };
    Some(lower_dynamic_method_expr_call(
        ctx,
        object,
        method,
        &args[1..],
        expr,
    ))
}

/// Lowers `$object->{$method}(...)` as a dynamic method call, preserving PHP's
/// receiver/name evaluation before the null check and lazy argument evaluation.
fn lower_dynamic_method_expr_call(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    method: &Expr,
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let object = lower_expr(ctx, object);
    let method = lower_expr(ctx, method);
    let method_type = ctx.builder.value_php_type(method.value);
    let method_name = ctx.declare_hidden_temp(method_type.clone());
    ctx.store_local(&method_name, method, method_type, Some(expr.span));
    let method_expr = Expr::new(ExprKind::Variable(method_name), expr.span);
    let object_type = ctx.builder.value_php_type(object.value).codegen_repr();
    if !matches!(object_type, PhpType::Object(_))
        && !value_is_nullable(ctx, object.value)
        && !value_may_carry_container_miss(ctx, object.value)
    {
        return lower_dynamic_method_call_with_receiver(ctx, object, &method_expr, args, expr);
    }
    lower_nullable_dynamic_method_expr_call(ctx, object, &method_expr, args, expr)
}

/// Splits a dynamic method call so a null receiver throws before lowering any
/// call argument, while the already evaluated runtime method name is preserved.
fn lower_nullable_dynamic_method_expr_call(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    method: &Expr,
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let fatal_block = ctx
        .builder
        .create_named_block("dynamic_method.null.fatal", Vec::new());
    let call_block = ctx
        .builder
        .create_named_block("dynamic_method.non_null.call", Vec::new());
    let is_null = ctx.emit_value(
        Op::IsNull,
        vec![object.value],
        None,
        PhpType::Bool,
        Op::IsNull.default_effects(),
        Some(expr.span),
    );
    ctx.builder.terminate(Terminator::CondBr {
        cond: is_null.value,
        then_target: fatal_block,
        then_args: Vec::new(),
        else_target: call_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(fatal_block);
    terminate_dynamic_method_call_on_null(ctx, method, expr);

    ctx.builder.position_at_end(call_block);
    lower_dynamic_method_call_with_receiver(ctx, object, method, args, expr)
}

/// Throws a catchable PHP `Error` with the runtime dynamic method name.
fn terminate_dynamic_method_call_on_null(
    ctx: &mut LoweringContext<'_, '_>,
    method: &Expr,
    expr: &Expr,
) {
    let prefix = Expr::new(
        ExprKind::StringLiteral("Call to a member function ".to_string()),
        expr.span,
    );
    let prefix_and_method = Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(prefix),
            op: BinOp::Concat,
            right: Box::new(method.clone()),
        },
        expr.span,
    );
    let suffix = Expr::new(ExprKind::StringLiteral("() on null".to_string()), expr.span);
    let message = Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(prefix_and_method),
            op: BinOp::Concat,
            right: Box::new(suffix),
        },
        expr.span,
    );
    let message = lower_expr(ctx, &message);
    let message = ctx.emit_value(
        Op::StrPersist,
        vec![message.value],
        None,
        PhpType::Str,
        Op::StrPersist.default_effects(),
        Some(expr.span),
    );
    ctx.emit_void(
        Op::ThrowErrorValue,
        vec![message.value],
        None,
        Op::ThrowErrorValue.default_effects(),
        Some(expr.span),
    );
    ctx.builder.terminate(Terminator::Unreachable);
}

/// Lowers direct calls to literal callable arrays through descriptor metadata.
fn lower_literal_callable_array_expr_call(
    ctx: &mut LoweringContext<'_, '_>,
    callee: &Expr,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let ExprKind::ArrayLiteral(items) = &callee.kind else {
        return None;
    };
    if let Some(StaticCallableBinding::StaticMethodDescriptor { receiver, method }) =
        static_array_callable_descriptor_target(ctx, items)
    {
        return Some(lower_static_method_descriptor_call(ctx, &receiver, &method, args, expr));
    }
    instance_array_callable_target(ctx, items)?;
    let lowered_callee = lower_expr(ctx, callee);
    let result_type = dynamic_callable_result_type(ctx, lowered_callee.value, expr);
    let arg_container = lower_untyped_descriptor_invoker_arg_container(ctx, args, expr.span)?;
    Some(emit_callable_descriptor_invoke(
        ctx,
        lowered_callee,
        arg_container,
        result_type,
        expr.span,
    ))
}

/// Lowers an expression call once the callable expression is already evaluated.
fn lower_expr_call_from_value(
    ctx: &mut LoweringContext<'_, '_>,
    callee: LoweredValue,
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let result_type = dynamic_callable_result_type(ctx, callee.value, expr);
    if let Some(arg_container) =
        lower_untyped_descriptor_invoker_arg_container(ctx, args, expr.span)
    {
        return emit_callable_descriptor_invoke(ctx, callee, arg_container, result_type, expr.span);
    }
    let mut operands = vec![callee.value];
    operands.extend(lower_args(ctx, args));
    ctx.emit_value(
        Op::ExprCall,
        operands,
        None,
        result_type,
        Op::ExprCall.default_effects(),
        Some(expr.span),
    )
}

/// Lowers explicit named arguments for signature-unknown descriptor invocations.
fn lower_untyped_descriptor_invoker_arg_container(
    ctx: &mut LoweringContext<'_, '_>,
    args: &[Expr],
    span: Span,
) -> Option<LoweredValue> {
    if crate::types::call_args::has_named_args(args) {
        return Some(lower_untyped_descriptor_invoker_hash_container(ctx, args, span));
    }
    Some(lower_untyped_descriptor_invoker_indexed_container(ctx, args, span))
}

/// Builds an indexed descriptor-invoker container for signature-unknown calls.
fn lower_untyped_descriptor_invoker_indexed_container(
    ctx: &mut LoweringContext<'_, '_>,
    args: &[Expr],
    span: Span,
) -> LoweredValue {
    let elem_ty = PhpType::Mixed;
    let array_ty = PhpType::Array(Box::new(elem_ty.clone()));
    let array = ctx.emit_value(
        Op::ArrayNew,
        Vec::new(),
        Some(Immediate::Capacity(args.len() as u32)),
        array_ty.clone(),
        Op::ArrayNew.default_effects(),
        Some(span),
    );
    for arg in args {
        if let ExprKind::Spread(inner) = &arg.kind {
            let source = lower_expr(ctx, inner);
            lower_indexed_array_spread_into_array(ctx, array, source, Some(&elem_ty), arg.span);
            continue;
        }
        let value = lower_untyped_descriptor_invoker_arg_value(ctx, arg);
        ctx.emit_void(
            Op::ArrayPush,
            vec![array.value, value.value],
            None,
            Op::ArrayPush.default_effects(),
            Some(arg.span),
        );
        super::stmt::release_indexed_array_write_operand(ctx, Some(&elem_ty), value, arg.span);
    }
    array
}

/// Builds an associative descriptor-invoker container for named or named/spread calls.
fn lower_untyped_descriptor_invoker_hash_container(
    ctx: &mut LoweringContext<'_, '_>,
    args: &[Expr],
    span: Span,
) -> LoweredValue {
    let hash_ty = PhpType::AssocArray {
        key: Box::new(PhpType::Mixed),
        value: Box::new(PhpType::Mixed),
    };
    let hash = ctx.emit_value(
        Op::HashNew,
        Vec::new(),
        Some(Immediate::Capacity(args.len() as u32)),
        hash_ty,
        Op::HashNew.default_effects(),
        Some(span),
    );
    let mut next_positional_key = emit_i64_at_span(ctx, 0, span);
    for arg in args {
        match &arg.kind {
            ExprKind::NamedArg { name, value } => {
                let key = lower_string_literal(ctx, name, arg);
                let value = lower_untyped_descriptor_invoker_arg_value(ctx, value);
                ctx.emit_void(
                    Op::HashSet,
                    vec![hash.value, key.value, value.value],
                    None,
                    Op::HashSet.default_effects(),
                    Some(arg.span),
                );
            }
            ExprKind::Spread(inner) => {
                let source = lower_expr(ctx, inner);
                next_positional_key = lower_untyped_descriptor_invoker_spread_into_hash(
                    ctx,
                    hash,
                    source,
                    next_positional_key,
                    arg.span,
                );
            }
            _ => {
                let key = next_positional_key;
                let value = lower_untyped_descriptor_invoker_arg_value(ctx, arg);
                ctx.emit_void(
                    Op::HashSet,
                    vec![hash.value, key.value, value.value],
                    None,
                    Op::HashSet.default_effects(),
                    Some(arg.span),
                );
                let one = emit_i64_at_span(ctx, 1, arg.span);
                next_positional_key = ctx.emit_value(
                    Op::IAdd,
                    vec![key.value, one.value],
                    None,
                    PhpType::Int,
                    Op::IAdd.default_effects(),
                    Some(arg.span),
                );
            }
        }
    }
    ctx.box_value_as_mixed(hash, PhpType::Mixed, Some(span))
}

/// Copies an indexed spread source into a descriptor-invoker hash with numeric keys.
fn lower_untyped_descriptor_invoker_spread_into_hash(
    ctx: &mut LoweringContext<'_, '_>,
    hash: LoweredValue,
    source: LoweredValue,
    start_key: LoweredValue,
    span: Span,
) -> LoweredValue {
    let source_elem_ty = match ctx.builder.value_php_type(source.value).codegen_repr() {
        PhpType::Array(elem_ty) => elem_ty.codegen_repr(),
        _ => PhpType::Mixed,
    };
    let len = ctx.emit_value(
        Op::ArrayLen,
        vec![source.value],
        None,
        PhpType::Int,
        Op::ArrayLen.default_effects(),
        Some(span),
    );
    let zero = emit_i64_at_span(ctx, 0, span);
    let header = ctx.builder.create_named_block("descriptor.spread.next", vec![(IrType::I64, PhpType::Int)]);
    let body = ctx.builder.create_named_block("descriptor.spread.body", Vec::new());
    let exit = ctx.builder.create_named_block("descriptor.spread.exit", Vec::new());
    ctx.builder.terminate(Terminator::Br { target: header, args: vec![zero.value] });

    ctx.builder.position_at_end(header);
    let index = ctx.builder.block_param(header, 0);
    let has_next = ctx.emit_value(
        Op::ICmp,
        vec![index, len.value],
        Some(Immediate::CmpPredicate(CmpPredicate::Slt)),
        PhpType::Bool,
        Op::ICmp.default_effects(),
        Some(span),
    );
    ctx.builder.terminate(Terminator::CondBr {
        cond: has_next.value,
        then_target: body,
        then_args: Vec::new(),
        else_target: exit,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(body);
    let key = ctx.emit_value(
        Op::IAdd,
        vec![start_key.value, index],
        None,
        PhpType::Int,
        Op::IAdd.default_effects(),
        Some(span),
    );
    let value = ctx.emit_value(
        Op::ArrayGet,
        vec![source.value, index],
        None,
        source_elem_ty,
        Op::ArrayGet.default_effects(),
        Some(span),
    );
    let value = coerce_descriptor_invoker_mixed_value(ctx, value, span);
    ctx.emit_void(
        Op::HashSet,
        vec![hash.value, key.value, value.value],
        None,
        Op::HashSet.default_effects(),
        Some(span),
    );
    release_value_after_retaining_insert(ctx, Some(&PhpType::Mixed), value, span);
    let one = emit_i64_at_span(ctx, 1, span);
    let next = ctx.emit_value(
        Op::IAdd,
        vec![index, one.value],
        None,
        PhpType::Int,
        Op::IAdd.default_effects(),
        Some(span),
    );
    ctx.builder.terminate(Terminator::Br { target: header, args: vec![next.value] });

    ctx.builder.position_at_end(exit);
    crate::ir_lower::ownership::release_if_owned(ctx, source, Some(span));
    ctx.emit_value(
        Op::IAdd,
        vec![start_key.value, len.value],
        None,
        PhpType::Int,
        Op::IAdd.default_effects(),
        Some(span),
    )
}

/// Lowers one untyped descriptor argument, preserving variables as ref markers.
fn lower_untyped_descriptor_invoker_arg_value(
    ctx: &mut LoweringContext<'_, '_>,
    arg: &Expr,
) -> LoweredValue {
    let value = match &arg.kind {
        ExprKind::Variable(name) => lower_invoker_ref_arg_marker(ctx, name, arg.span),
        _ => lower_expr(ctx, arg),
    };
    coerce_descriptor_invoker_mixed_value(ctx, value, arg.span)
}

/// Boxes a descriptor-invoker argument value into the Mixed slot shape.
fn coerce_descriptor_invoker_mixed_value(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Span,
) -> LoweredValue {
    if ctx.builder.value_php_type(value.value).codegen_repr() == PhpType::Mixed {
        return value;
    }
    ctx.box_value_as_mixed(value, PhpType::Mixed, Some(span))
}

/// Returns the result storage type for an indirect callable with no static signature.
fn dynamic_callable_result_type(
    ctx: &LoweringContext<'_, '_>,
    callable: ValueId,
    expr: &Expr,
) -> PhpType {
    match ctx.builder.value_php_type(callable).codegen_repr() {
        PhpType::Callable | PhpType::Str | PhpType::Array(_) | PhpType::Mixed | PhpType::Union(_) => PhpType::Mixed,
        _ => fallback_expr_type(expr),
    }
}

/// Resolves an assignment-expression callee whose assigned value is a static callable.
fn static_assignment_callable_target(
    ctx: &LoweringContext<'_, '_>,
    callee: &Expr,
) -> Option<StaticCallableBinding> {
    let ExprKind::Assignment { target, value, .. } = &callee.kind else {
        return None;
    };
    if !matches!(target.kind, ExprKind::Variable(_)) {
        return None;
    }
    static_callable_binding_for_expr(ctx, value).and_then(direct_static_callable_binding)
}

/// Lowers direct invocation of a literal first-class callable target.
fn lower_first_class_callable_expr_call(
    ctx: &mut LoweringContext<'_, '_>,
    callee: &Expr,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    match &callee.kind {
        ExprKind::FirstClassCallable(CallableTarget::Function(name)) => {
            Some(lower_function_call(ctx, name, args, expr))
        }
        ExprKind::FirstClassCallable(CallableTarget::StaticMethod { receiver, method }) => {
            Some(lower_static_method_call(ctx, receiver, method, args, expr))
        }
        ExprKind::FirstClassCallable(target @ CallableTarget::Method { .. }) => {
            let signature = static_callable_binding_for_expr(ctx, callee)
                .and_then(|target| signature_for_static_callable_binding(ctx, target));
            let callable = lower_first_class_callable(ctx, target, callee);
            let result_type = signature
                .as_ref()
                .map(|signature| normalize_value_php_type(signature.return_type.codegen_repr()))
                .unwrap_or_else(|| dynamic_callable_result_type(ctx, callable.value, expr));
            let arg_container =
                lower_untyped_descriptor_invoker_arg_container(ctx, args, expr.span)?;
            Some(emit_callable_descriptor_invoke(
                ctx,
                callable,
                arg_container,
                result_type,
                expr.span,
            ))
        }
        _ => None,
    }
}

/// Lowers fixed-class object construction.
fn lower_new_object(
    ctx: &mut LoweringContext<'_, '_>,
    class_name: &Name,
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    if php_symbol_key(class_name.as_str().trim_start_matches('\\')) == "reflectionclass" {
        if let Some(operands) = lower_reflection_class_constructor_operands(ctx, args) {
            let php_type = PhpType::Object(class_name.as_str().to_string());
            return emit_fixed_object_new(ctx, class_name.as_str(), operands, php_type, expr.span);
        }
    }
    if php_symbol_key(class_name.as_str().trim_start_matches('\\')) == "reflectionparameter" {
        if let Some(operands) = lower_reflection_parameter_constructor_operands(ctx, args) {
            let php_type = PhpType::Object(class_name.as_str().to_string());
            return emit_fixed_object_new(ctx, class_name.as_str(), operands, php_type, expr.span);
        }
    }
    if php_symbol_key(class_name.as_str().trim_start_matches('\\')) == "reflectionmethod" {
        if let Some(operands) = lower_reflection_method_constructor_operands(ctx, args) {
            let php_type = PhpType::Object(class_name.as_str().to_string());
            return emit_fixed_object_new(ctx, class_name.as_str(), operands, php_type, expr.span);
        }
    }
    if ctx.has_eval_barrier()
        && !ctx.classes.contains_key(class_name.as_str())
        && plain_positional_call_args(args)
    {
        let operands = lower_args_with_signature(ctx, None, args);
        let data = ctx.intern_class_name(class_name.as_str());
        return ctx.emit_value(
            Op::EvalObjectNew,
            operands,
            Some(Immediate::Data(data)),
            PhpType::Mixed,
            Op::EvalObjectNew.default_effects(),
            Some(expr.span),
        );
    }
    let sig = constructor_signature(ctx, class_name).cloned();
    let operands = lower_args_with_signature(ctx, sig.as_ref(), args);
    let php_type = PhpType::Object(class_name.as_str().to_string());
    emit_fixed_object_new(ctx, class_name.as_str(), operands, php_type, expr.span)
}

/// Emits fixed-class object construction and releases owned constructor argument temporaries.
///
/// A newly allocated object cannot alias a constructor argument. The constructor has already
/// retained or copied every argument it keeps by the time `ObjectNew` returns, so the caller's
/// owning temporary references can be dropped without the general call-result alias guard.
fn emit_fixed_object_new(
    ctx: &mut LoweringContext<'_, '_>,
    class_name: &str,
    operands: Vec<ValueId>,
    php_type: PhpType,
    span: Span,
) -> LoweredValue {
    let data = ctx.intern_class_name(class_name);
    let object = ctx.emit_value(
        Op::ObjectNew,
        operands.clone(),
        Some(Immediate::Data(data)),
        php_type,
        Op::ObjectNew.default_effects(),
        Some(span),
    );
    release_owned_call_arg_temporaries(
        ctx,
        &operands,
        None,
        &ReturnArgAlias::None,
        span,
    );
    object
}

/// Lowers `ReflectionClass(object)` while preserving object operands for runtime class metadata.
fn lower_reflection_class_constructor_operands(
    ctx: &mut LoweringContext<'_, '_>,
    args: &[Expr],
) -> Option<Vec<ValueId>> {
    let reflected_arg = reflection_class_constructor_class_arg(ctx, args)?;
    let class_name = instance_callable_object_class(ctx, &reflected_arg)?;
    let lowered = lower_expr(ctx, &reflected_arg);
    if matches!(
        ctx.builder.value_php_type(lowered.value).codegen_repr(),
        PhpType::Object(_)
    ) {
        return Some(vec![lowered.value]);
    }
    if ctx.value_is_owning_temporary(lowered) {
        crate::ir_lower::ownership::release_if_owned(ctx, lowered, Some(reflected_arg.span));
    }
    let data = ctx.intern_class_name(&class_name);
    let value = ctx.emit_value(
        Op::ConstClassName,
        Vec::new(),
        Some(Immediate::Data(data)),
        PhpType::Str,
        Op::ConstClassName.default_effects(),
        Some(reflected_arg.span),
    );
    Some(vec![value.value])
}

/// Lowers direct `ReflectionMethod` constructor operands to literal class and method names.
fn lower_reflection_method_constructor_operands(
    ctx: &mut LoweringContext<'_, '_>,
    args: &[Expr],
) -> Option<Vec<ValueId>> {
    let (class_arg, method_arg) = reflection_method_constructor_regular_args(ctx, args)?;
    Some(vec![
        lower_expr(ctx, &class_arg).value,
        lower_expr(ctx, &method_arg).value,
    ])
}

/// Lowers PHP `clone $object` to a shallow object-copy opcode and optional `__clone()` hook.
fn lower_clone(ctx: &mut LoweringContext<'_, '_>, inner: &Expr, expr: &Expr) -> LoweredValue {
    let object = lower_expr(ctx, inner);
    let object_ty = ctx.builder.value_php_type(object.value);
    let Some((class_name, false)) = singular_object_class(&object_ty) else {
        unreachable!("clone expressions must be type-checked as non-null objects before lowering");
    };
    let class_name = class_name.to_string();
    let data = ctx.intern_class_name(&class_name);
    let result_ty = PhpType::Object(class_name.clone());
    let cloned = ctx.emit_value(
        Op::ObjectCloneShallow,
        vec![object.value],
        Some(Immediate::Data(data)),
        result_ty,
        Op::ObjectCloneShallow.default_effects(),
        Some(expr.span),
    );
    if class_method_signature(ctx, &class_name, &php_symbol_key("__clone")).is_some() {
        lower_method_call_with_receiver(ctx, cloned, "__clone", &[], Op::MethodCall, expr);
    }
    cloned
}

/// Metadata operand source for direct `ReflectionParameter` constructor lowering.
enum ReflectionParameterConstructorOperand {
    Expr(Expr),
    ClassName { name: String, span: Span },
    ObjectExpr { expr: Expr, span: Span },
}

/// Lowers validated `ReflectionParameter` constructor arguments into metadata operands.
///
/// Method targets lower as `[class, method, parameter]`; function targets lower
/// as `[function, parameter]`.
fn lower_reflection_parameter_constructor_operands(
    ctx: &mut LoweringContext<'_, '_>,
    args: &[Expr],
) -> Option<Vec<ValueId>> {
    let arg_exprs = reflection_parameter_constructor_arg_exprs(ctx, args)?;
    Some(
        arg_exprs
            .iter()
            .map(|arg| lower_reflection_parameter_constructor_operand(ctx, arg))
            .collect(),
    )
}

/// Lowers one direct `ReflectionParameter` metadata operand.
fn lower_reflection_parameter_constructor_operand(
    ctx: &mut LoweringContext<'_, '_>,
    operand: &ReflectionParameterConstructorOperand,
) -> ValueId {
    match operand {
        ReflectionParameterConstructorOperand::Expr(expr) => lower_expr(ctx, expr).value,
        ReflectionParameterConstructorOperand::ObjectExpr { expr, span } => {
            let object = lower_expr(ctx, expr);
            let class_name = reflection_parameter_lowered_object_class_name(ctx, object.value)
                .expect("ReflectionParameter object target must be type-checked as a known object");
            if ctx.value_is_owning_temporary(object) {
                crate::ir_lower::ownership::release_if_owned(ctx, object, Some(*span));
            }
            emit_reflection_parameter_class_name_operand(ctx, &class_name, *span)
        }
        ReflectionParameterConstructorOperand::ClassName { name, span } => {
            emit_reflection_parameter_class_name_operand(ctx, name, *span)
        }
    }
}

/// Emits one class-name operand for direct `ReflectionParameter` metadata.
fn emit_reflection_parameter_class_name_operand(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    span: Span,
) -> ValueId {
    let data = ctx.intern_class_name(name);
    ctx.emit_value(
        Op::ConstClassName,
        Vec::new(),
        Some(Immediate::Data(data)),
        PhpType::Str,
        Op::ConstClassName.default_effects(),
        Some(span),
    )
    .value
}

/// Returns metadata operand expressions from a normalized static `ReflectionParameter` call.
fn reflection_parameter_constructor_arg_exprs(
    ctx: &LoweringContext<'_, '_>,
    args: &[Expr],
) -> Option<Vec<ReflectionParameterConstructorOperand>> {
    let args = expand_static_call_spread_args(args);
    if args.iter().any(is_spread_arg) {
        return None;
    }
    let (target, parameter) = if crate::types::call_args::has_named_args(&args) {
        let sig = ctx
            .classes
            .get("ReflectionParameter")
            .and_then(|class_info| class_info.methods.get("__construct"))?;
        let call_span = args
            .first()
            .map(|arg| arg.span)
            .unwrap_or_else(crate::span::Span::dummy);
        let plan =
            crate::types::call_args::plan_call_args_with_regular_param_count_and_assoc_spreads(
                sig,
                &args,
                call_span,
                crate::types::call_args::regular_param_count(sig),
                false,
                true,
                &assoc_spread_sources(ctx, &args),
            )
            .ok()?;
        if plan.has_spread_args() {
            return None;
        }
        (
            planned_regular_arg_expr(plan.regular_args.first()?)?.clone(),
            planned_regular_arg_expr(plan.regular_args.get(1)?)?.clone(),
        )
    } else {
        (args.first()?.clone(), args.get(1)?.clone())
    };
    match &target.kind {
        ExprKind::ArrayLiteral(items) if items.len() == 2 => {
            let owner = reflection_parameter_method_owner_operand(ctx, &items[0])?;
            Some(vec![
                owner,
                ReflectionParameterConstructorOperand::Expr(items[1].clone()),
                ReflectionParameterConstructorOperand::Expr(parameter),
            ])
        }
        ExprKind::StringLiteral(_) => Some(vec![
            ReflectionParameterConstructorOperand::Expr(target),
            ReflectionParameterConstructorOperand::Expr(parameter),
        ]),
        _ => None,
    }
}

/// Returns the static class-name operand for a ReflectionParameter method target.
fn reflection_parameter_method_owner_operand(
    ctx: &LoweringContext<'_, '_>,
    owner: &Expr,
) -> Option<ReflectionParameterConstructorOperand> {
    match &owner.kind {
        ExprKind::StringLiteral(name) => Some(ReflectionParameterConstructorOperand::ClassName {
            name: name.clone(),
            span: owner.span,
        }),
        ExprKind::ClassConstant { receiver } => {
            static_receiver_class_name(ctx, receiver).map(|name| {
                ReflectionParameterConstructorOperand::ClassName {
                    name,
                    span: owner.span,
                }
            })
        }
        ExprKind::Variable(name) => {
            let PhpType::Object(class_name) = ctx.local_type(name).codegen_repr() else {
                return None;
            };
            if class_name.is_empty() {
                return None;
            }
            Some(ReflectionParameterConstructorOperand::ClassName {
                name: class_name,
                span: owner.span,
            })
        }
        ExprKind::This => {
            ctx.current_class
                .clone()
                .map(|name| ReflectionParameterConstructorOperand::ClassName {
                    name,
                    span: owner.span,
                })
        }
        _ => Some(ReflectionParameterConstructorOperand::ObjectExpr {
            expr: owner.clone(),
            span: owner.span,
        }),
    }
}

/// Returns the concrete class name from a lowered object target.
fn reflection_parameter_lowered_object_class_name(
    ctx: &LoweringContext<'_, '_>,
    value: ValueId,
) -> Option<String> {
    let PhpType::Object(class_name) = ctx.builder.value_php_type(value).codegen_repr() else {
        return None;
    };
    if class_name.is_empty() || !ctx.classes.contains_key(class_name.as_str()) {
        return None;
    }
    Some(class_name)
}

/// Lowers PHP `new $class(...)` into the generic dynamic-new EIR opcode.
fn lower_new_dynamic(
    ctx: &mut LoweringContext<'_, '_>,
    name_expr: &Expr,
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let mut operands = vec![lower_expr(ctx, name_expr).value];
    operands.extend(lower_args(ctx, args));
    ctx.emit_value(
        Op::DynamicObjectNewMixed,
        operands,
        None,
        PhpType::Mixed,
        Op::DynamicObjectNewMixed.default_effects(),
        Some(expr.span),
    )
}

/// Lowers dynamic object construction.
fn lower_new_dynamic_object(
    ctx: &mut LoweringContext<'_, '_>,
    class_name: &Expr,
    fallback_class: &Name,
    required_parent: &Name,
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let mut operands = vec![lower_expr(ctx, class_name).value];
    operands.extend(lower_args(ctx, args));
    let name = format!("{}|{}", fallback_class.as_str(), required_parent.as_str());
    let data = ctx.intern_class_name(&name);
    ctx.emit_value(
        Op::DynamicObjectNew,
        operands,
        Some(Immediate::Data(data)),
        PhpType::Object(fallback_class.as_str().to_string()),
        Op::DynamicObjectNew.default_effects(),
        Some(expr.span),
    )
}

/// Returns constructor signature metadata when available for a fixed class.
fn constructor_signature<'a>(
    ctx: &'a LoweringContext<'_, '_>,
    class_name: &Name,
) -> Option<&'a FunctionSig> {
    let key = php_symbol_key("__construct");
    ctx.classes
        .get(class_name.as_str().trim_start_matches('\\'))
        .and_then(|class_info| class_info.methods.get(&key))
}

/// Lowers an object property read.
fn lower_property_get(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
    op: Op,
    expr: &Expr,
) -> LoweredValue {
    let object = lower_expr(ctx, object);
    lower_property_get_from_value(ctx, object, property, op, expr)
}

/// Lowers `$target = &$obj->prop`: binds the local `$target` to the reference cell
/// stored in the object's reference-property slot, so reads/writes of either side go
/// through the same cell (write-through). The property was promoted to a reference
/// property by the checker, so its slot holds a live cell pointer.
pub(crate) fn lower_ref_assign_property(
    ctx: &mut LoweringContext<'_, '_>,
    target: &str,
    source: &Expr,
    span: Span,
) {
    let ExprKind::PropertyAccess { object, property } = &source.kind else {
        return;
    };
    let object = lower_expr(ctx, object);
    let value_type = property_get_result_type(ctx, object.value, property, Op::PropGet, source);
    let data = ctx.intern_string(property);
    let cell_ptr = ctx.emit_value(
        Op::LoadPropRefCell,
        vec![object.value],
        Some(Immediate::Data(data)),
        value_type.clone(),
        Op::LoadPropRefCell.default_effects(),
        Some(span),
    );
    ctx.bind_local_ref_cell_ptr(target, cell_ptr, value_type, Some(span));
}

/// Lowers `$target = &call()`: binds `$target` to the reference cell returned by a
/// by-reference-returning callee. The call yields the cell pointer; the target shares it
/// non-owning (the owner is the object property the callee returned a reference to).
pub(crate) fn lower_ref_assign_call(
    ctx: &mut LoweringContext<'_, '_>,
    target: &str,
    source: &Expr,
    span: Span,
) {
    let cell_ptr = lower_expr(ctx, source);
    let value_type = ctx.builder.value_php_type(cell_ptr.value);
    ctx.bind_local_ref_cell_ptr(target, cell_ptr, value_type, Some(span));
}

/// Lowers `$target =& $arr[idx]`: promotes the indexed-array element's inline storage to a
/// reference cell and binds `$target` to it non-owning. The returned cell pointer addresses
/// the element within the array payload, so writes through `$target` propagate to `$arr[idx]`
/// and vice versa. The array must remain live while the alias is in use (the local does not
/// own the storage). Operands: the lowered array value and the lowered index value.
pub(crate) fn lower_ref_assign_array_elem(
    ctx: &mut LoweringContext<'_, '_>,
    target: &str,
    source: &Expr,
    span: Span,
) {
    let ExprKind::ArrayAccess { array, index } = &source.kind else {
        return;
    };
    let array_value = lower_expr(ctx, array);
    let mut index_value = lower_expr(ctx, index);
    index_value = coerce_to_int_at_span(ctx, index_value, Some(index.span));
    // Use the array's declared element type (the inline storage shape), not the
    // null-capable `TaggedScalar` result type that `array_access_result_type` widens
    // Int elements to. The ref-cell aliases the raw element slot, so loads and stores
    // through the alias must match the element's storage width, not the read result.
    let value_type = match ctx.builder.value_php_type(array_value.value).codegen_repr() {
        PhpType::Array(elem_ty) => normalize_value_php_type(*elem_ty),
        _ => array_access_result_type(ctx, array_value.value, Op::ArrayGet, source),
    };
    let cell_ptr = ctx.emit_value(
        Op::LoadArrayElemRefCell,
        vec![array_value.value, index_value.value],
        None,
        value_type.clone(),
        Op::LoadArrayElemRefCell.default_effects(),
        Some(span),
    );
    ctx.bind_local_ref_cell_ptr(target, cell_ptr, value_type, Some(span));
}

/// Lowers a named property read once the receiver is already evaluated.
fn lower_property_get_from_value(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    property: &str,
    op: Op,
    expr: &Expr,
) -> LoweredValue {
    if op == Op::NullsafePropGet && value_is_definitely_null(ctx, object.value) {
        return lower_boxed_null(ctx, expr);
    }
    // Route a read of a get-hooked property to its synthetic accessor, except inside that property's
    // own accessor, where `$this->prop` must read the raw backing slot to avoid infinite recursion.
    // A nullsafe read (`$obj?->prop`) routes to a nullsafe call so the null short-circuit is kept.
    if matches!(op, Op::PropGet | Op::NullsafePropGet)
        && class_declares_hook_accessor(ctx, object.value, &property_hook_get_method(property))
        && !ctx.in_own_property_accessor(property)
    {
        let accessor = property_hook_get_method(property);
        let call_op = if op == Op::NullsafePropGet {
            Op::NullsafeMethodCall
        } else {
            Op::MethodCall
        };
        return lower_method_call_with_receiver(ctx, object, &accessor, &[], call_op, expr);
    }
    let data = ctx.intern_string(property);
    let result_type = property_get_result_type(ctx, object.value, property, op, expr);
    let result = ctx.emit_value(
        op,
        vec![object.value],
        Some(Immediate::Data(data)),
        result_type,
        op.default_effects(),
        Some(expr.span),
    );
    stabilize_borrowed_result_and_release_receiver(ctx, object, result, expr.span)
}

/// Returns true when value metadata proves the runtime value is PHP null.
fn value_is_definitely_null(ctx: &LoweringContext<'_, '_>, value: crate::ir::ValueId) -> bool {
    matches!(ctx.builder.value_php_type(value), PhpType::Void | PhpType::Never)
}

/// Returns true when value metadata permits PHP null at runtime.
fn value_is_nullable(ctx: &LoweringContext<'_, '_>, value: crate::ir::ValueId) -> bool {
    match ctx.builder.value_php_type(value) {
        PhpType::Void | PhpType::Never => true,
        PhpType::Union(members) => members.iter().any(|member| matches!(member, PhpType::Void)),
        _ => false,
    }
}

/// Returns precise PHP metadata for a named property read when class metadata is available.
fn property_get_result_type(
    ctx: &LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    property: &str,
    op: Op,
    expr: &Expr,
) -> PhpType {
    if op == Op::NullsafePropGet {
        return PhpType::Mixed;
    }
    let object_ty = ctx.builder.value_php_type(object);
    let Some((class_name, nullable)) = singular_object_class(&object_ty) else {
        if matches!(object_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
            return PhpType::Mixed;
        }
        if let PhpType::Packed(class_name) = object_ty.codegen_repr() {
            let normalized = class_name.trim_start_matches('\\');
            let Some(class_info) = ctx.packed_classes.get(normalized) else {
                return fallback_expr_type(expr);
            };
            let Some(field) = class_info.fields.iter().find(|field| field.name == property) else {
                return fallback_expr_type(expr);
            };
            return normalize_value_php_type(field.php_type.codegen_repr());
        }
        return fallback_expr_type(expr);
    };
    let nullable = nullable || value_may_carry_container_miss(ctx, object);
    let normalized = class_name.trim_start_matches('\\');
    if is_builtin_stdclass_name(normalized) {
        return if nullable {
            nullable_result_type(PhpType::Mixed)
        } else {
            PhpType::Mixed
        };
    }
    let Some(class_info) = ctx.classes.get(normalized) else {
        return fallback_expr_type(expr);
    };
    if let Some(property_ty) = runtime_property_type_override(ctx, normalized, property) {
        let property_ty = normalize_value_php_type(property_ty);
        return if nullable {
            nullable_result_type(property_ty)
        } else {
            property_ty
        };
    }
    let Some((_, (_, property_ty))) = class_info.visible_property(property) else {
        if let Some(magic_ty) = magic_get_result_type(ctx, normalized) {
            return if nullable {
                nullable_result_type(magic_ty)
            } else {
                magic_ty
            };
        }
        if class_info.allow_dynamic_properties {
            return if nullable {
                nullable_result_type(PhpType::Mixed)
            } else {
                PhpType::Mixed
            };
        }
        return fallback_expr_type(expr);
    };
    let property_ty = normalize_value_php_type(property_ty.clone());
    if nullable {
        nullable_result_type(property_ty)
    } else {
        property_ty
    }
}

/// Returns whether a container read can carry PHP null in a statically non-null pointer type.
fn value_may_carry_container_miss(
    ctx: &LoweringContext<'_, '_>,
    value: crate::ir::ValueId,
) -> bool {
    let Some(inst) = ctx.builder.value_defining_instruction(value) else {
        return false;
    };
    match inst.op {
        Op::ArrayGet | Op::ArrayGetSilent | Op::HashGet | Op::HashGetSilent => true,
        Op::Acquire => inst
            .operands
            .first()
            .copied()
            .is_some_and(|source| value_may_carry_container_miss(ctx, source)),
        _ => false,
    }
}

/// Returns the normalized return type for a class `__get` magic property hook.
fn magic_get_result_type(ctx: &LoweringContext<'_, '_>, class_name: &str) -> Option<PhpType> {
    class_method_signature(ctx, class_name, &php_symbol_key("__get"))
        .map(|signature| normalize_value_php_type(signature.return_type.clone()))
}

/// Adds nullability to a result type without nesting existing union metadata.
fn nullable_result_type(php_type: PhpType) -> PhpType {
    match php_type {
        PhpType::Union(mut members) => {
            if !members.iter().any(|member| matches!(member, PhpType::Void)) {
                members.push(PhpType::Void);
            }
            PhpType::Union(members)
        }
        other => PhpType::Union(vec![other, PhpType::Void]),
    }
}

/// Returns true when the runtime class of `object` declares the synthetic property-hook accessor
/// `accessor_method` (`__propget_<p>` / `__propset_<p>`). Drives the decision to route a property
/// read/write to a hook; inherited (flattened) methods count, so subclasses inherit hooks.
fn class_declares_hook_accessor(
    ctx: &LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    accessor_method: &str,
) -> bool {
    let object_ty = ctx.builder.value_php_type(object);
    let Some((class_name, _nullable)) = singular_object_class(&object_ty) else {
        return false;
    };
    let key = php_symbol_key(accessor_method);
    ctx.classes
        .get(class_name)
        .is_some_and(|info| info.methods.contains_key(&key))
}

/// Returns the class name and nullability if `php_type` is a single object type (optionally
/// nullable). Heterogeneous unions and non-object types return `None`.
fn singular_object_class(php_type: &PhpType) -> Option<(&str, bool)> {
    match php_type {
        PhpType::Object(name) => Some((name.as_str(), false)),
        PhpType::Union(members) => {
            let mut found = None;
            let mut nullable = false;
            for member in members {
                match member {
                    PhpType::Void => nullable = true,
                    PhpType::Object(name) => {
                        if found.is_some_and(|existing| existing != name.as_str()) {
                            return None;
                        }
                        found = Some(name.as_str());
                    }
                    _ => return None,
                }
            }
            found.map(|class_name| (class_name, nullable))
        }
        _ => None,
    }
}

/// Returns precise runtime storage types for inherited SPL callback-filter internals.
fn runtime_property_type_override(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    property: &str,
) -> Option<PhpType> {
    if !class_extends_class(ctx, class_name, "CallbackFilterIterator") {
        return None;
    }
    match property {
        "callback" => Some(PhpType::Callable),
        "callbackEnv" => Some(PhpType::Pointer(None)),
        _ => None,
    }
}

/// Returns true when a class is or extends the target class.
fn class_extends_class(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    target_class: &str,
) -> bool {
    let target_key = php_symbol_key(target_class);
    let mut current = Some(class_name.trim_start_matches('\\').to_string());
    while let Some(name) = current {
        if php_symbol_key(&name) == target_key {
            return true;
        }
        current = ctx
            .classes
            .get(name.as_str())
            .and_then(|class_info| class_info.parent.clone());
    }
    false
}

/// Lowers a dynamic property read.
fn lower_dynamic_property_get(ctx: &mut LoweringContext<'_, '_>, object: &Expr, property: &Expr, expr: &Expr) -> LoweredValue {
    let object = lower_expr(ctx, object);
    lower_dynamic_property_get_from_value(ctx, object, property, expr)
}

/// Lowers a dynamic property read once the receiver is already evaluated.
fn lower_dynamic_property_get_from_value(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    property: &Expr,
    expr: &Expr,
) -> LoweredValue {
    let result_type = dynamic_property_get_result_type(ctx, object.value, property, expr);
    let property = lower_expr(ctx, property);
    let result = ctx.emit_value(
        Op::DynamicPropGet,
        vec![object.value, property.value],
        None,
        result_type,
        Op::DynamicPropGet.default_effects(),
        Some(expr.span),
    );
    stabilize_borrowed_result_and_release_receiver(ctx, object, result, expr.span)
}

/// Returns precise metadata for dynamic property reads when class slots are statically known.
fn dynamic_property_get_result_type(
    ctx: &LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    property: &Expr,
    expr: &Expr,
) -> PhpType {
    if let ExprKind::StringLiteral(name) = &property.kind {
        return property_get_result_type(ctx, object, name, Op::DynamicPropGet, expr);
    }
    let object_ty = ctx.builder.value_php_type(object);
    if matches!(object_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        return PhpType::Mixed;
    }
    let Some((class_name, nullable)) = singular_object_class(&object_ty) else {
        return fallback_expr_type(expr);
    };
    let nullable = nullable || value_may_carry_container_miss(ctx, object);
    let normalized = class_name.trim_start_matches('\\');
    if is_builtin_stdclass_name(normalized) {
        return if nullable {
            nullable_result_type(PhpType::Mixed)
        } else {
            PhpType::Mixed
        };
    }
    let Some(class_info) = ctx.classes.get(normalized) else {
        return fallback_expr_type(expr);
    };
    let members = class_info
        .properties
        .iter()
        .map(|(_, property_ty)| {
            let property_ty = normalize_value_php_type(property_ty.clone());
            if nullable {
                nullable_result_type(property_ty)
            } else {
                property_ty
            }
        })
        .collect::<Vec<_>>();
    normalize_union_members(members).unwrap_or_else(|| fallback_expr_type(expr))
}

/// Returns true when the normalized class name refers to PHP's builtin stdClass.
fn is_builtin_stdclass_name(class_name: &str) -> bool {
    crate::types::checker::builtin_stdclass::is_stdclass(class_name)
}

/// Flattens and deduplicates union candidates, with `Mixed` absorbing all members.
fn normalize_union_members(members: Vec<PhpType>) -> Option<PhpType> {
    let mut deduped = Vec::new();
    for member in members {
        match member {
            PhpType::Union(inner) => {
                for inner_member in inner {
                    if inner_member == PhpType::Mixed {
                        return Some(PhpType::Mixed);
                    }
                    if !deduped.iter().any(|existing| existing == &inner_member) {
                        deduped.push(inner_member);
                    }
                }
            }
            PhpType::Mixed => return Some(PhpType::Mixed),
            other => {
                if !deduped.iter().any(|existing| existing == &other) {
                    deduped.push(other);
                }
            }
        }
    }
    match deduped.len() {
        0 => None,
        1 => deduped.pop(),
        _ => Some(PhpType::Union(deduped)),
    }
}

/// Lowers a static property read.
fn lower_static_property_get(ctx: &mut LoweringContext<'_, '_>, receiver: &StaticReceiver, property: &str, expr: &Expr) -> LoweredValue {
    let name = format!("{}::{}", receiver_name(receiver), property);
    let data = ctx.intern_string(&name);
    let result_type = static_property_result_type(ctx, receiver, property, expr);
    ctx.emit_value(
        Op::LoadStaticProperty,
        Vec::new(),
        Some(Immediate::Data(data)),
        result_type,
        Op::LoadStaticProperty.default_effects(),
        Some(expr.span),
    )
}

/// Returns precise PHP metadata for a static property read when class metadata is available.
fn static_property_result_type(
    ctx: &LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    property: &str,
    _expr: &Expr,
) -> PhpType {
    let Some(class_name) = static_receiver_class_name(ctx, receiver) else {
        return PhpType::Mixed;
    };
    let Some(class_info) = ctx.classes.get(class_name.as_str()) else {
        return PhpType::Mixed;
    };
    let Some((_, property_ty)) = class_info
        .static_properties
        .iter()
        .find(|(name, _)| name == property)
    else {
        return PhpType::Mixed;
    };
    normalize_value_php_type(property_ty.codegen_repr())
}

/// Lowers an object method call.
fn lower_method_call(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    method: &str,
    args: &[Expr],
    op: Op,
    expr: &Expr,
) -> LoweredValue {
    // A statically-decided private/protected method access from an inaccessible
    // scope raises a catchable `Error` in PHP rather than a compile-time error,
    // but the receiver expression must still be evaluated first.
    let throw_access_message = if op == Op::MethodCall {
        ctx.throw_access_sites.get(&expr.span).and_then(|info| {
            if let ThrowAccessKind::PrivateMethod {
                visibility,
                class_name,
                method: m,
            } = &info.kind
            {
                Some(format!(
                    "Call to {} method {}::{}() from global scope",
                    visibility, class_name, m
                ))
            } else {
                None
            }
        })
    } else {
        None
    };
    let object_expr = object;
    let object = lower_expr(ctx, object_expr);
    if let Some(message) = throw_access_message {
        release_owning_receiver_temporary(ctx, object, expr.span);
        return crate::ir_lower::stmt::lower_throw_access_error_expr(ctx, &message, expr.span);
    }
    if op == Op::MethodCall && value_is_definitely_null(ctx, object.value) {
        let null_value = lower_null(ctx, expr);
        terminate_method_call_on_null(ctx, method);
        return null_value;
    }
    if op == Op::MethodCall {
        if let Some(value) =
            lower_reflection_function_invoke_call(ctx, Some(object_expr), method, args, expr)
        {
            return value;
        }
        if let Some(value) =
            lower_reflection_method_invoke_call(ctx, Some(object_expr), method, args, expr)
        {
            return value;
        }
    }
    if op == Op::MethodCall
        && (value_is_nullable(ctx, object.value)
            || value_may_carry_container_miss(ctx, object.value))
    {
        return lower_nullable_regular_method_call(ctx, object, method, args, expr);
    }
    if op == Op::MethodCall && is_reflection_class_new_instance_call(ctx, object.value, method) {
        return lower_reflection_class_new_instance(ctx, Some(object_expr), object, args, expr);
    }
    if op == Op::MethodCall && is_reflection_class_new_instance_args_call(ctx, object.value, method)
    {
        return lower_reflection_class_new_instance_args(
            ctx,
            Some(object_expr),
            object,
            args,
            expr,
        );
    }
    if op == Op::MethodCall
        && is_reflection_class_new_instance_without_constructor_call(ctx, object.value, method)
    {
        return lower_reflection_class_new_instance_without_constructor(ctx, object, args, expr);
    }
    if op == Op::MethodCall {
        if let Some(value) = lower_reflection_class_static_property_value_call(
            ctx,
            Some(object_expr),
            method,
            args,
            expr,
        ) {
            return value;
        }
    }
    if op == Op::MethodCall {
        if let Some(value) =
            lower_reflection_class_member_list_call(ctx, Some(object_expr), method, args, expr)
        {
            return value;
        }
    }
    if op == Op::MethodCall {
        if let Some(value) =
            lower_reflection_property_value_call(ctx, Some(object_expr), method, args, expr)
        {
            return value;
        }
    }
    if matches!(
        ctx.builder.value_php_type(object.value).codegen_repr(),
        PhpType::Callable
    ) {
        if let Some(result) = lower_closure_bind_method(ctx, &object, method, args, expr) {
            return result;
        }
    }
    let magic_args;
    let (dispatch_method, args) = if let Some(args) =
        magic_call_dispatch_args(ctx, object.value, method, args, object_expr.span)
    {
        magic_args = args;
        ("__call", magic_args.as_slice())
    } else {
        (method, args)
    };
    let result_type = method_call_result_type(ctx, object.value, dispatch_method, op, expr);
    let mut operands = vec![object.value];
    let sig = method_call_argument_signature(ctx, object_expr, object.value, dispatch_method);
    let arg_values = lower_args_with_signature(ctx, sig.as_ref(), args);
    operands.extend(arg_values.iter().copied());
    let data = ctx.intern_string(dispatch_method);
    let call = ctx.emit_value(
        op,
        operands,
        Some(Immediate::Data(data)),
        result_type,
        op.default_effects(),
        Some(expr.span),
    );
    let return_alias = method_return_arg_alias(ctx, object.value, dispatch_method);
    release_owned_call_arg_temporaries(
        ctx,
        &arg_values,
        Some(call.value),
        &return_alias,
        expr.span,
    );
    release_owning_receiver_temporary(ctx, object, expr.span);
    call
}

/// Lowers the `Closure` rebinding methods on a closure (`Callable`) receiver:
/// `$closure->bindTo($newThis [, $scope])` and `$closure->call($newThis, ...$args)`.
/// Returns `None` for any other method so normal dispatch (and its diagnostics)
/// still apply. The `$scope` argument is accepted and ignored — visibility is
/// resolved at compile time in elephc's closed-world model.
fn lower_closure_bind_method(
    ctx: &mut LoweringContext<'_, '_>,
    closure: &LoweredValue,
    method: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    match php_symbol_key(method).as_str() {
        "bindto" => {
            let new_this = match args.first() {
                Some(arg) => lower_expr(ctx, arg),
                None => lower_null(ctx, expr),
            };
            Some(emit_closure_bind(ctx, closure.value, new_this.value, expr))
        }
        "call" => {
            // `$closure->call($newThis, ...$args)`: bind `$this` then invoke the
            // bound closure with the remaining arguments in one step.
            let new_this = match args.first() {
                Some(arg) => lower_expr(ctx, arg),
                None => lower_null(ctx, expr),
            };
            let bound = emit_closure_bind(ctx, closure.value, new_this.value, expr);
            let call_args = &args[args.len().min(1)..];
            let arg_container =
                lower_untyped_descriptor_invoker_arg_container(ctx, call_args, expr.span)?;
            Some(ctx.emit_value(
                Op::CallableDescriptorInvoke,
                vec![bound.value, arg_container.value],
                None,
                PhpType::Mixed,
                Op::CallableDescriptorInvoke.default_effects(),
                Some(expr.span),
            ))
        }
        _ => None,
    }
}

/// Emits the `closure_bind` runtime call that rebinds a closure's captured
/// `$this`, yielding a new closure (`Callable`) descriptor.
fn emit_closure_bind(
    ctx: &mut LoweringContext<'_, '_>,
    closure: crate::ir::ValueId,
    new_this: crate::ir::ValueId,
    expr: &Expr,
) -> LoweredValue {
    ctx.emit_value(
        Op::ClosureBind,
        vec![closure, new_this],
        None,
        PhpType::Callable,
        Op::ClosureBind.default_effects(),
        Some(expr.span),
    )
}

/// Builds synthetic `__call` arguments when a class lacks the requested method.
fn magic_call_dispatch_args(
    ctx: &LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    method: &str,
    args: &[Expr],
    span: Span,
) -> Option<Vec<Expr>> {
    if method_signature(ctx, object, method).is_some() {
        return None;
    }
    let object_ty = ctx.builder.value_php_type(object);
    let Some((class_name, _)) = singular_object_class(&object_ty) else {
        return None;
    };
    let normalized = class_name.trim_start_matches('\\');
    class_method_signature(ctx, normalized, &php_symbol_key("__call"))?;
    Some(vec![
        Expr::new(ExprKind::StringLiteral(method.to_string()), span),
        Expr::new(ExprKind::ArrayLiteral(args.to_vec()), span),
    ])
}

/// Returns the signature to use for method-call argument normalization.
fn method_call_argument_signature(
    ctx: &LoweringContext<'_, '_>,
    object_expr: &Expr,
    object: crate::ir::ValueId,
    method: &str,
) -> Option<FunctionSig> {
    if method_is_fiber_start(ctx, object, method) {
        return crate::ir_lower::fibers::start_sig_for_expr(ctx, object_expr);
    }
    method_signature(ctx, object, method)
}

/// Returns true when a method call targets PHP's built-in `Fiber::start()`.
fn method_is_fiber_start(
    ctx: &LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    method: &str,
) -> bool {
    if php_symbol_key(method) != "start" {
        return false;
    }
    let object_ty = ctx.builder.value_php_type(object);
    let Some((class_name, _)) = singular_object_class(&object_ty) else {
        return false;
    };
    php_symbol_key(class_name.trim_start_matches('\\')) == "fiber"
}

/// Lowers `?Object->method()` calls so null receivers fatal before argument evaluation.
fn lower_nullable_regular_method_call(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    method: &str,
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let result_type = method_call_result_type(ctx, object.value, method, Op::MethodCall, expr);
    let temp_name = ctx.declare_owned_hidden_temp(result_type.clone());
    let fatal_block = ctx
        .builder
        .create_named_block("method.null.fatal", Vec::new());
    let call_block = ctx
        .builder
        .create_named_block("method.non_null.call", Vec::new());
    let merge = ctx
        .builder
        .create_named_block("method.nullable.merge", Vec::new());
    let is_null = ctx.emit_value(
        Op::IsNull,
        vec![object.value],
        None,
        PhpType::Bool,
        Op::IsNull.default_effects(),
        Some(expr.span),
    );
    ctx.builder.terminate(Terminator::CondBr {
        cond: is_null.value,
        then_target: fatal_block,
        then_args: Vec::new(),
        else_target: call_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(fatal_block);
    terminate_method_call_on_null(ctx, method);

    ctx.builder.position_at_end(call_block);
    let call = lower_method_call_with_receiver(ctx, object, method, args, Op::MethodCall, expr);
    store_value_into_temp(ctx, &temp_name, result_type.clone(), call, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    take_owned_temp(ctx, &temp_name, expr.span)
}

/// Lowers `ReflectionClass::newInstance()` by constructing the reflected class name.
fn lower_reflection_class_new_instance(
    ctx: &mut LoweringContext<'_, '_>,
    object_expr: Option<&Expr>,
    object: LoweredValue,
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let args = reflection_class_new_instance_args(args);
    let constructor_sig =
        reflection_class_new_instance_constructor_signature(ctx, object_expr, &args).cloned();
    if args.iter().any(is_spread_arg)
        || (crate::types::call_args::has_named_args(&args) && constructor_sig.is_none())
    {
        return lower_reflection_class_new_instance_unsupported(ctx, expr);
    }
    let class_name = lower_property_get_from_value(ctx, object, "__name", Op::PropGet, expr);
    let mut operands = vec![class_name.value];
    operands.extend(lower_args_with_signature(
        ctx,
        constructor_sig.as_ref(),
        &args,
    ));
    ctx.emit_value(
        Op::DynamicObjectNewMixed,
        operands,
        None,
        PhpType::Mixed,
        Op::DynamicObjectNewMixed.default_effects(),
        Some(expr.span),
    )
}

/// Lowers `ReflectionClass::newInstanceArgs()` by unpacking one static argument array.
fn lower_reflection_class_new_instance_args(
    ctx: &mut LoweringContext<'_, '_>,
    object_expr: Option<&Expr>,
    object: LoweredValue,
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let Some(forwarded_args) = reflection_class_new_instance_args_array(ctx, args) else {
        return lower_reflection_class_new_instance_args_unsupported(ctx, expr);
    };
    lower_reflection_class_new_instance(ctx, object_expr, object, &forwarded_args, expr)
}

/// Lowers `ReflectionClass::newInstanceWithoutConstructor()` to constructorless allocation.
fn lower_reflection_class_new_instance_without_constructor(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    if !args.is_empty() {
        return lower_reflection_class_new_instance_without_constructor_unsupported(ctx, expr);
    }
    let class_name = lower_property_get_from_value(ctx, object, "__name", Op::PropGet, expr);
    ctx.emit_value(
        Op::DynamicObjectNewWithoutConstructorMixed,
        vec![class_name.value],
        None,
        PhpType::Mixed,
        Op::DynamicObjectNewWithoutConstructorMixed.default_effects(),
        Some(expr.span),
    )
}

/// Lowers live static-property value access for statically-known `ReflectionClass` calls.
fn lower_reflection_class_static_property_value_call(
    ctx: &mut LoweringContext<'_, '_>,
    object_expr: Option<&Expr>,
    method: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let class_name = reflection_class_reflected_class(ctx, object_expr?)?;
    match php_symbol_key(method).as_str() {
        "getstaticproperties" => {
            lower_reflection_class_get_static_properties(ctx, &class_name, args, expr)
        }
        "getstaticpropertyvalue" => {
            lower_reflection_class_get_static_property_value(ctx, &class_name, args, expr)
        }
        "setstaticpropertyvalue" => {
            lower_reflection_class_set_static_property_value(ctx, &class_name, args, expr)
        }
        _ => None,
    }
}

/// Lowers statically-known filtered ReflectionClass member-list calls.
fn lower_reflection_class_member_list_call(
    ctx: &mut LoweringContext<'_, '_>,
    object_expr: Option<&Expr>,
    method: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let class_name = reflection_class_reflected_class(ctx, object_expr?)?;
    let (member_class, items): (&str, Vec<Expr>) = match php_symbol_key(method).as_str() {
        "getproperties" => {
            let filter = reflection_class_get_properties_filter_arg(ctx, args)?;
            (
                "ReflectionProperty",
                reflection_class_property_names_for_filter(ctx, &class_name, filter)?
                    .into_iter()
                    .map(|property| {
                        reflection_member_constructor_expr(
                            "ReflectionProperty",
                            &class_name,
                            &property,
                            expr.span,
                        )
                    })
                    .collect::<Vec<_>>(),
            )
        }
        "getmethods" => {
            let filter = reflection_class_get_methods_filter_arg(ctx, args)?;
            (
                "ReflectionMethod",
                reflection_class_method_names_for_filter(ctx, &class_name, filter)?
                    .into_iter()
                    .map(|method| {
                        reflection_member_constructor_expr(
                            "ReflectionMethod",
                            &class_name,
                            &method,
                            expr.span,
                        )
                    })
                    .collect::<Vec<_>>(),
            )
        }
        _ => return None,
    };
    Some(lower_reflection_member_array(
        ctx,
        member_class,
        &items,
        expr,
    ))
}

/// Lowers a statically materialized Reflection member list with an explicit element type.
fn lower_reflection_member_array(
    ctx: &mut LoweringContext<'_, '_>,
    member_class: &str,
    items: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let elem_ty = PhpType::Object(member_class.to_string());
    let array_ty = PhpType::Array(Box::new(elem_ty.clone()));
    let array = ctx.emit_value(
        Op::ArrayNew,
        Vec::new(),
        Some(Immediate::Capacity(items.len() as u32)),
        array_ty,
        Op::ArrayNew.default_effects(),
        Some(expr.span),
    );
    for item in items {
        let value = lower_expr(ctx, item);
        ctx.emit_void(
            Op::ArrayPush,
            vec![array.value, value.value],
            None,
            Op::ArrayPush.default_effects(),
            Some(item.span),
        );
        release_value_after_retaining_insert(ctx, Some(&elem_ty), value, item.span);
    }
    array
}

/// Builds a direct Reflection member constructor expression for known metadata.
fn reflection_member_constructor_expr(
    reflection_class: &str,
    reflected_class: &str,
    member: &str,
    span: Span,
) -> Expr {
    Expr::new(
        ExprKind::NewObject {
            class_name: Name::unqualified(reflection_class),
            args: vec![
                Expr::new(ExprKind::StringLiteral(reflected_class.to_string()), span),
                Expr::new(ExprKind::StringLiteral(member.to_string()), span),
            ],
        },
        span,
    )
}

/// Lowers reflected function invocation for statically-known `ReflectionFunction` objects.
fn lower_reflection_function_invoke_call(
    ctx: &mut LoweringContext<'_, '_>,
    object_expr: Option<&Expr>,
    method: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let method_key = php_symbol_key(method);
    let object_expr = object_expr?;
    let function_name = reflection_function_reflected_target(ctx, object_expr)?;
    let Some(forwarded_args) = (match method_key.as_str() {
        "invoke" => Some(reflection_function_invoke_args(args)),
        "invokeargs" => reflection_function_invoke_args_array(ctx, args),
        _ => return None,
    }) else {
        return Some(lower_reflection_function_invoke_unsupported(
            ctx,
            &method_key,
            expr,
        ));
    };
    if let Some(signature) = first_class_builtin_signature(&function_name) {
        return Some(lower_reflection_builtin_function_call(
            ctx,
            &function_name,
            &signature,
            &forwarded_args,
            expr,
        ));
    }
    let name = Name::from(function_name);
    Some(lower_function_call(ctx, &name, &forwarded_args, expr))
}

/// Lowers reflected invocation of a supported callable builtin.
fn lower_reflection_builtin_function_call(
    ctx: &mut LoweringContext<'_, '_>,
    function_name: &str,
    signature: &FunctionSig,
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let operands = lower_builtin_call_args(ctx, function_name, Some(signature), args);
    let php_type = registry_builtin_result_type(ctx, function_name, args, &operands, expr.span)
        .unwrap_or_else(|| call_return_type(ctx, function_name, &operands));
    emit_builtin_call_value(
        ctx,
        function_name,
        operands,
        php_type,
        expr.span,
        None,
    )
}

/// Returns direct `ReflectionFunction::invoke(...$args)` arguments after static spread expansion.
fn reflection_function_invoke_args(args: &[Expr]) -> Vec<Expr> {
    reflection_class_new_instance_args(args)
}

/// Extracts the argument list passed to `ReflectionFunction::invokeArgs($args)`.
fn reflection_function_invoke_args_array(
    ctx: &LoweringContext<'_, '_>,
    args: &[Expr],
) -> Option<Vec<Expr>> {
    let args = reflection_class_new_instance_args(args);
    if args.iter().any(is_spread_arg) {
        return None;
    }
    if !crate::types::call_args::has_named_args(&args) {
        return match args.as_slice() {
            [forwarded] => reflection_class_new_instance_args_value(ctx, forwarded),
            _ => None,
        };
    }
    let sig = ctx
        .classes
        .get("ReflectionFunction")
        .and_then(|class_info| class_info.methods.get(&php_symbol_key("invokeArgs")))?;
    let call_span = args
        .first()
        .map(|arg| arg.span)
        .unwrap_or_else(crate::span::Span::dummy);
    let plan = crate::types::call_args::plan_call_args_with_regular_param_count_and_assoc_spreads(
        sig,
        &args,
        call_span,
        crate::types::call_args::regular_param_count(sig),
        false,
        true,
        &assoc_spread_sources(ctx, &args),
    )
    .ok()?;
    if plan.has_spread_args() {
        return None;
    }
    let forwarded_arg = planned_regular_arg_expr(plan.regular_args.first()?)?;
    reflection_class_new_instance_args_value(ctx, forwarded_arg)
}

/// Emits a runtime fatal for ReflectionFunction invocation forms not yet lowered.
fn lower_reflection_function_invoke_unsupported(
    ctx: &mut LoweringContext<'_, '_>,
    method_key: &str,
    expr: &Expr,
) -> LoweredValue {
    let result = lower_boxed_null(ctx, expr);
    let method_name = if method_key == "invokeargs" {
        "invokeArgs"
    } else {
        "invoke"
    };
    let message = ctx.intern_string(&format!(
        "Fatal error: unsupported ReflectionFunction::{}() target or argument forwarding\n",
        method_name
    ));
    ctx.builder.terminate(Terminator::Fatal { message });
    result
}

/// Lowers reflected method invocation for statically-known `ReflectionMethod` objects.
fn lower_reflection_method_invoke_call(
    ctx: &mut LoweringContext<'_, '_>,
    object_expr: Option<&Expr>,
    method: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let method_key = php_symbol_key(method);
    let object_expr = object_expr?;
    let (class_name, reflected_method) = reflection_method_reflected_target(ctx, object_expr)?;
    let Some((object_arg, forwarded_args)) = (match method_key.as_str() {
        "invoke" => reflection_method_invoke_args(args),
        "invokeargs" => reflection_method_invoke_args_array(ctx, args),
        _ => return None,
    }) else {
        return Some(lower_reflection_method_invoke_unsupported(
            ctx,
            &method_key,
            expr,
        ));
    };
    let Some(target_kind) = reflection_method_target_kind(ctx, &class_name, &reflected_method)
    else {
        return Some(lower_reflection_method_invoke_unsupported(
            ctx,
            &method_key,
            expr,
        ));
    };
    match target_kind {
        ReflectionMethodTargetKind::Static => Some(lower_reflection_static_method_invoke(
            ctx,
            &class_name,
            &reflected_method,
            &object_arg,
            &forwarded_args,
            expr,
        )),
        ReflectionMethodTargetKind::Instance => Some(lower_reflection_instance_method_invoke(
            ctx,
            &reflected_method,
            &object_arg,
            &forwarded_args,
            expr,
        )),
    }
}

/// Lowers a static reflected-method invocation after evaluating the ignored object slot.
fn lower_reflection_static_method_invoke(
    ctx: &mut LoweringContext<'_, '_>,
    class_name: &str,
    reflected_method: &str,
    object_arg: &Expr,
    forwarded_args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let ignored_object = lower_expr(ctx, object_arg);
    if ctx.value_is_owning_temporary(ignored_object) {
        crate::ir_lower::ownership::release_if_owned(ctx, ignored_object, Some(object_arg.span));
    }
    let receiver = StaticReceiver::Named(Name::from(class_name.to_string()));
    lower_static_method_call(ctx, &receiver, reflected_method, forwarded_args, expr)
}

/// Lowers an instance reflected-method invocation using the first invoke argument as receiver.
fn lower_reflection_instance_method_invoke(
    ctx: &mut LoweringContext<'_, '_>,
    reflected_method: &str,
    object_arg: &Expr,
    forwarded_args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let object = lower_expr(ctx, object_arg);
    if value_is_definitely_null(ctx, object.value) {
        let null_value = lower_null(ctx, expr);
        terminate_method_call_on_null(ctx, reflected_method);
        return null_value;
    }
    if value_is_nullable(ctx, object.value) {
        return lower_nullable_regular_method_call(
            ctx,
            object,
            reflected_method,
            forwarded_args,
            expr,
        );
    }
    lower_method_call_with_receiver(
        ctx,
        object,
        reflected_method,
        forwarded_args,
        Op::MethodCall,
        expr,
    )
}

/// Splits `ReflectionMethod::invoke($object, ...$args)` into receiver and method args.
fn reflection_method_invoke_args(args: &[Expr]) -> Option<(Expr, Vec<Expr>)> {
    let args = reflection_class_new_instance_args(args);
    if !crate::types::call_args::has_named_args(&args) {
        return match args.as_slice() {
            [object, forwarded @ ..] => Some((object.clone(), forwarded.to_vec())),
            _ => None,
        };
    }
    let mut object = None;
    let mut forwarded = Vec::new();
    let mut args = args.into_iter();
    if let Some(first) = args.next() {
        match first.kind {
            ExprKind::NamedArg {
                ref name,
                ref value,
            } if php_symbol_key(name) == "object" => {
                object = Some((**value).clone());
            }
            ExprKind::NamedArg { .. } => forwarded.push(first),
            _ => object = Some(first),
        }
    }
    for arg in args {
        match arg.kind {
            ExprKind::NamedArg {
                ref name,
                ref value,
            } if php_symbol_key(name) == "object" => {
                if object.replace((**value).clone()).is_some() {
                    return None;
                }
            }
            _ => forwarded.push(arg),
        }
    }
    object.map(|object| (object, forwarded))
}

/// Splits `ReflectionMethod::invokeArgs($object, $args)` into receiver and method args.
fn reflection_method_invoke_args_array(
    ctx: &LoweringContext<'_, '_>,
    args: &[Expr],
) -> Option<(Expr, Vec<Expr>)> {
    let args = reflection_class_new_instance_args(args);
    if args.iter().any(is_spread_arg) {
        return None;
    }
    if !crate::types::call_args::has_named_args(&args) {
        return match args.as_slice() {
            [object, forwarded] => {
                let forwarded = reflection_class_new_instance_args_value(ctx, forwarded)?;
                Some((object.clone(), forwarded))
            }
            _ => None,
        };
    }
    let sig = ctx
        .classes
        .get("ReflectionMethod")
        .and_then(|class_info| class_info.methods.get(&php_symbol_key("invokeArgs")))?;
    let call_span = args
        .first()
        .map(|arg| arg.span)
        .unwrap_or_else(crate::span::Span::dummy);
    let plan = crate::types::call_args::plan_call_args_with_regular_param_count_and_assoc_spreads(
        sig,
        &args,
        call_span,
        crate::types::call_args::regular_param_count(sig),
        false,
        true,
        &assoc_spread_sources(ctx, &args),
    )
    .ok()?;
    if plan.has_spread_args() {
        return None;
    }
    let object = planned_regular_arg_expr(plan.regular_args.first()?)?.clone();
    let forwarded_arg = planned_regular_arg_expr(plan.regular_args.get(1)?)?;
    let forwarded = reflection_class_new_instance_args_value(ctx, forwarded_arg)?;
    Some((object, forwarded))
}

/// Classifies whether a known reflected method is static or instance-dispatched.
fn reflection_method_target_kind(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    method: &str,
) -> Option<ReflectionMethodTargetKind> {
    let class_info = ctx.classes.get(class_name.trim_start_matches('\\'))?;
    let method_key = php_symbol_key(method);
    if class_info.static_methods.contains_key(&method_key) {
        return Some(ReflectionMethodTargetKind::Static);
    }
    if class_info.methods.contains_key(&method_key) {
        return Some(ReflectionMethodTargetKind::Instance);
    }
    None
}

/// Dispatch kind for a statically-known reflected method.
#[derive(Clone, Copy)]
enum ReflectionMethodTargetKind {
    Instance,
    Static,
}

/// Emits a runtime fatal for ReflectionMethod invocation forms not yet lowered.
fn lower_reflection_method_invoke_unsupported(
    ctx: &mut LoweringContext<'_, '_>,
    method_key: &str,
    expr: &Expr,
) -> LoweredValue {
    let result = lower_boxed_null(ctx, expr);
    let method_name = if method_key == "invokeargs" {
        "invokeArgs"
    } else {
        "invoke"
    };
    let message = ctx.intern_string(&format!(
        "Fatal error: unsupported ReflectionMethod::{}() target or argument forwarding\n",
        method_name
    ));
    ctx.builder.terminate(Terminator::Fatal { message });
    result
}

/// Lowers `ReflectionProperty::getValue($object)` when the reflected property is known.
fn lower_reflection_property_value_call(
    ctx: &mut LoweringContext<'_, '_>,
    object_expr: Option<&Expr>,
    method: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let object_expr = object_expr?;
    match php_symbol_key(method).as_str() {
        "getvalue" => {
            if let Some((declaring_class, property, property_ty)) =
                reflection_property_static_target(ctx, object_expr)
            {
                return lower_reflection_property_get_static_value(
                    ctx,
                    &declaring_class,
                    &property,
                    property_ty,
                    args,
                    expr,
                );
            }
            let (_, property, _) = reflection_property_instance_target(ctx, object_expr)?;
            lower_reflection_property_get_value(ctx, &property, args, expr)
        }
        "setvalue" => {
            if let Some((declaring_class, property, _)) =
                reflection_property_static_target(ctx, object_expr)
            {
                return lower_reflection_property_set_static_value(
                    ctx,
                    &declaring_class,
                    &property,
                    args,
                    expr,
                );
            }
            let (_, property, _) = reflection_property_instance_target(ctx, object_expr)?;
            lower_reflection_property_set_value(ctx, &property, args, expr)
        }
        "isinitialized" => {
            if let Some((declaring_class, property, _)) =
                reflection_property_static_target(ctx, object_expr)
            {
                return lower_reflection_property_static_is_initialized(
                    ctx,
                    &declaring_class,
                    &property,
                    args,
                    expr,
                );
            }
            let (_, property, _) = reflection_property_any_instance_target(ctx, object_expr)?;
            lower_reflection_property_is_initialized(ctx, &property, args, expr)
        }
        _ => None,
    }
}

/// Lowers `ReflectionProperty::getValue($object)` to a direct property read.
fn lower_reflection_property_get_value(
    ctx: &mut LoweringContext<'_, '_>,
    property: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let object_arg = reflection_property_get_value_arg(args)?;
    let object = lower_expr(ctx, &object_arg);
    Some(lower_property_get_from_value(
        ctx,
        object,
        property,
        Op::PropGet,
        expr,
    ))
}

/// Lowers `ReflectionProperty::setValue($object, $value)` to a direct property write.
fn lower_reflection_property_set_value(
    ctx: &mut LoweringContext<'_, '_>,
    property: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let (object_arg, value_arg) = reflection_property_set_value_args(args)?;
    let target = Expr::new(
        ExprKind::PropertyAccess {
            object: Box::new(object_arg),
            property: property.to_string(),
        },
        expr.span,
    );
    lower_non_local_assignment_write(ctx, &target, &value_arg, expr.span);
    Some(lower_null(ctx, expr))
}

/// Lowers `ReflectionProperty::isInitialized($object)` to a direct slot probe.
fn lower_reflection_property_is_initialized(
    ctx: &mut LoweringContext<'_, '_>,
    property: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let object_arg = reflection_property_get_value_arg(args)?;
    let object = lower_expr(ctx, &object_arg);
    let data = ctx.intern_string(property);
    Some(ctx.emit_value(
        Op::PropInitialized,
        vec![object.value],
        Some(Immediate::Data(data)),
        PhpType::Bool,
        Op::PropInitialized.default_effects(),
        Some(expr.span),
    ))
}

/// Lowers static `ReflectionProperty::getValue()` to a reflection static-property read.
fn lower_reflection_property_get_static_value(
    ctx: &mut LoweringContext<'_, '_>,
    declaring_class: &str,
    property: &str,
    property_ty: PhpType,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if let Some(ignored_object) = reflection_property_static_get_value_ignored_arg(args)? {
        lower_ignored_reflection_argument(ctx, &ignored_object);
    }
    Some(lower_reflection_static_property_get_by_class_name(
        ctx,
        declaring_class,
        property,
        property_ty,
        expr,
    ))
}

/// Lowers static `ReflectionProperty::isInitialized()` to a direct static-slot probe.
fn lower_reflection_property_static_is_initialized(
    ctx: &mut LoweringContext<'_, '_>,
    declaring_class: &str,
    property: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if let Some(ignored_object) = reflection_property_static_get_value_ignored_arg(args)? {
        lower_ignored_reflection_argument(ctx, &ignored_object);
    }
    Some(lower_reflection_static_property_initialized_by_class_name(
        ctx,
        declaring_class,
        property,
        expr,
    ))
}

/// Lowers static `ReflectionProperty::setValue(null, $value)` to a reflection static-property write.
fn lower_reflection_property_set_static_value(
    ctx: &mut LoweringContext<'_, '_>,
    declaring_class: &str,
    property: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let (ignored_object, value_arg) = reflection_property_static_set_value_args(args)?;
    lower_ignored_reflection_argument(ctx, &ignored_object);
    let value = lower_expr(ctx, &value_arg);
    store_reflection_static_property_by_class_name(
        ctx,
        declaring_class,
        property,
        value.value,
        expr.span,
    );
    Some(lower_null(ctx, expr))
}

/// Evaluates an ignored Reflection argument and releases temporary objects.
fn lower_ignored_reflection_argument(ctx: &mut LoweringContext<'_, '_>, arg: &Expr) {
    let value = lower_expr(ctx, arg);
    if ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(arg.span));
    }
}

/// Returns the explicit object argument passed to `ReflectionProperty::getValue()`.
fn reflection_property_get_value_arg(args: &[Expr]) -> Option<Expr> {
    let args = reflection_class_new_instance_args(args);
    if args.iter().any(is_spread_arg) {
        return None;
    }
    let object = if !crate::types::call_args::has_named_args(&args) {
        match args.as_slice() {
            [object] => object.clone(),
            _ => return None,
        }
    } else {
        reflection_property_named_object_arg(&args)?
    };
    (!matches!(&object.kind, ExprKind::Null)).then_some(object)
}

/// Returns the explicit object and value arguments passed to `ReflectionProperty::setValue()`.
fn reflection_property_set_value_args(args: &[Expr]) -> Option<(Expr, Expr)> {
    let args = reflection_class_new_instance_args(args);
    if args.iter().any(is_spread_arg) {
        return None;
    }
    let (object, value) =
        reflection_class_static_property_regular_args(&args, "object", Some("value"))?;
    let object = object?;
    if matches!(&object.kind, ExprKind::Null) {
        return None;
    }
    Some((object, value?))
}

/// Returns the optional ignored object argument for static `ReflectionProperty::getValue()`.
fn reflection_property_static_get_value_ignored_arg(args: &[Expr]) -> Option<Option<Expr>> {
    let args = reflection_class_new_instance_args(args);
    if args.iter().any(is_spread_arg) {
        return None;
    }
    if !crate::types::call_args::has_named_args(&args) {
        return match args.as_slice() {
            [] => Some(None),
            [object] => Some(Some(object.clone())),
            _ => None,
        };
    }
    reflection_property_named_optional_object_arg(&args)
}

/// Returns the ignored object and value arguments for static `ReflectionProperty::setValue()`.
fn reflection_property_static_set_value_args(args: &[Expr]) -> Option<(Expr, Expr)> {
    let args = reflection_class_new_instance_args(args);
    if args.iter().any(is_spread_arg) {
        return None;
    }
    let (object, value) =
        reflection_class_static_property_regular_args(&args, "object", Some("value"))?;
    Some((object?, value?))
}

/// Returns a required named `object` argument for ReflectionProperty value access.
fn reflection_property_named_object_arg(args: &[Expr]) -> Option<Expr> {
    reflection_property_named_optional_object_arg(args)?
}

/// Returns an optional named `object` argument for ReflectionProperty value access.
fn reflection_property_named_optional_object_arg(args: &[Expr]) -> Option<Option<Expr>> {
    let mut object = None;
    for arg in args {
        match &arg.kind {
            ExprKind::NamedArg { name, value } if php_symbol_key(name) == "object" => {
                object = Some((**value).clone());
            }
            _ => return None,
        }
    }
    Some(object)
}

/// Resolves an inline `new ReflectionProperty(Known::class, "prop")` instance property target.
fn reflection_property_instance_target(
    ctx: &LoweringContext<'_, '_>,
    object_expr: &Expr,
) -> Option<(String, String, PhpType)> {
    let (class_name, property) = reflection_property_reflected_target(ctx, object_expr)?;
    let class_info = ctx.classes.get(class_name.trim_start_matches('\\'))?;
    if class_info
        .static_properties
        .iter()
        .any(|(name, _)| name == &property)
    {
        return None;
    }
    if class_info.property_visibilities.get(&property) != Some(&Visibility::Public) {
        return None;
    }
    let (_, (_, property_ty)) = class_info.visible_property(&property)?;
    Some((
        class_name,
        property,
        normalize_value_php_type(property_ty.codegen_repr()),
    ))
}

/// Resolves a known non-static ReflectionProperty target without enforcing visibility.
fn reflection_property_any_instance_target(
    ctx: &LoweringContext<'_, '_>,
    object_expr: &Expr,
) -> Option<(String, String, PhpType)> {
    let (class_name, property) = reflection_property_reflected_target(ctx, object_expr)?;
    let class_info = ctx.classes.get(class_name.trim_start_matches('\\'))?;
    if class_info
        .static_properties
        .iter()
        .any(|(name, _)| name == &property)
    {
        return None;
    }
    let (_, (_, property_ty)) = class_info.visible_property(&property)?;
    Some((
        class_name,
        property,
        normalize_value_php_type(property_ty.codegen_repr()),
    ))
}

/// Resolves an inline `ReflectionProperty` target for a static property.
fn reflection_property_static_target(
    ctx: &LoweringContext<'_, '_>,
    object_expr: &Expr,
) -> Option<(String, String, PhpType)> {
    let (class_name, property) = reflection_property_reflected_target(ctx, object_expr)?;
    let (declaring_class, property_ty) =
        reflection_class_static_property_target(ctx, &class_name, &property)?;
    Some((declaring_class, property, property_ty))
}

/// Extracts the known class and property name from a supported ReflectionProperty source.
fn reflection_property_reflected_target(
    ctx: &LoweringContext<'_, '_>,
    object_expr: &Expr,
) -> Option<(String, String)> {
    reflection_property_constructor_target(ctx, object_expr)
        .or_else(|| reflection_property_class_get_property_target(ctx, object_expr))
        .or_else(|| reflection_property_class_get_properties_index_target(ctx, object_expr))
        .or_else(|| {
            let ExprKind::Variable(name) = &object_expr.kind else {
                return None;
            };
            ctx.reflection_property_local(name)
        })
}

/// Extracts the known class and method name from a supported ReflectionMethod source.
fn reflection_method_reflected_target(
    ctx: &LoweringContext<'_, '_>,
    object_expr: &Expr,
) -> Option<(String, String)> {
    reflection_method_constructor_target(ctx, object_expr)
        .or_else(|| reflection_method_class_get_constructor_target(ctx, object_expr))
        .or_else(|| reflection_method_class_get_method_target(ctx, object_expr))
        .or_else(|| reflection_method_class_get_methods_index_target(ctx, object_expr))
        .or_else(|| {
            let ExprKind::Variable(name) = &object_expr.kind else {
                return None;
            };
            ctx.reflection_method_local(name)
        })
}

/// Extracts the known function name from a supported ReflectionFunction source.
fn reflection_function_reflected_target(
    ctx: &LoweringContext<'_, '_>,
    object_expr: &Expr,
) -> Option<String> {
    reflection_function_constructor_target(ctx, object_expr).or_else(|| {
        let ExprKind::Variable(name) = &object_expr.kind else {
            return None;
        };
        ctx.reflection_function_local(name)
    })
}

/// Extracts a known ReflectionMethod from `ReflectionClass::getMethods()[N]`.
fn reflection_method_class_get_methods_index_target(
    ctx: &LoweringContext<'_, '_>,
    object_expr: &Expr,
) -> Option<(String, String)> {
    let ExprKind::ArrayAccess { array, index } = &object_expr.kind else {
        return None;
    };
    let ExprKind::IntLiteral(raw_index) = &index.kind else {
        return None;
    };
    if *raw_index < 0 {
        return None;
    }
    let ExprKind::MethodCall {
        object,
        method,
        args,
    } = &array.kind
    else {
        return None;
    };
    if php_symbol_key(method) != "getmethods" {
        return None;
    }
    let filter = reflection_class_get_methods_filter_arg(ctx, args)?;
    let class_name = reflection_class_reflected_class(ctx, object)?;
    let method =
        reflection_class_method_name_at_index(ctx, &class_name, *raw_index as usize, filter)?;
    Some((class_name, method))
}

/// Returns the `ReflectionClass::getMethods()` method name at a known index.
fn reflection_class_method_name_at_index(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    index: usize,
    filter: Option<i64>,
) -> Option<String> {
    reflection_class_method_names_for_filter(ctx, class_name, filter)?
        .into_iter()
        .nth(index)
}

/// Extracts a known ReflectionProperty from `ReflectionClass::getProperties()[N]`.
fn reflection_property_class_get_properties_index_target(
    ctx: &LoweringContext<'_, '_>,
    object_expr: &Expr,
) -> Option<(String, String)> {
    let ExprKind::ArrayAccess { array, index } = &object_expr.kind else {
        return None;
    };
    let ExprKind::IntLiteral(raw_index) = &index.kind else {
        return None;
    };
    if *raw_index < 0 {
        return None;
    }
    let ExprKind::MethodCall {
        object,
        method,
        args,
    } = &array.kind
    else {
        return None;
    };
    if php_symbol_key(method) != "getproperties" {
        return None;
    }
    let filter = reflection_class_get_properties_filter_arg(ctx, args)?;
    let class_name = reflection_class_reflected_class(ctx, object)?;
    let property =
        reflection_class_property_name_at_index(ctx, &class_name, *raw_index as usize, filter)?;
    Some((class_name, property))
}

/// Returns the `ReflectionClass::getProperties()` property name at a known index.
fn reflection_class_property_name_at_index(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    index: usize,
    filter: Option<i64>,
) -> Option<String> {
    reflection_class_property_names_for_filter(ctx, class_name, filter)?
        .into_iter()
        .nth(index)
}

/// Returns `ReflectionClass::getProperties()` names after applying a known filter.
fn reflection_class_property_names_for_filter(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    filter: Option<i64>,
) -> Option<Vec<String>> {
    let class_info = ctx.classes.get(class_name.trim_start_matches('\\'))?;
    Some(
        class_info
            .properties
            .iter()
            .chain(class_info.static_properties.iter())
            .map(|(name, _)| name)
            .filter(|name| reflection_property_matches_filter(class_info, name, filter))
            .cloned()
            .collect(),
    )
}

/// Returns `ReflectionClass::getMethods()` names after applying a known filter.
fn reflection_class_method_names_for_filter(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    filter: Option<i64>,
) -> Option<Vec<String>> {
    let class_info = ctx.classes.get(class_name.trim_start_matches('\\'))?;
    let mut names = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for name in class_info
        .methods
        .keys()
        .chain(class_info.static_methods.keys())
    {
        if seen.insert(php_symbol_key(name))
            && reflection_method_matches_filter(class_info, name, filter)
        {
            names.push(name.clone());
        }
    }
    Some(names)
}

/// Returns the optional `ReflectionClass::getProperties()` modifier filter.
fn reflection_class_get_properties_filter_arg(
    ctx: &LoweringContext<'_, '_>,
    args: &[Expr],
) -> Option<Option<i64>> {
    reflection_class_member_filter_arg(ctx, args, "ReflectionProperty")
}

/// Returns the optional `ReflectionClass::getMethods()` modifier filter.
fn reflection_class_get_methods_filter_arg(
    ctx: &LoweringContext<'_, '_>,
    args: &[Expr],
) -> Option<Option<i64>> {
    reflection_class_member_filter_arg(ctx, args, "ReflectionMethod")
}

/// Returns the optional ReflectionClass member-list modifier filter.
fn reflection_class_member_filter_arg(
    ctx: &LoweringContext<'_, '_>,
    args: &[Expr],
    constant_class: &str,
) -> Option<Option<i64>> {
    let args = reflection_class_new_instance_args(args);
    if args.iter().any(is_spread_arg) {
        return None;
    }
    if !crate::types::call_args::has_named_args(&args) {
        return match args.as_slice() {
            [] => Some(None),
            [filter] => reflection_member_filter_value(ctx, filter, constant_class),
            _ => None,
        };
    }
    let (filter, _) = reflection_class_static_property_regular_args(&args, "filter", None)?;
    filter
        .as_ref()
        .map(|filter| reflection_member_filter_value(ctx, filter, constant_class))
        .unwrap_or(Some(None))
}

/// Returns a known integer modifier filter expression.
fn reflection_member_filter_value(
    ctx: &LoweringContext<'_, '_>,
    expr: &Expr,
    constant_class: &str,
) -> Option<Option<i64>> {
    match &expr.kind {
        ExprKind::Null => Some(None),
        ExprKind::IntLiteral(value) => Some(Some(*value)),
        ExprKind::ScopedConstantAccess { receiver, name } => Some(Some(
            reflection_member_filter_constant(ctx, receiver, name, constant_class)?,
        )),
        _ => None,
    }
}

/// Resolves a `Reflection*::IS_*` class constant to its integer value.
fn reflection_member_filter_constant(
    ctx: &LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    name: &str,
    constant_class: &str,
) -> Option<i64> {
    let class_name = static_receiver_class_name(ctx, receiver)?;
    if php_symbol_key(class_name.trim_start_matches('\\')) != php_symbol_key(constant_class) {
        return None;
    }
    let value = ctx.scoped_constant_value(&class_name, name)?;
    let ExprKind::IntLiteral(value) = value.kind else {
        return None;
    };
    Some(value)
}

/// Returns whether a method should be present for a modifier filter.
fn reflection_method_matches_filter(
    class_info: &crate::types::ClassInfo,
    method: &str,
    filter: Option<i64>,
) -> bool {
    let Some(filter) = filter else {
        return true;
    };
    reflection_method_filter_modifiers(class_info, method)
        .is_some_and(|modifiers| modifiers & filter != 0)
}

/// Returns whether a property should be present for a modifier filter.
fn reflection_property_matches_filter(
    class_info: &crate::types::ClassInfo,
    property: &str,
    filter: Option<i64>,
) -> bool {
    let Some(filter) = filter else {
        return true;
    };
    reflection_property_filter_modifiers(class_info, property)
        .is_some_and(|modifiers| modifiers & filter != 0)
}

/// Computes ReflectionMethod modifier bits for static filter resolution.
fn reflection_method_filter_modifiers(
    class_info: &crate::types::ClassInfo,
    method: &str,
) -> Option<i64> {
    let method_key = php_symbol_key(method);
    if class_info.methods.contains_key(&method_key) {
        let visibility = class_info
            .method_visibilities
            .get(&method_key)
            .unwrap_or(&Visibility::Public);
        return Some(reflection_method_filter_modifier_bits(
            visibility,
            false,
            class_info.final_methods.contains(&method_key),
            !class_info.method_impl_classes.contains_key(&method_key),
        ));
    }
    if class_info.static_methods.contains_key(&method_key) {
        let visibility = class_info
            .static_method_visibilities
            .get(&method_key)
            .unwrap_or(&Visibility::Public);
        return Some(reflection_method_filter_modifier_bits(
            visibility,
            true,
            class_info.final_static_methods.contains(&method_key),
            !class_info
                .static_method_impl_classes
                .contains_key(&method_key),
        ));
    }
    None
}

/// Computes ReflectionProperty modifier bits for static filter resolution.
fn reflection_property_filter_modifiers(
    class_info: &crate::types::ClassInfo,
    property: &str,
) -> Option<i64> {
    if class_info
        .properties
        .iter()
        .any(|(name, _)| name == property)
    {
        let visibility = class_info
            .property_visibilities
            .get(property)
            .unwrap_or(&Visibility::Public);
        return Some(reflection_property_filter_modifier_bits(
            visibility,
            false,
            class_info.final_properties.contains(property),
            class_info.abstract_properties.contains(property),
            class_info.readonly_properties.contains(property),
            reflection_property_filter_is_virtual(class_info, property),
            class_info.property_set_visibilities.get(property),
        ));
    }
    if class_info
        .static_properties
        .iter()
        .any(|(name, _)| name == property)
    {
        let visibility = class_info
            .static_property_visibilities
            .get(property)
            .unwrap_or(&Visibility::Public);
        return Some(reflection_property_filter_modifier_bits(
            visibility,
            true,
            class_info.final_static_properties.contains(property),
            false,
            false,
            false,
            None,
        ));
    }
    None
}

/// Builds the ReflectionMethod modifier bitmask for filter matching.
fn reflection_method_filter_modifier_bits(
    visibility: &Visibility,
    is_static: bool,
    is_final: bool,
    is_abstract: bool,
) -> i64 {
    let mut modifiers = match visibility {
        Visibility::Public => 1,
        Visibility::Protected => 2,
        Visibility::Private => 4,
    };
    if is_static {
        modifiers |= 16;
    }
    if is_final {
        modifiers |= 32;
    }
    if is_abstract {
        modifiers |= 64;
    }
    modifiers
}

/// Returns whether a property has hook metadata that makes it virtual.
fn reflection_property_filter_is_virtual(
    class_info: &crate::types::ClassInfo,
    property: &str,
) -> bool {
    let get_method = php_symbol_key(&property_hook_get_method(property));
    let set_method = php_symbol_key(&property_hook_set_method(property));
    class_info.abstract_property_hooks.contains_key(property)
        || class_info.methods.contains_key(&get_method)
        || class_info.methods.contains_key(&set_method)
}

/// Builds the ReflectionProperty modifier bitmask for filter matching.
fn reflection_property_filter_modifier_bits(
    visibility: &Visibility,
    is_static: bool,
    is_final: bool,
    is_abstract: bool,
    is_readonly: bool,
    is_virtual: bool,
    set_visibility: Option<&Visibility>,
) -> i64 {
    let mut modifiers = match visibility {
        Visibility::Public => 1,
        Visibility::Protected => 2,
        Visibility::Private => 4,
    };
    if is_static {
        modifiers |= 16;
    }
    if is_final {
        modifiers |= 32;
    }
    if is_abstract {
        modifiers |= 64;
    }
    if is_readonly {
        modifiers |= 128;
    }
    if is_virtual {
        modifiers |= 512;
    }
    match set_visibility {
        Some(Visibility::Private) => modifiers |= 32 | 4096,
        Some(Visibility::Protected) => modifiers |= 2048,
        Some(Visibility::Public) | None => {
            if is_readonly && visibility == &Visibility::Public {
                modifiers |= 2048;
            }
        }
    }
    modifiers
}

/// Extracts the known function name from an inline ReflectionFunction constructor.
fn reflection_function_constructor_target(
    ctx: &LoweringContext<'_, '_>,
    object_expr: &Expr,
) -> Option<String> {
    let ExprKind::NewObject { class_name, args } = &object_expr.kind else {
        return None;
    };
    if php_symbol_key(class_name.as_str().trim_start_matches('\\')) != "reflectionfunction" {
        return None;
    }
    let function_arg = reflection_function_constructor_regular_arg(ctx, args)?;
    let ExprKind::StringLiteral(function_name) = function_arg.kind else {
        return None;
    };
    resolve_known_reflection_function_name(ctx, &function_name)
}

/// Resolves function names accepted by static `ReflectionFunction` metadata.
fn resolve_known_reflection_function_name(
    ctx: &LoweringContext<'_, '_>,
    function_name: &str,
) -> Option<String> {
    resolve_known_function_name(ctx, function_name)
        .or_else(|| resolve_known_reflection_builtin_name(function_name))
}

/// Resolves a supported callable builtin name for `ReflectionFunction`.
fn resolve_known_reflection_builtin_name(function_name: &str) -> Option<String> {
    let canonical = canonical_builtin_function_name(function_name.trim_start_matches('\\'))?;
    first_class_builtin_signature(&canonical).map(|_| canonical)
}

/// Extracts the known class and property name from an inline ReflectionProperty constructor.
fn reflection_property_constructor_target(
    ctx: &LoweringContext<'_, '_>,
    object_expr: &Expr,
) -> Option<(String, String)> {
    let ExprKind::NewObject { class_name, args } = &object_expr.kind else {
        return None;
    };
    if php_symbol_key(class_name.as_str().trim_start_matches('\\')) != "reflectionproperty" {
        return None;
    }
    let (class_arg, property_arg) = reflection_property_constructor_regular_args(ctx, args)?;
    let raw_class_name = match &class_arg.kind {
        ExprKind::StringLiteral(value) => value.clone(),
        ExprKind::ClassConstant { receiver } => static_receiver_class_name(ctx, receiver)?,
        _ => return None,
    };
    let class_name = resolve_known_class_name(ctx, &raw_class_name)?;
    let ExprKind::StringLiteral(property) = property_arg.kind else {
        return None;
    };
    Some((class_name, property))
}

/// Extracts the known class and method name from an inline ReflectionMethod constructor.
fn reflection_method_constructor_target(
    ctx: &LoweringContext<'_, '_>,
    object_expr: &Expr,
) -> Option<(String, String)> {
    let ExprKind::NewObject { class_name, args } = &object_expr.kind else {
        return None;
    };
    if php_symbol_key(class_name.as_str().trim_start_matches('\\')) != "reflectionmethod" {
        return None;
    }
    let (class_arg, method_arg) = reflection_method_constructor_regular_args(ctx, args)?;
    let raw_class_name = match &class_arg.kind {
        ExprKind::StringLiteral(value) => value.clone(),
        ExprKind::ClassConstant { receiver } => static_receiver_class_name(ctx, receiver)?,
        _ => return None,
    };
    let class_name = resolve_known_class_name(ctx, &raw_class_name)?;
    let ExprKind::StringLiteral(method) = method_arg.kind else {
        return None;
    };
    let method = resolve_known_class_method_name(ctx, &class_name, &method)?;
    Some((class_name, method))
}

/// Extracts the constructor target from inline `ReflectionClass::getConstructor()` calls.
fn reflection_method_class_get_constructor_target(
    ctx: &LoweringContext<'_, '_>,
    object_expr: &Expr,
) -> Option<(String, String)> {
    let ExprKind::MethodCall {
        object,
        method,
        args,
    } = &object_expr.kind
    else {
        return None;
    };
    if php_symbol_key(method) != "getconstructor" {
        return None;
    }
    if !reflection_class_new_instance_args(args).is_empty() {
        return None;
    }
    let class_name = reflection_class_reflected_class(ctx, object)?;
    let method = resolve_known_class_method_name(ctx, &class_name, "__construct")?;
    Some((class_name, method))
}

/// Extracts the property target from inline `ReflectionClass::getProperty()` calls.
fn reflection_property_class_get_property_target(
    ctx: &LoweringContext<'_, '_>,
    object_expr: &Expr,
) -> Option<(String, String)> {
    let ExprKind::MethodCall {
        object,
        method,
        args,
    } = &object_expr.kind
    else {
        return None;
    };
    if php_symbol_key(method) != "getproperty" {
        return None;
    }
    let class_name = reflection_class_reflected_class(ctx, object)?;
    let property = reflection_class_member_name_arg(args)?;
    Some((class_name, property))
}

/// Extracts the method target from inline `ReflectionClass::getMethod()` calls.
fn reflection_method_class_get_method_target(
    ctx: &LoweringContext<'_, '_>,
    object_expr: &Expr,
) -> Option<(String, String)> {
    let ExprKind::MethodCall {
        object,
        method,
        args,
    } = &object_expr.kind
    else {
        return None;
    };
    if php_symbol_key(method) != "getmethod" {
        return None;
    }
    let class_name = reflection_class_reflected_class(ctx, object)?;
    let method = reflection_class_member_name_arg(args)?;
    let method = resolve_known_class_method_name(ctx, &class_name, &method)?;
    Some((class_name, method))
}

/// Returns the literal name argument passed to a ReflectionClass member lookup.
fn reflection_class_member_name_arg(args: &[Expr]) -> Option<String> {
    let args = reflection_class_new_instance_args(args);
    if args.iter().any(is_spread_arg) {
        return None;
    }
    let (name, _) = reflection_class_static_property_regular_args(&args, "name", None)?;
    reflection_class_static_property_name_arg(name.as_ref()?)
}

/// Returns normalized constructor args for `ReflectionFunction($function)`.
fn reflection_function_constructor_regular_arg(
    ctx: &LoweringContext<'_, '_>,
    args: &[Expr],
) -> Option<Expr> {
    let args = reflection_class_new_instance_args(args);
    if args.iter().any(is_spread_arg) {
        return None;
    }
    if !crate::types::call_args::has_named_args(&args) {
        return match args.as_slice() {
            [function_arg] => Some(function_arg.clone()),
            _ => None,
        };
    }
    let sig = ctx
        .classes
        .get("ReflectionFunction")
        .and_then(|class_info| class_info.methods.get("__construct"))?;
    let call_span = args
        .first()
        .map(|arg| arg.span)
        .unwrap_or_else(crate::span::Span::dummy);
    let plan = crate::types::call_args::plan_call_args_with_regular_param_count_and_assoc_spreads(
        sig,
        &args,
        call_span,
        crate::types::call_args::regular_param_count(sig),
        false,
        true,
        &assoc_spread_sources(ctx, &args),
    )
    .ok()?;
    if plan.has_spread_args() {
        return None;
    }
    planned_regular_arg_expr(plan.regular_args.first()?).cloned()
}

/// Returns normalized constructor args for `ReflectionProperty($class, $property)`.
fn reflection_property_constructor_regular_args(
    ctx: &LoweringContext<'_, '_>,
    args: &[Expr],
) -> Option<(Expr, Expr)> {
    let args = reflection_class_new_instance_args(args);
    if args.iter().any(is_spread_arg) {
        return None;
    }
    if !crate::types::call_args::has_named_args(&args) {
        return match args.as_slice() {
            [class_arg, property_arg] => Some((class_arg.clone(), property_arg.clone())),
            _ => None,
        };
    }
    let sig = ctx
        .classes
        .get("ReflectionProperty")
        .and_then(|class_info| class_info.methods.get("__construct"))?;
    let call_span = args
        .first()
        .map(|arg| arg.span)
        .unwrap_or_else(crate::span::Span::dummy);
    let plan = crate::types::call_args::plan_call_args_with_regular_param_count_and_assoc_spreads(
        sig,
        &args,
        call_span,
        crate::types::call_args::regular_param_count(sig),
        false,
        true,
        &assoc_spread_sources(ctx, &args),
    )
    .ok()?;
    if plan.has_spread_args() {
        return None;
    }
    let class_arg = planned_regular_arg_expr(plan.regular_args.first()?)?.clone();
    let property_arg = planned_regular_arg_expr(plan.regular_args.get(1)?)?.clone();
    Some((class_arg, property_arg))
}

/// Returns normalized constructor args for `ReflectionMethod($class, $method)`.
fn reflection_method_constructor_regular_args(
    ctx: &LoweringContext<'_, '_>,
    args: &[Expr],
) -> Option<(Expr, Expr)> {
    let args = reflection_class_new_instance_args(args);
    if args.iter().any(is_spread_arg) {
        return None;
    }
    if args.len() == 1 {
        return reflection_method_constructor_single_target(ctx, &args[0]);
    }
    if !crate::types::call_args::has_named_args(&args) {
        return match args.as_slice() {
            [class_arg, method_arg] => Some((class_arg.clone(), method_arg.clone())),
            _ => None,
        };
    }
    let sig = ctx
        .classes
        .get("ReflectionMethod")
        .and_then(|class_info| class_info.methods.get("__construct"))?;
    let call_span = args
        .first()
        .map(|arg| arg.span)
        .unwrap_or_else(crate::span::Span::dummy);
    let plan = crate::types::call_args::plan_call_args_with_regular_param_count_and_assoc_spreads(
        sig,
        &args,
        call_span,
        crate::types::call_args::regular_param_count(sig),
        false,
        true,
        &assoc_spread_sources(ctx, &args),
    )
    .ok()?;
    if plan.has_spread_args() {
        return None;
    }
    let class_arg = planned_regular_arg_expr(plan.regular_args.first()?)?.clone();
    let method_arg = planned_regular_arg_expr(plan.regular_args.get(1)?)?.clone();
    Some((class_arg, method_arg))
}

/// Splits deprecated `ReflectionMethod("Class::method")` constructor syntax.
fn reflection_method_constructor_single_target(
    ctx: &LoweringContext<'_, '_>,
    arg: &Expr,
) -> Option<(Expr, Expr)> {
    let arg = match &arg.kind {
        ExprKind::NamedArg { name, value } if name == "class_name" => value.as_ref(),
        ExprKind::NamedArg { name, value } if name == "objectOrMethod" => value.as_ref(),
        ExprKind::NamedArg { .. } => return None,
        _ => arg,
    };
    let ExprKind::StringLiteral(target) = &arg.kind else {
        return None;
    };
    let (raw_class_name, raw_method_name) = target.rsplit_once("::")?;
    if raw_class_name.is_empty() || raw_method_name.is_empty() {
        return None;
    }
    let class_name = resolve_known_class_name(ctx, raw_class_name)?;
    let method_name = resolve_known_class_method_name(ctx, &class_name, raw_method_name)?;
    Some((
        Expr::new(ExprKind::StringLiteral(class_name), arg.span),
        Expr::new(ExprKind::StringLiteral(method_name), arg.span),
    ))
}

/// Lowers `ReflectionClass::getStaticProperties()` to a live static-property map.
fn lower_reflection_class_get_static_properties(
    ctx: &mut LoweringContext<'_, '_>,
    class_name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if !args.is_empty() {
        return None;
    }
    let properties = reflection_class_static_property_map_entries(ctx, class_name)?;
    let hash_ty = PhpType::AssocArray {
        key: Box::new(PhpType::Str),
        value: Box::new(PhpType::Mixed),
    };
    let hash = ctx.emit_value(
        Op::HashNew,
        Vec::new(),
        Some(Immediate::Capacity(properties.len() as u32)),
        hash_ty,
        Op::HashNew.default_effects(),
        Some(expr.span),
    );
    for (property, declaring_class, property_ty) in properties {
        let key_expr = Expr::new(ExprKind::StringLiteral(property.clone()), expr.span);
        let key = lower_string_literal(ctx, &property, &key_expr);
        let value = lower_reflection_static_property_get_by_class_name(
            ctx,
            &declaring_class,
            &property,
            property_ty,
            expr,
        );
        let value = box_value_as_mixed(ctx, value, expr.span);
        ctx.emit_void(
            Op::HashSet,
            vec![hash.value, key.value, value.value],
            None,
            Op::HashSet.default_effects(),
            Some(expr.span),
        );
    }
    Some(hash)
}

/// Lowers `ReflectionClass::getStaticPropertyValue()` to a live static-property read.
fn lower_reflection_class_get_static_property_value(
    ctx: &mut LoweringContext<'_, '_>,
    class_name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let (property, default) = reflection_class_get_static_property_value_args(args)?;
    if let Some((declaring_class, property_ty)) =
        reflection_class_static_property_target(ctx, class_name, &property)
    {
        if default.is_none() {
            return Some(lower_reflection_static_property_get_by_class_name(
                ctx,
                &declaring_class,
                &property,
                property_ty,
                expr,
            ));
        }
        return None;
    }
    Some(match default {
        Some(default) => lower_expr(ctx, &default),
        None => lower_reflection_class_missing_static_property(ctx, class_name, &property, expr),
    })
}

/// Lowers `ReflectionClass::setStaticPropertyValue()` to a live static-property write.
fn lower_reflection_class_set_static_property_value(
    ctx: &mut LoweringContext<'_, '_>,
    class_name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let (property, value) = reflection_class_set_static_property_value_args(args)?;
    let (declaring_class, _) = reflection_class_static_property_target(ctx, class_name, &property)?;
    let value = lower_expr(ctx, &value);
    store_reflection_static_property_by_class_name(
        ctx,
        &declaring_class,
        &property,
        value.value,
        expr.span,
    );
    Some(lower_null(ctx, expr))
}

/// Lowers a missing static-property lookup to PHP's catchable ReflectionException.
fn lower_reflection_class_missing_static_property(
    ctx: &mut LoweringContext<'_, '_>,
    class_name: &str,
    property: &str,
    expr: &Expr,
) -> LoweredValue {
    let message = format!(
        "Property {}::${} does not exist",
        class_name.trim_start_matches('\\'),
        property
    );
    let exception = Expr::new(
        ExprKind::NewObject {
            class_name: Name::unqualified("ReflectionException"),
            args: vec![Expr::new(ExprKind::StringLiteral(message), expr.span)],
        },
        expr.span,
    );
    let placeholder = lower_null(ctx, expr);
    let exception = lower_expr(ctx, &exception);
    ctx.builder.terminate(Terminator::Throw {
        value: exception.value,
    });
    placeholder
}

/// Returns synthetic array entries for current static-property values on a reflected class.
fn reflection_class_static_property_map_entries(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
) -> Option<Vec<(String, String, PhpType)>> {
    let class_info = ctx.classes.get(class_name.trim_start_matches('\\'))?;
    Some(
        class_info
            .static_properties
            .iter()
            .map(|(property, property_ty)| {
                let declaring_class = class_info
                    .static_property_declaring_classes
                    .get(property)
                    .cloned()
                    .unwrap_or_else(|| class_name.trim_start_matches('\\').to_string());
                let property_ty = normalize_value_php_type(property_ty.codegen_repr());
                (property.clone(), declaring_class, property_ty)
            })
            .collect(),
    )
}

/// Boxes a concrete PHP value into the runtime `Mixed` cell representation.
fn box_value_as_mixed(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Span,
) -> LoweredValue {
    if ctx.builder.value_php_type(value.value).codegen_repr() == PhpType::Mixed {
        return value;
    }
    ctx.emit_value(
        Op::MixedBox,
        vec![value.value],
        None,
        PhpType::Mixed,
        Op::MixedBox.default_effects(),
        Some(span),
    )
}

/// Returns the literal property name and optional explicit default argument for a get call.
fn reflection_class_get_static_property_value_args(
    args: &[Expr],
) -> Option<(String, Option<Expr>)> {
    let args = reflection_class_new_instance_args(args);
    if args.iter().any(is_spread_arg) {
        return None;
    }
    let (name, default) =
        reflection_class_static_property_regular_args(&args, "name", Some("default"))?;
    let property = reflection_class_static_property_name_arg(name.as_ref()?)?;
    Some((property, default))
}

/// Returns the literal property name and value expression for a set call.
fn reflection_class_set_static_property_value_args(args: &[Expr]) -> Option<(String, Expr)> {
    let args = reflection_class_new_instance_args(args);
    if args.iter().any(is_spread_arg) {
        return None;
    }
    let (name, value) =
        reflection_class_static_property_regular_args(&args, "name", Some("value"))?;
    let property = reflection_class_static_property_name_arg(name.as_ref()?)?;
    let value = value?;
    Some((property, value))
}

/// Normalizes supported static-property method arguments into parameter order.
fn reflection_class_static_property_regular_args(
    args: &[Expr],
    first_name: &str,
    second_name: Option<&str>,
) -> Option<(Option<Expr>, Option<Expr>)> {
    if !crate::types::call_args::has_named_args(args) {
        return match args {
            [first] => Some((Some(first.clone()), None)),
            [first, second] => Some((Some(first.clone()), Some(second.clone()))),
            _ => None,
        };
    }

    let mut first = None;
    let mut second = None;
    for arg in args {
        match &arg.kind {
            ExprKind::NamedArg { name, value } if php_symbol_key(name) == first_name => {
                first = Some((**value).clone());
            }
            ExprKind::NamedArg { name, value }
                if second_name.is_some_and(|expected| php_symbol_key(name) == expected) =>
            {
                second = Some((**value).clone());
            }
            _ => return None,
        }
    }
    Some((first, second))
}

/// Extracts a literal property name from a ReflectionClass static-property call argument.
fn reflection_class_static_property_name_arg(arg: &Expr) -> Option<String> {
    match &arg.kind {
        ExprKind::StringLiteral(name) => Some(name.clone()),
        _ => None,
    }
}

/// Returns the declaring class and retained PHP type for one reflected static property.
fn reflection_class_static_property_target(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    property: &str,
) -> Option<(String, PhpType)> {
    let class_info = ctx.classes.get(class_name.trim_start_matches('\\'))?;
    let property_ty = class_info
        .static_properties
        .iter()
        .find(|(name, _)| name == property)
        .map(|(_, property_ty)| normalize_value_php_type(property_ty.codegen_repr()))?;
    let declaring_class = class_info
        .static_property_declaring_classes
        .get(property)
        .cloned()
        .unwrap_or_else(|| class_name.trim_start_matches('\\').to_string());
    Some((declaring_class, property_ty))
}

/// Emits a visibility-bypassing reflection static-property read.
fn lower_reflection_static_property_get_by_class_name(
    ctx: &mut LoweringContext<'_, '_>,
    class_name: &str,
    property: &str,
    result_type: PhpType,
    expr: &Expr,
) -> LoweredValue {
    lower_static_property_get_by_class_name_with_op(
        ctx,
        class_name,
        property,
        result_type,
        expr,
        Op::LoadReflectionStaticProperty,
    )
}

/// Emits a visibility-bypassing static-property initialization probe.
fn lower_reflection_static_property_initialized_by_class_name(
    ctx: &mut LoweringContext<'_, '_>,
    class_name: &str,
    property: &str,
    expr: &Expr,
) -> LoweredValue {
    lower_static_property_get_by_class_name_with_op(
        ctx,
        class_name,
        property,
        PhpType::Bool,
        expr,
        Op::ReflectionStaticPropertyInitialized,
    )
}

/// Emits a static-property read using the requested static-property opcode.
fn lower_static_property_get_by_class_name_with_op(
    ctx: &mut LoweringContext<'_, '_>,
    class_name: &str,
    property: &str,
    result_type: PhpType,
    expr: &Expr,
    op: Op,
) -> LoweredValue {
    let data = ctx.intern_string(&format!("{}::{}", class_name, property));
    ctx.emit_value(
        op,
        Vec::new(),
        Some(Immediate::Data(data)),
        result_type,
        op.default_effects(),
        Some(expr.span),
    )
}

/// Emits a visibility-bypassing reflection static-property write.
fn store_reflection_static_property_by_class_name(
    ctx: &mut LoweringContext<'_, '_>,
    class_name: &str,
    property: &str,
    value: ValueId,
    span: Span,
) {
    store_static_property_by_class_name_with_op(
        ctx,
        class_name,
        property,
        value,
        span,
        Op::StoreReflectionStaticProperty,
    );
}

/// Emits a static-property write using the requested static-property opcode.
fn store_static_property_by_class_name_with_op(
    ctx: &mut LoweringContext<'_, '_>,
    class_name: &str,
    property: &str,
    value: ValueId,
    span: Span,
    op: Op,
) {
    let data = ctx.intern_string(&format!("{}::{}", class_name, property));
    ctx.emit_void(
        op,
        vec![value],
        Some(Immediate::Data(data)),
        op.default_effects(),
        Some(span),
    );
}

/// Returns the source arguments that can be forwarded to `new $class(...)`.
fn reflection_class_new_instance_args(args: &[Expr]) -> Vec<Expr> {
    if has_static_call_spread_args(args) {
        return expand_static_call_spread_args(args);
    }
    args.to_vec()
}

/// Returns constructor arguments carried by a static `newInstanceArgs()` array argument.
fn reflection_class_new_instance_args_array(
    ctx: &LoweringContext<'_, '_>,
    args: &[Expr],
) -> Option<Vec<Expr>> {
    let args = reflection_class_new_instance_args(args);
    match args.as_slice() {
        [] => Some(Vec::new()),
        [arg] => reflection_class_new_instance_args_value(ctx, arg),
        _ => None,
    }
}

/// Extracts the actual array value passed to the `newInstanceArgs()` `$args` parameter.
fn reflection_class_new_instance_args_value(
    ctx: &LoweringContext<'_, '_>,
    arg: &Expr,
) -> Option<Vec<Expr>> {
    let array_expr = match &arg.kind {
        ExprKind::NamedArg { name, value } if php_symbol_key(name) == "args" => value.as_ref(),
        ExprKind::NamedArg { .. } => return None,
        _ => arg,
    };
    if let ExprKind::Variable(name) = &array_expr.kind {
        return ctx.reflection_arg_array_local(name);
    }
    reflection_class_new_instance_args_value_without_locals(array_expr)
}

/// Extracts an inline static array value passed to a reflection argument-array API.
fn reflection_class_new_instance_args_value_without_locals(arg: &Expr) -> Option<Vec<Expr>> {
    let array_expr = match &arg.kind {
        ExprKind::NamedArg { name, value } if php_symbol_key(name) == "args" => value.as_ref(),
        ExprKind::NamedArg { .. } => return None,
        _ => arg,
    };
    match &array_expr.kind {
        ExprKind::ArrayLiteral(items) => Some(items.clone()),
        ExprKind::ArrayLiteralAssoc(entries) => reflection_class_new_instance_assoc_args(entries),
        _ => None,
    }
}

/// Converts a static associative argument array into positional and named call arguments.
fn reflection_class_new_instance_assoc_args(entries: &[(Expr, Expr)]) -> Option<Vec<Expr>> {
    entries
        .iter()
        .map(|(key, value)| reflection_class_new_instance_assoc_arg(key, value))
        .collect()
}

/// Converts one `newInstanceArgs()` associative-array element into a constructor argument.
fn reflection_class_new_instance_assoc_arg(key: &Expr, value: &Expr) -> Option<Expr> {
    match &key.kind {
        ExprKind::IntLiteral(_) | ExprKind::BoolLiteral(_) | ExprKind::FloatLiteral(_) => {
            Some(value.clone())
        }
        ExprKind::StringLiteral(name) if crate::types::is_php_integer_array_key(name) => {
            Some(value.clone())
        }
        ExprKind::StringLiteral(name) => Some(Expr::new(
            ExprKind::NamedArg {
                name: name.clone(),
                value: Box::new(value.clone()),
            },
            value.span,
        )),
        _ => None,
    }
}

/// Returns the reflected constructor signature when the ReflectionClass receiver
/// is an inline `new ReflectionClass(Known::class)` expression.
fn reflection_class_new_instance_constructor_signature<'a>(
    ctx: &'a LoweringContext<'_, '_>,
    object_expr: Option<&Expr>,
    forwarded_args: &[Expr],
) -> Option<&'a FunctionSig> {
    let class_name = reflection_class_reflected_class(ctx, object_expr?)?;
    if forwarded_args.is_empty() && constructor_signature_for_class_name(ctx, &class_name).is_none()
    {
        return None;
    }
    constructor_signature_for_class_name(ctx, &class_name)
}

/// Resolves the target class from an inline `ReflectionClass` construction when
/// its constructor argument is a literal class string or `ClassName::class`.
fn reflection_class_new_instance_reflected_class(
    ctx: &LoweringContext<'_, '_>,
    object_expr: &Expr,
) -> Option<String> {
    let ExprKind::NewObject { class_name, args } = &object_expr.kind else {
        return None;
    };
    match php_symbol_key(class_name.as_str().trim_start_matches('\\')).as_str() {
        "reflectionclass" => reflection_class_reflected_class_from_args(ctx, args),
        "reflectionobject" => reflection_object_reflected_class_from_args(ctx, args),
        _ => None,
    }
}

/// Resolves the target class from a static `ReflectionClass(...)` argument list.
fn reflection_class_reflected_class_from_args(
    ctx: &LoweringContext<'_, '_>,
    args: &[Expr],
) -> Option<String> {
    let reflected_arg = reflection_class_constructor_class_arg(ctx, args)?;
    let raw_class_name = match &reflected_arg.kind {
        ExprKind::StringLiteral(value) => value.clone(),
        ExprKind::ClassConstant { receiver } => static_receiver_class_name(ctx, receiver)?,
        _ => return None,
    };
    resolve_known_class_name(ctx, &raw_class_name)
}

/// Resolves the target class from a static `ReflectionObject(...)` argument list.
fn reflection_object_reflected_class_from_args(
    ctx: &LoweringContext<'_, '_>,
    args: &[Expr],
) -> Option<String> {
    let object_arg = reflection_object_constructor_object_arg(ctx, args)?;
    isset_object_expr_class(ctx, &object_arg).map(|(class_name, _)| class_name)
}

/// Resolves a reflected class from an inline constructor or tracked local receiver.
fn reflection_class_reflected_class(
    ctx: &LoweringContext<'_, '_>,
    object_expr: &Expr,
) -> Option<String> {
    reflection_class_new_instance_reflected_class(ctx, object_expr).or_else(|| {
        let ExprKind::Variable(name) = &object_expr.kind else {
            return None;
        };
        ctx.reflection_class_local(name)
    })
}

/// Returns the `ReflectionClass::__construct()` class-name argument after static
/// spread and named-argument normalization.
fn reflection_class_constructor_class_arg(
    ctx: &LoweringContext<'_, '_>,
    args: &[Expr],
) -> Option<Expr> {
    let args = reflection_class_new_instance_args(args);
    if args.iter().any(is_spread_arg) {
        return None;
    }
    if !crate::types::call_args::has_named_args(&args) {
        return args.first().cloned();
    }
    let sig = ctx
        .classes
        .get("ReflectionClass")
        .and_then(|class_info| class_info.methods.get("__construct"))?;
    let call_span = args
        .first()
        .map(|arg| arg.span)
        .unwrap_or_else(crate::span::Span::dummy);
    let plan = crate::types::call_args::plan_call_args_with_regular_param_count_and_assoc_spreads(
        sig,
        &args,
        call_span,
        crate::types::call_args::regular_param_count(sig),
        false,
        true,
        &assoc_spread_sources(ctx, &args),
    )
    .ok()?;
    if plan.has_spread_args() {
        return None;
    }
    planned_regular_arg_expr(plan.regular_args.first()?).cloned()
}

/// Returns the `ReflectionObject::__construct()` object argument after normalization.
fn reflection_object_constructor_object_arg(
    ctx: &LoweringContext<'_, '_>,
    args: &[Expr],
) -> Option<Expr> {
    let args = reflection_class_new_instance_args(args);
    if args.iter().any(is_spread_arg) {
        return None;
    }
    if !crate::types::call_args::has_named_args(&args) {
        return args.first().cloned();
    }
    let sig = ctx
        .classes
        .get("ReflectionObject")
        .and_then(|class_info| class_info.methods.get("__construct"))?;
    let call_span = args
        .first()
        .map(|arg| arg.span)
        .unwrap_or_else(crate::span::Span::dummy);
    let plan = crate::types::call_args::plan_call_args_with_regular_param_count_and_assoc_spreads(
        sig,
        &args,
        call_span,
        crate::types::call_args::regular_param_count(sig),
        false,
        true,
        &assoc_spread_sources(ctx, &args),
    )
    .ok()?;
    if plan.has_spread_args() {
        return None;
    }
    planned_regular_arg_expr(plan.regular_args.first()?).cloned()
}

/// Resolves a PHP class name case-insensitively against known class metadata.
fn resolve_known_class_name(ctx: &LoweringContext<'_, '_>, class_name: &str) -> Option<String> {
    let key = php_symbol_key(class_name.trim_start_matches('\\'));
    ctx.classes
        .keys()
        .find(|candidate| php_symbol_key(candidate.trim_start_matches('\\')) == key)
        .cloned()
}

/// Resolves a PHP function name case-insensitively against known user functions.
fn resolve_known_function_name(
    ctx: &LoweringContext<'_, '_>,
    function_name: &str,
) -> Option<String> {
    let key = php_symbol_key(function_name.trim_start_matches('\\'));
    ctx.functions
        .keys()
        .find(|candidate| php_symbol_key(candidate.trim_start_matches('\\')) == key)
        .cloned()
}

/// Resolves a PHP method name case-insensitively against known class metadata.
fn resolve_known_class_method_name(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    method: &str,
) -> Option<String> {
    let class_info = ctx.classes.get(class_name.trim_start_matches('\\'))?;
    let key = php_symbol_key(method);
    class_info
        .methods
        .keys()
        .chain(class_info.static_methods.keys())
        .find(|candidate| php_symbol_key(candidate) == key)
        .cloned()
}

/// Returns constructor signature metadata for a known class name.
fn constructor_signature_for_class_name<'a>(
    ctx: &'a LoweringContext<'_, '_>,
    class_name: &str,
) -> Option<&'a FunctionSig> {
    let key = php_symbol_key("__construct");
    ctx.classes
        .get(class_name.trim_start_matches('\\'))
        .and_then(|class_info| class_info.methods.get(&key))
}

/// Emits a runtime fatal for ReflectionClass newInstance argument forms not yet lowered.
fn lower_reflection_class_new_instance_unsupported(
    ctx: &mut LoweringContext<'_, '_>,
    expr: &Expr,
) -> LoweredValue {
    let result = lower_boxed_null(ctx, expr);
    let message = ctx.intern_string(
        "Fatal error: unsupported ReflectionClass::newInstance() argument forwarding\n",
    );
    ctx.builder.terminate(Terminator::Fatal { message });
    result
}

/// Emits a runtime fatal for unsupported `newInstanceArgs()` argument-array forms.
fn lower_reflection_class_new_instance_args_unsupported(
    ctx: &mut LoweringContext<'_, '_>,
    expr: &Expr,
) -> LoweredValue {
    let result = lower_boxed_null(ctx, expr);
    let message = ctx.intern_string(
        "Fatal error: unsupported ReflectionClass::newInstanceArgs() argument array\n",
    );
    ctx.builder.terminate(Terminator::Fatal { message });
    result
}

/// Emits a runtime fatal for unsupported `newInstanceWithoutConstructor()` argument forms.
fn lower_reflection_class_new_instance_without_constructor_unsupported(
    ctx: &mut LoweringContext<'_, '_>,
    expr: &Expr,
) -> LoweredValue {
    let result = lower_boxed_null(ctx, expr);
    let message = ctx.intern_string(
        "Fatal error: unsupported ReflectionClass::newInstanceWithoutConstructor() arguments\n",
    );
    ctx.builder.terminate(Terminator::Fatal { message });
    result
}

/// Returns true when a method call targets the built-in `ReflectionClass::newInstance()`.
fn is_reflection_class_new_instance_call(
    ctx: &LoweringContext<'_, '_>,
    object: ValueId,
    method: &str,
) -> bool {
    if php_symbol_key(method) != "newinstance" {
        return false;
    }
    is_reflection_class_construction_receiver(ctx, object)
}

/// Returns true when a method call targets `ReflectionClass::newInstanceArgs()`.
fn is_reflection_class_new_instance_args_call(
    ctx: &LoweringContext<'_, '_>,
    object: ValueId,
    method: &str,
) -> bool {
    if php_symbol_key(method) != "newinstanceargs" {
        return false;
    }
    is_reflection_class_construction_receiver(ctx, object)
}

/// Returns true when a method call targets `ReflectionClass::newInstanceWithoutConstructor()`.
fn is_reflection_class_new_instance_without_constructor_call(
    ctx: &LoweringContext<'_, '_>,
    object: ValueId,
    method: &str,
) -> bool {
    if php_symbol_key(method) != "newinstancewithoutconstructor" {
        return false;
    }
    is_reflection_class_construction_receiver(ctx, object)
}

/// Returns true when a receiver can use ReflectionClass construction helper lowering.
fn is_reflection_class_construction_receiver(
    ctx: &LoweringContext<'_, '_>,
    object: ValueId,
) -> bool {
    let object_ty = ctx.builder.value_php_type(object);
    let Some((class_name, false)) = singular_object_class(&object_ty) else {
        return false;
    };
    matches!(
        php_symbol_key(class_name.trim_start_matches('\\')).as_str(),
        "reflectionclass" | "reflectionobject"
    )
}

/// Emits the PHP fatal terminator for an ordinary method call on null.
fn terminate_method_call_on_null(ctx: &mut LoweringContext<'_, '_>, method: &str) {
    let message = format!("Call to a member function {}() on null", method);
    let message = ctx.intern_string(&message);
    ctx.emit_void(
        Op::ThrowError,
        Vec::new(),
        Some(Immediate::Data(message)),
        Op::ThrowError.default_effects(),
        None,
    );
    ctx.builder.terminate(Terminator::Unreachable);
}

/// Lowers a nullsafe method call with lazy argument evaluation for nullable receivers.
fn lower_nullsafe_method_call(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    method: &str,
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let object = lower_expr(ctx, object);
    let object_ty = ctx.builder.value_php_type(object.value);
    if value_is_definitely_null(ctx, object.value) {
        return lower_boxed_null(ctx, expr);
    }
    let Some((_, true)) = singular_object_class(&object_ty) else {
        return lower_method_call_with_receiver(
            ctx,
            object,
            method,
            args,
            Op::NullsafeMethodCall,
            expr,
        );
    };
    let result_type = method_call_result_type(
        ctx,
        object.value,
        method,
        Op::NullsafeMethodCall,
        expr,
    );
    let temp_name = ctx.declare_hidden_temp(result_type.clone());
    let null_block = ctx.builder.create_named_block("nullsafe.method.null", Vec::new());
    let call_block = ctx.builder.create_named_block("nullsafe.method.call", Vec::new());
    let merge = ctx.builder.create_named_block("nullsafe.method.merge", Vec::new());
    let is_null = ctx.emit_value(
        Op::IsNull,
        vec![object.value],
        None,
        PhpType::Bool,
        Op::IsNull.default_effects(),
        Some(expr.span),
    );
    ctx.builder.terminate(Terminator::CondBr {
        cond: is_null.value,
        then_target: null_block,
        then_args: Vec::new(),
        else_target: call_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(null_block);
    let null_value = lower_null(ctx, expr);
    let null_value = if result_type.codegen_repr() == PhpType::Mixed {
        ctx.box_value_as_mixed(null_value, result_type.clone(), Some(expr.span))
    } else {
        null_value
    };
    store_value_into_temp(ctx, &temp_name, result_type.clone(), null_value, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(call_block);
    let call = lower_method_call_with_receiver(
        ctx,
        object,
        method,
        args,
        Op::NullsafeMethodCall,
        expr,
    );
    store_value_into_temp(ctx, &temp_name, result_type.clone(), call, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    ctx.load_local(&temp_name, Some(expr.span))
}

/// Lowers a method call using an already evaluated receiver value.
fn lower_method_call_with_receiver(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    method: &str,
    args: &[Expr],
    op: Op,
    expr: &Expr,
) -> LoweredValue {
    if op == Op::MethodCall && is_reflection_class_new_instance_call(ctx, object.value, method) {
        return lower_reflection_class_new_instance(ctx, None, object, args, expr);
    }
    if op == Op::MethodCall && is_reflection_class_new_instance_args_call(ctx, object.value, method)
    {
        return lower_reflection_class_new_instance_args(ctx, None, object, args, expr);
    }
    if op == Op::MethodCall
        && is_reflection_class_new_instance_without_constructor_call(ctx, object.value, method)
    {
        return lower_reflection_class_new_instance_without_constructor(ctx, object, args, expr);
    }
    let magic_args;
    let (dispatch_method, args) =
        if let Some(args) = magic_call_dispatch_args(ctx, object.value, method, args, expr.span) {
            magic_args = args;
            ("__call", magic_args.as_slice())
        } else {
            (method, args)
        };
    let result_type = method_call_result_type(ctx, object.value, dispatch_method, op, expr);
    let mut operands = vec![object.value];
    let sig = method_signature(ctx, object.value, dispatch_method);
    let arg_values = lower_args_with_signature(ctx, sig.as_ref(), args);
    operands.extend(arg_values.iter().copied());
    let data = ctx.intern_string(dispatch_method);
    let call = ctx.emit_value(
        op,
        operands,
        Some(Immediate::Data(data)),
        result_type,
        op.default_effects(),
        Some(expr.span),
    );
    let return_alias = method_return_arg_alias(ctx, object.value, dispatch_method);
    release_owned_call_arg_temporaries(
        ctx,
        &arg_values,
        Some(call.value),
        &return_alias,
        expr.span,
    );
    release_owning_receiver_temporary(ctx, object, expr.span);
    call
}

/// Lowers a nullsafe dynamic instance method call after the receiver was evaluated and guarded.
///
/// The non-null receiver is stored in a hidden temp so the existing
/// `call_user_func([$obj, $method], ...)` lowering can be reused without
/// evaluating the original receiver expression again.
pub(super) fn lower_dynamic_method_call_with_receiver(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    method: &Expr,
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let receiver_type = strip_void_from_union(ctx.builder.value_php_type(object.value));
    let receiver_name = ctx.declare_hidden_temp(receiver_type.clone());
    ctx.store_local(&receiver_name, object, receiver_type, Some(expr.span));
    let receiver = Expr::new(ExprKind::Variable(receiver_name), expr.span);
    let callback = Expr::new(
        ExprKind::ArrayLiteral(vec![receiver, method.clone()]),
        Span::dummy(),
    );
    let mut call_args = Vec::with_capacity(args.len() + 1);
    call_args.push(callback);
    call_args.extend(args.iter().cloned());
    let call = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("call_user_func"),
            args: call_args,
        },
        expr.span,
    );
    lower_expr(ctx, &call)
}

/// Releases normalized call arguments that cannot be returned by this call.
fn release_owned_call_arg_temporaries(
    ctx: &mut LoweringContext<'_, '_>,
    args: &[crate::ir::ValueId],
    result: Option<crate::ir::ValueId>,
    return_alias: &ReturnArgAlias,
    span: Span,
) {
    release_owned_call_arg_temporaries_with_signature(
        ctx,
        args,
        result,
        return_alias,
        None,
        span,
    );
}

/// Releases call arguments while accounting for fresh Mixed boxes created by the ABI.
fn release_owned_call_arg_temporaries_with_signature(
    ctx: &mut LoweringContext<'_, '_>,
    args: &[crate::ir::ValueId],
    result: Option<crate::ir::ValueId>,
    return_alias: &ReturnArgAlias,
    signature: Option<&FunctionSig>,
    span: Span,
) {
    for (parameter_index, value) in args.iter().enumerate() {
        let php_type = ctx.builder.value_php_type(*value);
        let lowered = LoweredValue {
            value: *value,
            ir_type: value_ir_type(&php_type),
        };
        if ctx.value_is_owning_temporary(lowered) {
            let independently_boxed = signature.is_some_and(|signature| {
                call_arg_gets_independent_mixed_box(signature, parameter_index, &php_type)
            });
            if !independently_boxed
                && return_alias.may_alias_parameter(parameter_index)
                && result.is_some_and(|result| ctx.call_result_may_alias_arg(*value, result))
            {
                continue;
            }
            crate::ir_lower::ownership::release_if_owned(ctx, lowered, Some(span));
        }
    }
}

/// Returns true when ABI materialization wraps a concrete argument in fresh Mixed storage.
fn call_arg_gets_independent_mixed_box(
    signature: &FunctionSig,
    parameter_index: usize,
    source_type: &PhpType,
) -> bool {
    if signature
        .ref_params
        .get(parameter_index)
        .copied()
        .unwrap_or(false)
    {
        return false;
    }
    signature
        .params
        .get(parameter_index)
        .is_some_and(|(_, parameter_type)| {
            parameter_type.codegen_repr() == PhpType::Mixed
                && !matches!(
                    source_type.codegen_repr(),
                    PhpType::Mixed | PhpType::Union(_)
                )
        })
}

/// Makes a borrowed read result independent from an owning receiver before releasing it.
///
/// Property and indexed reads can return strings, arrays, objects, or callables
/// borrowed from the receiver. When that receiver is an owned temporary — notably
/// an object retained while unboxing a Mixed local — releasing it first could
/// destroy the result payload. Reads that already materialize an independent owned
/// value must not be acquired a second time.
fn stabilize_borrowed_result_and_release_receiver(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: LoweredValue,
    result: LoweredValue,
    span: Span,
) -> LoweredValue {
    if !ctx.value_is_owning_temporary(receiver) {
        return result;
    }
    let result = if ctx.value_is_owning_temporary(result) {
        result
    } else {
        crate::ir_lower::ownership::acquire_if_refcounted(ctx, result, Some(span))
    };
    crate::ir_lower::ownership::release_if_owned(ctx, receiver, Some(span));
    result
}

/// Releases the receiver of a method call when it was an owning temporary.
///
/// A method borrows its receiver, so a receiver that is itself a temporary — the
/// result of a prior chained call (`$o->a()->b()`) or an inline `new X()->m()` —
/// has no owner once the call returns and would otherwise never reach refcount
/// zero (a leak; its destructor never runs). A plain local or `$this` receiver is
/// not an owning temporary and is left to normal scope cleanup. This must run
/// after the call is emitted (and after `return $this` has acquired its own
/// reference) so the released reference is the receiver's, not the result's.
fn release_owning_receiver_temporary(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: LoweredValue,
    span: Span,
) {
    if ctx.value_is_owning_temporary(receiver) {
        crate::ir_lower::ownership::release_if_owned(ctx, receiver, Some(span));
    }
}

/// Returns the checked signature for an instance method call when metadata is available.
fn method_signature(
    ctx: &LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    method: &str,
) -> Option<FunctionSig> {
    let object_ty = ctx.builder.value_php_type(object);
    let key = php_symbol_key(method);
    if let Some((class_name, _)) = singular_object_class(&object_ty) {
        let normalized = class_name.trim_start_matches('\\');
        return class_method_signature(ctx, normalized, &key).cloned();
    }
    if dynamic_method_receiver_needs_mixed_fallback(&object_ty) {
        if ctx.has_eval_barrier() {
            return None;
        }
        return common_dynamic_method_signature(ctx, &key);
    }
    None
}

/// Returns the conservative return-to-argument alias summary for a method dispatch.
///
/// A non-final receiver type includes every closed-world descendant implementation,
/// because runtime dispatch can select an override. Missing or synthetic summaries
/// therefore fall back to `Unknown` rather than enabling unsafe cleanup.
fn method_return_arg_alias(
    ctx: &LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    method: &str,
) -> ReturnArgAlias {
    let object_ty = ctx.builder.value_php_type(object);
    let method_key = php_symbol_key(method);
    let mut summary: Option<ReturnArgAlias> = None;
    if let Some((class_name, _)) = singular_object_class(&object_ty) {
        let base_class = class_name.trim_start_matches('\\');
        let Some(base_info) = ctx.classes.get(base_class) else {
            return ReturnArgAlias::Unknown;
        };
        if base_info.is_final || base_info.final_methods.contains(&method_key) {
            return class_method_return_arg_alias(ctx, base_class, &method_key)
                .unwrap_or(ReturnArgAlias::Unknown);
        }
        for candidate in ctx.classes.keys() {
            if !is_same_or_descendant_class(ctx, candidate, base_class) {
                continue;
            }
            let Some(alias) = class_method_return_arg_alias(ctx, candidate, &method_key) else {
                continue;
            };
            summary = Some(match summary {
                Some(current) => current.merge(&alias),
                None => alias,
            });
        }
        return summary.unwrap_or(ReturnArgAlias::Unknown);
    }
    if dynamic_method_receiver_needs_mixed_fallback(&object_ty) {
        if ctx.has_eval_barrier() {
            return ReturnArgAlias::Unknown;
        }
        for candidate in ctx.classes.keys() {
            let Some(alias) = class_method_return_arg_alias(ctx, candidate, &method_key) else {
                continue;
            };
            summary = Some(match summary {
                Some(current) => current.merge(&alias),
                None => alias,
            });
        }
    }
    summary.unwrap_or(ReturnArgAlias::Unknown)
}

/// Resolves one concrete class's dispatched implementation and its source summary.
fn class_method_return_arg_alias(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    method_key: &str,
) -> Option<ReturnArgAlias> {
    class_method_signature(ctx, class_name, method_key)?;
    let class_info = ctx.classes.get(class_name)?;
    let impl_class = class_info
        .method_impl_classes
        .get(method_key)
        .map(String::as_str)
        .unwrap_or(class_name);
    Some(
        ctx.return_alias_summaries
            .method(impl_class, method_key)
            .cloned()
            .unwrap_or(ReturnArgAlias::Unknown),
    )
}

/// Returns a class/interface method signature, preferring the implementing class metadata.
fn class_method_signature<'a>(
    ctx: &'a LoweringContext<'_, '_>,
    class_name: &str,
    method_key: &str,
) -> Option<&'a FunctionSig> {
    let normalized = class_name.trim_start_matches('\\');
    if let Some(class_info) = ctx.classes.get(normalized) {
        let impl_class = class_info
            .method_impl_classes
            .get(method_key)
            .map(String::as_str)
            .unwrap_or(normalized);
        return ctx
            .classes
            .get(impl_class)
            .and_then(|impl_info| impl_info.methods.get(method_key))
            .or_else(|| class_info.methods.get(method_key));
    }
    ctx.interfaces
        .get(normalized)
        .and_then(|interface_info| interface_info.methods.get(method_key))
}

/// Returns the checked return type for an instance method call when metadata is available.
fn method_call_result_type(
    ctx: &LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    method: &str,
    op: Op,
    expr: &Expr,
) -> PhpType {
    let object_ty = ctx.builder.value_php_type(object);
    let nullable = singular_object_class(&object_ty)
        .map(|(_, nullable)| nullable)
        .unwrap_or(false);
    let Some(return_ty) = method_signature(ctx, object, method)
        .map(|signature| normalize_value_php_type(signature.return_type))
    else {
        if dynamic_method_receiver_needs_mixed_fallback(&object_ty) {
            return PhpType::Mixed;
        }
        return fallback_expr_type(expr);
    };
    let return_ty = if let Some((receiver_name, _)) = singular_object_class(&object_ty) {
        instance_method_late_static_return_for_ir(ctx, receiver_name, &php_symbol_key(method))
            .map(|return_type| late_static_return_type_for_ir(ctx, &return_type, receiver_name))
            .unwrap_or(return_ty)
    } else {
        return_ty
    };
    if op == Op::NullsafeMethodCall && nullable {
        nullable_result_type(return_ty)
    } else {
        return_ty
    }
}

/// Returns preserved late-static return syntax for EIR instance dispatch.
fn instance_method_late_static_return_for_ir(
    ctx: &LoweringContext<'_, '_>,
    receiver_type: &str,
    method_key: &str,
) -> Option<TypeExpr> {
    let normalized = receiver_type.trim_start_matches('\\');
    if let Some(class_info) = ctx.classes.get(normalized) {
        if let Some(return_type) = class_info.late_static_method_returns.get(method_key) {
            return Some(return_type.clone());
        }
    }
    ctx.interfaces
        .get(normalized)
        .and_then(|interface_info| interface_info.late_static_method_returns.get(method_key))
        .cloned()
}

/// Binds preserved late-static return syntax to an EIR call-site receiver type.
fn late_static_return_type_for_ir(
    ctx: &LoweringContext<'_, '_>,
    return_type: &TypeExpr,
    receiver_type: &str,
) -> PhpType {
    let bound = return_type.substitute_relative_class_types(receiver_type, None);
    normalize_value_php_type(ctx.type_expr_to_php_type_for_value(&bound))
}

/// Returns a common method signature for dynamic receivers when every candidate agrees.
fn common_dynamic_method_signature(
    ctx: &LoweringContext<'_, '_>,
    method_key: &str,
) -> Option<FunctionSig> {
    let mut common = None;
    for class_name in ctx.classes.keys() {
        let Some(signature) = class_method_signature(ctx, class_name, method_key).cloned() else {
            continue;
        };
        match common.as_ref() {
            Some(existing) if existing != &signature => return None,
            Some(_) => {}
            None => common = Some(signature),
        }
    }
    common
}

/// Returns true when an instance-method receiver has no single compile-time class.
fn dynamic_method_receiver_needs_mixed_fallback(php_type: &PhpType) -> bool {
    match php_type {
        PhpType::Mixed => true,
        PhpType::Union(members) => members
            .iter()
            .any(|member| matches!(member, PhpType::Mixed | PhpType::Object(_))),
        _ => false,
    }
}

/// Lowers a static method call.
fn lower_static_method_call(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    method: &str,
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    // `Closure::bind($closure, $newThis [, $scope])` — static form of bindTo.
    if let StaticReceiver::Named(name) = receiver {
        if name.trim_start_matches('\\') == "Closure"
            && php_symbol_key(method) == "bind"
            && !args.is_empty()
        {
            let closure = lower_expr(ctx, &args[0]);
            let new_this = match args.get(1) {
                Some(arg) => lower_expr(ctx, arg),
                None => lower_null(ctx, expr),
            };
            return emit_closure_bind(ctx, closure.value, new_this.value, expr);
        }
    }

    let magic_args;
    let (dispatch_method, call_args) = if let Some(args) =
        magic_static_call_dispatch_args(ctx, receiver, method, args, expr.span)
    {
        magic_args = args;
        ("__callStatic", magic_args.as_slice())
    } else {
        (method, args)
    };
    if ctx.has_eval_barrier()
        && matches!(receiver, StaticReceiver::Named(_))
        && plain_positional_call_args(args)
    {
        if let Some(class_name) = static_receiver_class_name(ctx, receiver) {
            if !ctx.classes.contains_key(class_name.as_str()) {
                let operands = lower_args_with_signature(ctx, None, args);
                let name = format!("{}::{}", class_name, dispatch_method);
                let data = ctx.intern_string(&name);
                return ctx.emit_value(
                    Op::EvalStaticMethodCall,
                    operands,
                    Some(Immediate::Data(data)),
                    PhpType::Mixed,
                    Op::EvalStaticMethodCall.default_effects(),
                    Some(expr.span),
                );
            }
        }
    }
    let sig = static_method_implementation_signature(ctx, receiver, dispatch_method)
        .or_else(|| lexical_instance_static_call_signature(ctx, receiver, dispatch_method))
        .cloned();
    let operands = lower_args_with_signature(ctx, sig.as_ref(), call_args);
    let operands =
        coerce_int_backed_enum_string_argument(ctx, receiver, dispatch_method, operands, expr);
    let name = format!("{}::{}", receiver_name(receiver), dispatch_method);
    let data = ctx.intern_string(&name);
    let result_type = sig
        .as_ref()
        .map(|signature| normalize_value_php_type(signature.return_type.codegen_repr()))
        .unwrap_or_else(|| {
            if ctx.has_eval_barrier() && matches!(receiver, StaticReceiver::Named(_)) {
                PhpType::Mixed
            } else {
                fallback_expr_type(expr)
            }
        });
    let late_static_receiver_type = static_late_binding_receiver_type_for_ir(ctx, receiver);
    let result_type = match (
        static_method_late_static_return_for_ir(ctx, receiver, dispatch_method),
        late_static_receiver_type.as_deref(),
    ) {
        (Some(return_type), Some(receiver_type)) => {
            late_static_return_type_for_ir(ctx, &return_type, receiver_type)
        }
        _ => result_type,
    };
    let call = ctx.emit_value(
        Op::StaticMethodCall,
        operands.clone(),
        Some(Immediate::Data(data)),
        result_type,
        Op::StaticMethodCall.default_effects(),
        Some(expr.span),
    );
    let return_alias = static_method_return_arg_alias(ctx, receiver, dispatch_method);
    release_owned_call_arg_temporaries_with_signature(
        ctx,
        &operands,
        Some(call.value),
        &return_alias,
        sig.as_ref(),
        expr.span,
    );
    call
}

/// Returns preserved late-static return syntax for EIR static dispatch.
fn static_method_late_static_return_for_ir(
    ctx: &LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    method: &str,
) -> Option<TypeExpr> {
    let class_name = static_receiver_class_name(ctx, receiver)?;
    let method_key = php_symbol_key(method);
    let class_info = ctx.classes.get(&class_name)?;
    if static_method_implementation_signature(ctx, receiver, method).is_some() {
        return class_info
            .late_static_static_method_returns
            .get(&method_key)
            .cloned();
    }
    lexical_instance_static_call_signature(ctx, receiver, method)?;
    class_info.late_static_method_returns.get(&method_key).cloned()
}

/// Resolves the receiver type used to bind `static` for an EIR static-style call.
fn static_late_binding_receiver_type_for_ir(
    ctx: &LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
) -> Option<String> {
    match receiver {
        StaticReceiver::Named(name) => Some(name.as_str().trim_start_matches('\\').to_string()),
        StaticReceiver::Self_ | StaticReceiver::Static | StaticReceiver::Parent => {
            ctx.current_class.clone()
        }
    }
}

/// PHP coerces a numeric string to the integer backing value for an int-backed enum's
/// `from()`/`tryFrom()`. When the sole argument lowered to a string, insert an explicit
/// `EnumBackingStringToInt` coercion (issue #349) so the enum call receives a plain integer
/// operand: the backing scan then runs on an int rather than a heap string, and a
/// non-numeric string throws `TypeError` inside the coercion at runtime. Non-matching
/// receivers/methods/argument types pass the operands through unchanged.
fn coerce_int_backed_enum_string_argument(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    method: &str,
    mut operands: Vec<crate::ir::ValueId>,
    expr: &Expr,
) -> Vec<crate::ir::ValueId> {
    let key = php_symbol_key(method);
    if (key != "from" && key != "tryfrom") || operands.len() != 1 {
        return operands;
    }
    let StaticReceiver::Named(name) = receiver else {
        return operands;
    };
    let enum_name = name.trim_start_matches('\\');
    let is_int_backed = ctx
        .enums
        .get(enum_name)
        .and_then(|info| info.backing_type.as_ref())
        .is_some_and(|backing| matches!(backing, PhpType::Int));
    if !is_int_backed {
        return operands;
    }
    let method_display = if key == "tryfrom" { "tryFrom" } else { "from" };
    // A `string` argument coerces via a strict numeric probe; a `Mixed` argument dispatches
    // on its runtime tag (int/bool/float/null coerce, string coerces, others `TypeError`).
    // The string op carries the full message; the Mixed op carries the message prefix and
    // appends the runtime type word in codegen.
    let (op, message) = match ctx.builder.value_php_type(operands[0]).codegen_repr() {
        PhpType::Str => (
            Op::EnumBackingStringToInt,
            format!(
                "{}::{}(): Argument #1 ($value) must be of type int, string given",
                enum_name, method_display
            ),
        ),
        PhpType::Mixed | PhpType::Union(_) => (
            Op::EnumBackingMixedToInt,
            format!(
                "{}::{}(): Argument #1 ($value) must be of type int, ",
                enum_name, method_display
            ),
        ),
        _ => return operands,
    };
    let message_data = ctx.intern_string(&message);
    let coerced = ctx.emit_value(
        op,
        vec![operands[0]],
        Some(Immediate::Data(message_data)),
        PhpType::Int,
        op.default_effects(),
        Some(expr.span),
    );
    operands[0] = coerced.value;
    operands
}

/// Builds synthetic `__callStatic` arguments when a class lacks the requested static method.
fn magic_static_call_dispatch_args(
    ctx: &LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    method: &str,
    args: &[Expr],
    span: Span,
) -> Option<Vec<Expr>> {
    if static_method_implementation_signature(ctx, receiver, method).is_some()
        || lexical_instance_static_call_signature(ctx, receiver, method).is_some()
    {
        return None;
    }
    let class_name = static_receiver_class_name(ctx, receiver)?;
    let class_info = ctx.classes.get(class_name.as_str())?;
    if class_info.methods.contains_key(&php_symbol_key(method)) {
        return None;
    }
    static_method_implementation_signature(ctx, receiver, "__callStatic")?;
    Some(vec![
        Expr::new(ExprKind::StringLiteral(method.to_string()), span),
        Expr::new(ExprKind::ArrayLiteral(args.to_vec()), span),
    ])
}

/// Lowers a static-method callable-array call through a descriptor invoker.
fn lower_static_method_descriptor_call(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    method: &str,
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let sig = static_method_implementation_signature(ctx, receiver, method).cloned();
    let wrapper_sig = sig
        .as_ref()
        .map(crate::codegen::callable_dispatch::static_method_runtime_wrapper_sig);
    let target = CallableTarget::StaticMethod {
        receiver: receiver.clone(),
        method: method.to_string(),
    };
    let descriptor = lower_first_class_callable(ctx, &target, expr);
    let mut operands = Vec::with_capacity(args.len() + 1);
    operands.push(descriptor.value);
    operands.extend(lower_args_with_signature(ctx, wrapper_sig.as_ref(), args));
    let result_type = sig
        .as_ref()
        .map(|signature| normalize_value_php_type(signature.return_type.codegen_repr()))
        .unwrap_or_else(|| fallback_expr_type(expr));
    ctx.emit_value(
        Op::ExprCall,
        operands,
        None,
        result_type,
        Op::ExprCall.default_effects(),
        Some(expr.span),
    )
}

/// Lowers a static-method descriptor call when operands have already been evaluated.
fn lower_static_method_descriptor_value_call(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    method: &str,
    args: Vec<crate::ir::ValueId>,
    expr: &Expr,
) -> Option<LoweredValue> {
    let sig = static_method_implementation_signature(ctx, receiver, method).cloned();
    let target = CallableTarget::StaticMethod {
        receiver: receiver.clone(),
        method: method.to_string(),
    };
    let descriptor = lower_first_class_callable(ctx, &target, expr);
    let mut operands = Vec::with_capacity(args.len() + 1);
    operands.push(descriptor.value);
    operands.extend(args);
    let result_type = sig
        .as_ref()
        .map(|signature| normalize_value_php_type(signature.return_type.codegen_repr()))
        .unwrap_or_else(|| fallback_expr_type(expr));
    Some(ctx.emit_value(
        Op::ExprCall,
        operands,
        None,
        result_type,
        Op::ExprCall.default_effects(),
        Some(expr.span),
    ))
}

/// Returns the conservative return-to-argument alias summary for static dispatch.
fn static_method_return_arg_alias(
    ctx: &LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    method: &str,
) -> ReturnArgAlias {
    let Some(class_name) = static_receiver_class_name(ctx, receiver) else {
        return ReturnArgAlias::Unknown;
    };
    let method_key = php_symbol_key(method);
    let Some(class_info) = ctx.classes.get(&class_name) else {
        return ReturnArgAlias::Unknown;
    };
    if !matches!(receiver, StaticReceiver::Static)
        || class_info.is_final
        || class_info.final_static_methods.contains(&method_key)
    {
        return class_static_method_return_arg_alias(ctx, &class_name, &method_key)
            .unwrap_or(ReturnArgAlias::Unknown);
    }

    let mut summary: Option<ReturnArgAlias> = None;
    for candidate in ctx.classes.keys() {
        if !is_same_or_descendant_class(ctx, candidate, &class_name) {
            continue;
        }
        let Some(alias) = class_static_method_return_arg_alias(ctx, candidate, &method_key) else {
            continue;
        };
        summary = Some(match summary {
            Some(current) => current.merge(&alias),
            None => alias,
        });
    }
    summary.unwrap_or(ReturnArgAlias::Unknown)
}

/// Resolves one class's static implementation and its source alias summary.
fn class_static_method_return_arg_alias(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    method_key: &str,
) -> Option<ReturnArgAlias> {
    let class_info = ctx.classes.get(class_name)?;
    class_info.static_methods.get(method_key)?;
    let impl_class = class_info
        .static_method_impl_classes
        .get(method_key)
        .map(String::as_str)
        .unwrap_or(class_name);
    Some(
        ctx.return_alias_summaries
            .method(impl_class, method_key)
            .cloned()
            .unwrap_or(ReturnArgAlias::Unknown),
    )
}

/// Returns the implementation signature used by the static method symbol that will run.
fn static_method_implementation_signature<'a>(
    ctx: &'a LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    method: &str,
) -> Option<&'a FunctionSig> {
    let class_name = static_receiver_class_name(ctx, receiver)?;
    let key = php_symbol_key(method);
    let receiver_info = ctx.classes.get(class_name.as_str())?;
    let impl_class = receiver_info
        .static_method_impl_classes
        .get(&key)
        .map(String::as_str)
        .unwrap_or(class_name.as_str());
    ctx.classes
        .get(impl_class)
        .and_then(|class_info| class_info.static_methods.get(&key))
}

/// Returns the declared result type for a static method call before its arguments are lowered.
pub(super) fn static_method_call_expr_type_for_ir(
    ctx: &LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    method: &str,
) -> Option<PhpType> {
    let nominal = static_method_implementation_signature(ctx, receiver, method)
        .or_else(|| lexical_instance_static_call_signature(ctx, receiver, method))
        .map(|signature| normalize_value_php_type(signature.return_type.codegen_repr()))?;
    match (
        static_method_late_static_return_for_ir(ctx, receiver, method),
        static_late_binding_receiver_type_for_ir(ctx, receiver),
    ) {
        (Some(return_type), Some(receiver_type)) => Some(late_static_return_type_for_ir(
            ctx,
            &return_type,
            &receiver_type,
        )),
        _ => Some(nominal),
    }
}

/// Returns the instance-method signature used by `self::method()` or `parent::method()`.
fn lexical_instance_static_call_signature<'a>(
    ctx: &'a LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    method: &str,
) -> Option<&'a FunctionSig> {
    if !matches!(receiver, StaticReceiver::Self_ | StaticReceiver::Parent) {
        return None;
    }
    let class_name = static_receiver_class_name(ctx, receiver)?;
    let key = php_symbol_key(method);
    class_method_signature(ctx, &class_name, &key)
}

/// Resolves a static receiver to a concrete class name when lexical metadata is available.
fn static_receiver_class_name(
    ctx: &LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
) -> Option<String> {
    match receiver {
        StaticReceiver::Named(name) => Some(name.as_str().trim_start_matches('\\').to_string()),
        StaticReceiver::Self_ | StaticReceiver::Static => ctx.current_class.clone(),
        StaticReceiver::Parent => {
            let current = ctx.current_class.as_deref()?;
            ctx.classes.get(current).and_then(|class_info| class_info.parent.clone())
        }
    }
}

/// Lowers first-class callable creation.
fn lower_first_class_callable(ctx: &mut LoweringContext<'_, '_>, target: &CallableTarget, expr: &Expr) -> LoweredValue {
    let operands = if let CallableTarget::Method { object, .. } = target {
        vec![lower_expr(ctx, object).value]
    } else {
        Vec::new()
    };
    let data = ctx.intern_string(&callable_target_name(target));
    ctx.emit_value(
        Op::FirstClassCallableNew,
        operands,
        Some(Immediate::Data(data)),
        PhpType::Callable,
        Op::FirstClassCallableNew.default_effects(),
        Some(expr.span),
    )
}

/// Lowers a pointer cast.
fn lower_ptr_cast(ctx: &mut LoweringContext<'_, '_>, target_type: &str, inner: &Expr, expr: &Expr) -> LoweredValue {
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
fn lower_buffer_new(
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
fn lower_class_constant(ctx: &mut LoweringContext<'_, '_>, receiver: &StaticReceiver, expr: &Expr) -> LoweredValue {
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
fn lower_object_class_name(
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
fn lower_scoped_constant(ctx: &mut LoweringContext<'_, '_>, receiver: &StaticReceiver, name: &str, expr: &Expr) -> LoweredValue {
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
fn scoped_constant_receiver_name(ctx: &LoweringContext<'_, '_>, receiver: &StaticReceiver) -> String {
    match receiver {
        StaticReceiver::Static => receiver_name(receiver),
        _ => static_receiver_class_name(ctx, receiver).unwrap_or_else(|| receiver_name(receiver)),
    }
}

/// Lowers `static::CONST` using late static binding: emits a runtime dispatch over the
/// called-class id so that each descendant class that overrides the constant contributes
/// its own value. Falls back to the lexical (declaring-class) constant value.
fn lower_late_static_scoped_constant(ctx: &mut LoweringContext<'_, '_>, name: &str, expr: &Expr) -> LoweredValue {
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
fn late_static_constant_candidates(
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
fn is_same_or_descendant_class(
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
fn expr_literals_equal(a: &Expr, b: &Expr) -> bool {
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
fn lower_scoped_constant_fallback(ctx: &mut LoweringContext<'_, '_>, class_name: &str, name: &str, expr: &Expr) -> LoweredValue {
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
fn lower_new_scoped_object(ctx: &mut LoweringContext<'_, '_>, receiver: &StaticReceiver, args: &[Expr], expr: &Expr) -> LoweredValue {
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
fn lower_magic_constant(ctx: &mut LoweringContext<'_, '_>, kind: &MagicConstant, expr: &Expr) -> LoweredValue {
    let value = format!("__{:?}__", kind);
    lower_string_literal(ctx, &value, expr)
}

/// Lowers `yield`.
fn lower_yield(ctx: &mut LoweringContext<'_, '_>, key: Option<&Expr>, value: Option<&Expr>, expr: &Expr) -> LoweredValue {
    let mut operands = Vec::new();
    if let Some(key) = key {
        operands.push(lower_expr(ctx, key).value);
    }
    if let Some(value) = value {
        operands.push(lower_expr(ctx, value).value);
    }
    ctx.emit_value(Op::GeneratorYield, operands, None, PhpType::Mixed, Op::GeneratorYield.default_effects(), Some(expr.span))
}

/// Lowers `yield from`.
///
/// `yield from <generator|Traversable>` lowers to `Op::GeneratorYieldFrom`, which
/// the backend delegates to `__rt_gen_delegate` (forwarding sent values and
/// producing the inner generator's return value). `yield from <array>` is
/// desugared here into an iterator loop that re-yields each key/value pair,
/// reusing the foreach iterator opcodes; its result is PHP null.
fn lower_yield_from(ctx: &mut LoweringContext<'_, '_>, inner: &Expr, expr: &Expr) -> LoweredValue {
    let value = lower_expr(ctx, inner);
    let source_ty = ctx.builder.value_php_type(value.value).codegen_repr();
    if matches!(source_ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
        return lower_yield_from_array(ctx, value, expr);
    }
    let result = ctx.emit_value(
        Op::GeneratorYieldFrom,
        vec![value.value],
        None,
        PhpType::Mixed,
        Op::GeneratorYieldFrom.default_effects(),
        Some(expr.span),
    );
    // `__rt_gen_delegate` borrows the inner generator. When it is a fresh owning
    // temporary (`yield from inner()`) nothing else frees it once delegation
    // ends, so release it here; a borrowed local (`yield from $g`) keeps its
    // owner and must not be released (it would double-free at scope end).
    if ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(expr.span));
    }
    result
}

/// Desugars `yield from <array>` into an iterator loop that re-yields each
/// key/value pair, returning a boxed PHP null (arrays have no delegated return
/// value). Reuses the foreach iterator opcodes so every array kind (indexed,
/// associative, by-element-type) is handled by the existing iterator lowering.
fn lower_yield_from_array(
    ctx: &mut LoweringContext<'_, '_>,
    source: LoweredValue,
    expr: &Expr,
) -> LoweredValue {
    let span = expr.span;
    let iterator = ctx.emit_value(
        Op::IterStart,
        vec![source.value],
        None,
        PhpType::Iterable,
        Op::IterStart.default_effects(),
        Some(span),
    );
    let header = ctx.builder.create_named_block("yieldfrom.next", Vec::new());
    let body = ctx.builder.create_named_block("yieldfrom.body", Vec::new());
    let exit = ctx.builder.create_named_block("yieldfrom.exit", Vec::new());
    if !ctx.builder.insertion_block_is_terminated() {
        ctx.builder.terminate(Terminator::Br {
            target: header,
            args: Vec::new(),
        });
    }

    ctx.builder.position_at_end(header);
    let has_next = ctx.emit_value(
        Op::IterNext,
        vec![iterator.value],
        None,
        PhpType::Bool,
        Op::IterNext.default_effects(),
        Some(span),
    );
    ctx.builder.terminate(Terminator::CondBr {
        cond: has_next.value,
        then_target: body,
        then_args: Vec::new(),
        else_target: exit,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(body);
    let key = ctx.emit_value(
        Op::IterCurrentKey,
        vec![iterator.value],
        None,
        PhpType::Mixed,
        Op::IterCurrentKey.default_effects(),
        Some(span),
    );
    let element = ctx.emit_value(
        Op::IterCurrentValue,
        vec![iterator.value],
        None,
        PhpType::Mixed,
        Op::IterCurrentValue.default_effects(),
        Some(span),
    );
    // Re-yield the inner key/value pair through the outer generator. The sent
    // value is discarded (arrays ignore it), exactly like a `yield $k => $v;`
    // statement.
    ctx.emit_value(
        Op::GeneratorYield,
        vec![key.value, element.value],
        None,
        PhpType::Mixed,
        Op::GeneratorYield.default_effects(),
        Some(span),
    );
    if !ctx.builder.insertion_block_is_terminated() {
        ctx.builder.terminate(Terminator::Br {
            target: header,
            args: Vec::new(),
        });
    }

    ctx.builder.position_at_end(exit);
    // The iterator borrows a freshly-created array (e.g. a literal): release it
    // once iteration ends, mirroring `lower_foreach`.
    if ctx.value_is_owning_temporary(source) {
        crate::ir_lower::ownership::release_if_owned(ctx, source, Some(span));
    }
    let null_value = ctx
        .builder
        .emit_with_effects(
            Op::ConstNull,
            Vec::new(),
            None,
            IrType::I64,
            PhpType::Void,
            Ownership::NonHeap,
            Op::ConstNull.default_effects(),
            Some(span),
        )
        .expect("const_null produces a value");
    // A fresh null is non-refcounted: there is no producer reference to release,
    // so this boxes directly rather than via box_value_as_mixed (issue #484).
    ctx.emit_value(
        Op::MixedBox,
        vec![null_value],
        None,
        PhpType::Mixed,
        Op::MixedBox.default_effects(),
        Some(span),
    )
}

/// Lowers `instanceof`.
fn lower_instanceof(
    ctx: &mut LoweringContext<'_, '_>,
    value: &Expr,
    target: &InstanceOfTarget,
    expr: &Expr,
) -> LoweredValue {
    let mut operands = vec![lower_expr(ctx, value).value];
    let immediate = match target {
        InstanceOfTarget::Name(name) => {
            if name.as_str().trim_start_matches('\\') == "static" && ctx.local_slots.contains_key("this") {
                operands.push(ctx.load_local("this", Some(expr.span)).value);
                None
            } else {
                Some(Immediate::Data(ctx.intern_class_name(&instanceof_target_name(ctx, name.as_str()))))
            }
        }
        InstanceOfTarget::Expr(expr) => {
            operands.push(lower_expr(ctx, expr).value);
            None
        }
    };
    let op = if immediate.is_some() { Op::InstanceOf } else { Op::InstanceOfDynamic };
    ctx.emit_value(op, operands, immediate, PhpType::Bool, op.default_effects(), Some(expr.span))
}

/// Resolves lexical `instanceof` target keywords to concrete class names when possible.
fn instanceof_target_name(ctx: &LoweringContext<'_, '_>, name: &str) -> String {
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

/// Coerces a value to integer storage before integer-only operations.
fn coerce_to_int(ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, expr: &Expr) -> LoweredValue {
    coerce_to_int_at_span(ctx, value, Some(expr.span))
}

/// Coerces a value to integer storage using an explicit source span.
pub(crate) fn coerce_to_int_at_span(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<crate::span::Span>,
) -> LoweredValue {
    match value.ir_type {
        IrType::I64 => value,
        IrType::F64 => ctx.emit_value(Op::FToI, vec![value.value], None, PhpType::Int, Op::FToI.default_effects(), span),
        IrType::Str => ctx.emit_value(Op::StrToI, vec![value.value], None, PhpType::Int, Op::StrToI.default_effects(), span),
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

/// Coerces a value to float when the storage type allows a direct conversion.
fn coerce_to_float(ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, expr: &Expr) -> LoweredValue {
    coerce_to_float_at_span(ctx, value, Some(expr.span))
}

/// Coerces a value to float storage using an explicit source span.
fn coerce_to_float_at_span(
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
fn coerce_to_string(ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, expr: &Expr) -> LoweredValue {
    coerce_to_string_at_span(ctx, value, Some(expr.span))
}

/// Coerces a value to string storage using an explicit source span.
fn coerce_to_string_at_span(
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

/// Stores a lowered expression result into a hidden merge temporary.
fn store_expr_into_temp(
    ctx: &mut LoweringContext<'_, '_>,
    temp_name: &str,
    temp_type: PhpType,
    expr: &Expr,
    span: crate::span::Span,
) {
    let value = lower_expr(ctx, expr);
    store_value_into_temp(ctx, temp_name, temp_type, value, span);
}

/// Stores an already lowered value into a hidden merge temporary.
fn store_value_into_temp(
    ctx: &mut LoweringContext<'_, '_>,
    temp_name: &str,
    temp_type: PhpType,
    value: LoweredValue,
    span: crate::span::Span,
) {
    let value = coerce_value_for_temp(ctx, value, &temp_type, span);
    let source = value;
    let stored = crate::ir_lower::ownership::acquire_if_refcounted(ctx, value, Some(span));
    ctx.store_local(temp_name, stored, temp_type, Some(span));
    if stored.value != source.value && ctx.value_needs_release_after_retaining_store(source) {
        crate::ir_lower::ownership::release_if_owned(ctx, source, Some(span));
    }
}

/// Loads an owned hidden temp into SSA and clears the backing slot without releasing it.
fn take_owned_temp(
    ctx: &mut LoweringContext<'_, '_>,
    temp_name: &str,
    span: crate::span::Span,
) -> LoweredValue {
    let value = ctx.load_local(temp_name, Some(span));
    ctx.clear_owned_hidden_temp(temp_name, Some(span));
    value
}

/// Chooses a merge temp type from contextual branch materialization and fallback metadata.
fn branch_merge_result_type(
    ctx: &LoweringContext<'_, '_>,
    then_expr: &Expr,
    else_expr: &Expr,
    expr: &Expr,
) -> PhpType {
    let then_ty = materialized_expr_type_for_merge(ctx, then_expr);
    let else_ty = materialized_expr_type_for_merge(ctx, else_expr);
    let branch_ty = nullable_aware_branch_merge_type(&then_ty, &else_ty);
    if php_type_allows_null(&branch_ty) {
        return branch_ty;
    }
    let fallback_ty = fallback_expr_type(expr).codegen_repr();
    wider_type_for_merge(&fallback_ty, &branch_ty.codegen_repr())
}

/// Chooses a match hidden-temp type by merging every arm result type, so
/// heterogeneous arms (e.g. object/array/string) materialize a Mixed temp
/// boxed per arm instead of coercing all arms to one unified scalar type.
fn match_merge_result_type(
    ctx: &LoweringContext<'_, '_>,
    arms: &[(Vec<Expr>, Expr)],
    default: Option<&Expr>,
    expr: &Expr,
) -> PhpType {
    let mut merged: Option<PhpType> = None;
    for result in arms.iter().map(|(_, result)| result).chain(default) {
        let arm_ty = materialized_expr_type_for_merge(ctx, result);
        merged = Some(match merged {
            Some(acc) => nullable_aware_branch_merge_type(&acc, &arm_ty),
            None => arm_ty,
        });
    }
    let Some(merged) = merged else {
        return fallback_expr_type(expr);
    };
    if php_type_allows_null(&merged) {
        return merged;
    }
    let fallback_ty = fallback_expr_type(expr).codegen_repr();
    wider_type_for_merge(&fallback_ty, &merged.codegen_repr())
}

/// Chooses a short-ternary hidden-temp type without reintroducing the
/// scalar-biased syntactic join used by the parser-only fallback inference.
fn short_ternary_merge_result_type(
    ctx: &LoweringContext<'_, '_>,
    value: &Expr,
    default: &Expr,
) -> PhpType {
    let value_ty = materialized_expr_type_for_merge(ctx, value).codegen_repr();
    let default_ty = materialized_expr_type_for_merge(ctx, default).codegen_repr();
    wider_type_for_merge(&value_ty, &default_ty)
}

/// Chooses a ternary branch merge type without erasing PHP null branches.
fn nullable_aware_branch_merge_type(left: &PhpType, right: &PhpType) -> PhpType {
    if php_type_allows_null(left) || php_type_allows_null(right) {
        let left_non_null = strip_void_from_union(left.clone());
        let right_non_null = strip_void_from_union(right.clone());
        return normalize_union_members(vec![PhpType::Void, left_non_null, right_non_null])
            .unwrap_or(PhpType::Void);
    }
    wider_type_for_merge(&left.codegen_repr(), &right.codegen_repr())
}

/// Returns true when a PHP type can materialize PHP null at runtime.
fn php_type_allows_null(php_type: &PhpType) -> bool {
    match php_type {
        PhpType::Void | PhpType::Never | PhpType::Mixed => true,
        PhpType::Union(members) => members
            .iter()
            .any(|member| matches!(member, PhpType::Void | PhpType::Never | PhpType::Mixed)),
        _ => false,
    }
}

/// Estimates the value type an expression will materialize during branch lowering.
fn materialized_expr_type_for_merge(ctx: &LoweringContext<'_, '_>, expr: &Expr) -> PhpType {
    match &expr.kind {
        ExprKind::Variable(name) => normalize_value_php_type(ctx.local_type(name).codegen_repr()),
        ExprKind::ErrorSuppress(inner) => materialized_expr_type_for_merge(ctx, inner),
        ExprKind::BinaryOp { left, op, right } if mixed_numeric_op(op).is_some() => {
            let left_ty = materialized_expr_type_for_merge(ctx, left).codegen_repr();
            let right_ty = materialized_expr_type_for_merge(ctx, right).codegen_repr();
            if matches!(left_ty, PhpType::Mixed | PhpType::Union(_))
                || matches!(right_ty, PhpType::Mixed | PhpType::Union(_))
            {
                PhpType::Mixed
            } else {
                fallback_expr_type(expr)
            }
        }
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => branch_merge_result_type(ctx, then_expr, else_expr, expr),
        ExprKind::Match { arms, default, .. } => {
            match_merge_result_type(ctx, arms, default.as_deref(), expr)
        }
        ExprKind::ShortTernary { value, default } => {
            short_ternary_merge_result_type(ctx, value, default)
        }
        ExprKind::ArrayAccess { array, .. } => array_access_expr_value_type_for_ir(ctx, array)
            .unwrap_or_else(|| fallback_expr_type(expr)),
        ExprKind::PropertyAccess { object, property } => {
            property_access_expr_type_for_ir(ctx, object, property)
                .unwrap_or_else(|| fallback_expr_type(expr))
        }
        _ => fallback_expr_type(expr),
    }
}

/// Coerces branch values to the hidden temp storage type before storing them.
fn coerce_value_for_temp(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    temp_type: &PhpType,
    span: crate::span::Span,
) -> LoweredValue {
    let target_ty = temp_type.codegen_repr();
    let source_ty = ctx.builder.value_php_type(value.value).codegen_repr();
    if source_ty == target_ty {
        return value;
    }
    match &target_ty {
        PhpType::Mixed => ctx.box_value_as_mixed(value, PhpType::Mixed, Some(span)),
        PhpType::Int | PhpType::Bool | PhpType::Void | PhpType::Never => {
            coerce_to_int_at_span(ctx, value, Some(span))
        }
        PhpType::Float => coerce_to_float_at_span(ctx, value, Some(span)),
        PhpType::Str => coerce_to_string_at_span(ctx, value, Some(span)),
        _ => widen_container_value_for_temp(ctx, value, &source_ty, &target_ty, span),
    }
}

/// Widens a typed container branch value to a hidden temp's boxed-Mixed
/// element storage before it is stored.
///
/// Mismatched array/array (or assoc/assoc) branch merges declare the temp with
/// `Mixed` element storage (`wider_type_for_merge`, issue #549), so each
/// branch's concrete container must box its slots via `ArrayToMixed` /
/// `HashToMixed`: storing the raw pointer would let Mixed-element reads
/// misinterpret the typed slot bytes. Borrowed sources (live locals, container
/// element reads) are retained first so the conversion's copy-on-write split
/// rewrites a private copy instead of boxing the source's slots in place; the
/// conversion consumes that reference, and owning temporaries transfer their
/// reference into the converted result, so no release is emitted here
/// (mirrors `coerce_container_to_return_type`).
fn widen_container_value_for_temp(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    source_ty: &PhpType,
    target_ty: &PhpType,
    span: crate::span::Span,
) -> LoweredValue {
    let target_has_mixed_payload = match target_ty {
        PhpType::Array(elem) => elem.codegen_repr() == PhpType::Mixed,
        PhpType::AssocArray { value, .. } => value.codegen_repr() == PhpType::Mixed,
        _ => false,
    };
    if !target_has_mixed_payload {
        return value;
    }
    let op = match (source_ty, target_ty) {
        (PhpType::Array(source_elem), PhpType::Array(_))
            if source_elem.codegen_repr() != PhpType::Mixed =>
        {
            Op::ArrayToMixed
        }
        (PhpType::AssocArray { value: source_value, .. }, PhpType::AssocArray { .. })
            if source_value.codegen_repr() != PhpType::Mixed =>
        {
            Op::HashToMixed
        }
        (PhpType::Mixed | PhpType::Union(_), _)
            if value.ir_type == IrType::Heap(IrHeapKind::Mixed) =>
        {
            // Whole-boxed sources (a `?array` value flowing through `??`)
            // unbox the cell payload and convert it with the same
            // runtime-call coercion declared container returns use. The
            // conversion borrows the cell and owns a fresh container
            // reference, so an owning cell must be consumed here.
            //
            // The indexed conversion consumes one owned payload reference
            // and rewrites sole-owner arrays in place, which is only sound
            // when the cell owns its payload. A borrowed cell (a `?array`
            // parameter or local) shares its payload with a live caller
            // array, so it unboxes through the owned-payload coercion —
            // which retains the payload — and the consuming `ArrayToMixed`
            // copy-on-write-splits into a private converted copy. The
            // associative helper returns a fresh hash without consuming the
            // payload reference, so borrowed hash cells keep the
            // single-call coercion.
            let cell_is_owning = ctx.value_is_owning_temporary(value);
            if !cell_is_owning && matches!(target_ty, PhpType::Array(_)) {
                let unboxed = ctx.emit_value(
                    Op::RuntimeCall,
                    vec![value.value],
                    None,
                    PhpType::Array(Box::new(PhpType::Never)),
                    effects_lookup::runtime_effects(),
                    Some(span),
                );
                return ctx.emit_value(
                    Op::ArrayToMixed,
                    vec![unboxed.value],
                    None,
                    target_ty.clone(),
                    Op::ArrayToMixed.default_effects(),
                    Some(span),
                );
            }
            let converted = ctx.emit_value(
                Op::RuntimeCall,
                vec![value.value],
                None,
                target_ty.clone(),
                effects_lookup::runtime_effects(),
                Some(span),
            );
            if cell_is_owning {
                crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
            }
            return converted;
        }
        _ => return value,
    };
    // Local loads report as *provisional* owners (their compensating releases
    // are pruned at builder finalization when the slot stays concrete), so
    // they must be treated as borrowed here: without a real retain the
    // conversion's copy-on-write split would never trigger and the local's
    // own array would be boxed in place while its slot type stays concrete.
    let source_is_consumable = ctx.value_is_owning_temporary(value)
        && !ctx.value_is_owned_unboxed_local_load(value.value);
    let source = if source_is_consumable {
        value
    } else {
        crate::ir_lower::ownership::acquire_if_refcounted(ctx, value, Some(span))
    };
    ctx.emit_value(
        op,
        vec![source.value],
        None,
        target_ty.clone(),
        op.default_effects(),
        Some(span),
    )
}

/// Emits a branch to a target block when the current block can still fall through.
fn branch_to(ctx: &mut LoweringContext<'_, '_>, target: BlockId) {
    if !ctx.builder.insertion_block_is_terminated() {
        ctx.builder.terminate(Terminator::Br { target, args: Vec::new() });
    }
}

/// Computes definitely initialized slots after a two-way expression split.
fn merge_initialized_slots_for_expr(
    split_initialized: &HashSet<LocalSlotId>,
    then_initialized: HashSet<LocalSlotId>,
    then_reachable: bool,
    else_initialized: HashSet<LocalSlotId>,
    else_reachable: bool,
) -> HashSet<LocalSlotId> {
    match (then_reachable, else_reachable) {
        (true, true) => then_initialized
            .intersection(&else_initialized)
            .copied()
            .collect(),
        (true, false) => then_initialized,
        (false, true) => else_initialized,
        (false, false) => split_initialized.clone(),
    }
}

/// Emits a boolean literal value for control-expression lowering.
fn emit_bool_literal(
    ctx: &mut LoweringContext<'_, '_>,
    value: bool,
    span: Option<crate::span::Span>,
) -> LoweredValue {
    let value = ctx
        .builder
        .emit_with_effects(
            Op::ConstBool,
            Vec::new(),
            Some(Immediate::Bool(value)),
            IrType::I64,
            PhpType::Bool,
            Ownership::NonHeap,
            Op::ConstBool.default_effects(),
            span,
        )
        .expect("const_bool produces a value");
    LoweredValue { value, ir_type: IrType::I64 }
}

/// Returns a printable static receiver name.
fn receiver_name(receiver: &StaticReceiver) -> String {
    match receiver {
        StaticReceiver::Named(name) => name.as_str().to_string(),
        StaticReceiver::Self_ => "self".to_string(),
        StaticReceiver::Static => "static".to_string(),
        StaticReceiver::Parent => "parent".to_string(),
    }
}

/// Returns a printable callable target name.
fn callable_target_name(target: &CallableTarget) -> String {
    match target {
        CallableTarget::Function(name) => name.as_str().to_string(),
        CallableTarget::StaticMethod { receiver, method } => {
            format!("{}::{}", receiver_name(receiver), method)
        }
        CallableTarget::Method { method, .. } => format!("object::{}", method),
    }
}

/// Returns a syntactic fallback PHP type for an expression.
fn fallback_expr_type(expr: &Expr) -> PhpType {
    normalize_value_php_type(infer_expr_type_syntactic(expr))
}

/// Normalizes non-materializable expression types to the EIR null sentinel.
fn normalize_value_php_type(php_type: PhpType) -> PhpType {
    if matches!(php_type, PhpType::Never) {
        PhpType::Void
    } else {
        php_type
    }
}
