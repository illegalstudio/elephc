//! Purpose:
//! Dispatches already evaluated string, hash, encoding, and ctype builtins by dynamic callable name.
//!
//! Called from:
//! - `crate::interpreter::builtins::registry::dispatch`.
//!
//! Key details:
//! - Returns `Ok(None)` for names outside this domain so the parent dispatcher can
//!   continue probing other builtin families.

use super::super::super::super::*;
use super::super::super::*;

/// Attempts to dispatch evaluated string, hash, encoding, and ctype builtins.
pub(in crate::interpreter) fn eval_strings_builtin_with_values(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let result = match name {
        "gzcompress" | "gzdeflate" | "gzinflate" | "gzuncompress" => {
            eval_gzip_result(name, evaluated_args, values)?
        }
        "explode" => {
            let [separator, string] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_explode_result(*separator, *string, values)?
        }
        "implode" => {
            let [separator, array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_implode_result(*separator, *array, values)?
        }
        "hash" | "hash_file" | "hash_hmac" | "md5" | "sha1" => {
            eval_hash_one_shot_result(name, evaluated_args, values)?
        }
        "hash_algos" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_hash_algos_result(values)?
        }
        "hash_copy" => {
            let [hash_context] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_hash_copy_result(*hash_context, context, values)?
        }
        "hash_final" => match evaluated_args {
            [hash_context] => eval_hash_final_result(*hash_context, false, context, values)?,
            [hash_context, binary] => {
                let binary = values.truthy(*binary)?;
                eval_hash_final_result(*hash_context, binary, context, values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "hash_init" => {
            let [algo] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_hash_init_result(*algo, context, values)?
        }
        "hash_update" => {
            let [hash_context, data] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_hash_update_result(*hash_context, *data, context, values)?
        }
        _ => return Ok(None),
    };
    Ok(Some(result))
}
