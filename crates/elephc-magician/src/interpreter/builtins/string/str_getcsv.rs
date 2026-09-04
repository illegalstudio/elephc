//! Purpose:
//! Declarative eval registry entry for `str_getcsv`, and the CSV record parser behind it.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Applies the same rule as the compiled `__rt_str_getcsv` helper, derived from `php -n`
//!   8.5.6 over 22 inputs: strip ONE trailing line terminator; if nothing is left the answer
//!   is the single-element `[null]` array php-src builds with `php_bc_fgetcsv_empty_line()`;
//!   strip ONE more; then parse with a newline as ordinary data.
//! - A newline is only structural at the very end. `"a\nb"` is one field containing a
//!   newline, and `"\n\n"` yields one empty field while `"\n"` yields the single-element
//!   answer from the early exit.

eval_builtin! {
    contract: "str_getcsv",
    area: String,
    direct: StrGetcsv,
    values: StrGetcsv,
}

use super::super::super::*;

/// Evaluates PHP `str_getcsv()` over its subject and optional control characters.
pub(in crate::interpreter) fn eval_builtin_str_getcsv(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.is_empty() || args.len() > 4 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let subject = eval_expr(&args[0], context, scope, values)?;
    let mut controls = Vec::new();
    for arg in &args[1..] {
        controls.push(eval_expr(arg, context, scope, values)?);
    }
    eval_str_getcsv_result(subject, &controls, context, values)
}

/// Parses one CSV record out of an evaluated string.
pub(in crate::interpreter) fn eval_str_getcsv_result(
    subject: RuntimeCellHandle,
    controls: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    // php validates each control BEFORE it parses: exactly one character for the separator and
    // the enclosure, empty or one for the escape. An empty escape is doubling mode, which the
    // parser below spells as a ZERO escape byte — it is not a way to reach the `"\\"` default.
    use crate::interpreter::{eval_csv_control_byte, CsvControlArgument};
    let separator = eval_csv_control_byte(
        controls.first().copied(),
        b',',
        CsvControlArgument {
            function: "str_getcsv",
            position: 2,
            parameter: "separator",
            empty_allowed: false,
        },
        context,
        values,
    )?;
    let enclosure = eval_csv_control_byte(
        controls.get(1).copied(),
        b'"',
        CsvControlArgument {
            function: "str_getcsv",
            position: 3,
            parameter: "enclosure",
            empty_allowed: false,
        },
        context,
        values,
    )?;
    let escape = eval_csv_control_byte(
        controls.get(2).copied(),
        b'\\',
        CsvControlArgument {
            function: "str_getcsv",
            position: 4,
            parameter: "escape",
            empty_allowed: true,
        },
        context,
        values,
    )?;

    let subject = values.string_bytes(subject)?;
    let trimmed = strip_one_terminator(&subject);
    let mut result = values.array_new(0)?;
    if trimmed.is_empty() {
        // php-src's `php_fgetcsv()` reports NO RECORD here and its callers substitute
        // `php_bc_fgetcsv_empty_line()`: one element, holding null. Measured on `php -n` 8.5.6,
        // `str_getcsv("")` is `[NULL]` while `str_getcsv(" ")` is `[" "]`. A boxed cell holds
        // that null; the string slot this used to push could only hold `""`.
        let key = values.int(0)?;
        let null = values.null()?;
        result = values.array_set(result, key, null)?;
        return Ok(result);
    }
    let record = strip_one_terminator(trimmed);
    for field in parse_csv_record(record, separator, enclosure, escape) {
        // CSV fields are arbitrary bytes; the eval array stores them as PHP strings.
        let field = String::from_utf8_lossy(&field).into_owned();
        result = values.string_array_push(result, &field)?;
    }
    Ok(result)
}

/// Removes one trailing `\r\n`, `\n` or `\r`, treating `\r\n` as a single terminator.
fn strip_one_terminator(bytes: &[u8]) -> &[u8] {
    match bytes.last() {
        Some(b'\n') => {
            let rest = &bytes[..bytes.len() - 1];
            match rest.last() {
                Some(b'\r') => &rest[..rest.len() - 1],
                _ => rest,
            }
        }
        Some(b'\r') => &bytes[..bytes.len() - 1],
        _ => bytes,
    }
}

/// Splits one CSV record, unescaping enclosures the way php-src does.
///
/// A newline is ordinary data here: the caller has already removed the structural ones.
fn parse_csv_record(bytes: &[u8], separator: u8, enclosure: u8, escape: u8) -> Vec<Vec<u8>> {
    #[derive(Clone, Copy)]
    enum State {
        Outside,
        Field,
        Quoted,
        AfterEscape,
        AfterCloseQuote,
    }
    let mut fields = Vec::new();
    let mut field = Vec::new();
    let mut state = State::Outside;
    for &byte in bytes {
        match state {
            State::Outside => {
                if byte == separator {
                    fields.push(std::mem::take(&mut field));
                } else if byte == enclosure {
                    state = State::Quoted;
                } else {
                    field.push(byte);
                    state = State::Field;
                }
            }
            State::Field => {
                if byte == separator {
                    fields.push(std::mem::take(&mut field));
                    state = State::Outside;
                } else {
                    field.push(byte);
                }
            }
            State::Quoted => {
                if escape != 0 && byte == escape {
                    state = State::AfterEscape;
                } else if byte == enclosure {
                    state = State::AfterCloseQuote;
                } else {
                    field.push(byte);
                }
            }
            State::AfterEscape => {
                if byte != enclosure {
                    field.push(escape);
                }
                field.push(byte);
                state = State::Quoted;
            }
            State::AfterCloseQuote => {
                if byte == enclosure {
                    field.push(enclosure);
                    state = State::Quoted;
                } else if byte == separator {
                    fields.push(std::mem::take(&mut field));
                    state = State::Outside;
                } else {
                    field.push(enclosure);
                    field.push(byte);
                    state = State::Field;
                }
            }
        }
    }
    fields.push(field);
    fields
}
