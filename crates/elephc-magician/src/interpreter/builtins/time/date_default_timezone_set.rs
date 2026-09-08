//! Purpose:
//! Eval registry entry and implementation for `date_default_timezone_set`.
//!
//! Called from:
//! - `crate::interpreter::builtins::time` direct and by-value dispatch.
//!
//! Key details:
//! - Valid identifiers update the shared AOT request timezone when the runtime bridge is present.

use super::super::super::*;

eval_builtin! {
    contract: "date_default_timezone_set",
    area: Time,
    direct: Time,
    values: Time,
}

/// Evaluates PHP `date_default_timezone_set($timezoneId)`.
pub(in crate::interpreter) fn eval_builtin_date_default_timezone_set(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [timezone] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let timezone = eval_expr(timezone, context, scope, values)?;
    eval_date_default_timezone_set_result(timezone, context, values)
}

/// Validates and stores one eval-local default timezone identifier.
pub(in crate::interpreter) fn eval_date_default_timezone_set_result(
    timezone: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let timezone_handle = timezone;
    let timezone_bytes = values.string_bytes(timezone_handle)?;
    let Some(timezone) = std::str::from_utf8(&timezone_bytes).ok()
        .filter(|timezone| elephc_tz::timezone_identifier_valid(timezone))
    else {
        values.notice(&invalid_timezone_notice(&timezone_bytes))?;
        return values.bool_value(false);
    };
    if let Some(result) = values.runtime_builtin_call(
        elephc_builtin_contract::RuntimeBuiltinId::DateDefaultTimezoneSet,
        &[timezone_handle],
    )? {
        context.set_default_timezone(timezone.to_owned());
        return Ok(result);
    }
    context.set_default_timezone(timezone.to_owned());
    values.bool_value(true)
}

/// Builds a notice without replacing invalid UTF-8 bytes in the supplied identifier.
fn invalid_timezone_notice(identifier: &[u8]) -> Vec<u8> {
    let mut message = b"\nNotice: date_default_timezone_set(): Timezone ID '".to_vec();
    message.extend_from_slice(identifier);
    message.extend_from_slice(b"' is invalid\n");
    message
}

#[cfg(test)]
mod tests {
    /// Invalid identifier bytes survive notice construction unchanged.
    #[test]
    fn invalid_timezone_notice_preserves_php_bytes() {
        assert_eq!(super::invalid_timezone_notice(b"bad\xff\0zone"),
            b"\nNotice: date_default_timezone_set(): Timezone ID 'bad\xff\0zone' is invalid\n");
    }
}
