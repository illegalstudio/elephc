//! Purpose:
//! Eval registry entry and implementation for `http_response_code`.
//!
//! Called from:
//! - `crate::interpreter::builtins::time` direct and by-value dispatch.
//!
//! Key details:
//! - Response-code state is eval-local and observable by later calls.

use super::super::super::*;
use super::super::*;

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "http_response_code",
    area: Time,
    params: [response_code = EvalBuiltinDefaultValue::Int(0)],
    direct: Time,
    values: Time,
}

/// Evaluates PHP `http_response_code($response_code = 0)`.
pub(in crate::interpreter) fn eval_builtin_http_response_code(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [] => eval_http_response_code_result(None, context, values),
        [response_code] => {
            let response_code = eval_expr(response_code, context, scope, values)?;
            eval_http_response_code_result(Some(response_code), context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Reads or updates the eval-local HTTP response code.
pub(in crate::interpreter) fn eval_http_response_code_result(
    response_code: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let result = match response_code {
        Some(response_code) => context.replace_http_response_code(eval_int_value(response_code, values)?),
        None => context.http_response_code(),
    };
    values.int(result)
}
