//! Purpose:
//! Shares internal-object operator dispatch for boxed operands and scalar counterparts.
//!
//! Called from:
//! - Dynamic object expression lowering records ordinary EIR calls to these helpers.
//! - Program lowering materializes the requested helpers after source bodies are complete.
//!
//! Key details:
//! - Each helper uses the original expression lowerer, including its scalar fallback.
//! - Borrowed parameters preserve operand order and callers retain their original cleanup.
//! - Synthetic helpers are absent from PHP declaration and instrumentation inventories.

use crate::ir::{IrHeapKind, IrType, Module, Op, Immediate};
use crate::ir_lower::context::{LoweredValue, LoweringContext};
use crate::parser::ast::{BinOp, Expr, ExprKind};
use crate::synthetic_class::*;
use crate::types::{FunctionSig, PhpType};

const PREFIX: &str = "__eir_boxed_operator@";

/// Maps the operators supported by internal object contracts to stable helper names.
fn operations() -> [(BinOp, &'static str); 20] {
    [
        (BinOp::Add, "add"), (BinOp::Sub, "sub"), (BinOp::Mul, "mul"),
        (BinOp::Div, "div"), (BinOp::Mod, "mod"), (BinOp::Pow, "pow"),
        (BinOp::BitAnd, "bitand"), (BinOp::BitOr, "bitor"), (BinOp::BitXor, "bitxor"),
        (BinOp::ShiftLeft, "shl"), (BinOp::ShiftRight, "shr"),
        (BinOp::Eq, "eq"), (BinOp::NotEq, "ne"), (BinOp::Lt, "lt"),
        (BinOp::LtEq, "le"), (BinOp::Gt, "gt"), (BinOp::GtEq, "ge"),
        (BinOp::Spaceship, "cmp"),
        (BinOp::Add, "inc"), (BinOp::Sub, "dec"),
    ]
}

/// Boxes scalar counterparts and calls a shared graph without changing source evaluation.
pub(super) fn lower_call(
    ctx: &mut LoweringContext<'_, '_>, lhs: LoweredValue, op: &BinOp,
    rhs: LoweredValue, expr: &Expr, result_type: PhpType, consume: bool,
) -> Option<LoweredValue> {
    let name = &ctx.builder.function().name;
    // Codegen creates builtin wrappers after the module's helpers have been sealed.
    // Their bodies retain the inline path, as does the helper's own implementation.
    let mixed = IrType::Heap(IrHeapKind::Mixed);
    let supports = |value: LoweredValue| matches!(value.ir_type,
        IrType::I64 | IrType::F64 | IrType::Str | IrType::Void) || value.ir_type == mixed;
    if name.starts_with(PREFIX) || name.starts_with("_eir_")
        || (lhs.ir_type != mixed && rhs.ir_type != mixed)
        || !supports(lhs) || !supports(rhs)
    { return None; }
    // Increment has PHP string/bool/null rules distinct from adding one. Its caller still
    // selects the post-form's old value; the shared graph must compute the new value.
    let suffix = match expr.kind {
        ExprKind::PreIncrement(_) | ExprKind::PostIncrement(_) => "inc",
        ExprKind::PreDecrement(_) | ExprKind::PostDecrement(_) => "dec",
        _ => operations().into_iter().find(|(candidate, _)| candidate == op)?.1,
    };
    let callee = ctx.intern_function_name(&format!("{PREFIX}{suffix}"));
    let mut boxes = Vec::new();
    let operands = [lhs, rhs].into_iter().map(|value| {
        if value.ir_type == mixed { return value.value; }
        let boxed = ctx.emit_value(Op::MixedBox, vec![value.value], None, PhpType::Mixed,
            Op::MixedBox.default_effects(), Some(expr.span));
        boxes.push(boxed);
        boxed.value
    }).collect::<Vec<_>>();
    let mut live = operands.clone();
    live.extend([lhs.value, rhs.value]);
    let pins = super::expr::pin_in_flight_owners(ctx, &live, expr.span);
    let result = if result_type == PhpType::Mixed {
        ctx.emit_owned_value(Op::Call, operands, Some(Immediate::Data(callee)), result_type,
            Op::Call.default_effects(), Some(expr.span))
    } else {
        ctx.emit_value(Op::Call, operands, Some(Immediate::Data(callee)), result_type,
            Op::Call.default_effects(), Some(expr.span))
    };
    super::expr::unpin_in_flight_owners(ctx, pins, expr.span);
    for boxed in boxes {
        super::expr::release_binary_operand_temporary(ctx, boxed, expr.span);
    }
    if consume {
        super::expr::release_binary_operand_temporary(ctx, lhs, expr.span);
        if rhs.value != lhs.value {
            super::expr::release_binary_operand_temporary(ctx, rhs, expr.span);
        }
    }
    Some(result)
}

/// Materializes only helpers referenced by completed native bodies, without recursive outlining.
pub(super) fn lower_requested(module: &mut Module) {
    for (op, suffix) in operations() {
        let name = format!("{PREFIX}{suffix}");
        if !module.data.function_names.contains(&name) { continue; }
        let result_type = match op {
            BinOp::Spaceship => PhpType::Int,
            BinOp::Eq | BinOp::NotEq | BinOp::Lt | BinOp::LtEq | BinOp::Gt | BinOp::GtEq => PhpType::Bool,
            _ => PhpType::Mixed,
        };
        let signature = FunctionSig {
            params: vec![("left".into(), PhpType::Mixed), ("right".into(), PhpType::Mixed)],
            param_type_exprs: vec![Some(t_mixed()); 2], param_attributes: vec![Vec::new(); 2],
            defaults: vec![None; 2], return_type: result_type, declared_return: true,
            by_ref_return: false, ref_params: vec![false; 2], declared_params: vec![true; 2],
            variadic: None, deprecation: None, is_generator: false,
        };
        let expression = match suffix {
            "inc" => Expr::new(ExprKind::PreIncrement("left".into()), crate::span::Span::dummy()),
            "dec" => Expr::new(ExprKind::PreDecrement("left".into()), crate::span::Span::dummy()),
            _ => e_binop(e_var("left"), op, e_var("right")),
        };
        let mut function = super::function::lower_builtin_callable_body(
            module, &name, &signature, false,
            vec![s_return(expression)],
        );
        function.flags.is_internal = true;
        function.flags.is_synthetic = true;
        module.add_function(function);
    }
}
