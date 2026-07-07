//! Purpose:
//! Dispatches already evaluated scalar math, casts, predicates, and random builtins by dynamic callable name.
//!
//! Called from:
//! - `crate::interpreter::builtins::registry::dispatch`.
//!
//! Key details:
//! - Returns `Ok(None)` for names outside this domain so the parent dispatcher can
//!   continue probing other builtin families.

use super::super::super::super::*;
use super::super::super::*;

/// Attempts to dispatch evaluated scalar math, casts, predicates, and random builtins.
pub(in crate::interpreter) fn eval_scalars_builtin_with_values(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let result = match name {
        "abs" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.abs(*value)?
        }
        "acos" | "asin" | "atan" | "cos" | "cosh" | "deg2rad" | "exp" | "log2" | "log10"
        | "rad2deg" | "sin" | "sinh" | "tan" | "tanh" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_float_unary_result(name, *value, values)?
        }
        "atan2" | "hypot" => {
            let [left, right] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_float_pair_result(name, *left, *right, values)?
        }
        "clamp" => {
            let [value, min, max] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_clamp_result(*value, *min, *max, values)?
        }
        "fdiv" | "fmod" => {
            let [left, right] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_float_binary_result(name, *left, *right, values)?
        }
        "log" => match evaluated_args {
            [num] => eval_log_result(*num, None, values)?,
            [num, base] => eval_log_result(*num, Some(*base), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "rand" | "mt_rand" => match evaluated_args {
            [] => eval_rand_result(None, None, values)?,
            [min, max] => eval_rand_result(Some(*min), Some(*max), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "random_int" => {
            let [min, max] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_random_int_result(*min, *max, values)?
        }
        "sqrt" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.sqrt(*value)?
        }
        "settype" => {
            let [value, type_name] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_settype_value_result(*value, *type_name, values)?
        }
        "max" | "min" => eval_min_max_result(name, evaluated_args, values)?,
        "intdiv" => {
            let [left, right] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_intdiv_result(*left, *right, values)?
        }
        _ => return Ok(None),
    };
    Ok(Some(result))
}
