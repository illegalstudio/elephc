//! Purpose:
//! Shared eval implementation of the `sodium_crypto_box_*` sealed-box family.
//!
//! Called from:
//! - The four `sodium_crypto_box_*` eval home files in this directory.
//!
//! Key details:
//! - The compiled program reaches the same surface through `sodium_prelude`'s AST-built
//!   wrappers; interpreted code (Symfony's `SodiumVault` in an eval-heavy build) never runs
//!   through that prelude, so the interpreter calls `elephc_crypto::sodium` directly.
//! - Argument-length failures throw `SodiumException`, which the host program declares whenever
//!   it contains `eval` (see `sodium_prelude::inject_if_used`); an unopenable box returns false.

use super::super::super::*;
use elephc_crypto::sodium::{
    sodium_op, SODIUM_ERR_ARG1_LENGTH, SODIUM_ERR_ARG2_LENGTH, SODIUM_ERR_OPEN_FAILED,
    SODIUM_OP_BOX_KEYPAIR, SODIUM_OP_BOX_PUBLICKEY, SODIUM_OP_BOX_SEAL, SODIUM_OP_BOX_SEAL_OPEN,
};

/// Evaluates every argument expression, then runs the named sodium builtin.
pub(in crate::interpreter) fn eval_builtin_sodium_box(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut evaluated = Vec::with_capacity(args.len());
    for arg in args {
        evaluated.push(eval_expr(arg, context, scope, values)?);
    }
    eval_sodium_box_result(name, &evaluated, context, values)
}

/// Runs the named sodium builtin over already evaluated arguments.
pub(in crate::interpreter) fn eval_sodium_box_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (op, arity, arg2_message) = match name {
        "sodium_crypto_box_keypair" => (SODIUM_OP_BOX_KEYPAIR, 0, ""),
        "sodium_crypto_box_publickey" => (SODIUM_OP_BOX_PUBLICKEY, 1, ""),
        "sodium_crypto_box_seal" => (
            SODIUM_OP_BOX_SEAL,
            2,
            "sodium_crypto_box_seal(): Argument #2 ($public_key) must be SODIUM_CRYPTO_BOX_PUBLICKEYBYTES bytes long",
        ),
        "sodium_crypto_box_seal_open" => (
            SODIUM_OP_BOX_SEAL_OPEN,
            2,
            "sodium_crypto_box_seal_open(): Argument #2 ($key_pair) must be SODIUM_CRYPTO_BOX_KEYPAIRBYTES bytes long",
        ),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    if evaluated_args.len() != arity {
        return Err(EvalStatus::RuntimeFatal);
    }
    let first = match evaluated_args.first() {
        Some(cell) => values.string_bytes(*cell)?,
        None => Vec::new(),
    };
    let second = match evaluated_args.get(1) {
        Some(cell) => values.string_bytes(*cell)?,
        None => Vec::new(),
    };
    match sodium_op(op, &first, &second) {
        Ok(bytes) => values.string_bytes_value(&bytes),
        Err(SODIUM_ERR_OPEN_FAILED) => values.bool_value(false),
        Err(SODIUM_ERR_ARG1_LENGTH) => eval_throw_sodium_exception(
            "sodium_crypto_box_publickey(): Argument #1 ($key_pair) must be SODIUM_CRYPTO_BOX_KEYPAIRBYTES bytes long",
            context,
            values,
        ),
        Err(SODIUM_ERR_ARG2_LENGTH) => eval_throw_sodium_exception(arg2_message, context, values),
        Err(_) => eval_throw_sodium_exception("internal error", context, values),
    }
}

/// Creates and schedules a `SodiumException` through eval's normal Throwable channel.
fn eval_throw_sodium_exception(
    message: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let exception = values.new_object("SodiumException")?;
    let message = values.string(message)?;
    values.construct_object(exception, vec![message])?;
    context.set_pending_throw(exception);
    Err(EvalStatus::UncaughtThrowable)
}
