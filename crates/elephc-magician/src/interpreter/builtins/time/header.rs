//! Purpose:
//! Eval registry entry and implementation for `header`.
//!
//! Called from:
//! - `crate::interpreter::builtins::time` direct and by-value dispatch.
//!
//! Key details:
//! - Only eval-local side effects observable without a web bridge are modeled.

use super::super::super::*;
use super::super::*;

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "header",
    area: Time,
    params: [
        header,
        replace = EvalBuiltinDefaultValue::Bool(true),
        response_code = EvalBuiltinDefaultValue::Int(0),
    ],
    direct: Time,
    values: Time,
}

/// Evaluates PHP `header($header, $replace = true, $response_code = 0)`.
pub(in crate::interpreter) fn eval_builtin_header(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [line] => {
            let line = eval_expr(line, context, scope, values)?;
            eval_header_result(line, None, None, context, values)
        }
        [line, replace] => {
            let line = eval_expr(line, context, scope, values)?;
            let replace = eval_expr(replace, context, scope, values)?;
            eval_header_result(line, Some(replace), None, context, values)
        }
        [line, replace, response_code] => {
            let line = eval_expr(line, context, scope, values)?;
            let replace = eval_expr(replace, context, scope, values)?;
            let response_code = eval_expr(response_code, context, scope, values)?;
            eval_header_result(line, Some(replace), Some(response_code), context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Applies eval-local `header()` side effects that are observable without a web bridge.
pub(in crate::interpreter) fn eval_header_result(
    line: RuntimeCellHandle,
    replace: Option<RuntimeCellHandle>,
    response_code: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let _ = values.string_bytes(line)?;
    if let Some(replace) = replace {
        let _ = values.truthy(replace)?;
    }
    if let Some(response_code) = response_code {
        let response_code = eval_int_value(response_code, values)?;
        let _ = context.replace_http_response_code(response_code);
    }
    values.null()
}
