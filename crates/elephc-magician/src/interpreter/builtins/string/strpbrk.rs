//! Purpose:
//! Declarative eval registry entry and byte-oriented implementation of `strpbrk`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string` through direct and evaluated-value dispatch.
//!
//! Key details:
//! - Matches php-src's byte membership semantics, including embedded NUL bytes.
//! - An empty character list raises a catchable `ValueError` with PHP's exact message.

use super::super::super::*;

eval_builtin! {
    contract: "strpbrk",
    area: String,
    direct: Strpbrk,
    values: Strpbrk,
}

/// Evaluates `strpbrk()` after evaluating its two arguments in PHP source order.
pub(in crate::interpreter) fn eval_builtin_strpbrk(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [string, characters] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let string = eval_expr(string, context, scope, values)?;
    let characters = eval_expr(characters, context, scope, values)?;
    eval_strpbrk_result(string, characters, context, values)
}

/// Returns the suffix at the first byte present in `$characters`, or PHP `false` on a miss.
pub(in crate::interpreter) fn eval_strpbrk_result(
    string: RuntimeCellHandle,
    characters: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let string = values.string_bytes(string)?;
    let characters = values.string_bytes(characters)?;
    if characters.is_empty() {
        return eval_strpbrk_empty_characters_error(context, values);
    }
    let mut membership = [false; 256];
    for byte in characters {
        membership[usize::from(byte)] = true;
    }
    let Some(position) = string.iter().position(|byte| membership[usize::from(*byte)]) else {
        return values.bool_value(false);
    };
    values.string_bytes_value(&string[position..])
}

/// Raises php-src's catchable `ValueError` for an empty `$characters` argument.
fn eval_strpbrk_empty_characters_error<T>(
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_throw_builtin_exception_with_code("ValueError", "strpbrk(): Argument #2 ($characters) must be a non-empty string", 0, context, values)
}
