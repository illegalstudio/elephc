//! Purpose:
//! Declarative eval registry entry for `ctype_alnum`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the existing ASCII ctype hook.

eval_builtin! {
    contract: "ctype_alnum",
    area: String,
    direct: Ctype,
    values: Ctype,
}

use super::super::super::*;

/// Evaluates PHP `ctype_alnum(...)` over one eval string expression.
pub(in crate::interpreter) fn eval_builtin_ctype_alnum(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::ctype_alnum::eval_builtin_ctype_named("ctype_alnum", args, context, scope, values)
}

/// Returns the PHP boolean result for `ctype_alnum(...)` from one evaluated value.
pub(in crate::interpreter) fn eval_ctype_alnum_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::ctype_alnum::eval_ctype_named_result("ctype_alnum", value, values)
}

/// Evaluates a named PHP `ctype_*` predicate over one eval string expression.
pub(in crate::interpreter) fn eval_builtin_ctype_named(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_ctype_named_result(name, value, values)
}

/// Returns the PHP boolean result for one named ASCII `ctype_*` byte-string check.
pub(in crate::interpreter) fn eval_ctype_named_result(
    name: &str,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let mut matches = !bytes.is_empty();
    for byte in bytes {
        if !eval_ctype_byte_matches(name, byte)? {
            matches = false;
            break;
        }
    }
    values.bool_value(matches)
}

/// Checks one byte against the class of the selected predicate, on this platform's libc table.
pub(in crate::interpreter) fn eval_ctype_byte_matches(
    name: &str,
    byte: u8,
) -> Result<bool, EvalStatus> {
    let class = elephc_builtin_contract::ctype_class_of(name).ok_or(EvalStatus::UnsupportedConstruct)?;
    Ok(elephc_builtin_contract::ctype_class_bits(byte, cfg!(target_os = "macos")) & class != 0)
}
