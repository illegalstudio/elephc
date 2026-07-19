//! Purpose:
//! Declarative eval registry entry for `hash_file`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the one-shot hash hook.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "hash_file",
    area: String,
    params: [algo, filename, binary = EvalBuiltinDefaultValue::Bool(false)],
    direct: HashOneShot,
    values: HashOneShot,
}

use super::super::super::*;
use super::super::*;

/// Evaluates PHP `hash_file(...)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_hash_file(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::hash::eval_builtin_hash_one_shot_named("hash_file", args, context, scope, values)
}

/// Applies PHP `hash_file(...)` to already evaluated arguments.
pub(in crate::interpreter) fn eval_hash_file_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::hash::eval_hash_one_shot_named_result("hash_file", evaluated_args, values)
}

/// Reads a local file and returns its PHP hash digest or false when it cannot be read.
pub(in crate::interpreter) fn eval_hash_file_digest_result(
    algo: RuntimeCellHandle,
    filename: RuntimeCellHandle,
    binary: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let algo = values.string_bytes(algo)?;
    let path = eval_path_string(filename, values)?;
    match std::fs::read(path) {
        Ok(data) => super::hash::eval_hash_digest_result(&algo, &data, binary, values),
        Err(_) => values.bool_value(false),
    }
}
