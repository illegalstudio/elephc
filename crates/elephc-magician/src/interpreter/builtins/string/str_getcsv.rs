//! Purpose:
//! Declarative eval registry entry for `str_getcsv`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Parses with the port of php-src's `php_fgetcsv()` in `filesystem::csv_record`, the reader
//!   `fgetcsv()` shares: a blank subject is the single-element `[null]` array php-src builds with
//!   `php_bc_fgetcsv_empty_line()`, and a newline is only structural at the very end.
//! - An omitted `$escape` is deprecated since php 8.4, as for `fgetcsv()` and `fputcsv()`.

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
    let escape = crate::interpreter::eval_csv_escape_argument(
        controls.get(2).copied(),
        "str_getcsv",
        4,
        context,
        values,
    )?;

    // php hands the whole subject to `php_fgetcsv()` with no stream behind it, so a quoted
    // field that runs past a newline simply ends with the subject.
    let subject = values.string_bytes(subject)?;
    let record = crate::interpreter::eval_php_fgetcsv(subject, separator, enclosure, escape, || None);
    crate::interpreter::eval_csv_record_array(record, values)
}
