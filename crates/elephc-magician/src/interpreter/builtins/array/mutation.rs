//! Purpose:
//! Shared lvalue binding for source-sensitive array mutator builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins::array` mutating builtin owners.
//!
//! Key details:
//! - The helper keeps the by-reference storage target together with the current
//!   array cell so callers can write back replacements after PHP-visible work.

use super::super::super::*;

/// Captures the first by-reference array mutator argument as a writable lvalue.
///
/// A non-array receiver is php's `TypeError`, not a fatal: `sort($d)` on a `false` is
/// catchable, and every builtin routed here words argument #1 the same way — `array_push`,
/// `array_pop`, `array_shift`, `array_unshift`, `array_splice`, `array_walk`, `end`,
/// `next`, `prev`, `reset` and the whole ordering family all name it `$array`, measured
/// against `php -n` 8.5.6. `name` is the PHP-visible builtin the message must quote.
pub(in crate::interpreter) fn eval_array_mutation_lvalue_arg(
    name: &str,
    arg: &EvalCallArg,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, EvalReferenceTarget), EvalStatus> {
    if arg.is_spread() || !matches!(arg.name(), None | Some("array")) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let (array, target) = eval_call_arg_value(arg.value(), context, scope, values)?;
    let target = target.ok_or(EvalStatus::RuntimeFatal)?;
    super::array_arg_check::eval_expect_sort_array_arg(array, name, context, values)?;
    Ok((array, target))
}
