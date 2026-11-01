//! Purpose:
//! Eval registry entry and implementation for `unpack`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks` direct and evaluated-argument dispatch.
//!
//! Key details:
//! - Supports PHP's integer codes (`c C s S n v l L N V q Q J P`), repeat counts and `*`,
//!   `/`-separated segments, named fields (`Nlen`, `Vexpiry/Nctime`), and the byte offset.
//!   These are the forms the Symfony example's vendor tree reaches: `C*` in the mbstring
//!   polyfill's width and ord helpers and in symfony/string, `n*`, `Nlen`, `Vexpiry/Nctime`.
//! - Unnamed fields take PHP's one-based integer keys; a named field repeated N times takes
//!   `name1`..`nameN`. Short input returns `false` with PHP's warning.
//! - Any other code is refused with a runtime fatal rather than guessed.

use super::super::super::*;

eval_builtin! {
    contract: "unpack",
    area: String,
    direct: Unpack,
    values: Unpack,
}

/// Evaluates direct `unpack(format, string[, offset])` calls.
pub(in crate::interpreter) fn eval_builtin_unpack(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(2..=3).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let mut evaluated = Vec::with_capacity(args.len());
    for arg in args {
        evaluated.push(eval_expr(arg, context, scope, values)?);
    }
    eval_unpack_values_result(&evaluated, values)
}

/// Evaluates `unpack()` over already evaluated arguments.
pub(in crate::interpreter) fn eval_unpack_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (format, data, offset) = match evaluated_args {
        [format, data] => (*format, *data, 0),
        [format, data, offset] => {
            let offset = values.cast_int(*offset)?;
            let offset = values.raw_value_word(offset)? as i64;
            (*format, *data, offset)
        }
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let format = values.string_bytes(format)?;
    let data = values.string_bytes(data)?;
    if offset < 0 || offset as usize > data.len() {
        values.warning("Warning: unpack(): Argument #3 ($offset) must be contained in argument #2 ($data)\n")?;
        return values.bool_value(false);
    }
    let data = &data[offset as usize..];
    let fields = match parse_unpack_format(&format) {
        Some(fields) => fields,
        None => return Err(EvalStatus::RuntimeFatal),
    };
    let result = values.assoc_new(fields.len())?;
    let mut position = 0usize;
    for field in &fields {
        let count = match field.count {
            Repeat::Count(count) => count,
            Repeat::Star => (data.len().saturating_sub(position)) / field.width,
        };
        for index in 0..count {
            if position + field.width > data.len() {
                values.warning(&format!(
                    "Warning: unpack(): Type {}: not enough input, need {}, have {}\n",
                    field.code as char,
                    field.width,
                    data.len().saturating_sub(position)
                ))?;
                return values.bool_value(false);
            }
            let number = read_field(field, &data[position..position + field.width]);
            position += field.width;
            let key = if field.name.is_empty() {
                values.int(index as i64 + 1)?
            } else if matches!(field.count, Repeat::Count(1)) {
                values.string_bytes_value(&field.name)?
            } else {
                let mut name = field.name.clone();
                name.extend_from_slice((index + 1).to_string().as_bytes());
                values.string_bytes_value(&name)?
            };
            let value = values.int(number)?;
            values.array_set(result, key, value)?;
        }
    }
    Ok(result)
}

/// How many times one format code repeats.
#[derive(Clone, Copy)]
enum Repeat {
    Count(usize),
    Star,
}

/// One parsed format segment.
struct Field {
    code: u8,
    width: usize,
    count: Repeat,
    name: Vec<u8>,
}

/// Parses `C*`, `n2`, `Nlen`, `Vexpiry/Nctime`-style formats; `None` for unsupported codes.
fn parse_unpack_format(format: &[u8]) -> Option<Vec<Field>> {
    let mut fields = Vec::new();
    for segment in format.split(|byte| *byte == b'/') {
        let (&code, rest) = segment.split_first()?;
        let width = code_width(code)?;
        let mut digits = 0;
        while digits < rest.len() && rest[digits].is_ascii_digit() {
            digits += 1;
        }
        let (count, name) = if rest.first() == Some(&b'*') {
            (Repeat::Star, rest[1..].to_vec())
        } else if digits > 0 {
            let count = std::str::from_utf8(&rest[..digits]).ok()?.parse().ok()?;
            (Repeat::Count(count), rest[digits..].to_vec())
        } else {
            (Repeat::Count(1), rest.to_vec())
        };
        fields.push(Field {
            code,
            width,
            count,
            name,
        });
    }
    Some(fields)
}

/// Returns the byte width of one supported integer code.
fn code_width(code: u8) -> Option<usize> {
    Some(match code {
        b'c' | b'C' => 1,
        b's' | b'S' | b'n' | b'v' => 2,
        b'l' | b'L' | b'N' | b'V' => 4,
        b'q' | b'Q' | b'J' | b'P' => 8,
        _ => return None,
    })
}

/// Decodes one field's bytes with its code's byte order and signedness.
fn read_field(field: &Field, bytes: &[u8]) -> i64 {
    let big_endian = matches!(field.code, b'n' | b'N' | b'J');
    let little_endian = matches!(field.code, b'v' | b'V' | b'P');
    let mut raw: u64 = 0;
    if big_endian || (!little_endian && cfg!(target_endian = "big")) {
        for byte in bytes {
            raw = (raw << 8) | u64::from(*byte);
        }
    } else {
        for byte in bytes.iter().rev() {
            raw = (raw << 8) | u64::from(*byte);
        }
    }
    match field.code {
        b'c' => raw as u8 as i8 as i64,
        b's' => raw as u16 as i16 as i64,
        b'l' => raw as u32 as i32 as i64,
        _ => raw as i64,
    }
}
