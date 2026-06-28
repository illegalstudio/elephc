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
    BlockId, CmpPredicate, Effects, Immediate, IrHeapKind, IrType, LocalSlotId, MixedNumericOp, Op,
    Ownership, Terminator, ValueId,
};
use crate::ir_lower::context::{
    value_ir_type, ClosureCapture, LoweredValue, LoweringContext, StaticCallableBinding,
};
use crate::ir_lower::effects_lookup;
use crate::ir_lower::function;
use crate::names::{php_symbol_key, property_hook_get_method, Name};
use crate::parser::ast::{
    BinOp, CallableTarget, CastType, Expr, ExprKind, InstanceOfTarget, MagicConstant,
    StaticReceiver, Stmt, StmtKind, TypeExpr, Visibility,
};
use crate::span::Span;
use crate::types::checker::builtins::canonical_builtin_function_name;
use crate::types::{
    array_key_type_from_value_type, checker::infer_expr_type_syntactic,
    merge_array_key_types, normalized_array_key_type, ExternFunctionSig, FunctionSig, PhpType,
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
            return_type,
            body,
            captures,
            capture_refs,
            ..
        } => lower_closure(
            ctx,
            params,
            variadic.as_deref(),
            return_type.as_ref(),
            body,
            captures,
            capture_refs,
            expr,
        ),
        ExprKind::NamedArg { value, .. } => lower_expr(ctx, value),
        ExprKind::Spread(inner) => lower_expr(ctx, inner),
        ExprKind::ClosureCall { var, args } => lower_closure_call(ctx, var, args, expr),
        ExprKind::ExprCall { callee, args } => lower_expr_call(ctx, callee, args, expr),
        ExprKind::ConstRef(name) => constants::lower_const_ref(ctx, name, expr),
        ExprKind::NewObject { class_name, args } => lower_new_object(ctx, class_name, args, expr),
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
        ExprKind::MethodCall { object, method, args } => lower_method_call(ctx, object, method, args, Op::MethodCall, expr),
        ExprKind::NullsafeMethodCall { object, method, args } => {
            lower_nullsafe_method_call(ctx, object, method, args, expr)
        }
        ExprKind::StaticMethodCall { receiver, method, args } => {
            lower_static_method_call(ctx, receiver, method, args, expr)
        }
        ExprKind::FirstClassCallable(target) => lower_first_class_callable(ctx, target, expr),
        ExprKind::This => ctx.load_local("this", Some(expr.span)),
        ExprKind::PtrCast { target_type, expr: inner } => lower_ptr_cast(ctx, target_type, inner, expr),
        ExprKind::BufferNew { element_type, len } => lower_buffer_new(ctx, element_type, len, expr),
        ExprKind::ClassConstant { receiver } => lower_class_constant(ctx, receiver, expr),
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
            PhpType::Bool,
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
    ctx.emit_value(
        Op::MixedBox,
        vec![null.value],
        None,
        PhpType::Mixed,
        Op::MixedBox.default_effects(),
        Some(expr.span),
    )
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
            return lower_mixed_numeric_binary(ctx, lhs, rhs, mixed_op, expr);
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
    if lhs.ir_type == IrType::I64 && rhs.ir_type == IrType::I64 {
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
        return lower_mixed_numeric_binary(ctx, lhs, rhs, mixed_op, expr);
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
        | ExprKind::StaticMethodCall { .. }
        | ExprKind::NewObject { .. }
        | ExprKind::NewDynamic { .. }
        | ExprKind::NewDynamicObject { .. }
        | ExprKind::NewScopedObject { .. }
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
        | ExprKind::BufferNew { len: inner, .. } => expr_can_reset_concat_storage(inner),
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
        IrType::I64 => ctx.emit_value(int_op, vec![value.value], None, PhpType::Int, int_op.default_effects(), Some(expr.span)),
        IrType::TaggedScalar => {
            let narrowed = lower_tagged_scalar_to_int(ctx, value, Some(expr.span));
            ctx.emit_value(int_op, vec![narrowed.value], None, PhpType::Int, int_op.default_effects(), Some(expr.span))
        }
        _ if int_op == Op::INeg => {
            let zero = lower_int_literal(ctx, 0, expr);
            lower_mixed_numeric_binary(ctx, zero, value, MixedNumericOp::Sub, expr)
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
    ctx.emit_void(Op::ThrowException, vec![value.value], None, Op::ThrowException.default_effects(), Some(expr.span));
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
    let value = lower_expr(ctx, value);
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
    let default_block = ctx.builder.create_named_block("coalesce.default", Vec::new());
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
        (PhpType::Array(_), PhpType::Array(_)) => right.clone(),
        (PhpType::AssocArray { .. }, PhpType::AssocArray { .. }) => right.clone(),
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
    let value = lower_expr(ctx, value);
    let cond = ctx.truthy(value, Some(condition_span));
    let result_type = fallback_expr_type(expr);
    let temp_name = ctx.declare_owned_hidden_temp(result_type.clone());
    let split_initialized = ctx.initialized_slots_snapshot();
    let value_block = ctx.builder.create_named_block("short_ternary.value", Vec::new());
    let default_block = ctx.builder.create_named_block("short_ternary.default", Vec::new());
    let merge = ctx.builder.create_named_block("short_ternary.merge", Vec::new());
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
    let static_callable = assigned_name.and_then(|_| static_callable_binding_for_expr(ctx, value));
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
        let php_type = ctx.builder.value_php_type(lowered.value);
        result = ctx.store_local(name, lowered, php_type, Some(expr.span));
        let static_callable = callable_array
            .map(|assignment| assignment.target)
            .or(static_callable);
        if let Some(target) = static_callable {
            ctx.bind_static_callable_local(name, target);
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
    ctx.declare_hidden_temp_with_name(temp_name, result_type.clone());
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
fn lower_inc_dec(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    increment: bool,
    post: bool,
    expr: &Expr,
) -> LoweredValue {
    let old = ctx.load_local(name, Some(expr.span));
    let one = lower_int_literal(ctx, 1, expr);
    let operand = coerce_to_int(ctx, old, expr);
    let op = if increment { Op::IAdd } else { Op::ISub };
    let new = ctx.emit_value(op, vec![operand.value, one.value], None, PhpType::Int, op.default_effects(), Some(expr.span));
    ctx.store_local(name, new, PhpType::Int, Some(expr.span));
    if post { old } else { new }
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
    let sig = call_signature(ctx, canonical, args);
    let is_extern = ctx.extern_functions.contains_key(canonical);
    let is_user_function = ctx.functions.contains_key(canonical);
    let operands = if is_extern || is_user_function {
        lower_args_with_signature(ctx, sig.as_ref(), args)
    } else {
        lower_builtin_call_args(ctx, canonical, sig.as_ref(), args)
    };
    let php_type = if is_extern || is_user_function {
        call_return_type(ctx, canonical, &operands)
    } else {
        call_return_type_for_args(ctx, canonical, args, &operands)
            .unwrap_or_else(|| call_return_type(ctx, canonical, &operands))
    };
    if is_extern {
        let data = ctx.intern_function_name(canonical);
        return ctx.emit_value(
            Op::ExternCall,
            operands,
            Some(Immediate::Data(data)),
            php_type,
            Op::ExternCall.default_effects(),
            Some(expr.span),
        );
    }
    if is_user_function {
        let data = ctx.intern_function_name(canonical);
        return ctx.emit_value(
            Op::Call,
            operands,
            Some(Immediate::Data(data)),
            php_type,
            effects_lookup::user_call_effects(canonical),
            Some(expr.span),
        );
    }
    emit_builtin_call_value(ctx, canonical, operands, php_type, expr.span)
}

/// Emits a builtin call and releases owned temporary arguments after the call consumes them.
fn emit_builtin_call_value(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    operands: Vec<crate::ir::ValueId>,
    php_type: PhpType,
    span: Span,
) -> LoweredValue {
    let data = ctx.intern_function_name(name);
    let call = ctx.emit_value(
        Op::BuiltinCall,
        operands.clone(),
        Some(Immediate::Data(data)),
        php_type,
        effects_lookup::builtin_effects(name),
        Some(span),
    );
    release_owned_call_arg_temporaries(ctx, &operands, Some(call.value), span);
    call
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
        return Some(lower_int_literal(ctx, 0, expr));
    }

    let temp_name = ctx.declare_hidden_temp(PhpType::Int);
    let false_block = ctx.builder.create_named_block("isset.lazy_false", Vec::new());
    let merge = ctx.builder.create_named_block("isset.lazy_merge", Vec::new());
    for (idx, arg) in args.iter().enumerate() {
        let checked = lower_lazy_isset_operand(ctx, arg).unwrap_or_else(|| {
            let value = lower_expr(ctx, arg);
            emit_builtin_call_value(ctx, name, vec![value.value], PhpType::Int, arg.span)
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

    let true_value = lower_int_literal(ctx, 1, expr);
    store_value_into_temp(ctx, &temp_name, PhpType::Int, true_value, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(false_block);
    let false_value = lower_int_literal(ctx, 0, expr);
    store_value_into_temp(ctx, &temp_name, PhpType::Int, false_value, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    Some(take_owned_temp(ctx, &temp_name, expr.span))
}

/// Lowers a single `isset()` operand that has special lazy PHP semantics.
fn lower_lazy_isset_operand(
    ctx: &mut LoweringContext<'_, '_>,
    arg: &Expr,
) -> Option<LoweredValue> {
    let ExprKind::ArrayAccess { array, index } = &arg.kind else {
        return None;
    };
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

/// Lowers native array/hash `isset($array[$key])` without reading the element value.
fn lower_native_isset_offset_probe(
    ctx: &mut LoweringContext<'_, '_>,
    array: &Expr,
    index: &Expr,
    expr: &Expr,
) -> LoweredValue {
    let array_value = lower_expr(ctx, array);
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
    let null_block = ctx.builder.create_named_block("isset.native.null", Vec::new());
    let probe_block = ctx.builder.create_named_block("isset.native.probe", Vec::new());
    let merge = ctx.builder.create_named_block("isset.native.merge", Vec::new());
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
            index_value = coerce_to_int_at_span(ctx, index_value, Some(index.span));
            ctx.emit_value(
                Op::ArrayIsset,
                vec![array_value.value, index_value.value],
                None,
                PhpType::Bool,
                Op::ArrayIsset.default_effects(),
                Some(expr.span),
            )
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
            let read_value = lower_array_access_from_value(ctx, array_value, index, expr);
            emit_builtin_call_value(ctx, "isset", vec![read_value.value], PhpType::Int, expr.span)
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
            let callback = static_call_user_func_callback(ctx, callback_expr)?;
            lower_static_callable_call(ctx, callback, callback_args, expr)
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
            let callback_args = static_call_user_func_array_args(arg_array)?;
            if let Some(callback) = instance_call_user_func_callback(ctx, callback_arg) {
                return lower_instance_callable_call_user_func(
                    ctx,
                    callback_arg,
                    callback,
                    &callback_args,
                    expr,
                );
            }
            let callback = static_call_user_func_callback(ctx, callback_arg)?;
            lower_static_callable_call(ctx, callback, &callback_args, expr)
        }
        _ => None,
    }
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
    let arg_array =
        lower_descriptor_invoker_arg_array_for_call_user_func_array(
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
        defaults: vec![None; sig.params.len()],
        return_type: sig.return_type.clone(),
        declared_return: true,
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
fn descriptor_callback_php_type_supported(php_type: &PhpType) -> bool {
    matches!(
        php_type,
        PhpType::Str | PhpType::Callable | PhpType::Array(_) | PhpType::Object(_)
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
    ctx.emit_value(
        Op::MixedBox,
        vec![hash.value],
        None,
        PhpType::Mixed,
        Op::MixedBox.default_effects(),
        Some(span),
    )
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
            Some(emit_builtin_call_value(ctx, &function_name, operands, php_type, expr.span))
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
            let sig = call_signature(ctx, &function_name, callback_args);
            let operands = lower_builtin_call_args(ctx, &function_name, sig.as_ref(), callback_args);
            let php_type = call_return_type(ctx, &function_name, &operands);
            Some(emit_builtin_call_value(ctx, &function_name, operands, php_type, expr.span))
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
    if let Some(function_name) = lookup_folded_name(ctx.functions.keys(), callback) {
        return Some(StaticCallableBinding::UserFunction(function_name));
    }
    canonical_builtin_function_name(callback).map(StaticCallableBinding::Builtin)
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
    args: &[Expr],
) -> Option<FunctionSig> {
    if let Some(sig) = ctx.functions.get(name) {
        return Some(sig.clone());
    }
    if let Some(sig) = ctx.extern_functions.get(name) {
        return Some(function_sig_from_extern_for_descriptor(sig));
    }
    if crate::types::call_args::has_named_args(args) {
        return builtin_call_signature(name);
    }
    None
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
        ExprKind::ArrayAccess { array, .. } => unset_array_access_has_object_receiver(ctx, array),
        _ => false,
    }
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

/// Lowers `unset($object[$key])` as `ArrayAccess::offsetUnset($key)`.
fn lower_unset_array_access(
    ctx: &mut LoweringContext<'_, '_>,
    array: &Expr,
    index: &Expr,
    expr: &Expr,
) {
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
    let elem_ty = super::stmt::indexed_array_write_element_type(
        ctx,
        array_value,
        updated_ty.as_ref(),
    );
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
    match php_symbol_key(name.trim_start_matches('\\')).as_str() {
        "date" => lower_date_args(ctx, sig, args),
        "json_decode" => lower_json_decode_args(ctx, sig, args),
        "preg_replace_callback"
            if !crate::types::call_args::has_named_args(args)
                && !args.iter().any(is_spread_arg) =>
        {
            lower_preg_replace_callback_args(ctx, sig, args)
        }
        "preg_match" | "preg_split"
            if !crate::types::call_args::has_named_args(args)
                && !args.iter().any(is_spread_arg) =>
        {
            lower_args(ctx, args)
        }
        "usort" | "uasort"
            if !crate::types::call_args::has_named_args(args)
                && !args.iter().any(is_spread_arg) =>
        {
            lower_user_value_sort_args(ctx, sig, args)
        }
        _ => lower_args_with_signature(ctx, sig, args),
    }
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
        return_type,
        body,
        captures,
        capture_refs,
        ..
    } = &callback.kind
    else {
        return lower_expr(ctx, callback).value;
    };
    lower_closure_with_context(
        ctx,
        params,
        variadic.as_deref(),
        return_type.as_ref(),
        body,
        captures,
        capture_refs,
        callback,
        &[elem_ty.clone(), elem_ty],
        None,
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
    let sig = call_signature(ctx, name, args);
    let operands = lower_builtin_call_args(ctx, name, sig.as_ref(), args);
    let result = emit_builtin_call_value(ctx, name, operands, PhpType::Bool, expr.span);
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
    let sig = call_signature(ctx, name, args)?;
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
        return_type,
        body,
        captures,
        capture_refs,
        ..
    } = &callback.kind
    else {
        return None;
    };
    Some(lower_closure_with_context(
        ctx,
        params,
        variadic.as_deref(),
        return_type.as_ref(),
        body,
        captures,
        capture_refs,
        callback,
        &[PhpType::Array(Box::new(PhpType::Str))],
        None,
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
    if let Some(value) = lower_by_ref_array_arg_with_signature(ctx, sig, index, arg) {
        return value;
    }
    let lowered = lower_expr(ctx, arg);
    coerce_scalar_arg_to_param_storage(ctx, sig, index, lowered, arg).value
}

/// Coerces a positional argument's storage to match a declared scalar parameter type.
///
/// EIR passes each call argument in its natural storage. A declared `float` parameter is
/// materialized into the callee's floating-point register/slot, so an integer argument must be
/// converted with `IToF` first: without it the raw 64-bit integer bit-pattern lands in the
/// float slot and the callee reads garbage (and, when other float arguments are present, the
/// unconverted slot is overwritten by a neighbouring float argument). Only the int→float case
/// is adjusted; every other argument/parameter storage combination is passed through unchanged.
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
    if value.ir_type == IrType::I64 && param_ty.codegen_repr() == PhpType::Float {
        return coerce_to_float(ctx, value, arg);
    }
    value
}

/// Widens positional call operands to their declared scalar parameter types.
///
/// The C/native ABI places an argument in an integer or floating-point register based
/// on the *value's* type, while the callee reads each parameter from the register class
/// of the *parameter's* type. Without this step an `int` (or `bool`) argument passed to
/// a `float` parameter is deposited in an integer register and then read back as garbage
/// from a floating-point slot. Only pure `float` parameters receiving an integer/bool
/// operand are rewritten with an int→float conversion; by-reference parameters and the
/// variadic tail operand are left untouched.
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
        if param_ty.codegen_repr() != PhpType::Float {
            continue;
        }
        let value = operands[index];
        let operand_ty = ctx.builder.value_php_type(value).codegen_repr();
        if !matches!(operand_ty, PhpType::Int | PhpType::Bool) {
            continue;
        }
        let lowered = LoweredValue { value, ir_type: IrType::I64 };
        operands[index] = coerce_to_float_at_span(ctx, lowered, None).value;
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
    for source in tail {
        if source.param_idx().is_some() {
            continue;
        }
        let value = source_values[source.source_index()];
        let value = lowered_value_from_id(ctx, value);
        let value = coerce_variadic_tail_value(ctx, value, &array_ty, source.expr().span);
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
        let value = source_values[source.source_index()];
        let value = lowered_value_from_id(ctx, value);
        let array_ty = PhpType::Array(Box::new(value_ty.clone()));
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
    for item in tail {
        let value = lower_expr(ctx, item);
        let value = coerce_variadic_tail_value(ctx, value, &array_ty, item.span);
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

/// Returns the element type expected inside a variadic tail container.
fn variadic_tail_value_type(sig: &FunctionSig) -> PhpType {
    let Some(variadic_name) = sig.variadic.as_ref() else {
        return PhpType::Mixed;
    };
    sig.params
        .iter()
        .find(|(name, _)| name == variadic_name)
        .map(|(_, ty)| match ty.codegen_repr() {
            PhpType::Array(elem_ty) => *elem_ty,
            other => other,
        })
        .unwrap_or(PhpType::Mixed)
}

/// Returns the runtime array type used for a variadic parameter slot.
fn variadic_array_type(sig: &FunctionSig) -> PhpType {
    let Some(variadic_name) = sig.variadic.as_ref() else {
        return PhpType::Array(Box::new(PhpType::Mixed));
    };
    sig.params
        .iter()
        .find(|(name, _)| name == variadic_name)
        .map(|(_, ty)| match ty.codegen_repr() {
            PhpType::Array(elem_ty) => PhpType::Array(elem_ty),
            other => PhpType::Array(Box::new(other)),
        })
        .unwrap_or_else(|| PhpType::Array(Box::new(PhpType::Mixed)))
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
    ctx.emit_value(
        Op::MixedBox,
        vec![value.value],
        None,
        PhpType::Mixed,
        Op::MixedBox.default_effects(),
        Some(span),
    )
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
fn call_return_type(
    ctx: &LoweringContext<'_, '_>,
    name: &str,
    operands: &[crate::ir::ValueId],
) -> PhpType {
    let php_type = if let Some(sig) = ctx.functions.get(name) {
        eir_user_function_return_type(sig)
    } else if let Some(sig) = ctx.extern_functions.get(name) {
        sig.return_type.clone()
    } else if let Some(php_type) = builtin_return_type_override(name) {
        php_type
    } else if let Some(php_type) = pointer_builtin_return_type(ctx, name, operands) {
        php_type
    } else if let Some(php_type) = numeric_builtin_return_type(ctx, name, operands) {
        php_type
    } else if let Some(php_type) = pathinfo_builtin_return_type(name, operands) {
        php_type
    } else if let Some(php_type) = regex_builtin_return_type(name) {
        php_type
    } else if let Some(php_type) = array_builtin_return_type(ctx, name, operands) {
        php_type
    } else if let Some(sig) = first_class_builtin_signature(name) {
        sig.return_type
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

/// Returns argument-sensitive builtin result metadata when AST operands are still available.
fn call_return_type_for_args(
    ctx: &LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    operands: &[crate::ir::ValueId],
) -> Option<PhpType> {
    match php_symbol_key(name.trim_start_matches('\\')).as_str() {
        "array_fill" => array_fill_builtin_return_type_for_args(ctx, args, operands),
        "array_map" => array_map_builtin_return_type(ctx, args, operands),
        "iterator_to_array" => iterator_to_array_builtin_return_type(ctx, args, operands),
        "microtime" => microtime_builtin_return_type_for_args(args),
        _ => None,
    }
}

/// Returns `microtime()` metadata when the literal `as_float` flag is still available.
///
/// `microtime(true)` is a float; `microtime()` / `microtime(false)` is the "0.NNNNNNNN sec"
/// string; a non-literal flag returns `None` so the result type falls back to the `string|float`
/// union (boxed `Mixed`) declared in `call_return_type`. This must match the checker
/// (`src/types/checker/builtins/system.rs`) and the EIR backend dispatch in `lower_microtime`.
fn microtime_builtin_return_type_for_args(args: &[Expr]) -> Option<PhpType> {
    match args.first() {
        Some(arg) => match &arg.kind {
            ExprKind::BoolLiteral(true) => Some(PhpType::Float),
            ExprKind::BoolLiteral(false) => Some(PhpType::Str),
            _ => None,
        },
        None => Some(PhpType::Str),
    }
}

/// Returns `array_fill()` metadata when the literal start expression is still available.
fn array_fill_builtin_return_type_for_args(
    ctx: &LoweringContext<'_, '_>,
    args: &[Expr],
    operands: &[crate::ir::ValueId],
) -> Option<PhpType> {
    if args.len() != 3 {
        return None;
    }
    let value = operands.get(2)?;
    let value_ty = ctx.builder.value_php_type(*value).codegen_repr();
    let start_is_literal_zero = matches!(args[0].kind, ExprKind::IntLiteral(0));
    // A non-literal-zero start builds a keyed Mixed-valued hash (`__rt_array_fill_assoc`,
    // keys start..start+count-1). A literal-zero start builds the 0-indexed path: string
    // values use the dedicated 16-byte-slot `__rt_array_fill_str` helper, scalars use the
    // single-word `__rt_array_fill` / `__rt_array_fill_refcounted` helpers. This must match
    // the checker (`src/types/checker/builtins/arrays.rs`) and `infer_local_type`.
    if !start_is_literal_zero {
        return Some(PhpType::AssocArray {
            key: Box::new(PhpType::Int),
            value: Box::new(PhpType::Mixed),
        });
    }
    Some(PhpType::Array(Box::new(array_fill_indexed_element_type(value_ty))))
}

/// Returns the EIR result metadata for `array_map()` when a callable param signature is known.
fn array_map_builtin_return_type(
    ctx: &LoweringContext<'_, '_>,
    args: &[Expr],
    operands: &[crate::ir::ValueId],
) -> Option<PhpType> {
    if args.len() != 2 {
        return None;
    }
    let callback_sig = callable_expr_signature(ctx, &args[0])?;
    let return_ty = normalize_value_php_type(callback_sig.return_type.codegen_repr());
    if return_ty == PhpType::Mixed {
        return None;
    }
    let array = operands.get(1)?;
    match ctx.builder.value_php_type(*array).codegen_repr() {
        PhpType::Array(_) => Some(PhpType::Array(Box::new(return_ty))),
        _ => None,
    }
}

/// Returns the EIR result metadata for `iterator_to_array()` when preserve_keys is static.
fn iterator_to_array_builtin_return_type(
    ctx: &LoweringContext<'_, '_>,
    args: &[Expr],
    operands: &[crate::ir::ValueId],
) -> Option<PhpType> {
    let source = operands.first()?;
    let preserve_keys = match args.get(1) {
        Some(arg) => static_preserve_keys_expr(arg),
        None => Some(true),
    };
    preserve_keys
        .map(|value| {
            iterator_to_array_static_return_type(
                &ctx.builder.value_php_type(*source).codegen_repr(),
                value,
            )
        })
        .or(Some(PhpType::Mixed))
}

/// Computes the concrete `iterator_to_array()` container type for one preserve_keys value.
fn iterator_to_array_static_return_type(source_ty: &PhpType, preserve_keys: bool) -> PhpType {
    match source_ty.codegen_repr() {
        PhpType::Array(elem_ty) => PhpType::Array(elem_ty),
        PhpType::AssocArray { key, value } if preserve_keys => PhpType::AssocArray { key, value },
        PhpType::AssocArray { value, .. } => PhpType::Array(value),
        _ if preserve_keys => PhpType::AssocArray {
            key: Box::new(PhpType::Mixed),
            value: Box::new(PhpType::Mixed),
        },
        _ => PhpType::Array(Box::new(PhpType::Mixed)),
    }
}

/// Evaluates literal PHP truthiness used by static `iterator_to_array()` preserve_keys.
fn static_preserve_keys_expr(expr: &Expr) -> Option<bool> {
    match &expr.kind {
        ExprKind::BoolLiteral(value) => Some(*value),
        ExprKind::IntLiteral(value) => Some(*value != 0),
        ExprKind::FloatLiteral(value) => Some(*value != 0.0),
        ExprKind::StringLiteral(value) => Some(!value.is_empty() && value != "0"),
        ExprKind::Null => Some(false),
        ExprKind::Negate(inner) => match &inner.kind {
            ExprKind::IntLiteral(value) => Some(*value != 0),
            ExprKind::FloatLiteral(value) => Some(*value != 0.0),
            _ => None,
        },
        _ => None,
    }
}

/// Resolves callable expression metadata tracked during type checking and lowering.
fn callable_expr_signature<'a>(
    ctx: &'a LoweringContext<'_, '_>,
    callback: &Expr,
) -> Option<&'a FunctionSig> {
    match &callback.kind {
        ExprKind::Variable(name) => ctx.callable_param_signature(name),
        _ => None,
    }
}

/// Returns precise return metadata for pointer-extension builtins.
fn pointer_builtin_return_type(
    ctx: &LoweringContext<'_, '_>,
    name: &str,
    operands: &[crate::ir::ValueId],
) -> Option<PhpType> {
    match php_symbol_key(name.trim_start_matches('\\')).as_str() {
        "ptr" => Some(PhpType::Pointer(None)),
        "ptr_null" => Some(PhpType::Pointer(None)),
        "ptr_is_null" => Some(PhpType::Bool),
        "ptr_get" | "ptr_read8" | "ptr_read16" | "ptr_read32" | "ptr_sizeof" => {
            Some(PhpType::Int)
        }
        "ptr_read_string" => Some(PhpType::Str),
        "ptr_set" | "ptr_write8" | "ptr_write16" | "ptr_write32" => Some(PhpType::Void),
        "ptr_write_string" => Some(PhpType::Int),
        "ptr_offset" => {
            let pointer = operands.first()?;
            match ctx.builder.value_php_type(*pointer).codegen_repr() {
                PhpType::Pointer(tag) => Some(PhpType::Pointer(tag)),
                _ => Some(PhpType::Pointer(None)),
            }
        }
        _ => None,
    }
}

/// Returns precise return metadata for numeric builtins whose result depends on operands.
fn numeric_builtin_return_type(
    ctx: &LoweringContext<'_, '_>,
    name: &str,
    operands: &[crate::ir::ValueId],
) -> Option<PhpType> {
    match php_symbol_key(name.trim_start_matches('\\')).as_str() {
        "abs" => {
            let value = operands.first()?;
            let ty = ctx.builder.value_php_type(*value).codegen_repr();
            Some(abs_builtin_return_type(&ty))
        }
        "min" | "max" => {
            let mut saw_float = false;
            for value in operands {
                match ctx.builder.value_php_type(*value).codegen_repr() {
                    PhpType::Float => saw_float = true,
                    PhpType::Int | PhpType::Bool => {}
                    _ => return Some(PhpType::Mixed),
                }
            }
            Some(if saw_float {
                PhpType::Float
            } else {
                PhpType::Int
            })
        }
        "clamp" => {
            let mut saw_float = false;
            let mut all_int = operands.len() == 3;
            let mut all_string = operands.len() == 3;
            let mut all_numeric = operands.len() == 3;
            for value in operands.iter().take(3) {
                match ctx.builder.value_php_type(*value).codegen_repr() {
                    PhpType::Int => {
                        all_string = false;
                    }
                    PhpType::Float => {
                        saw_float = true;
                        all_int = false;
                        all_string = false;
                    }
                    PhpType::Str => {
                        all_int = false;
                        all_numeric = false;
                    }
                    _ => {
                        all_int = false;
                        all_string = false;
                        all_numeric = false;
                    }
                }
            }
            if all_string {
                Some(PhpType::Str)
            } else if all_int {
                Some(PhpType::Int)
            } else if all_numeric {
                Some(if saw_float {
                    PhpType::Float
                } else {
                    PhpType::Int
                })
            } else {
                Some(PhpType::Mixed)
            }
        }
        _ => None,
    }
}

/// Returns the EIR storage type for `abs()` after operand-sensitive narrowing.
fn abs_builtin_return_type(ty: &PhpType) -> PhpType {
    match ty {
        PhpType::Float => PhpType::Float,
        PhpType::Mixed | PhpType::Union(_) => PhpType::Mixed,
        _ => PhpType::Int,
    }
}

/// Returns EIR result metadata for `pathinfo()` based on argument shape.
fn pathinfo_builtin_return_type(name: &str, operands: &[crate::ir::ValueId]) -> Option<PhpType> {
    if php_symbol_key(name.trim_start_matches('\\')).as_str() != "pathinfo" {
        return None;
    }
    if operands.len() == 1 {
        return Some(PhpType::AssocArray {
            key: Box::new(PhpType::Str),
            value: Box::new(PhpType::Str),
        });
    }
    Some(PhpType::Mixed)
}

/// Returns precise EIR result metadata for regex builtins lowered by `codegen_ir`.
fn regex_builtin_return_type(name: &str) -> Option<PhpType> {
    match php_symbol_key(name.trim_start_matches('\\')).as_str() {
        "preg_match" | "preg_match_all" => Some(PhpType::Int),
        "preg_replace" => Some(PhpType::Str),
        "preg_split" => Some(PhpType::Array(Box::new(PhpType::Mixed))),
        _ => None,
    }
}

/// Returns precise return metadata for array builtins that preserve operand element type.
fn array_builtin_return_type(
    ctx: &LoweringContext<'_, '_>,
    name: &str,
    operands: &[crate::ir::ValueId],
) -> Option<PhpType> {
    match php_symbol_key(name.trim_start_matches('\\')).as_str() {
        "array_combine" => array_combine_builtin_return_type(ctx, operands),
        "array_column" => array_column_builtin_return_type(ctx, operands),
        "array_flip" => array_flip_builtin_return_type(ctx, operands),
        "array_fill" => array_fill_builtin_return_type(ctx, operands),
        "array_fill_keys" => array_fill_keys_builtin_return_type(ctx, operands),
        "array_merge" => array_merge_builtin_return_type(ctx, operands),
        "array_splice" | "array_filter" | "array_diff" | "array_intersect" | "array_diff_key"
        | "array_intersect_key" => array_preserve_first_builtin_return_type(ctx, operands),
        "in_array" => Some(PhpType::Bool),
        "range" => Some(PhpType::Array(Box::new(PhpType::Int))),
        "array_values" => {
            let array = operands.first()?;
            match ctx.builder.value_php_type(*array).codegen_repr() {
                PhpType::Array(elem) => Some(PhpType::Array(elem)),
                PhpType::AssocArray { value, .. } => Some(PhpType::Array(value)),
                other => Some(other),
            }
        }
        "array_reverse" | "array_unique" | "array_pad" => {
            let array = operands.first()?;
            match ctx.builder.value_php_type(*array).codegen_repr() {
                PhpType::Array(elem) => Some(PhpType::Array(elem)),
                other => Some(other),
            }
        }
        "array_chunk" => {
            let array = operands.first()?;
            match ctx.builder.value_php_type(*array).codegen_repr() {
                PhpType::Array(elem) => Some(PhpType::Array(Box::new(PhpType::Array(elem)))),
                other => Some(other),
            }
        }
        _ => None,
    }
}

/// Returns precise return metadata for `array_fill(start, count, value)`.
fn array_fill_builtin_return_type(
    ctx: &LoweringContext<'_, '_>,
    operands: &[crate::ir::ValueId],
) -> Option<PhpType> {
    let value = operands.get(2)?;
    let value_ty = ctx.builder.value_php_type(*value).codegen_repr();
    Some(PhpType::Array(Box::new(array_fill_indexed_element_type(value_ty))))
}

/// Returns the indexed element storage type for EIR `array_fill()` results.
fn array_fill_indexed_element_type(value_ty: PhpType) -> PhpType {
    match value_ty.codegen_repr() {
        PhpType::Void | PhpType::Never => PhpType::Mixed,
        other => other,
    }
}

/// Returns the extracted column element type for `array_column()`.
fn array_column_builtin_return_type(
    ctx: &LoweringContext<'_, '_>,
    operands: &[crate::ir::ValueId],
) -> Option<PhpType> {
    let array = operands.first()?;
    match ctx.builder.value_php_type(*array).codegen_repr() {
        PhpType::Array(inner) => match inner.codegen_repr() {
            PhpType::AssocArray { value, .. } => Some(PhpType::Array(value)),
            other => Some(other),
        },
        other => Some(other),
    }
}

/// Returns precise return metadata for array builtins that preserve the first operand type.
fn array_preserve_first_builtin_return_type(
    ctx: &LoweringContext<'_, '_>,
    operands: &[crate::ir::ValueId],
) -> Option<PhpType> {
    let first = operands.first()?;
    Some(ctx.builder.value_php_type(*first).codegen_repr())
}

/// Returns precise return metadata for `array_fill_keys(keys, value)`.
fn array_fill_keys_builtin_return_type(
    ctx: &LoweringContext<'_, '_>,
    operands: &[crate::ir::ValueId],
) -> Option<PhpType> {
    let keys = operands.first()?;
    let value = operands.get(1)?;
    let key_ty = match ctx.builder.value_php_type(*keys).codegen_repr() {
        PhpType::Array(elem) => array_key_type_from_value_type(elem.codegen_repr()),
        _ => return None,
    };
    let value_ty = ctx.builder.value_php_type(*value).codegen_repr();
    Some(PhpType::AssocArray {
        key: Box::new(key_ty),
        value: Box::new(value_ty),
    })
}

/// Returns precise return metadata for `array_flip(array)`.
fn array_flip_builtin_return_type(
    ctx: &LoweringContext<'_, '_>,
    operands: &[crate::ir::ValueId],
) -> Option<PhpType> {
    let array = operands.first()?;
    match ctx.builder.value_php_type(*array).codegen_repr() {
        PhpType::Array(value) => Some(PhpType::AssocArray {
            key: Box::new(array_key_type_from_value_type(value.codegen_repr())),
            value: Box::new(PhpType::Int),
        }),
        PhpType::AssocArray { key, value } => Some(PhpType::AssocArray {
            key: Box::new(array_key_type_from_value_type(value.codegen_repr())),
            value: key,
        }),
        _ => None,
    }
}

/// Returns precise return metadata for `array_combine(keys, values)`.
fn array_combine_builtin_return_type(
    ctx: &LoweringContext<'_, '_>,
    operands: &[crate::ir::ValueId],
) -> Option<PhpType> {
    let keys = operands.first()?;
    let values = operands.get(1)?;
    let key_ty = match ctx.builder.value_php_type(*keys).codegen_repr() {
        PhpType::Array(elem) => array_key_type_from_value_type(elem.codegen_repr()),
        _ => return None,
    };
    let value_ty = match ctx.builder.value_php_type(*values).codegen_repr() {
        PhpType::Array(elem) => elem.codegen_repr(),
        _ => return None,
    };
    Some(PhpType::AssocArray {
        key: Box::new(key_ty),
        value: Box::new(value_ty),
    })
}

/// Returns precise return metadata for `array_merge()`.
///
/// Empty indexed arrays lower as `Array<Void>`; when that is the first operand, the merged
/// array inherits the second operand's element metadata so later indexed reads materialize
/// real payload values instead of void sentinels.
fn array_merge_builtin_return_type(
    ctx: &LoweringContext<'_, '_>,
    operands: &[crate::ir::ValueId],
) -> Option<PhpType> {
    let first = operands.first()?;
    let first_ty = ctx.builder.value_php_type(*first).codegen_repr();
    let second_ty = operands
        .get(1)
        .map(|value| ctx.builder.value_php_type(*value).codegen_repr());
    match first_ty {
        PhpType::Array(elem) if is_empty_array_element_type(elem.as_ref()) => match second_ty {
            Some(PhpType::Array(right)) if is_scalar_merge_element_type(right.as_ref()) => {
                Some(PhpType::Array(right))
            }
            _ => Some(PhpType::Array(elem)),
        },
        PhpType::Array(elem) => Some(PhpType::Array(elem)),
        other => Some(other),
    }
}

/// Returns true for the element sentinel used by statically empty indexed arrays.
fn is_empty_array_element_type(ty: &PhpType) -> bool {
    matches!(ty.codegen_repr(), PhpType::Void)
}

/// Returns true for element types copied safely by the scalar merge runtime helper.
fn is_scalar_merge_element_type(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Int | PhpType::Bool | PhpType::Float | PhpType::Callable | PhpType::Void
    )
}

/// Returns precise builtin return types needed by EIR value materialization.
fn builtin_return_type_override(name: &str) -> Option<PhpType> {
    match php_symbol_key(name.trim_start_matches('\\')).as_str() {
        "chdir" | "checkdate" | "chgrp" | "chmod" | "chown" | "lchgrp" | "lchown"
        | "class_alias" | "class_exists" | "copy" | "define" | "defined"
        | "empty" | "file_exists" | "fnmatch" | "function_exists" | "is_a" | "is_callable"
        | "is_array" | "is_object" | "is_scalar"
        | "fdatasync" | "fflush" | "flock" | "fsync" | "ftruncate" | "interface_exists" | "is_dir"
        | "is_executable" | "is_file" | "is_link" | "is_numeric" | "link" | "mkdir" | "rename"
        | "enum_exists" | "trait_exists" | "putenv" | "rmdir" | "is_readable"
        | "is_subclass_of" | "is_writeable" | "is_writable" | "settype"
        | "is_resource" | "hash_equals" | "hash_update" | "spl_autoload_register"
        | "spl_autoload_unregister" | "stream_context_set_option" | "stream_context_set_params"
        | "stream_filter_register" | "stream_filter_remove" | "__elephc_phar_set_compression"
        | "stream_wrapper_register" | "stream_wrapper_restore" | "stream_wrapper_unregister"
        | "stream_isatty" | "stream_is_local" | "stream_set_blocking" | "stream_set_timeout"
        | "stream_socket_enable_crypto" | "stream_socket_shutdown" | "stream_supports_lock" | "symlink" | "touch"
        | "unlink" => {
            Some(PhpType::Bool)
        }
        "basename" | "date" | "gmdate" | "dirname" | "exec" | "get_class" | "get_parent_class"
        | "getcwd" | "getenv" | "gethostname" | "gethostbyname" | "php_uname"
        | "readline" | "shell_exec" | "sys_get_temp_dir"
        | "fread" | "get_resource_type" | "gzcompress" | "gzdeflate" | "hash" | "hash_final" | "hash_hmac" | "long2ip"
        | "stream_get_line" | "system" | "spl_autoload_extensions" | "tempnam" | "vsprintf" => {
            Some(PhpType::Str)
        }
        "disk_free_space" | "disk_total_space" => Some(PhpType::Float),
        "clearstatcache" | "closedir" | "exit" | "die" | "passthru" | "rewinddir"
        | "stream_bucket_append" | "stream_bucket_prepend" | "unset" => Some(PhpType::Void),
        "fclose" | "feof" | "rewind" => Some(PhpType::Bool),
        "printf" | "array_rand" | "array_unshift" | "file_put_contents" | "filemtime"
        | "filesize" | "fprintf" | "fpassthru" | "fputcsv" | "fseek" | "ftell" | "fwrite"
        | "crc32" | "get_resource_id" | "isset" | "linkinfo" | "mktime" | "gmmktime" | "sleep"
        | "__elephc_mktime_raw" | "__elephc_gmmktime_raw"
        | "pclose" | "spl_object_id" | "stream_select" | "stream_set_chunk_size"
        | "stream_set_read_buffer" | "stream_set_write_buffer"
        | "__elephc_strtotime_raw" | "time"
        | "umask" | "vfprintf" | "vprintf" | "realpath_cache_size" => {
            Some(PhpType::Int)
        }
        // strtotime() is `int|false`: a real timestamp (including a valid -1 pre-epoch) on success,
        // or boolean false when the string cannot be parsed. The backend boxes the result so
        // `=== false` and `echo` observe the distinct false; `__elephc_strtotime_raw` (the DateTime
        // internal alias above) stays a plain Int that maps the failure sentinel to -1.
        "strtotime" => Some(PhpType::Union(vec![PhpType::Int, PhpType::Bool])),
        // microtime() with a non-literal `as_float` flag yields `string|float` (boxed `Mixed`):
        // the runtime branches on the flag and boxes either the "0.NNNNNNNN sec" string or the
        // float. Literal-true / literal-false / omitted cases are resolved earlier by
        // `call_return_type_for_args` (Float / Str), so this entry is only reached for a
        // non-literal flag.
        "microtime" => Some(PhpType::Union(vec![PhpType::Str, PhpType::Float])),
        "spl_object_hash" => Some(PhpType::Str),
        "spl_autoload" | "spl_autoload_call" | "usleep" => Some(PhpType::Void),
        "stream_context_create" | "stream_context_get_default" | "stream_context_set_default" => {
            Some(PhpType::stream_resource())
        }
        "realpath_cache_get" | "stream_context_get_options" | "stream_context_get_params"
        | "stream_get_meta_data" => Some(PhpType::AssocArray {
            key: Box::new(PhpType::Str),
            value: Box::new(PhpType::Mixed),
        }),
        "getdate" | "localtime" | "hrtime" | "file_get_contents" | "fileatime" | "filectime" | "filegroup" | "fileinode"
        | "fileowner" | "fileperms" | "filetype" | "readfile" | "readlink" | "realpath"
        | "fgetc" | "fgets" | "fopen" | "fstat" | "hash_copy" | "hash_file" | "hash_init"
        | "gethostbyaddr" | "getprotobyname" | "getprotobynumber" | "getservbyname"
        | "getservbyport" | "fsockopen" | "inet_ntop" | "inet_pton" | "ip2long" | "opendir"
        | "pfsockopen" | "readdir" | "popen" | "stat" | "lstat" | "stream_get_contents"
        | "stream_bucket_make_writeable" | "stream_bucket_new" | "stream_filter_append"
        | "stream_filter_prepend" | "stream_resolve_include_path" | "stream_socket_accept"
        | "stream_socket_client" | "stream_socket_pair" | "stream_copy_to_stream"
        | "stream_socket_get_name" | "stream_socket_recvfrom" | "stream_socket_sendto"
        | "stream_socket_server" | "tmpfile" | "gzinflate" | "gzuncompress" | "strpos" | "strrpos" => {
            Some(PhpType::Mixed)
        }
        "spl_autoload_functions" => Some(PhpType::Array(Box::new(PhpType::Int))),
        "__elephc_phar_list_entries" | "class_attribute_names" | "explode" | "fgetcsv"
        | "file" | "get_declared_classes" | "fscanf" | "get_declared_interfaces"
        | "get_declared_traits" | "glob" | "hash_algos" | "scandir" | "spl_classes"
        | "str_split" | "stream_get_filters" | "stream_get_transports" | "stream_get_wrappers"
        | "sscanf" => {
            Some(PhpType::Array(Box::new(PhpType::Str)))
        }
        "class_attribute_args" => Some(PhpType::Array(Box::new(PhpType::Mixed))),
        "class_get_attributes" => Some(PhpType::Array(Box::new(PhpType::Object(
            "ReflectionAttribute".to_string(),
        )))),
        _ => None,
    }
}

/// Lowers an indexed array literal.
fn lower_array_literal(ctx: &mut LoweringContext<'_, '_>, items: &[Expr], expr: &Expr) -> LoweredValue {
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
    for item in items {
        if let ExprKind::Spread(inner) = &item.kind {
            let source = lower_expr(ctx, inner);
            lower_indexed_array_spread_into_array(ctx, array, source, elem_ty.as_ref(), item.span);
            continue;
        }
        let value = lower_expr(ctx, item);
        ctx.emit_void(Op::ArrayPush, vec![array.value, value.value], None, Op::ArrayPush.default_effects(), Some(item.span));
        super::stmt::release_indexed_array_write_operand(ctx, elem_ty.as_ref(), value, item.span);
    }
    array
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

/// Returns the element/value type for an array-access expression used inside a literal.
fn array_access_expr_value_type_for_ir(
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
fn property_access_expr_type_for_ir(
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
    let result_type = fallback_expr_type(expr);
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
fn lower_array_access(ctx: &mut LoweringContext<'_, '_>, array: &Expr, index: &Expr, expr: &Expr) -> LoweredValue {
    let array_value = lower_expr(ctx, array);
    if value_is_nullable(ctx, array_value.value) {
        return lower_nullable_array_access(ctx, array_value, index, expr);
    }
    lower_array_access_from_value(ctx, array_value, index, expr)
}

/// Lowers array access once the receiver is already evaluated.
fn lower_array_access_from_value(
    ctx: &mut LoweringContext<'_, '_>,
    array_value: LoweredValue,
    index: &Expr,
    expr: &Expr,
) -> LoweredValue {
    let mut index_value = lower_expr(ctx, index);
    let op = match array_value.ir_type {
        IrType::Heap(IrHeapKind::Array) => {
            index_value = coerce_to_int_at_span(ctx, index_value, Some(index.span));
            Op::ArrayGet
        }
        IrType::Heap(IrHeapKind::Hash) => Op::HashGet,
        IrType::Heap(IrHeapKind::Buffer) => Op::BufferGet,
        IrType::Str => {
            index_value = coerce_to_int_at_span(ctx, index_value, Some(index.span));
            Op::StrCharAt
        }
        _ => Op::RuntimeCall,
    };
    let result_type = array_access_result_type(ctx, array_value.value, op, expr);
    ctx.emit_value(
        op,
        vec![array_value.value, index_value.value],
        None,
        result_type,
        op.default_effects(),
        Some(expr.span),
    )
}

/// Lowers nullable receiver indexing without evaluating the index on a null receiver.
fn lower_nullable_array_access(
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
    let result_type = PhpType::Mixed;
    let temp_name = ctx.declare_owned_hidden_temp(result_type.clone());
    let null_block = ctx.builder.create_named_block("nullable.index.null", Vec::new());
    let read_block = ctx.builder.create_named_block("nullable.index.read", Vec::new());
    let merge = ctx.builder.create_named_block("nullable.index.merge", Vec::new());
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
    let read_value = lower_array_access_from_value(ctx, array_value, index, expr);
    store_value_into_temp(ctx, &temp_name, result_type, read_value, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    take_owned_temp(ctx, &temp_name, expr.span)
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
        Op::ArrayGet => match ctx.builder.value_php_type(array).codegen_repr() {
            PhpType::Array(elem_ty) => {
                array_access_element_result_type(normalize_value_php_type(*elem_ty))
            }
            _ => fallback_expr_type(expr),
        },
        Op::HashGet => match ctx.builder.value_php_type(array).codegen_repr() {
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
        _ => match ctx.builder.value_php_type(array).codegen_repr() {
            PhpType::Mixed | PhpType::Union(_) => PhpType::Mixed,
            _ => fallback_expr_type(expr),
        },
    }
}

/// Returns the materialized result type for a PHP array read, including miss-capable int reads.
fn array_access_element_result_type(element_ty: PhpType) -> PhpType {
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
        release_stringified_source_if_owned(ctx, value, Some(expr.span));
    }
    result
}

/// Releases an owned source whose string result cannot alias the original storage.
fn release_stringified_source_if_owned(
    ctx: &mut LoweringContext<'_, '_>,
    source: LoweredValue,
    span: Option<crate::span::Span>,
) {
    if !ctx.value_is_owning_temporary(source) {
        return;
    }
    match ctx.builder.value_php_type(source.value).codegen_repr() {
        PhpType::Object(_) | PhpType::Array(_) | PhpType::AssocArray { .. } => {
            crate::ir_lower::ownership::release_if_owned(ctx, source, span);
        }
        _ => {}
    }
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
    return_type: Option<&TypeExpr>,
    body: &[crate::parser::ast::Stmt],
    captures: &[String],
    capture_refs: &[String],
    expr: &Expr,
) -> LoweredValue {
    lower_closure_with_context(
        ctx,
        params,
        variadic,
        return_type,
        body,
        captures,
        capture_refs,
        expr,
        &[],
        None,
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
        return_type,
        body,
        captures,
        capture_refs,
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
        return_type.as_ref(),
        body,
        captures,
        capture_refs,
        value,
        &[],
        Some(assigned_name),
    ))
}

/// Lowers a closure expression, applying contextual types to unannotated parameters.
fn lower_closure_with_context(
    ctx: &mut LoweringContext<'_, '_>,
    params: &[(String, Option<TypeExpr>, Option<Expr>, bool)],
    variadic: Option<&str>,
    return_type: Option<&TypeExpr>,
    body: &[crate::parser::ast::Stmt],
    captures: &[String],
    capture_refs: &[String],
    expr: &Expr,
    contextual_arg_types: &[PhpType],
    self_ref_callable_capture: Option<&str>,
) -> LoweredValue {
    let mut captured_values = Vec::with_capacity(captures.len());
    let mut capture_params = Vec::with_capacity(captures.len());
    for capture in captures {
        let by_ref = capture_refs.iter().any(|name| name == capture);
        let captured = ctx.load_local(capture, Some(expr.span));
        let php_type = if by_ref && self_ref_callable_capture == Some(capture.as_str()) {
            PhpType::Callable
        } else {
            ctx.builder.value_php_type(captured.value)
        };
        let immediate = by_ref.then_some(Immediate::I64(1));
        ctx.emit_void(Op::ClosureCapture, vec![captured.value], immediate, Op::ClosureCapture.default_effects(), Some(expr.span));
        captured_values.push(ClosureCapture { value: captured.value });
        capture_params.push((capture.clone(), php_type, by_ref));
    }
    let name = ctx.next_closure_name();
    let signature = if contextual_arg_types.is_empty() {
        function::lower_closure_function(
            ctx,
            &name,
            params,
            variadic,
            return_type,
            body,
            &capture_params,
            self_ref_callable_capture,
        )
    } else {
        function::lower_closure_function_with_context(
            ctx,
            &name,
            params,
            variadic,
            return_type,
            body,
            &capture_params,
            contextual_arg_types,
            self_ref_callable_capture,
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
    ctx.emit_value(
        Op::ClosureNew,
        closure_operands,
        Some(Immediate::Data(data)),
        PhpType::Callable,
        Op::ClosureNew.default_effects(),
        Some(expr.span),
    )
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
        if let Some(arg_container) = lower_untyped_descriptor_invoker_arg_container(ctx, args, expr.span) {
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
    let lowered_callee = lower_expr(ctx, callee);
    let result_type = dynamic_callable_result_type(ctx, lowered_callee.value, expr);
    if let Some(arg_container) = lower_untyped_descriptor_invoker_arg_container(ctx, args, expr.span) {
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
    if let Some(arg_container) = lower_untyped_descriptor_invoker_arg_container(ctx, args, expr.span) {
        return emit_callable_descriptor_invoke(
            ctx,
            callee,
            arg_container,
            result_type,
            expr.span,
        );
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
    ctx.emit_value(
        Op::MixedBox,
        vec![hash.value],
        None,
        PhpType::Mixed,
        Op::MixedBox.default_effects(),
        Some(span),
    )
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
    ctx.emit_value(
        Op::MixedBox,
        vec![value.value],
        None,
        PhpType::Mixed,
        Op::MixedBox.default_effects(),
        Some(span),
    )
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
            let arg_container = lower_untyped_descriptor_invoker_arg_container(ctx, args, expr.span)?;
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
fn lower_new_object(ctx: &mut LoweringContext<'_, '_>, class_name: &Name, args: &[Expr], expr: &Expr) -> LoweredValue {
    let sig = constructor_signature(ctx, class_name).cloned();
    let operands = lower_args_with_signature(ctx, sig.as_ref(), args);
    let php_type = PhpType::Object(class_name.as_str().to_string());
    let data = ctx.intern_class_name(class_name.as_str());
    ctx.emit_value(
        Op::ObjectNew,
        operands,
        Some(Immediate::Data(data)),
        php_type,
        Op::ObjectNew.default_effects(),
        Some(expr.span),
    )
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
    ctx.emit_value(
        op,
        vec![object.value],
        Some(Immediate::Data(data)),
        result_type,
        op.default_effects(),
        Some(expr.span),
    )
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
    let Some((_, property_ty)) = class_info.properties.iter().find(|(name, _)| name == property) else {
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
    ctx.emit_value(
        Op::DynamicPropGet,
        vec![object.value, property.value],
        None,
        result_type,
        Op::DynamicPropGet.default_effects(),
        Some(expr.span),
    )
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
    let Some((class_name, nullable)) = singular_object_class(&object_ty) else {
        return fallback_expr_type(expr);
    };
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
    expr: &Expr,
) -> PhpType {
    let Some(class_name) = static_receiver_class_name(ctx, receiver) else {
        return fallback_expr_type(expr);
    };
    let Some(class_info) = ctx.classes.get(class_name.as_str()) else {
        return fallback_expr_type(expr);
    };
    let Some((_, property_ty)) = class_info
        .static_properties
        .iter()
        .find(|(name, _)| name == property)
    else {
        return fallback_expr_type(expr);
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
    let object_expr = object;
    let object = lower_expr(ctx, object_expr);
    if op == Op::MethodCall && value_is_definitely_null(ctx, object.value) {
        let null_value = lower_null(ctx, expr);
        terminate_method_call_on_null(ctx, method);
        return null_value;
    }
    if op == Op::MethodCall && value_is_nullable(ctx, object.value) {
        return lower_nullable_regular_method_call(ctx, object, method, args, expr);
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
    release_owned_call_arg_temporaries(ctx, &arg_values, Some(call.value), expr.span);
    release_owning_receiver_temporary(ctx, object, expr.span);
    call
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
    let fatal_block = ctx.builder.create_named_block("method.null.fatal", Vec::new());
    let call_block = ctx.builder.create_named_block("method.non_null.call", Vec::new());
    let merge = ctx.builder.create_named_block("method.nullable.merge", Vec::new());
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

/// Emits the PHP fatal terminator for an ordinary method call on null.
fn terminate_method_call_on_null(ctx: &mut LoweringContext<'_, '_>, method: &str) {
    let message = format!("Fatal error: Call to a member function {}() on null\n", method);
    let message = ctx.intern_string(&message);
    ctx.builder.terminate(Terminator::Fatal { message });
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
        ctx.emit_value(
            Op::MixedBox,
            vec![null_value.value],
            None,
            result_type.clone(),
            Op::MixedBox.default_effects(),
            Some(expr.span),
        )
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
    release_owned_call_arg_temporaries(ctx, &arg_values, Some(call.value), expr.span);
    release_owning_receiver_temporary(ctx, object, expr.span);
    call
}

/// Releases normalized call arguments that cannot be returned by this call.
fn release_owned_call_arg_temporaries(
    ctx: &mut LoweringContext<'_, '_>,
    args: &[crate::ir::ValueId],
    result: Option<crate::ir::ValueId>,
    span: Span,
) {
    for value in args {
        let php_type = ctx.builder.value_php_type(*value);
        let lowered = LoweredValue {
            value: *value,
            ir_type: value_ir_type(&php_type),
        };
        if ctx.value_is_owning_temporary(lowered) {
            if call_result_may_alias_arg(ctx, *value, result) {
                continue;
            }
            crate::ir_lower::ownership::release_if_owned(ctx, lowered, Some(span));
        }
    }
}

/// Returns true when a call result can legally be the same refcounted payload as an argument.
fn call_result_may_alias_arg(
    ctx: &LoweringContext<'_, '_>,
    arg: crate::ir::ValueId,
    result: Option<crate::ir::ValueId>,
) -> bool {
    let Some(result) = result else {
        return false;
    };
    if ctx.builder.value_defining_op(arg) == Some(Op::MixedNumericBinop) {
        return false;
    }
    let arg_ty = ctx.builder.value_php_type(arg).codegen_repr();
    let result_ty = ctx.builder.value_php_type(result).codegen_repr();
    if !Ownership::php_type_needs_lifetime_tracking(&arg_ty)
        || !Ownership::php_type_needs_lifetime_tracking(&result_ty)
    {
        return false;
    }
    match (&arg_ty, &result_ty) {
        (PhpType::Mixed | PhpType::Union(_), _) | (_, PhpType::Mixed | PhpType::Union(_)) => true,
        (PhpType::Object(_), PhpType::Object(_)) => true,
        (PhpType::Array(_), PhpType::Array(_)) => true,
        (
            PhpType::AssocArray { .. },
            PhpType::AssocArray { .. } | PhpType::Array(_) | PhpType::Iterable,
        ) => true,
        (PhpType::Str, PhpType::Str) => true,
        (PhpType::Callable, PhpType::Callable) => true,
        (PhpType::Buffer(_), PhpType::Buffer(_)) => true,
        _ => arg_ty == result_ty,
    }
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
        return common_dynamic_method_signature(ctx, &key);
    }
    None
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
    if op == Op::NullsafeMethodCall && nullable {
        nullable_result_type(return_ty)
    } else {
        return_ty
    }
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
    let sig = static_method_implementation_signature(ctx, receiver, method)
        .or_else(|| lexical_instance_static_call_signature(ctx, receiver, method))
        .cloned();
    let operands = lower_args_with_signature(ctx, sig.as_ref(), args);
    let name = format!("{}::{}", receiver_name(receiver), method);
    let data = ctx.intern_string(&name);
    let result_type = sig
        .as_ref()
        .map(|signature| normalize_value_php_type(signature.return_type.codegen_repr()))
        .unwrap_or_else(|| fallback_expr_type(expr));
    ctx.emit_value(
        Op::StaticMethodCall,
        operands,
        Some(Immediate::Data(data)),
        result_type,
        Op::StaticMethodCall.default_effects(),
        Some(expr.span),
    )
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
    if let Some(value) = ctx.scoped_constant_value(&class_name, name) {
        return lower_expr(ctx, &value);
    }
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

/// Returns the class name to use for a scoped constant lookup.
fn scoped_constant_receiver_name(ctx: &LoweringContext<'_, '_>, receiver: &StaticReceiver) -> String {
    match receiver {
        StaticReceiver::Static => receiver_name(receiver),
        _ => static_receiver_class_name(ctx, receiver).unwrap_or_else(|| receiver_name(receiver)),
    }
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
    let data = ctx.intern_class_name(&name);
    ctx.emit_value(
        Op::ObjectNew,
        operands,
        Some(Immediate::Data(data)),
        PhpType::Object(name),
        Op::ObjectNew.default_effects(),
        Some(expr.span),
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
        ctx.builder
            .terminate(Terminator::Br { target: header, args: Vec::new() });
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
        ctx.builder
            .terminate(Terminator::Br { target: header, args: Vec::new() });
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
        _ => ctx.emit_value(
            Op::Cast,
            vec![value.value],
            Some(Immediate::CastTarget(IrType::I64)),
            PhpType::Int,
            Op::Cast.default_effects(),
            span,
        ),
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
        _ => ctx.emit_value(
            Op::Cast,
            vec![value.value],
            Some(Immediate::CastTarget(IrType::F64)),
            PhpType::Float,
            Op::Cast.default_effects(),
            span,
        ),
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
            release_stringified_source_if_owned(ctx, value, span);
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
        ExprKind::ShortTernary { value, default } => {
            let value_ty = materialized_expr_type_for_merge(ctx, value).codegen_repr();
            let default_ty = materialized_expr_type_for_merge(ctx, default).codegen_repr();
            wider_type_for_merge(&value_ty, &default_ty)
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
    match target_ty {
        PhpType::Mixed => ctx.emit_value(
            Op::MixedBox,
            vec![value.value],
            None,
            PhpType::Mixed,
            Op::MixedBox.default_effects(),
            Some(span),
        ),
        PhpType::Int | PhpType::Bool | PhpType::Void | PhpType::Never => {
            coerce_to_int_at_span(ctx, value, Some(span))
        }
        PhpType::Float => coerce_to_float_at_span(ctx, value, Some(span)),
        PhpType::Str => coerce_to_string_at_span(ctx, value, Some(span)),
        _ => value,
    }
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
