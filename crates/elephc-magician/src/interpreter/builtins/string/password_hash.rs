//! Purpose:
//! Eval registry entries and implementations for PHP's password hashing API: `password_hash`,
//! `password_verify`, `password_needs_rehash`, `password_get_info` and `password_algos`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - The same surface the compiled side declares as an AST prelude (`src/password_prelude.rs`),
//!   over the same `elephc_crypto::password` bcrypt: `$2y$` hashes, php's default cost 12, the
//!   algorithm given as `null`, `"2y"` or the legacy `1`, and Argon2 absent (a build without
//!   libargon2). Interpreted code cannot reach a compiler prelude, and Symfony's
//!   `NativePasswordHasher` verifies every login password from interpreted code.
//! - One file for the five: they share the algorithm and cost rules, and each of the four
//!   sibling files only declares its registry entry.

use super::super::super::*;

eval_builtin! {
    contract: "password_hash",
    area: String,
    direct: PasswordApi,
    values: PasswordApi,
}

/// php's default bcrypt cost (`PASSWORD_BCRYPT_DEFAULT_COST`), 12 since php 8.4.
const DEFAULT_COST: i64 = 12;

/// Evaluates one password-API call from unevaluated call-site expressions.
pub(in crate::interpreter) fn eval_builtin_password_api(
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
    eval_password_api_values(name, &evaluated, context, values)
}

/// Evaluates one password-API call from already-evaluated argument cells.
pub(in crate::interpreter) fn eval_password_api_values(
    name: &str,
    args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (min, max) = match name {
        "password_algos" => (0, 0),
        "password_get_info" => (1, 1),
        "password_verify" => (2, 2),
        "password_hash" | "password_needs_rehash" => (2, 3),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    if args.len() < min || args.len() > max {
        let message = if min == max {
            format!("{name}() expects exactly {min} argument{}, {} given", plural(min), args.len())
        } else if args.len() < min {
            format!("{name}() expects at least {min} arguments, {} given", args.len())
        } else {
            format!("{name}() expects at most {max} arguments, {} given", args.len())
        };
        return eval_throw_argument_count_error(&message, context, values);
    }
    match name {
        "password_algos" => {
            let algos = values.string_array_new(1)?;
            values.string_array_push(algos, "2y")
        }
        "password_verify" => {
            let password = values.string_bytes(args[0])?;
            let hash = values.string_bytes(args[1])?;
            let hash = String::from_utf8_lossy(&hash);
            values.bool_value(elephc_crypto::password::verify(until_nul(&password), &hash))
        }
        "password_get_info" => {
            let hash = values.string_bytes(args[0])?;
            password_info(bcrypt_cost(&hash), values)
        }
        "password_needs_rehash" => {
            // php: an unknown new algorithm never prompts a rehash, a hash of another
            // algorithm always does, and a bcrypt hash does when its cost differs.
            if !algo_is_bcrypt(args[1], values)? {
                return values.bool_value(false);
            }
            let hash = values.string_bytes(args[0])?;
            let wanted = requested_cost(args.get(2).copied(), values)?;
            values.bool_value(bcrypt_cost(&hash) != Some(wanted))
        }
        _ => {
            if !algo_is_bcrypt(args[1], values)? {
                return eval_throw_builtin_value_error(
                    "password_hash(): Argument #2 ($algo) must be a valid password hashing algorithm",
                    context,
                    values,
                );
            }
            let cost = requested_cost(args.get(2).copied(), values)?;
            if !(4..=31).contains(&cost) {
                return eval_throw_builtin_value_error(
                    &format!("Invalid bcrypt cost parameter specified: {cost}"),
                    context,
                    values,
                );
            }
            let password = values.string_bytes(args[0])?;
            let hash = elephc_crypto::password::hash(until_nul(&password), cost as u32)
                .ok_or(EvalStatus::RuntimeFatal)?;
            values.string(&hash)
        }
    }
}

/// `"s"` for a count other than one.
fn plural(count: usize) -> &'static str {
    if count == 1 {
        ""
    } else {
        "s"
    }
}

/// The bytes before the first NUL, which is where bcrypt stops reading a password.
fn until_nul(bytes: &[u8]) -> &[u8] {
    bytes
        .iter()
        .position(|byte| *byte == 0)
        .map_or(bytes, |end| &bytes[..end])
}

/// Whether `$algo` resolves to bcrypt as php's `php_password_algo_find_zval` resolves it:
/// `null` and the legacy `0` are the default (bcrypt), and `"2y"` and the legacy `1` name it.
/// Argon2 is absent from this build, so its spellings resolve to nothing, like every other.
fn algo_is_bcrypt(
    algo: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    Ok(match values.type_tag(algo)? {
        EVAL_TAG_NULL => true,
        EVAL_TAG_STRING => values.string_bytes(algo)? == b"2y",
        _ => matches!(eval_int_value(algo, values)?, 0 | 1),
    })
}

/// `(int) ($options['cost'] ?? 12)`.
fn requested_cost(
    options: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    let Some(options) = options else {
        return Ok(DEFAULT_COST);
    };
    if !matches!(values.type_tag(options)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        return Ok(DEFAULT_COST);
    }
    let key = values.string("cost")?;
    let cost = values.array_get(options, key)?;
    if values.is_null(cost)? {
        return Ok(DEFAULT_COST);
    }
    eval_int_value(cost, values)
}

/// The cost of a `$2y$` bcrypt hash (60 bytes, `$2y$` + two cost digits), or None otherwise.
fn bcrypt_cost(hash: &[u8]) -> Option<i64> {
    if hash.len() != 60 || !hash.starts_with(b"$2y$") {
        return None;
    }
    // `(int) substr($hash, 4, 2)`: the leading digits, 0 when there are none.
    Some(
        hash[4..6]
            .iter()
            .take_while(|byte| byte.is_ascii_digit())
            .fold(0, |cost, byte| cost * 10 + i64::from(byte - b'0')),
    )
}

/// `password_get_info()`'s array for a hash's bcrypt cost, or the unknown-algorithm answer.
fn password_info(
    cost: Option<i64>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut info = values.assoc_new(3)?;
    let mut options = values.assoc_new(1)?;
    let key = values.string("algo")?;
    let algo = match cost {
        Some(_) => values.string("2y")?,
        None => values.null()?,
    };
    info = values.array_set(info, key, algo)?;
    let key = values.string("algoName")?;
    let algo_name = values.string(if cost.is_some() { "bcrypt" } else { "unknown" })?;
    info = values.array_set(info, key, algo_name)?;
    if let Some(cost) = cost {
        let key = values.string("cost")?;
        let cost = values.int(cost)?;
        options = values.array_set(options, key, cost)?;
    }
    let key = values.string("options")?;
    values.array_set(info, key, options)
}
