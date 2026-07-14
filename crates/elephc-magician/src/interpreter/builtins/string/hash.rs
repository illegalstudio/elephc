//! Purpose:
//! Declarative eval registry entry for `hash`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the one-shot hash hook.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "hash",
    area: String,
    params: [algo, data, binary = EvalBuiltinDefaultValue::Bool(false)],
    direct: HashOneShot,
    values: HashOneShot,
}

use super::super::super::*;

/// Evaluates PHP `hash(...)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_hash(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::hash::eval_builtin_hash_one_shot_named("hash", args, context, scope, values)
}

/// Applies PHP `hash(...)` to already evaluated arguments.
pub(in crate::interpreter) fn eval_hash_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::hash::eval_hash_one_shot_named_result("hash", evaluated_args, values)
}

/// Evaluates a named one-shot PHP hash builtin over eval expressions.
pub(in crate::interpreter) fn eval_builtin_hash_one_shot_named(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut evaluated_args = Vec::with_capacity(args.len());
    for arg in args {
        evaluated_args.push(eval_expr(arg, context, scope, values)?);
    }
    eval_hash_one_shot_named_result(name, &evaluated_args, values)
}

/// Computes the result for one-shot PHP hash digest builtins from evaluated args.
pub(in crate::interpreter) fn eval_hash_one_shot_named_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "md5" | "sha1" => {
            let (data, binary) = match evaluated_args {
                [data] => (*data, false),
                [data, binary] => (*data, values.truthy(*binary)?),
                _ => return Err(EvalStatus::RuntimeFatal),
            };
            let data = values.string_bytes(data)?;
            eval_hash_digest_result(name.as_bytes(), &data, binary, values)
        }
        "hash" => {
            let (algo, data, binary) = match evaluated_args {
                [algo, data] => (*algo, *data, false),
                [algo, data, binary] => (*algo, *data, values.truthy(*binary)?),
                _ => return Err(EvalStatus::RuntimeFatal),
            };
            let algo = values.string_bytes(algo)?;
            let data = values.string_bytes(data)?;
            eval_hash_digest_result(&algo, &data, binary, values)
        }
        "hash_file" => {
            let (algo, filename, binary) = match evaluated_args {
                [algo, filename] => (*algo, *filename, false),
                [algo, filename, binary] => (*algo, *filename, values.truthy(*binary)?),
                _ => return Err(EvalStatus::RuntimeFatal),
            };
            super::hash_file::eval_hash_file_digest_result(algo, filename, binary, values)
        }
        "hash_hmac" => {
            let (algo, data, key, binary) = match evaluated_args {
                [algo, data, key] => (*algo, *data, *key, false),
                [algo, data, key, binary] => (*algo, *data, *key, values.truthy(*binary)?),
                _ => return Err(EvalStatus::RuntimeFatal),
            };
            let algo = values.string_bytes(algo)?;
            let data = values.string_bytes(data)?;
            let key = values.string_bytes(key)?;
            super::hash_hmac::eval_hash_hmac_digest_result(&algo, &data, &key, binary, values)
        }
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Computes a one-shot raw digest and formats it as PHP hex or raw bytes.
pub(in crate::interpreter) fn eval_hash_digest_result(
    algo: &[u8],
    data: &[u8],
    binary: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let raw = eval_crypto_hash(algo, data)?;
    eval_format_digest_result(&raw, binary, values)
}

/// Calls the elephc-crypto one-shot hash ABI and returns the raw digest bytes.
pub(in crate::interpreter) fn eval_crypto_hash(
    algo: &[u8],
    data: &[u8],
) -> Result<Vec<u8>, EvalStatus> {
    let mut output = [0_u8; 64];
    let len = unsafe {
        elephc_crypto::elephc_crypto_hash(
            algo.as_ptr(),
            algo.len(),
            data.as_ptr(),
            data.len(),
            output.as_mut_ptr(),
        )
    };
    eval_crypto_digest_bytes(len, &output)
}

/// Converts a crypto ABI digest length into an owned digest byte vector.
pub(in crate::interpreter) fn eval_crypto_digest_bytes(
    len: isize,
    output: &[u8; 64],
) -> Result<Vec<u8>, EvalStatus> {
    let len = usize::try_from(len).map_err(|_| EvalStatus::RuntimeFatal)?;
    if len > output.len() {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(output[..len].to_vec())
}

/// Formats a raw digest using PHP's `$binary` flag convention.
pub(in crate::interpreter) fn eval_format_digest_result(
    raw: &[u8],
    binary: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if binary {
        return values.string_bytes_value(raw);
    }
    values.string(&super::bin2hex::eval_lower_hex_bytes(raw))
}
