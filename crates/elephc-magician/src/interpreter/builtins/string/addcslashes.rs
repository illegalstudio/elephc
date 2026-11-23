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

/// The warning PHP emits when a range marker has no character on its left.
const ADDCSLASHES_LEFT_WARNING: &str =
    "Warning: addcslashes(): Invalid '..'-range, no character to the left of '..'\n";

/// The warning PHP emits when a range marker has no character on its right.
const ADDCSLASHES_RIGHT_WARNING: &str =
    "Warning: addcslashes(): Invalid '..'-range, no character to the right of '..'\n";

/// The warning PHP emits when a range marker descends.
const ADDCSLASHES_INCREMENT_WARNING: &str =
    "Warning: addcslashes(): Invalid '..'-range, '..'-range needs to be incrementing\n";

/// The warning PHP emits for a range marker it cannot classify further.
const ADDCSLASHES_PLAIN_WARNING: &str = "Warning: addcslashes(): Invalid '..'-range\n";

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
    while index < characters.len() {
        let current = characters[index];
        if index + 3 < characters.len()
            && characters[index + 1] == b'.'
            && characters[index + 2] == b'.'
            && characters[index + 3] >= current
        {
            // php_charmask() reads the range as `current..characters[index + 3]`, so a
            // three-dot list such as `H...o` starts the range at the first dot.
            for byte in current..=characters[index + 3] {
                table[usize::from(byte)] = true;
            }
            index += 4;
            continue;
        }
        if index + 1 < characters.len() && current == b'.' && characters[index + 1] == b'.' {
            // The remaining diagnostics mirror php_charmask()'s ordered checks. The dot
            // bytes are not added to the mask on this path; later iterations mark the
            // second dot when it cannot open another marker.
            if index == 0 {
                values.warning(ADDCSLASHES_LEFT_WARNING)?;
                index += 1;
                continue;
            }
            if index + 2 >= characters.len() {
                values.warning(ADDCSLASHES_RIGHT_WARNING)?;
                index += 1;
                continue;
            }
            if characters[index - 1] > characters[index + 2] {
                values.warning(ADDCSLASHES_INCREMENT_WARNING)?;
                index += 1;
                continue;
            }
            values.warning(ADDCSLASHES_PLAIN_WARNING)?;
            index += 1;
            continue;
        }
        table[usize::from(current)] = true;
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
