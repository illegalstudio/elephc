//! Purpose:
//! Eval registry entry and implementation for `addcslashes`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - A transcription of php-src's `php_addcslashes_str` over `php_charmask`: the character
//!   list accepts `a..z` ranges, the seven control bytes 7..13 escape as `\a\b\t\n\v\f\r`,
//!   any other byte outside 32..126 as a three-digit octal escape, and the rest as `\` + byte.
//! - The compiled side is the AST prelude `src/addcslashes_prelude.rs`; interpreted code cannot
//!   reach a compiler prelude, which is why this home exists (Twig's `Compiler` is interpreted).

use super::super::super::*;

eval_builtin! {
    contract: "addcslashes",
    area: String,
    direct: AddCSlashes,
    values: AddCSlashes,
}

/// Evaluates PHP `addcslashes(...)` from unevaluated call-site expressions.
pub(in crate::interpreter) fn eval_builtin_addcslashes(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut evaluated = Vec::with_capacity(args.len());
    for arg in args {
        evaluated.push(eval_expr(arg, context, scope, values)?);
    }
    eval_addcslashes_values(&evaluated, context, values)
}

/// Evaluates PHP `addcslashes(...)` from already-evaluated argument cells.
pub(in crate::interpreter) fn eval_addcslashes_values(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [string, characters] = evaluated_args else {
        return eval_throw_argument_count_error(
            &format!(
                "addcslashes() expects exactly 2 arguments, {} given",
                evaluated_args.len()
            ),
            context,
            values,
        );
    };
    let bytes = values.string_bytes(*string)?;
    let characters = values.string_bytes(*characters)?;
    values.string_bytes_value(&addcslashes_bytes(&bytes, &characters))
}

/// Builds php's 256-entry character mask, expanding `x..y` ranges.
fn addcslashes_mask(characters: &[u8]) -> [bool; 256] {
    let mut mask = [false; 256];
    let mut index = 0;
    while index < characters.len() {
        let first = characters[index];
        if index + 3 < characters.len()
            && characters[index + 1] == b'.'
            && characters[index + 2] == b'.'
            && characters[index + 3] >= first
        {
            for byte in first..=characters[index + 3] {
                mask[byte as usize] = true;
            }
            index += 4;
        } else {
            mask[first as usize] = true;
            index += 1;
        }
    }
    mask
}

/// Escapes every byte of `bytes` that `characters` lists.
fn addcslashes_bytes(bytes: &[u8], characters: &[u8]) -> Vec<u8> {
    let mask = addcslashes_mask(characters);
    let mut output = Vec::with_capacity(bytes.len());
    for &byte in bytes {
        if !mask[byte as usize] {
            output.push(byte);
            continue;
        }
        output.push(b'\\');
        match byte {
            7..=13 => output.push(b"abtnvfr"[(byte - 7) as usize]),
            32..=126 => output.push(byte),
            _ => output.extend_from_slice(format!("{byte:03o}").as_bytes()),
        }
    }
    output
}
