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
use elephc_pcntl::{ElephcPcntlRUsage, ElephcPcntlSigInfo};

/// Copies an eval indexed array into widened native integers.
pub(super) fn eval_pcntl_int_array(
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<i64>, EvalStatus> {
    if !values.is_array_like(array)? {
        return Err(EvalStatus::RuntimeFatal);
    }
    let len = values.array_len(array)?;
    let mut result = Vec::with_capacity(len);
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, key)?;
        result.push(eval_int_value(value, values)?);
    }
    Ok(result)
}

/// Builds a fresh indexed PHP integer array from a native slice.
pub(super) fn eval_pcntl_indexed_int_array(
    input: &[i64],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.array_new(input.len())?;
    for (position, value) in input.iter().copied().enumerate() {
        let key = values.int(position as i64)?;
        let value = values.int(value)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
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
        ("signo", info.signo, elephc_pcntl::SIGINFO_SIGNO),
        ("errno", info.error, elephc_pcntl::SIGINFO_ERRNO),
        ("code", info.code, elephc_pcntl::SIGINFO_CODE),
        ("status", info.status, elephc_pcntl::SIGINFO_STATUS),
        ("pid", info.pid, elephc_pcntl::SIGINFO_PID),
        ("uid", info.uid, elephc_pcntl::SIGINFO_UID),
        ("addr", info.address, elephc_pcntl::SIGINFO_ADDRESS),
    ];
    let extra_capacity = if cfg!(target_os = "linux") { 4 } else { 0 };
    let mut result = values.assoc_new(candidates.len() + extra_capacity)?;
    for (key, value, bit) in candidates {
        if info.present & bit != 0 {
            result = eval_pcntl_assoc_set_int(result, key, value, values)?;
        }
    }
    #[cfg(target_os = "linux")]
    {
        for (key, value, bit) in [
            ("utime", info.utime, elephc_pcntl::SIGINFO_UTIME),
            ("stime", info.stime, elephc_pcntl::SIGINFO_STIME),
            ("band", info.band, elephc_pcntl::SIGINFO_BAND),
            ("fd", info.fd, elephc_pcntl::SIGINFO_FD),
        ] {
            if info.present & bit != 0 {
                result = eval_pcntl_assoc_set_int(result, key, value, values)?;
            }
        }
    }
    Ok(result)
}

/// Builds an associative integer array from a static field list.
fn eval_pcntl_assoc_int_fields(
    fields: &[(&str, i64)],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.assoc_new(fields.len())?;
    for (key, value) in fields.iter().copied() {
        result = eval_pcntl_assoc_set_int(result, key, value, values)?;
    }
    Ok(result)
}

/// Inserts one string-keyed integer into an eval associative array.
fn eval_pcntl_assoc_set_int(
    array: RuntimeCellHandle,
    key: &str,
    value: i64,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let key = values.string(key)?;
    let value = values.int(value)?;
    values.array_set(array, key, value)
}
