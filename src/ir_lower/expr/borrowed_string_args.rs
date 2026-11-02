//! Purpose:
//! Borrows stable subjects for bounded string reads without whole-string operand copies.
//!
//! Called from:
//! - Builtin argument lowering before the general source-evaluation owner ledger.
//!
//! Key details:
//! - Uses the shared argument planner for parameter order and defaults.
//! - Admits only string reads and exact scalar arguments that cannot run user code.
//! - Property receiver owners stay published until the builtin result is stabilized.

use super::*;

/// Lowers a proved read-only string call, leaving all other argument forms to ordinary lowering.
pub(super) fn try_lower(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    signature: Option<&FunctionSig>,
    args: &[Expr],
) -> Option<Vec<ValueId>> {
    use crate::builtins::semantics::BuiltinLowering;
    use crate::ir::{RuntimeCallTarget, RuntimeFnId};
    use crate::types::call_args::{PlannedRegularArg, PlannedSourceValue};
    if !ctx.borrowed_string_calls_enabled || ctx.has_eval_barrier() || ctx.eval_executed() { return None; }
    let def = crate::builtins::registry::lookup(name.trim_start_matches('\\'))?;
    let target = match def.spec.semantics.lowering {
        BuiltinLowering::Runtime(RuntimeCallTarget::Function(target)
            | RuntimeCallTarget::ProfiledFunction { target, .. })
            if matches!(target, RuntimeFnId::Substr | RuntimeFnId::Strpos) => target,
        _ => return None,
    };
    if args.iter().any(is_spread_arg) { return None; }
    let sig = signature?;
    let span = args.first()?.span;
    let plan = crate::types::call_args::plan_call_args(sig, args, span, true, false).ok()?;
    if plan.has_spread_args() || !plan.variadic_args.is_empty() { return None; }
    // The planner keeps a plain positional call as a passthrough. Named calls carry
    // the separate source-to-parameter mapping, including any omitted defaults.
    let positional = plan.first_named_pos.is_none();
    let mut reads = Vec::new();
    if positional {
        if plan.source_args.len() > crate::types::call_args::regular_param_count(sig) { return None; }
        for (index, expr) in plan.source_args.iter().enumerate() { reads.push((index, index, expr)); }
    } else {
        for source in &plan.source_values {
            let PlannedSourceValue::Regular { source_index, param_idx, expr } = source else { return None; };
            reads.push((*source_index, *param_idx, expr));
        }
    }
    for (_, param_idx, expr) in &reads {
        let string = *param_idx == 0 || (target == RuntimeFnId::Strpos && *param_idx == 1);
        if !safe_read(ctx, expr, string, *param_idx == 0) { return None; }
    }
    for arg in &plan.regular_args {
        if let PlannedRegularArg::Default(expr) = arg {
            if !matches!(expr.kind, ExprKind::IntLiteral(_) | ExprKind::Null) { return None; }
        }
    }
    let mut sources = vec![None; plan.source_args.len()];
    for (source_index, param_idx, expr) in reads {
        let value = if let ExprKind::PropertyAccess { object, property } = &expr.kind {
            let object = lower_expr(ctx, object);
            let receiver = root_evaluated_call_argument(ctx, object, expr.span);
            let data = ctx.intern_string(property);
            let value = ctx.emit_value(Op::PropGet, vec![receiver.value], Some(Immediate::Data(data)),
                PhpType::Str, Op::PropGet.default_effects(), Some(expr.span));
            ctx.builder.set_value_ownership(value.value, Ownership::Borrowed);
            value.value
        } else {
            lower_arg_with_signature(ctx, sig, param_idx, expr)
        };
        sources[source_index] = Some(value);
    }
    let operands: Vec<_> = if positional {
        sources.into_iter().map(Option::unwrap).collect()
    } else { plan.regular_args.iter().enumerate().map(|(index, arg)| match arg {
        PlannedRegularArg::Source { source_index, .. } => sources[*source_index].unwrap(),
        PlannedRegularArg::Default(expr) => lower_arg_with_signature(ctx, sig, index, expr),
        PlannedRegularArg::SpreadElement { .. } => unreachable!(),
    }).collect() };
    if operands.is_empty() { return None; }
    for operand in &operands {
        if ctx.builder.value_php_type(*operand).codegen_repr() == PhpType::Str
            && ctx.builder.value_defining_op(*operand) != Some(Op::ConstStr)
        {
            ctx.borrowed_string_call_operands.insert(*operand);
        }
    }
    Some(operands)
}

/// Proves that an argument has its exact scalar representation and cannot invalidate a string.
fn safe_read(ctx: &LoweringContext<'_, '_>, expr: &Expr, string: bool, property_allowed: bool) -> bool {
    match &expr.kind {
        ExprKind::StringLiteral(_) => string,
        ExprKind::IntLiteral(_) => !string,
        ExprKind::Variable(name) => !ctx.local_uses_global_storage(name)
            && !ctx.extern_globals.contains_key(name)
            && ctx.local_type(name).codegen_repr() == if string { PhpType::Str } else { PhpType::Int },
        ExprKind::PropertyAccess { object, property } if string && property_allowed => {
            let name = match &object.kind {
                ExprKind::Variable(name) => name.as_str(),
                ExprKind::This => "this",
                _ => return false,
            };
            if ctx.local_uses_global_storage(name) || ctx.extern_globals.contains_key(name) { return false; }
            let PhpType::Object(class) = ctx.local_type(name).codegen_repr() else { return false; };
            let Some(info) = ctx.classes.get(class.trim_start_matches('\\')) else { return false; };
            if runtime_property_type_override(ctx, &class, property).is_some() { return false; }
            let mut ancestor = Some(info);
            while let Some(info) = ancestor {
                if info.is_internal { return false; }
                ancestor = info.parent.as_ref().and_then(|parent| ctx.classes.get(parent.as_str()));
            }
            info.visible_property(property).is_some_and(|(_, (_, ty))| ty.codegen_repr() == PhpType::Str)
                && !info.methods.contains_key(&php_symbol_key(&property_hook_get_method(property)))
                && matches!(crate::types::resolve_property_name(ctx.classes, &class, property,
                    ctx.current_class.as_deref()), crate::types::PropertyNameResolution::Visible)
        }
        _ => false,
    }
}
