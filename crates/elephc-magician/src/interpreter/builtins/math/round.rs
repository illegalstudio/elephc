//! Purpose:
//! Eval registry entry and implementation for `round`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - The optional precision defaults through registry metadata; direct calls
//!   still evaluate arguments in source order.

use super::super::super::*;
eval_builtin! {
    contract: "round",
    area: Math,
    direct: Round,
    values: Round,
}

/// Evaluates PHP `round()` over one value plus optional precision and mode expressions.
pub(in crate::interpreter) fn eval_builtin_round(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [num] => {
            let num = eval_expr(num, context, scope, values)?;
            eval_round_result(num, None, None, values)
        }
        [num, precision] => {
            let num = eval_expr(num, context, scope, values)?;
            let precision = eval_expr(precision, context, scope, values)?;
            eval_round_result(num, Some(precision), None, values)
        }
        [num, precision, mode] => {
            let num = eval_expr(num, context, scope, values)?;
            let precision = eval_expr(precision, context, scope, values)?;
            let mode = eval_expr(mode, context, scope, values)?;
            eval_round_result(num, Some(precision), Some(mode), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Applies PHP `round()` to already evaluated arguments.
///
/// The default half-away-from-zero mode delegates to the runtime rounding op. The
/// other three PHP modes differ only for exact `.5` ties, so they are resolved here
/// by scaling to the requested precision, detecting the tie against `floor + 0.5`,
/// and choosing the neighbour PHP's mode selects; everything that is not a tie falls
/// back to the same runtime op, which keeps ordinary values bit-identical.
pub(in crate::interpreter) fn eval_round_result(
    num: RuntimeCellHandle,
    precision: Option<RuntimeCellHandle>,
    mode: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(mode) = mode else {
        return values.round(num, precision);
    };
    for (tag, resolver) in [
        (EVAL_PHP_ROUND_HALF_DOWN, eval_round_half_down as RoundTieResolver),
        (EVAL_PHP_ROUND_HALF_EVEN, eval_round_half_even),
        (EVAL_PHP_ROUND_HALF_ODD, eval_round_half_odd),
    ] {
        let tag = values.int(tag)?;
        let selected = values.compare(EvalBinOp::StrictEq, mode, tag)?;
        if values.truthy(selected)? {
            return eval_round_tie_aware(num, precision, resolver, values);
        }
    }
    values.round(num, precision)
}

/// Chooses between the two neighbours of an exact `.5` tie for one rounding mode.
type RoundTieResolver = fn(
    lower: RuntimeCellHandle,
    upper: RuntimeCellHandle,
    values: &mut dyn RoundTieOps,
) -> Result<RuntimeCellHandle, EvalStatus>;

/// Rounds a tie toward zero, which for the scaled value means taking the lower neighbour.
fn eval_round_half_down(
    lower: RuntimeCellHandle,
    _upper: RuntimeCellHandle,
    _values: &mut dyn RoundTieOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    Ok(lower)
}

/// Rounds a tie to whichever neighbour is even.
fn eval_round_half_even(
    lower: RuntimeCellHandle,
    upper: RuntimeCellHandle,
    values: &mut dyn RoundTieOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if values.is_even(lower)? { Ok(lower) } else { Ok(upper) }
}

/// Rounds a tie to whichever neighbour is odd.
fn eval_round_half_odd(
    lower: RuntimeCellHandle,
    upper: RuntimeCellHandle,
    values: &mut dyn RoundTieOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if values.is_even(lower)? { Ok(upper) } else { Ok(lower) }
}

/// The parity probe the tie resolvers need, kept object-safe so they can be plain fn pointers.
pub(in crate::interpreter) trait RoundTieOps {
    /// Returns whether a scaled integral cell is an even number.
    fn is_even(&mut self, value: RuntimeCellHandle) -> Result<bool, EvalStatus>;
}

impl<T: RuntimeValueOps> RoundTieOps for T {
    /// Computes whether a runtime numeric cell is even using the shared remainder operation.
    fn is_even(&mut self, value: RuntimeCellHandle) -> Result<bool, EvalStatus> {
        let two = self.int(2)?;
        let remainder = self.fmod(value, two)?;
        let zero = self.int(0)?;
        let equal = self.compare(EvalBinOp::LooseEq, remainder, zero)?;
        self.truthy(equal)
    }
}

/// Applies a tie-aware rounding mode at the requested precision.
fn eval_round_tie_aware(
    num: RuntimeCellHandle,
    precision: Option<RuntimeCellHandle>,
    resolver: RoundTieResolver,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let ten = values.float(10.0)?;
    let exponent = match precision {
        Some(precision) => precision,
        None => values.int(0)?,
    };
    let scale = values.pow(ten, exponent)?;
    let scaled = values.mul(num, scale)?;
    let lower = values.floor(scaled)?;
    let half = values.float(0.5)?;
    let midpoint = values.add(lower, half)?;
    let is_tie = values.compare(EvalBinOp::LooseEq, scaled, midpoint)?;
    if !values.truthy(is_tie)? {
        return values.round(num, precision);
    }
    let one = values.int(1)?;
    let upper = values.add(lower, one)?;
    let chosen = resolver(lower, upper, values)?;
    values.div(chosen, scale)
}
