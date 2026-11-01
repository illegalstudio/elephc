//! Purpose:
//! Eval registry entry and implementation for `hash_pbkdf2`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Follows php-src's `PHP_FUNCTION(hash_pbkdf2)`: the algorithm is validated first (unknown
//!   names and non-cryptographic checksums are a ValueError for argument 1), then
//!   `$iterations > 0`, then `$length >= 0`. A zero `$length` is the digest size -- bytes for raw
//!   output, hex digits otherwise -- and hex output derives `ceil($length / 2)` bytes and keeps
//!   the first `$length` digits. `$options` cannot apply to a cryptographic algorithm and is
//!   ignored, as php does for these.
//! - The derivation is `elephc_crypto::pbkdf2`. The compiled side is the AST prelude
//!   `src/hash_pbkdf2_prelude.rs`; interpreted code cannot reach a compiler prelude, and
//!   Symfony's `Pbkdf2PasswordHasher` derives a key from interpreted code on every login.

use super::super::super::*;

eval_builtin! {
    contract: "hash_pbkdf2",
    area: String,
    direct: HashPbkdf2,
    values: HashPbkdf2,
}

/// Evaluates PHP `hash_pbkdf2(...)` from unevaluated call-site expressions.
pub(in crate::interpreter) fn eval_builtin_hash_pbkdf2(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut evaluated = Vec::with_capacity(args.len());
    for arg in args {
        evaluated.push(eval_expr(arg, context, scope, values)?);
    }
    eval_hash_pbkdf2_values(&evaluated, context, values)
}

/// Evaluates PHP `hash_pbkdf2(...)` from already-evaluated argument cells.
pub(in crate::interpreter) fn eval_hash_pbkdf2_values(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(4..=7).contains(&evaluated_args.len()) {
        let bound = if evaluated_args.len() < 4 { "at least 4" } else { "at most 7" };
        return eval_throw_argument_count_error(
            &format!(
                "hash_pbkdf2() expects {bound} arguments, {} given",
                evaluated_args.len()
            ),
            context,
            values,
        );
    }
    let algo = values.string_bytes(evaluated_args[0])?;
    let password = values.string_bytes(evaluated_args[1])?;
    let salt = values.string_bytes(evaluated_args[2])?;
    let iterations = eval_int_value(evaluated_args[3], values)?;
    let length = match evaluated_args.get(4) {
        Some(length) => eval_int_value(*length, values)?,
        None => 0,
    };
    let binary = match evaluated_args.get(5) {
        Some(binary) => values.truthy(*binary)?,
        None => false,
    };
    let algo = String::from_utf8_lossy(&algo).into_owned();
    let Some(digest_size) = elephc_crypto::hmac_digest_size(&algo) else {
        return eval_throw_builtin_value_error(
            "hash_pbkdf2(): Argument #1 ($algo) must be a valid cryptographic hashing algorithm",
            context,
            values,
        );
    };
    if iterations <= 0 {
        return eval_throw_builtin_value_error(
            "hash_pbkdf2(): Argument #4 ($iterations) must be greater than 0",
            context,
            values,
        );
    }
    if length < 0 {
        return eval_throw_builtin_value_error(
            "hash_pbkdf2(): Argument #5 ($length) must be greater than or equal to 0",
            context,
            values,
        );
    }
    let length = match (length as usize, binary) {
        (0, true) => digest_size,
        (0, false) => digest_size * 2,
        (length, _) => length,
    };
    let key_bytes = if binary { length } else { length.div_ceil(2) };
    let derived = elephc_crypto::pbkdf2(&algo, &password, &salt, iterations as u64, key_bytes)
        .ok_or(EvalStatus::RuntimeFatal)?;
    if binary {
        return values.string_bytes_value(&derived);
    }
    let mut hex: Vec<u8> = derived
        .iter()
        .flat_map(|byte| {
            let digits = b"0123456789abcdef";
            [digits[usize::from(byte >> 4)], digits[usize::from(byte & 0xf)]]
        })
        .collect();
    hex.truncate(length);
    values.string_bytes_value(&hex)
}
