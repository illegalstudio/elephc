//! Purpose:
//! Eval registry entry and implementation for `php_uname`.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env` direct and by-value dispatch.
//!
//! Key details:
//! - Platform uname fields are copied into PHP strings before formatting the requested mode.

use super::*;

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "php_uname",
    area: NetworkEnv,
    params: [mode = EvalBuiltinDefaultValue::String("a")],
    direct: NetworkEnv,
    values: NetworkEnv,
}

/// Evaluates PHP `php_uname($mode = "a")` over zero or one eval expression.
pub(in crate::interpreter) fn eval_builtin_php_uname(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [] => eval_php_uname_result(None, values),
        [mode] => {
            let mode = eval_expr(mode, context, scope, values)?;
            eval_php_uname_result(Some(mode), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Reads the local uname fields and formats the PHP `php_uname()` mode result.
pub(in crate::interpreter) fn eval_php_uname_result(
    mode: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mode = match mode {
        Some(mode) => {
            let bytes = values.string_bytes(mode)?;
            let [mode] = bytes.as_slice() else {
                return Err(EvalStatus::RuntimeFatal);
            };
            *mode
        }
        None => b'a',
    };

    let Some(uname) = eval_os_uname() else {
        return values.string("");
    };

    match mode {
        b'a' => {
            let mut output = Vec::new();
            for field in [
                &uname.sysname,
                &uname.nodename,
                &uname.release,
                &uname.version,
                &uname.machine,
            ] {
                if !output.is_empty() {
                    output.push(b' ');
                }
                output.extend_from_slice(field);
            }
            values.string_bytes_value(&output)
        }
        b's' => values.string_bytes_value(&uname.sysname),
        b'n' => values.string_bytes_value(&uname.nodename),
        b'r' => values.string_bytes_value(&uname.release),
        b'v' => values.string_bytes_value(&uname.version),
        b'm' => values.string_bytes_value(&uname.machine),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
