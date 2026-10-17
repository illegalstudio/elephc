//! Purpose:
//! Eval registry entry and implementation for `srand`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - An alias of `mt_srand()` since PHP 7.1.
//! - A missing or null seed draws one at random; `MT_RAND_PHP` (1) selects php's deprecated
//!   legacy twist and raises its deprecation, any other mode is `MT_RAND_MT19937`.

use super::super::super::*;

eval_builtin! {
    contract: "srand",
    area: Math,
    direct: Srand,
    values: Srand,
}

/// Evaluates PHP `srand()` over its optional seed and mode.
pub(in crate::interpreter) fn eval_builtin_srand(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut evaluated = Vec::with_capacity(args.len());
    for arg in args {
        evaluated.push(eval_expr(arg, context, scope, values)?);
    }
    eval_srand_values_result(&evaluated, values)
}

/// Seeds eval's Mersenne Twister from already evaluated `srand()` arguments.
pub(in crate::interpreter) fn eval_srand_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_seed_mt_from_args(evaluated_args, values)
}
