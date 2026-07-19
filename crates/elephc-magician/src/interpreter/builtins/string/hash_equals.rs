//! Purpose:
//! Declarative eval registry entry for `hash_equals`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the constant-time byte compare hook.

eval_builtin! {
    name: "hash_equals",
    area: String,
    params: [known_string, user_string],
    direct: HashEquals,
    values: HashEquals,
}

use super::super::super::*;

/// Evaluates PHP's `hash_equals(...)` over two eval expressions.
pub(in crate::interpreter) fn eval_builtin_hash_equals(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [known, user] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let known = eval_expr(known, context, scope, values)?;
    let user = eval_expr(user, context, scope, values)?;
    eval_hash_equals_result(known, user, values)
}

/// Compares two converted strings with PHP `hash_equals()` semantics.
pub(in crate::interpreter) fn eval_hash_equals_result(
    known: RuntimeCellHandle,
    user: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let known = values.string_bytes(known)?;
    let user = values.string_bytes(user)?;
    if known.len() != user.len() {
        return values.bool_value(false);
    }
    let mut diff = 0u8;
    for (known, user) in known.iter().zip(user.iter()) {
        diff |= known ^ user;
    }
    values.bool_value(diff == 0)
}
