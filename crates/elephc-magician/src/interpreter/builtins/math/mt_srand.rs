//! Purpose:
//! Eval registry entry and implementation for `mt_srand`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Seeds eval's Mersenne Twister, which `mt_rand()` and `rand()` then draw php's sequence from.
//! - A missing or null seed draws one at random; `MT_RAND_PHP` (1) selects php's deprecated
//!   legacy twist and raises its deprecation, any other mode is `MT_RAND_MT19937`.

use super::super::super::*;

eval_builtin! {
    contract: "mt_srand",
    area: Math,
    direct: MtSrand,
    values: MtSrand,
}

/// Evaluates PHP `mt_srand()` over its optional seed and mode.
pub(in crate::interpreter) fn eval_builtin_mt_srand(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut evaluated = Vec::with_capacity(args.len());
    for arg in args {
        evaluated.push(eval_expr(arg, context, scope, values)?);
    }
    eval_mt_srand_values_result(&evaluated, values)
}

/// Seeds eval's Mersenne Twister from already evaluated `mt_srand()` arguments.
pub(in crate::interpreter) fn eval_mt_srand_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_seed_mt_from_args(evaluated_args, values)
}

/// Shared body of `mt_srand()` and `srand()`: reads the optional mode, raises the `MT_RAND_PHP`
/// deprecation before seeding as php does, then seeds from the given or a random seed.
pub(in crate::interpreter) fn eval_seed_mt_from_args(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if evaluated_args.len() > 2 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let legacy = match evaluated_args.get(1) {
        Some(mode) => eval_int_value(*mode, values)? == 1,
        None => false,
    };
    if legacy {
        values.warning("Deprecated: The MT_RAND_PHP variant of Mt19937 is deprecated\n")?;
    }
    let seed = match evaluated_args.first() {
        Some(seed) if values.type_tag(*seed)? != EVAL_TAG_NULL => eval_int_value(*seed, values)? as u32,
        _ => eval_random_u128() as u32,
    };
    eval_mt_seed(seed, legacy);
    values.null()
}
