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
    BlockId, CmpPredicate, Effects, Immediate, IrHeapKind, IrType, Op, Ownership, Terminator,
};
use crate::ir_lower::context::{value_ir_type, LoweredValue, LoweringContext};
use crate::ir_lower::effects_lookup;
use crate::names::{php_symbol_key, Name};
use crate::parser::ast::{
    BinOp, CallableTarget, CastType, Expr, ExprKind, InstanceOfTarget, MagicConstant,
    StaticReceiver,
};
use crate::types::{
    array_key_type_from_value_type, checker::infer_expr_type_syntactic,
    merge_array_key_types, normalized_array_key_type, PhpType,
};

mod constants;

/// Lowers an expression and returns its EIR value.
pub(crate) fn lower_expr(ctx: &mut LoweringContext<'_, '_>, expr: &Expr) -> LoweredValue {
    match &expr.kind {
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
            conditional_value_temp: _,
        } => lower_assignment_expr(ctx, target, value, result_target.as_deref(), prelude, expr),
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
        ExprKind::Closure { captures, .. } => lower_closure(ctx, captures, expr),
        ExprKind::NamedArg { value, .. } => lower_expr(ctx, value),
        ExprKind::Spread(inner) => lower_expr(ctx, inner),
        ExprKind::ClosureCall { var, args } => lower_closure_call(ctx, var, args, expr),
        ExprKind::ExprCall { callee, args } => lower_expr_call(ctx, callee, args, expr),
        ExprKind::ConstRef(name) => constants::lower_const_ref(ctx, name, expr),
        ExprKind::NewObject { class_name, args } => lower_new_object(ctx, class_name, args, expr),
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
            lower_method_call(ctx, object, method, args, Op::NullsafeMethodCall, expr)
        }
        ExprKind::StaticMethodCall { receiver, method, args } => {
            lower_static_method_call(ctx, receiver, method, args, expr)
        }
        ExprKind::FirstClassCallable(target) => lower_first_class_callable(ctx, target, expr),
        ExprKind::This => ctx.load_local("this", Some(expr.span)),
        ExprKind::PtrCast { target_type, expr: inner } => lower_ptr_cast(ctx, target_type, inner, expr),
        ExprKind::BufferNew { element_type: _, len } => lower_buffer_new(ctx, len, expr),
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
    ctx.emit_value(
        Op::MixedNumericBinop,
        vec![lhs.value, rhs.value],
        None,
        PhpType::Mixed,
        Op::MixedNumericBinop.default_effects(),
        Some(expr.span),
    )
}

/// Lowers string concatenation.
fn lower_concat(ctx: &mut LoweringContext<'_, '_>, left: &Expr, right: &Expr, expr: &Expr) -> LoweredValue {
    let lhs = lower_expr(ctx, left);
    let lhs = coerce_to_string(ctx, lhs, expr);
    let rhs = lower_expr(ctx, right);
    let rhs = coerce_to_string(ctx, rhs, expr);
    if lhs.ir_type == IrType::Str && rhs.ir_type == IrType::Str {
        return ctx.emit_value(
            Op::StrConcat,
            vec![lhs.value, rhs.value],
            None,
            PhpType::Str,
            Op::StrConcat.default_effects(),
            Some(expr.span),
        );
    }
    ctx.emit_value(
        Op::RuntimeCall,
        vec![lhs.value, rhs.value],
        None,
        PhpType::Str,
        effects_lookup::runtime_effects(),
        Some(expr.span),
    )
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
    let opcode = match op {
        BinOp::StrictEq => Op::StrictEq,
        BinOp::StrictNotEq => Op::StrictNotEq,
        BinOp::Eq => Op::LooseEq,
        BinOp::NotEq => Op::LooseNotEq,
        BinOp::Spaceship => Op::Spaceship,
        _ if lhs.ir_type == IrType::F64 || rhs.ir_type == IrType::F64 => Op::FCmp,
        _ if lhs.ir_type == IrType::I64 && rhs.ir_type == IrType::I64 => Op::ICmp,
        _ if lhs.ir_type == IrType::Str && rhs.ir_type == IrType::Str => Op::StrCmp,
        _ => Op::LooseEq,
    };
    if matches!(opcode, Op::FCmp) {
        lhs = coerce_to_float(ctx, lhs, left);
        rhs = coerce_to_float(ctx, rhs, right);
    }
    let immediate = if matches!(opcode, Op::ICmp | Op::FCmp) {
        Some(Immediate::CmpPredicate(cmp_predicate(op)))
    } else {
        None
    };
    let php_type = if matches!(op, BinOp::Spaceship) { PhpType::Int } else { PhpType::Bool };
    ctx.emit_value(
        opcode,
        vec![lhs.value, rhs.value],
        immediate,
        php_type,
        opcode.default_effects(),
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
        _ => ctx.emit_value(Op::RuntimeCall, vec![value.value], None, PhpType::Mixed, Effects::all(), Some(expr.span)),
    }
}

