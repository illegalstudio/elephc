//! Purpose:
//! Completes mktime/gmmktime civil arguments for registry-backed callable paths.
//!
//! Called from:
//! - The shared builtin argument-lowering dispatcher.
//!
//! Key details:
//! - Bind and evaluate user arguments once before sampling the clock.
//! - Preserve null until default selection; release boxed argument temporaries after the call.

use super::*;

/// Lowers five nullable defaults from a single snapshot while keeping hour mandatory.
pub(super) fn lower_mktime_args(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    sig: Option<&FunctionSig>,
    args: &[Expr],
    utc: bool,
) -> Vec<ValueId> {
    use crate::synthetic_class::{e_call, e_cast, e_str, e_var};
    let operands = lower_args_with_signature_and_spread_bounds(
        ctx, sig, args, Some(SpreadOverflowError::Builtin(name)),
    );
    if operands.len() != 6 {
        // Keep the ordinary runtime arity error for invalid direct/callable calls.
        return operands;
    }
    let span = args.first().map_or(Span::dummy(), |arg| arg.span);
    let mut temps = Vec::with_capacity(6);
    for &value in &operands {
        let ty = ctx.builder.value_php_type(value).clone();
        let lowered = LoweredValue { value, ir_type: ctx.builder.value_type(value) };
        let temp = ctx.declare_hidden_temp(ty.clone());
        store_value_into_temp(ctx, &temp, ty, lowered, span);
        temps.push(temp);
    }
    let snapshot = lower_expr(ctx, &e_call("time", vec![]));
    let snapshot_temp = ctx.declare_hidden_temp(PhpType::Int);
    store_value_into_temp(ctx, &snapshot_temp, PhpType::Int, snapshot, span);
    let mut completed = Vec::with_capacity(6);
    let formats = crate::builtins::MKTIME_COMPONENT_FORMATS;
    for (index, temp) in temps.iter().enumerate() {
        let argument = if index == 0 {
            e_var(temp)
        } else {
            Expr::new(ExprKind::NullCoalesce {
                value: Box::new(e_var(temp)),
                default: Box::new(e_cast(CastType::Int, e_call(
                    if utc { "gmdate" } else { "date" },
                    vec![e_str(formats[index]), e_var(&snapshot_temp)],
                ))),
            }, span)
        };
        let value = lower_expr(ctx, &argument);
        let value = coerce_to_int_at_span(ctx, value, Some(span));
        ctx.transfer_call_arg_temp_cleanup(operands[index], value.value);
        completed.push(value.value);
    }
    for temp in temps {
        ctx.register_call_arg_temp_cleanup(completed[0], temp);
    }
    completed
}
