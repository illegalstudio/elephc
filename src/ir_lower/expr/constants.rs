//! Purpose:
//! Lowers PHP global constant references and static constant-introspection calls
//! encountered while converting AST expressions into EIR.
//!
//! Called from:
//! - `crate::ir_lower::expr::lower_expr()` and the function-call lowering path.
//!
//! Key details:
//! - `define("NAME", value)` updates the per-function lowering context in source
//!   order so later `ConstRef` expressions can keep precise PHP metadata.

use crate::ir::{Immediate, Op, Ownership};
use crate::ir_lower::context::{value_ir_type, LoweredValue, LoweringContext};
use crate::names::{php_symbol_key, Name};
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

/// Records `define("NAME", value)` constants for later source-order references.
pub(super) fn register_static_define_call(
    ctx: &mut LoweringContext<'_, '_>,
    name: &Name,
    args: &[Expr],
) {
    if php_symbol_key(name.as_str().trim_start_matches('\\')) != "define" || args.len() != 2 {
        return;
    }
    let ExprKind::StringLiteral(constant_name) = &args[0].kind else {
        return;
    };
    ctx.register_constant(
        constant_name.clone(),
        args[1].kind.clone(),
        constant_expr_type(&args[1].kind),
    );
}

/// Lowers `defined("NAME")` to a compile-time boolean when the name is literal.
pub(super) fn lower_static_defined_call(
    ctx: &mut LoweringContext<'_, '_>,
    name: &Name,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if php_symbol_key(name.as_str().trim_start_matches('\\')) != "defined" || args.len() != 1 {
        return None;
    }
    let ExprKind::StringLiteral(constant_name) = &args[0].kind else {
        return None;
    };
    let exists = ctx.constant_value(constant_name).is_some();
    if !exists && ctx.has_eval_barrier() {
        let data = ctx.intern_global_name(constant_name);
        return Some(ctx.emit_value(
            Op::EvalConstantExists,
            Vec::new(),
            Some(Immediate::Data(data)),
            PhpType::Bool,
            Op::EvalConstantExists.default_effects(),
            Some(expr.span),
        ));
    }
    Some(emit_typed_constant(
        ctx,
        Op::ConstBool,
        Some(Immediate::Bool(exists)),
        PhpType::Bool,
        expr,
    ))
}

/// Lowers a constant reference through prescanned metadata or global storage fallback.
pub(super) fn lower_const_ref(
    ctx: &mut LoweringContext<'_, '_>,
    name: &Name,
    expr: &Expr,
) -> LoweredValue {
    if let Some((value, php_type)) = ctx.constant_value(name.as_str()) {
        return lower_constant_value(ctx, value, php_type, expr);
    }
    if ctx.has_eval_barrier() {
        let data = ctx.intern_global_name(name.as_str());
        return ctx.emit_value(
            Op::EvalConstantFetch,
            Vec::new(),
            Some(Immediate::Data(data)),
            PhpType::Mixed,
            Op::EvalConstantFetch.default_effects(),
            Some(expr.span),
        );
    }
    let data = ctx.intern_global_name(name.as_str());
    ctx.emit_value(
        Op::LoadGlobal,
        Vec::new(),
        Some(Immediate::GlobalName(data)),
        super::fallback_expr_type(expr),
        Op::LoadGlobal.default_effects(),
        Some(expr.span),
    )
}

/// Lowers a prescanned constant value using its checker-visible PHP type.
fn lower_constant_value(
    ctx: &mut LoweringContext<'_, '_>,
    value: ExprKind,
    php_type: PhpType,
    expr: &Expr,
) -> LoweredValue {
    match value {
        ExprKind::IntLiteral(value) => emit_typed_constant(
            ctx,
            Op::ConstI64,
            Some(Immediate::I64(value)),
            php_type,
            expr,
        ),
        ExprKind::FloatLiteral(value) => emit_typed_constant(
            ctx,
            Op::ConstF64,
            Some(Immediate::F64(value)),
            php_type,
            expr,
        ),
        ExprKind::StringLiteral(value) => {
            let data = ctx.intern_string(&value);
            emit_typed_constant(ctx, Op::ConstStr, Some(Immediate::Data(data)), php_type, expr)
        }
        ExprKind::BoolLiteral(value) => emit_typed_constant(
            ctx,
            Op::ConstBool,
            Some(Immediate::Bool(value)),
            php_type,
            expr,
        ),
        ExprKind::Null => emit_typed_constant(ctx, Op::ConstNull, None, php_type, expr),
        other => {
            let synthetic = Expr::new(other, expr.span);
            super::lower_expr(ctx, &synthetic)
        }
    }
}

/// Emits a literal constant opcode with caller-supplied PHP metadata.
fn emit_typed_constant(
    ctx: &mut LoweringContext<'_, '_>,
    op: Op,
    immediate: Option<Immediate>,
    php_type: PhpType,
    expr: &Expr,
) -> LoweredValue {
    let ir_type = value_ir_type(&php_type);
    let value = ctx
        .builder
        .emit_with_effects(
            op,
            Vec::new(),
            immediate,
            ir_type,
            php_type.clone(),
            Ownership::for_php_type(&php_type),
            op.default_effects(),
            Some(expr.span),
        )
        .expect("constant opcode produces a value");
    LoweredValue { value, ir_type }
}

/// Returns the PHP type used for a compile-time constant expression.
fn constant_expr_type(kind: &ExprKind) -> PhpType {
    match kind {
        ExprKind::IntLiteral(_) => PhpType::Int,
        ExprKind::FloatLiteral(_) => PhpType::Float,
        ExprKind::StringLiteral(_) => PhpType::Str,
        ExprKind::BoolLiteral(false) => PhpType::False,
        ExprKind::BoolLiteral(true) => PhpType::Bool,
        ExprKind::Null => PhpType::Void,
        _ => PhpType::Int,
    }
}
