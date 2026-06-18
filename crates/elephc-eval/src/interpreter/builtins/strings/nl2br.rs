//! Purpose:
//! Newline-to-break string builtin.
//!
//! Called from:
//! - `crate::interpreter::builtins::strings` re-exports.
//!
//! Key details:
//! - Runtime cells remain opaque and string bytes are obtained through `RuntimeValueOps`.

use super::super::super::*;

/// Evaluates PHP's `nl2br(...)` over one eval expression and optional XHTML flag.
pub(in crate::interpreter) fn eval_builtin_nl2br(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value] => {
            let value = eval_expr(value, context, scope, values)?;
            eval_nl2br_result(value, true, values)
        }
        [value, use_xhtml] => {
            let value = eval_expr(value, context, scope, values)?;
            let use_xhtml = eval_expr(use_xhtml, context, scope, values)?;
            let use_xhtml = values.truthy(use_xhtml)?;
            eval_nl2br_result(value, use_xhtml, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Inserts an HTML line break before each PHP newline sequence while preserving bytes.
pub(in crate::interpreter) fn eval_nl2br_result(
    value: RuntimeCellHandle,
    use_xhtml: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let br = if use_xhtml {
        b"<br />".as_slice()
    } else {
        b"<br>".as_slice()
    };
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'\r' || byte == b'\n' {
            output.extend_from_slice(br);
            output.push(byte);
            if index + 1 < bytes.len()
                && ((byte == b'\r' && bytes[index + 1] == b'\n')
                    || (byte == b'\n' && bytes[index + 1] == b'\r'))
            {
                output.push(bytes[index + 1]);
                index += 2;
                continue;
            }
        } else {
            output.push(byte);
        }
        index += 1;
    }
    values.string_bytes_value(&output)
}
