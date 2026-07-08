//! Purpose:
//! Declarative eval registry entry for `hash_final`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the incremental hash-context hook.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "hash_final",
    area: String,
    params: [context, binary = EvalBuiltinDefaultValue::Bool(false)],
    direct: HashContext,
    values: HashContext,
}

use super::super::super::*;

/// Evaluates PHP `hash_final($context, $binary = false)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_hash_final(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(1..=2).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let hash_context = eval_expr(&args[0], context, scope, values)?;
    let binary = match args.get(1) {
        Some(binary) => {
            let binary = eval_expr(binary, context, scope, values)?;
            values.truthy(binary)?
        }
        None => false,
    };
    eval_hash_final_result(hash_context, binary, context, values)
}

/// Finalizes a materialized incremental hash context.
pub(in crate::interpreter) fn eval_hash_final_result(
    hash_context: RuntimeCellHandle,
    binary: bool,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = super::hash_init::eval_hash_context_resource_id(hash_context, values)?;
    let raw = context
        .stream_resources_mut()
        .finalize_hash_context(id)
        .ok_or(EvalStatus::RuntimeFatal)?;
    super::hash::eval_format_digest_result(&raw, binary, values)
}
