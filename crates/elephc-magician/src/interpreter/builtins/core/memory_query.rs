//! Purpose:
//! Completes heap-query error semantics outside the boxed runtime's scalar subset.
//!
//! Called from:
//! - The Core hooks for `memory_get_usage` and `memory_get_peak_usage`.
//!
//! Key details:
//! - Valid scalar calls remain in the shared runtime dispatcher.
//! - Container flags raise TypeError through eval's normal Throwable channel.

use super::super::super::*;
use elephc_builtin_contract::RuntimeBuiltinId;

/// Evaluates argument expressions once before entering the signature-subset adapter.
pub(super) fn direct(
    id: RuntimeBuiltinId,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let operands = args.iter().collect::<Vec<_>>();
    with_eval_operands(&operands, context, scope, values, |args, context, _, values| {
        let borrowed = args.iter().map(|value| value.borrowed()).collect::<Vec<_>>();
        from_values(id, &borrowed, context, values)
    })
}

/// Raises catchable parameter errors when the shared runtime declines a call shape.
pub(super) fn from_values(
    id: RuntimeBuiltinId,
    args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let name = elephc_builtin_contract::lookup_id(id.builtin_id())
        .ok_or(EvalStatus::UnsupportedConstruct)?.name;
    if args.len() > 1 {
        return eval_throw_argument_count_error(
            &format!("{name}() expects at most 1 argument, {} given", args.len()), context, values,
        );
    }
    if let Some(&flag) = args.first() {
        let tag = values.type_tag(flag)?;
        if !matches!(tag, EVAL_TAG_INT | EVAL_TAG_FLOAT | EVAL_TAG_STRING | EVAL_TAG_BOOL | EVAL_TAG_NULL) {
            let given = eval_given_type_name(flag, context, values)?;
            return eval_throw_type_error(
                &format!("{name}(): Argument #1 ($real_usage) must be of type bool, {given} given"),
                context, values,
            );
        }
    }
    // A supported scalar call reaches this branch only when the runtime capability is absent.
    values.runtime_builtin_call(id, args)?.ok_or(EvalStatus::UnsupportedConstruct)
}
