//! Purpose:
//! Declarative eval registry entry for `addcslashes`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Mirrors the native helper's range expansion, short C escapes, and octal escapes.
//! - Both the direct and pre-evaluated value hooks share one byte-level implementation.

eval_builtin! {
    contract: "addcslashes",
    area: String,
    direct: Addcslashes,
    values: Addcslashes,
}

use super::super::super::*;

/// Evaluates PHP `addcslashes(...)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_addcslashes(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value, characters] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    let characters = eval_expr(characters, context, scope, values)?;
    eval_addcslashes_result(value, characters, values)
}

/// Escapes bytes selected by a literal character list and ascending ranges.
pub(in crate::interpreter) fn eval_addcslashes_result(
    value: RuntimeCellHandle,
    characters: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let characters = values.string_bytes(characters)?;
    let mut table = [false; 256];
    let mut index = 0usize;
    let mut previous = None;
    while index < characters.len() {
        let current = characters[index];
        if current == b'.'
            && index + 2 < characters.len()
            && characters[index + 1] == b'.'
            && previous.is_some()
        {
            let start = previous.expect("range start checked above");
            let end = characters[index + 2];
            if start <= end {
                for byte in start..=end {
                    table[usize::from(byte)] = true;
                }
            }
            index += 3;
            previous = None;
            continue;
        }
        table[usize::from(current)] = true;
        previous = Some(current);
        index += 1;
    }
    let mut output = Vec::with_capacity(bytes.len().saturating_mul(4));
    for byte in bytes {
        if !table[usize::from(byte)] {
            output.push(byte);
            continue;
        }
        output.push(b'\\');
        match byte {
            7 => output.push(b'a'),
            8 => output.push(b'b'),
            9 => output.push(b't'),
            10 => output.push(b'n'),
            11 => output.push(b'v'),
            12 => output.push(b'f'),
            13 => output.push(b'r'),
            32..=126 => output.push(byte),
            _ => {
                output.push(b'0' + ((byte >> 6) & 7));
                output.push(b'0' + ((byte >> 3) & 7));
                output.push(b'0' + (byte & 7));
            }
        }
    }
    values.string_bytes_value(&output)
}
