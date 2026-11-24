//! Purpose:
//! Converts PCNTL bridge records and PHP runtime arrays for Magician.
//!
//! Called from:
//! - PCNTL wait, signal-mask, affinity, signal-wait, and exec adapters.
//!
//! Key details:
//! - Signal-information output honors the bridge presence mask.
//! - Array reads use runtime iteration order and PHP integer coercion.

use super::*;
use crate::interpreter::builtins::collection_builder::EvalArrayBuilder;
use elephc_pcntl::{ElephcPcntlRUsage, ElephcPcntlSigInfo};

/// Describes the numeric prefix PHP accepts while coercing a string signal value.
struct EvalPcntlNumericPrefix<'a> {
    /// The numeric bytes without surrounding whitespace or a trailing nonnumeric suffix.
    bytes: &'a [u8],
    /// Whether the accepted prefix has float syntax and therefore may lose precision.
    is_float: bool,
    /// Whether everything after the prefix is PHP whitespace rather than an invalid suffix.
    fully_numeric: bool,
}

/// Returns whether a byte is whitespace accepted around a PHP numeric string.
fn eval_pcntl_is_numeric_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

/// Scans the decimal numeric prefix PHP accepts for weak integer coercion.
fn eval_pcntl_numeric_prefix(bytes: &[u8]) -> Option<EvalPcntlNumericPrefix<'_>> {
    let mut start = 0;
    while start < bytes.len() && eval_pcntl_is_numeric_whitespace(bytes[start]) {
        start += 1;
    }

    let mut cursor = start;
    if bytes.get(cursor).is_some_and(|byte| matches!(byte, b'+' | b'-')) {
        cursor += 1;
    }

    let integer_start = cursor;
    while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
        cursor += 1;
    }
    let integer_digits = cursor - integer_start;

    let mut is_float = false;
    let mut fractional_digits = 0;
    if bytes.get(cursor) == Some(&b'.') {
        is_float = true;
        cursor += 1;
        let fractional_start = cursor;
        while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
            cursor += 1;
        }
        fractional_digits = cursor - fractional_start;
    }
    if integer_digits + fractional_digits == 0 {
        return None;
    }

    if bytes.get(cursor).is_some_and(|byte| matches!(byte, b'e' | b'E')) {
        let exponent_marker = cursor;
        let mut exponent_cursor = cursor + 1;
        if bytes
            .get(exponent_cursor)
            .is_some_and(|byte| matches!(byte, b'+' | b'-'))
        {
            exponent_cursor += 1;
        }
        let exponent_start = exponent_cursor;
        while bytes
            .get(exponent_cursor)
            .is_some_and(u8::is_ascii_digit)
        {
            exponent_cursor += 1;
        }
        if exponent_cursor > exponent_start {
            is_float = true;
            cursor = exponent_cursor;
        } else {
            cursor = exponent_marker;
        }
    }

    let numeric_end = cursor;
    while cursor < bytes.len() && eval_pcntl_is_numeric_whitespace(bytes[cursor]) {
        cursor += 1;
    }
    Some(EvalPcntlNumericPrefix {
        bytes: &bytes[start..numeric_end],
        is_float,
        fully_numeric: cursor == bytes.len(),
    })
}

/// Coerces one string signal value with PHP's leading-numeric warning and precision notice.
fn eval_pcntl_numeric_string_to_int(
    bytes: &[u8],
    name: &str,
    argument: usize,
    parameter: &str,
    element_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    let Some(prefix) = eval_pcntl_numeric_prefix(bytes) else {
        let _: RuntimeCellHandle = eval_throw_type_error(
            &format!(
                "{name}(): Argument #{argument} (${parameter}) {element_name} must be of type int, string given"
            ),
            context,
            values,
        )?;
        return Err(EvalStatus::RuntimeFatal);
    };

    if !prefix.fully_numeric {
        values.warning("Warning: A non-numeric value encountered\n")?;
    }

    let numeric = std::str::from_utf8(prefix.bytes).map_err(|_| EvalStatus::RuntimeFatal)?;
    if prefix.is_float {
        let float = numeric
            .parse::<f64>()
            .map_err(|_| EvalStatus::RuntimeFatal)?;
        let integer = float as i64;
        if float.is_finite() && float.trunc() != float {
            values.warning(&format!(
                "Deprecated: Implicit conversion from float-string \"{}\" to int loses precision\n",
                String::from_utf8_lossy(bytes)
            ))?;
        }
        Ok(integer)
    } else {
        numeric.parse::<i64>().or_else(|_| {
            numeric
                .parse::<f64>()
                .map(|float| float as i64)
                .map_err(|_| EvalStatus::RuntimeFatal)
        })
    }
}

/// Copies an eval indexed array into widened native integers.
pub(super) fn eval_pcntl_int_array(
    array: RuntimeCellHandle,
    name: &str,
    argument: usize,
    parameter: &str,
    element_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<i64>, EvalStatus> {
    if !values.is_array_like(array)? {
        return Err(EvalStatus::RuntimeFatal);
    }
    let len = values.array_len(array)?;
    let mut result = Vec::with_capacity(len);
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        with_eval_value_lease(key, context, values, |key, context, values| {
            let value = values.array_get(array, key)?;
            with_eval_value_lease(value, context, values, |value, context, values| {
                let integer = eval_pcntl_int_array_element(
                    value, name, argument, parameter, element_name, context, values,
                )?;
                result.push(integer);
                Ok(())
            })
        })?;
    }
    Ok(result)
}

