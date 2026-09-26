//! Purpose:
//! Lowers dynamically sized positional builtin arguments into an owned array of copied values.
//!
//! Called from:
//! - Builtin call lowering for runtime operations that preserve original PHP argument values.
//!
//! Key details:
//! - Shared call planning remains authoritative for source syntax and evaluation order.
//! - Every spread element is copied before the next source expression executes.
//! - A generic exception guard follows the argument array when appending relocates it.

use super::*;
use crate::builtins::semantics::{BuiltinArgumentLowering, BuiltinLowering, BuiltinSemanticInput};
use crate::ir::{RuntimeArgumentLayout, RuntimeCallTarget};

/// Preserves actual positional arity for a shared runtime builtin with indexed spread arguments.
pub(super) fn lower_packed_builtin_call(
    ctx: &mut LoweringContext<'_, '_>, name: &str, sig: Option<&FunctionSig>,
    args: &[Expr], expr: &Expr,
) -> Option<LoweredValue> {
    let def = crate::builtins::registry::lookup(name)?;
    if def.spec.semantics.argument_lowering != BuiltinArgumentLowering::PreserveValues
        || !args.iter().any(is_spread_arg) || crate::types::call_args::has_named_args(args) {
        return None;
    }
    let BuiltinLowering::Runtime(RuntimeCallTarget::Function(target)) = def.spec.semantics.lowering else { return None; };
    if !target.uses_mbstring_runtime() { return None; }
    let sig = sig?;
    if sig.ref_params.iter().any(|by_ref| *by_ref) { return None; }
    let plan = crate::types::call_args::plan_call_args(sig, args, expr.span, true, false).ok()?;
    for arg in &plan.source_args {
        match &arg.kind {
            ExprKind::Spread(inner) => { indexed_spread_source_type(ctx, inner)?; }
            ExprKind::NamedArg { .. } => return None,
            _ => {},
        }
    }
    let result_type = registry_builtin_result_type(ctx, name, args, &[], expr.span)
        .unwrap_or_else(|| def.return_type.clone());
    let effects = crate::builtins::semantics::resolve_builtin_effects(def,
        &BuiltinSemanticInput { name: def.name, args, arg_types: &[], span: expr.span });
    ctx.begin_argument_guard_scope();
    let array = ctx.emit_owned_value(Op::ArrayNew, Vec::new(), Some(Immediate::Capacity(4)),
        PhpType::Array(Box::new(PhpType::Mixed)), Op::ArrayNew.default_effects(), Some(expr.span));
    ctx.guard_call_argument(array, 0, expr.span);
    for arg in &plan.source_args {
        if let ExprKind::Spread(inner) = &arg.kind {
            let source = lower_expr(ctx, inner);
            append_spread(ctx, array, source, arg.span);
        } else {
            let source = lower_expr(ctx, arg);
            append_value(ctx, array, source, arg.span);
        }
    }
    ctx.end_argument_guard_scope();
    let call = ctx.emit_value(Op::RuntimeCall, vec![array.value],
        Some(Immediate::RuntimeCall(RuntimeCallTarget::ProfiledFunction {
            target, arguments: RuntimeArgumentLayout::IndexedArray,
            strict_php: crate::strict_php::is_enabled(),
            strict_types: Some(crate::source::current_strict_types()),
        })), result_type, effects, Some(expr.span));
    ctx.unguard_call_argument(array.value, expr.span);
    crate::ir_lower::ownership::release_if_owned(ctx, array, Some(expr.span));
    Some(call)
}

/// Copies one PHP value into call-owned boxed storage before later source effects can replace it.
fn append_value(ctx: &mut LoweringContext<'_, '_>, array: LoweredValue, value: LoweredValue, span: Span) {
    let ty = ctx.builder.value_php_type(value.value).codegen_repr();
    let copied = if matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
        let copied = ctx.emit_owned_value(Op::MixedClone, vec![value.value], None, PhpType::Mixed,
            Op::MixedClone.default_effects(), Some(span));
        if ctx.value_is_owning_temporary(value) {
            crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
        }
        copied
    } else {
        ctx.box_value_as_mixed(value, PhpType::Mixed, Some(span))
    };
    ctx.emit_void(Op::ArrayPush, vec![array.value, copied.value], None,
        Op::ArrayPush.default_effects(), Some(span));
    ctx.refresh_argument_array_guard(array, span);
    crate::ir_lower::ownership::release_if_owned(ctx, copied, Some(span));
}

/// Appends every indexed spread element in order, keeping excess arguments for runtime validation.
fn append_spread(ctx: &mut LoweringContext<'_, '_>, array: LoweredValue, source: LoweredValue, span: Span) {
    let PhpType::Array(element) = ctx.builder.value_php_type(source.value).codegen_repr() else {
        unreachable!("indexed spread type was checked before source evaluation");
    };
    let len = ctx.emit_value(Op::ArrayLen, vec![source.value], None, PhpType::Int,
        Op::ArrayLen.default_effects(), Some(span));
    let zero = emit_i64_at_span(ctx, 0, span);
    let header = ctx.builder.create_named_block("call.pack.next", vec![(IrType::I64, PhpType::Int)]);
    let body = ctx.builder.create_named_block("call.pack.value", Vec::new());
    let exit = ctx.builder.create_named_block("call.pack.done", Vec::new());
    ctx.builder.terminate(Terminator::Br { target: header, args: vec![zero.value] });
    ctx.builder.position_at_end(header);
    let index = ctx.builder.block_param(header, 0);
    let has_next = ctx.emit_value(Op::ICmp, vec![index, len.value],
        Some(Immediate::CmpPredicate(CmpPredicate::Slt)), PhpType::Bool, Op::ICmp.default_effects(), Some(span));
    ctx.builder.terminate(Terminator::CondBr {
        cond: has_next.value, then_target: body, then_args: Vec::new(), else_target: exit, else_args: Vec::new(),
    });
    ctx.builder.position_at_end(body);
    let value = ctx.emit_value(Op::ArrayGet, vec![source.value, index], None, *element,
        Op::ArrayGet.default_effects(), Some(span));
    append_value(ctx, array, value, span);
    let one = emit_i64_at_span(ctx, 1, span);
    let next = ctx.emit_value(Op::IAdd, vec![index, one.value], None, PhpType::Int,
        Op::IAdd.default_effects(), Some(span));
    ctx.builder.terminate(Terminator::Br { target: header, args: vec![next.value] });
    ctx.builder.position_at_end(exit);
    if ctx.value_is_owning_temporary(source) {
        crate::ir_lower::ownership::release_if_owned(ctx, source, Some(span));
    }
}