/// Lowers an integer unary operation.
fn lower_int_unary(ctx: &mut LoweringContext<'_, '_>, inner: &Expr, op: Op, expr: &Expr) -> LoweredValue {
    let value = lower_expr(ctx, inner);
    if value.ir_type == IrType::I64 {
        ctx.emit_value(op, vec![value.value], None, PhpType::Int, op.default_effects(), Some(expr.span))
    } else {
        ctx.emit_value(Op::RuntimeCall, vec![value.value], None, PhpType::Mixed, Effects::all(), Some(expr.span))
    }
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
    ctx.load_local(&temp_name, Some(expr.span))
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
    let result_type = fallback_expr_type(expr);
    let temp_name = ctx.declare_hidden_temp(result_type.clone());
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
    store_expr_into_temp(ctx, &temp_name, result_type.clone(), default, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(value_block);
    store_value_into_temp(ctx, &temp_name, result_type, value, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    ctx.load_local(&temp_name, Some(expr.span))
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
    let temp_name = ctx.declare_hidden_temp(result_type.clone());
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
    store_value_into_temp(ctx, &temp_name, result_type.clone(), value, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(default_block);
    store_expr_into_temp(ctx, &temp_name, result_type, default, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    ctx.load_local(&temp_name, Some(expr.span))
}

/// Lowers a pipe operation.
fn lower_pipe(ctx: &mut LoweringContext<'_, '_>, value: &Expr, callable: &Expr, expr: &Expr) -> LoweredValue {
    let value = lower_expr(ctx, value);
    let callable = lower_expr(ctx, callable);
    ctx.emit_value(
        Op::PipeCall,
        vec![value.value, callable.value],
        None,
        fallback_expr_type(expr),
        Op::PipeCall.default_effects(),
        Some(expr.span),
    )
}

/// Lowers an assignment expression.
fn lower_assignment_expr(
    ctx: &mut LoweringContext<'_, '_>,
    target: &Expr,
    value: &Expr,
    result_target: Option<&Expr>,
    prelude: &[crate::parser::ast::Stmt],
    expr: &Expr,
) -> LoweredValue {
    for stmt in prelude {
        crate::ir_lower::stmt::lower_stmt(ctx, stmt);
    }
    let lowered = lower_expr(ctx, value);
    if let ExprKind::Variable(name) = &target.kind {
        let php_type = ctx.builder.value_php_type(lowered.value);
        ctx.store_local(name, lowered, php_type, Some(expr.span));
    }
    if let Some(result_target) = result_target {
        return lower_expr(ctx, result_target);
    }
    lowered
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
    let operands = lower_args(ctx, args);
    let canonical = name.as_str();
    let php_type = call_return_type(ctx, canonical, &operands);
    if ctx.extern_functions.contains_key(canonical) {
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
    if ctx.functions.contains_key(canonical) {
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
    let data = ctx.intern_function_name(canonical);
    ctx.emit_value(
        Op::BuiltinCall,
        operands,
        Some(Immediate::Data(data)),
        php_type,
        effects_lookup::builtin_effects(canonical),
        Some(expr.span),
    )
}

/// Lowers positional/named/spread call arguments in source order.
fn lower_args(ctx: &mut LoweringContext<'_, '_>, args: &[Expr]) -> Vec<crate::ir::ValueId> {
    args.iter().map(|arg| lower_expr(ctx, arg).value).collect()
}

/// Returns the best available return type for a function-like call.
fn call_return_type(
    ctx: &LoweringContext<'_, '_>,
    name: &str,
    operands: &[crate::ir::ValueId],
) -> PhpType {
    let php_type = if let Some(php_type) = builtin_return_type_override(name) {
        php_type
    } else if let Some(php_type) = numeric_builtin_return_type(ctx, name, operands) {
        php_type
    } else if let Some(php_type) = array_builtin_return_type(ctx, name, operands) {
        php_type
    } else if let Some(sig) = ctx.functions.get(name) {
        sig.return_type.clone()
    } else if let Some(sig) = ctx.extern_functions.get(name) {
        sig.return_type.clone()
    } else if let Some(sig) = crate::types::first_class_callable_builtin_sig(name) {
        sig.return_type
    } else if let Some(sig) = crate::types::builtin_call_sig(name) {
        sig.return_type
    } else {
        PhpType::Mixed
    };
    normalize_value_php_type(php_type)
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
            Some(if ty == PhpType::Float {
                PhpType::Float
            } else {
                PhpType::Int
            })
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

/// Returns precise return metadata for array builtins that preserve operand element type.
fn array_builtin_return_type(
    ctx: &LoweringContext<'_, '_>,
    name: &str,
    operands: &[crate::ir::ValueId],
) -> Option<PhpType> {
    match php_symbol_key(name.trim_start_matches('\\')).as_str() {
        "array_combine" => array_combine_builtin_return_type(ctx, operands),
        "array_flip" => array_flip_builtin_return_type(ctx, operands),
        "array_fill_keys" => array_fill_keys_builtin_return_type(ctx, operands),
        "array_merge" => array_merge_builtin_return_type(ctx, operands),
        "array_diff" | "array_intersect" => array_preserve_first_builtin_return_type(ctx, operands),
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
        "define" | "defined" | "empty" | "function_exists" | "is_callable" | "is_numeric" => {
            Some(PhpType::Bool)
        }
        "printf" | "array_rand" | "array_unshift" => Some(PhpType::Int),
        "strpos" | "strrpos" => Some(PhpType::Mixed),
        "explode" | "str_split" | "sscanf" => Some(PhpType::Array(Box::new(PhpType::Str))),
        _ => None,
    }
}

/// Lowers an indexed array literal.
fn lower_array_literal(ctx: &mut LoweringContext<'_, '_>, items: &[Expr], expr: &Expr) -> LoweredValue {
    let array = ctx.emit_value(
        Op::ArrayNew,
        Vec::new(),
        Some(Immediate::Capacity(items.len() as u32)),
        fallback_expr_type(expr),
        Op::ArrayNew.default_effects(),
        Some(expr.span),
    );
    for item in items {
        let value = lower_expr(ctx, item);
        ctx.emit_void(Op::ArrayPush, vec![array.value, value.value], None, Op::ArrayPush.default_effects(), Some(item.span));
    }
    array
}

/// Lowers an associative array literal.
fn lower_assoc_array_literal(ctx: &mut LoweringContext<'_, '_>, pairs: &[(Expr, Expr)], expr: &Expr) -> LoweredValue {
    let hash = ctx.emit_value(
        Op::HashNew,
        Vec::new(),
        Some(Immediate::Capacity(pairs.len() as u32)),
        assoc_array_literal_type_for_ir(pairs, expr),
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
fn assoc_array_literal_type_for_ir(pairs: &[(Expr, Expr)], expr: &Expr) -> PhpType {
    if pairs.is_empty() {
        return fallback_expr_type(expr);
    }
    let mut key_ty = normalized_array_key_type(
        &pairs[0].0,
        infer_expr_type_syntactic(&pairs[0].0),
    );
    let mut value_ty = infer_expr_type_syntactic(&pairs[0].1).codegen_repr();
    for (key, value) in pairs.iter().skip(1) {
        key_ty = merge_array_key_types(
            key_ty,
            normalized_array_key_type(key, infer_expr_type_syntactic(key)),
        );
        value_ty = merge_ir_assoc_value_type(
            value_ty,
            infer_expr_type_syntactic(value).codegen_repr(),
        );
    }
    PhpType::AssocArray {
        key: Box::new(key_ty),
        value: Box::new(value_ty),
    }
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
    let temp_name = ctx.declare_hidden_temp(result_type.clone());
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
        let message = ctx.intern_string("unhandled match expression");
        ctx.builder.terminate(Terminator::Fatal { message });
    }
    ctx.builder.position_at_end(merge);
    ctx.load_local(&temp_name, Some(expr.span))
}

/// Lowers array, hash, string, or ArrayAccess indexing.
fn lower_array_access(ctx: &mut LoweringContext<'_, '_>, array: &Expr, index: &Expr, expr: &Expr) -> LoweredValue {
    let array_value = lower_expr(ctx, array);
    let index_value = lower_expr(ctx, index);
    let op = match array_value.ir_type {
        IrType::Heap(IrHeapKind::Array) => Op::ArrayGet,
        IrType::Heap(IrHeapKind::Hash) => Op::HashGet,
        IrType::Str if index_value.ir_type == IrType::I64 => Op::StrCharAt,
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
            PhpType::Array(elem_ty) => normalize_value_php_type(*elem_ty),
            _ => fallback_expr_type(expr),
        },
        Op::HashGet => match ctx.builder.value_php_type(array).codegen_repr() {
            PhpType::AssocArray { value, .. } => normalize_value_php_type(*value),
            _ => fallback_expr_type(expr),
        },
        _ => fallback_expr_type(expr),
    }
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
    let result_type = fallback_expr_type(expr);
    let temp_name = ctx.declare_hidden_temp(result_type.clone());
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
    store_expr_into_temp(ctx, &temp_name, result_type.clone(), then_expr, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(else_block);
    store_expr_into_temp(ctx, &temp_name, result_type, else_expr, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    ctx.load_local(&temp_name, Some(expr.span))
}

/// Lowers a cast expression.
fn lower_cast(ctx: &mut LoweringContext<'_, '_>, target: &CastType, inner: &Expr, expr: &Expr) -> LoweredValue {
    let value = lower_expr(ctx, inner);
    let php_type = cast_php_type(target);
    ctx.emit_value(
        Op::Cast,
        vec![value.value],
        Some(Immediate::CastTarget(value_ir_type(&php_type))),
        php_type,
        Op::Cast.default_effects(),
        Some(expr.span),
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

/// Lowers a closure expression.
fn lower_closure(ctx: &mut LoweringContext<'_, '_>, captures: &[String], expr: &Expr) -> LoweredValue {
    for capture in captures {
        let captured = ctx.load_local(capture, Some(expr.span));
        ctx.emit_void(Op::ClosureCapture, vec![captured.value], None, Op::ClosureCapture.default_effects(), Some(expr.span));
    }
    ctx.emit_value(Op::ClosureNew, Vec::new(), None, PhpType::Callable, Op::ClosureNew.default_effects(), Some(expr.span))
}

/// Lowers a closure variable call.
fn lower_closure_call(ctx: &mut LoweringContext<'_, '_>, var: &str, args: &[Expr], expr: &Expr) -> LoweredValue {
    let mut operands = vec![ctx.load_local(var, Some(expr.span)).value];
    operands.extend(lower_args(ctx, args));
    ctx.emit_value(Op::ClosureCall, operands, None, fallback_expr_type(expr), Op::ClosureCall.default_effects(), Some(expr.span))
}

/// Lowers an expression call.
fn lower_expr_call(ctx: &mut LoweringContext<'_, '_>, callee: &Expr, args: &[Expr], expr: &Expr) -> LoweredValue {
    let mut operands = vec![lower_expr(ctx, callee).value];
    operands.extend(lower_args(ctx, args));
    ctx.emit_value(Op::ExprCall, operands, None, fallback_expr_type(expr), Op::ExprCall.default_effects(), Some(expr.span))
}

/// Lowers fixed-class object construction.
fn lower_new_object(ctx: &mut LoweringContext<'_, '_>, class_name: &Name, args: &[Expr], expr: &Expr) -> LoweredValue {
    for arg in args {
        lower_expr(ctx, arg);
    }
    let php_type = PhpType::Object(class_name.as_str().to_string());
    let data = ctx.intern_class_name(class_name.as_str());
    ctx.emit_value(
        Op::ObjectNew,
        Vec::new(),
        Some(Immediate::Data(data)),
        php_type,
        Op::ObjectNew.default_effects(),
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

/// Lowers an object property read.
fn lower_property_get(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
    op: Op,
    expr: &Expr,
) -> LoweredValue {
    let object = lower_expr(ctx, object);
    let data = ctx.intern_string(property);
    ctx.emit_value(
        op,
        vec![object.value],
        Some(Immediate::Data(data)),
        fallback_expr_type(expr),
        op.default_effects(),
        Some(expr.span),
    )
}

/// Lowers a dynamic property read.
fn lower_dynamic_property_get(ctx: &mut LoweringContext<'_, '_>, object: &Expr, property: &Expr, expr: &Expr) -> LoweredValue {
    let object = lower_expr(ctx, object);
    let property = lower_expr(ctx, property);
    ctx.emit_value(
        Op::DynamicPropGet,
        vec![object.value, property.value],
        None,
        fallback_expr_type(expr),
        Op::DynamicPropGet.default_effects(),
        Some(expr.span),
    )
}

/// Lowers a static property read.
fn lower_static_property_get(ctx: &mut LoweringContext<'_, '_>, receiver: &StaticReceiver, property: &str, expr: &Expr) -> LoweredValue {
    let name = format!("{}::{}", receiver_name(receiver), property);
    let data = ctx.intern_string(&name);
    ctx.emit_value(
        Op::LoadStaticProperty,
        Vec::new(),
        Some(Immediate::Data(data)),
        fallback_expr_type(expr),
        Op::LoadStaticProperty.default_effects(),
        Some(expr.span),
    )
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
    let mut operands = vec![lower_expr(ctx, object).value];
    operands.extend(lower_args(ctx, args));
    let data = ctx.intern_string(method);
    ctx.emit_value(
        op,
        operands,
        Some(Immediate::Data(data)),
        fallback_expr_type(expr),
        op.default_effects(),
        Some(expr.span),
    )
}

/// Lowers a static method call.
fn lower_static_method_call(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    method: &str,
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let operands = lower_args(ctx, args);
    let name = format!("{}::{}", receiver_name(receiver), method);
    let data = ctx.intern_string(&name);
    ctx.emit_value(
        Op::StaticMethodCall,
        operands,
        Some(Immediate::Data(data)),
        fallback_expr_type(expr),
        Op::StaticMethodCall.default_effects(),
        Some(expr.span),
    )
}

/// Lowers first-class callable creation.
fn lower_first_class_callable(ctx: &mut LoweringContext<'_, '_>, target: &CallableTarget, expr: &Expr) -> LoweredValue {
    if let CallableTarget::Method { object, .. } = target {
        lower_expr(ctx, object);
    }
    let data = ctx.intern_string(&callable_target_name(target));
    ctx.emit_value(
        Op::FirstClassCallableNew,
        Vec::new(),
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
fn lower_buffer_new(ctx: &mut LoweringContext<'_, '_>, len: &Expr, expr: &Expr) -> LoweredValue {
    lower_expr(ctx, len);
    ctx.emit_value(
        Op::BufferNew,
        Vec::new(),
        None,
        fallback_expr_type(expr),
        Op::BufferNew.default_effects(),
        Some(expr.span),
    )
}

/// Lowers `::class`.
fn lower_class_constant(ctx: &mut LoweringContext<'_, '_>, receiver: &StaticReceiver, expr: &Expr) -> LoweredValue {
    let name = receiver_name(receiver);
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
    let key = format!("{}::{}", receiver_name(receiver), name);
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
    for arg in args {
        lower_expr(ctx, arg);
    }
    let name = receiver_name(receiver);
    let data = ctx.intern_class_name(&name);
    ctx.emit_value(
        Op::ObjectNew,
        Vec::new(),
        Some(Immediate::Data(data)),
        fallback_expr_type(expr),
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
fn lower_yield_from(ctx: &mut LoweringContext<'_, '_>, inner: &Expr, expr: &Expr) -> LoweredValue {
    let value = lower_expr(ctx, inner);
    ctx.emit_value(
        Op::GeneratorYieldFrom,
        vec![value.value],
        None,
        PhpType::Mixed,
        Op::GeneratorYieldFrom.default_effects(),
        Some(expr.span),
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
        InstanceOfTarget::Name(name) => Some(Immediate::Data(ctx.intern_class_name(name.as_str()))),
        InstanceOfTarget::Expr(expr) => {
            operands.push(lower_expr(ctx, expr).value);
            None
        }
    };
    let op = if immediate.is_some() { Op::InstanceOf } else { Op::InstanceOfDynamic };
    ctx.emit_value(op, operands, immediate, PhpType::Bool, op.default_effects(), Some(expr.span))
}

/// Coerces a value to integer storage before integer-only operations.
fn coerce_to_int(ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, expr: &Expr) -> LoweredValue {
    match value.ir_type {
        IrType::I64 => value,
        IrType::F64 => ctx.emit_value(Op::FToI, vec![value.value], None, PhpType::Int, Op::FToI.default_effects(), Some(expr.span)),
        IrType::Str => ctx.emit_value(Op::StrToI, vec![value.value], None, PhpType::Int, Op::StrToI.default_effects(), Some(expr.span)),
        _ => ctx.emit_value(
            Op::Cast,
            vec![value.value],
            Some(Immediate::CastTarget(IrType::I64)),
            PhpType::Int,
            Op::Cast.default_effects(),
            Some(expr.span),
        ),
    }
}

/// Coerces a value to float when the storage type allows a direct conversion.
fn coerce_to_float(ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, expr: &Expr) -> LoweredValue {
    match value.ir_type {
        IrType::F64 => value,
        IrType::I64 => ctx.emit_value(Op::IToF, vec![value.value], None, PhpType::Float, Op::IToF.default_effects(), Some(expr.span)),
        _ => ctx.emit_value(Op::RuntimeCall, vec![value.value], None, PhpType::Float, Effects::all(), Some(expr.span)),
    }
}

/// Coerces a value to string when possible.
fn coerce_to_string(ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, expr: &Expr) -> LoweredValue {
    match value.ir_type {
        IrType::Str => value,
        IrType::I64 => ctx.emit_value(Op::IToStr, vec![value.value], None, PhpType::Str, Op::IToStr.default_effects(), Some(expr.span)),
        IrType::F64 => ctx.emit_value(Op::FToStr, vec![value.value], None, PhpType::Str, Op::FToStr.default_effects(), Some(expr.span)),
        _ => ctx.emit_value(Op::RuntimeCall, vec![value.value], None, PhpType::Str, Effects::all(), Some(expr.span)),
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
    ctx.store_local(temp_name, value, temp_type, Some(span));
}

/// Emits a branch to a target block when the current block can still fall through.
fn branch_to(ctx: &mut LoweringContext<'_, '_>, target: BlockId) {
    if !ctx.builder.insertion_block_is_terminated() {
        ctx.builder.terminate(Terminator::Br { target, args: Vec::new() });
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
