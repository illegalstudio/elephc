//! Purpose:
//! Declarative eval registry entry for `hash_hmac`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the one-shot hash hook.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "hash_hmac",
    area: String,
    params: [algo, data, key, binary = EvalBuiltinDefaultValue::Bool(false)],
    direct: HashOneShot,
    values: HashOneShot,
}

use super::super::super::*;

/// Evaluates PHP `hash_hmac(...)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_hash_hmac(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::hash::eval_builtin_hash_one_shot_named("hash_hmac", args, context, scope, values)
}

/// Applies PHP `hash_hmac(...)` to already evaluated arguments.
pub(in crate::interpreter) fn eval_hash_hmac_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::hash::eval_hash_one_shot_named_result("hash_hmac", evaluated_args, values)
}

/// Computes a one-shot raw HMAC digest and formats it as PHP hex or raw bytes.
pub(in crate::interpreter) fn eval_hash_hmac_digest_result(
    algo: &[u8],
    data: &[u8],
    key: &[u8],
    binary: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let raw = eval_crypto_hmac(algo, data, key)?;
    super::hash::eval_format_digest_result(&raw, binary, values)
}

/// Calls the elephc-crypto one-shot HMAC ABI and returns the raw digest bytes.
pub(in crate::interpreter) fn eval_crypto_hmac(
    algo: &[u8],
    data: &[u8],
    key: &[u8],
) -> Result<Vec<u8>, EvalStatus> {
    let mut output = [0_u8; 64];
    let len = unsafe {
        elephc_crypto::elephc_crypto_hmac(
            algo.as_ptr(),
            algo.len(),
            key.as_ptr(),
            key.len(),
            data.as_ptr(),
            data.len(),
            output.as_mut_ptr(),
        )
    };
    super::hash::eval_crypto_digest_bytes(len, &output)
}