/// Coerces an owned array read while its caller tracks the element lease.
fn eval_pcntl_int_array_element(
    value: RuntimeCellHandle,
    name: &str,
    argument: usize,
    parameter: &str,
    element_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    let tag = values.type_tag(value)?;
    if tag == EVAL_TAG_STRING {
        eval_pcntl_numeric_string_to_int(
            &values.string_bytes(value)?,
            name,
            argument,
            parameter,
            element_name,
            context,
            values,
        )
    } else if tag == EVAL_TAG_OBJECT {
        let identity = values.object_identity(value)?;
        let class_name = match context.dynamic_object_class_name(identity) {
            Some(name) => name,
            None => runtime_object_class_name(value, values)?,
        };
        let _: RuntimeCellHandle = eval_throw_type_error(
            &format!(
                "{name}(): Argument #{argument} (${parameter}) {element_name} must be of type int, {class_name} given"
            ),
            context,
            values,
        )?;
        return Err(EvalStatus::RuntimeFatal)?;
    } else if tag == EVAL_TAG_CALLABLE {
        let _: RuntimeCellHandle = eval_throw_type_error(
            &format!(
                "{name}(): Argument #{argument} (${parameter}) {element_name} must be of type int, Closure given"
            ),
            context,
            values,
        )?;
        return Err(EvalStatus::RuntimeFatal)?;
    } else {
        eval_int_value(value, values)
    }
}

/// Builds a fresh indexed PHP integer array from a native slice.
pub(super) fn eval_pcntl_indexed_int_array(
    input: &[i64],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = EvalArrayBuilder::indexed(values, input.len())?;
    for (position, value) in input.iter().copied().enumerate() {
        result.index(position, |values| values.int(value))?;
    }
    Ok(result.finish())
}

/// Builds PHP's 17-field resource-usage associative array.
pub(super) fn eval_pcntl_rusage_array(
    usage: &ElephcPcntlRUsage,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let fields = [
        ("ru_oublock", usage.ru_oublock),
        ("ru_inblock", usage.ru_inblock),
        ("ru_msgsnd", usage.ru_msgsnd),
        ("ru_msgrcv", usage.ru_msgrcv),
        ("ru_maxrss", usage.ru_maxrss),
        ("ru_ixrss", usage.ru_ixrss),
        ("ru_idrss", usage.ru_idrss),
        ("ru_minflt", usage.ru_minflt),
        ("ru_majflt", usage.ru_majflt),
        ("ru_nsignals", usage.ru_nsignals),
        ("ru_nvcsw", usage.ru_nvcsw),
        ("ru_nivcsw", usage.ru_nivcsw),
        ("ru_nswap", usage.ru_nswap),
        ("ru_utime.tv_usec", usage.ru_utime_tv_usec),
        ("ru_utime.tv_sec", usage.ru_utime_tv_sec),
        ("ru_stime.tv_usec", usage.ru_stime_tv_usec),
        ("ru_stime.tv_sec", usage.ru_stime_tv_sec),
    ];
    eval_pcntl_assoc_int_fields(&fields, values)
}

/// Builds a PHP signal-information array from fields marked present by the bridge.
pub(super) fn eval_pcntl_siginfo_array(
    info: &ElephcPcntlSigInfo,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let candidates = [
        ("signo", info.signo, elephc_pcntl::SIGINFO_SIGNO, false),
        ("errno", info.error, elephc_pcntl::SIGINFO_ERRNO, false),
        ("code", info.code, elephc_pcntl::SIGINFO_CODE, false),
        ("status", info.status, elephc_pcntl::SIGINFO_STATUS, false),
        ("utime", info.utime, elephc_pcntl::SIGINFO_UTIME, true),
        ("stime", info.stime, elephc_pcntl::SIGINFO_STIME, true),
        ("pid", info.pid, elephc_pcntl::SIGINFO_PID, false),
        ("uid", info.uid, elephc_pcntl::SIGINFO_UID, false),
        ("addr", info.address, elephc_pcntl::SIGINFO_ADDRESS, true),
        ("band", info.band, elephc_pcntl::SIGINFO_BAND, false),
        ("fd", info.fd, elephc_pcntl::SIGINFO_FD, false),
    ];
    let mut result = EvalArrayBuilder::assoc(values, candidates.len())?;
    for (key, value, bit, is_float) in candidates {
        if info.present & bit != 0 {
            result.string(key, |values| {
                if is_float { values.float(value as f64) } else { values.int(value) }
            })?;
        }
    }
    Ok(result.finish())
}

/// Builds an associative integer array from a static field list.
fn eval_pcntl_assoc_int_fields(
    fields: &[(&str, i64)],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = EvalArrayBuilder::assoc(values, fields.len())?;
    for (key, value) in fields.iter().copied() {
        result.string(key, |values| values.int(value))?;
    }
    Ok(result.finish())
}
