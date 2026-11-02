//! Purpose:
//! Implements PHP `extract()` against the materialized eval activation scope.
//! Array keys become runtime-named scope entries consumed by eval and dynamic includes.
//!
//! Called from:
//! - Generated EIR backend assembly through `__elephc_eval_extract`.
//!
//! Key details:
//! - Extracted cells are transferred to the scope as owned values.
//! - `EXTR_*` mode values and the collision/prefix policy live in `crate::extract_policy`, shared
//!   with the tree-walking interpreter's own `extract()`
//!   (`crate::interpreter::builtins::array::extract`, which additionally supports `EXTR_REFS`) so
//!   the two backends cannot silently diverge on a mode number or a collision rule.

use crate::abi::ElephcEvalScope;
use crate::errors::EvalStatus;
use crate::extract_policy::{extract_key_name, extract_target_name, EXTR_IF_EXISTS, EXTR_REFS};
use crate::interpreter::RuntimeValueOps;
use crate::runtime_hooks::ElephcRuntimeOps;
use crate::scope::ScopeCellOwnership;
use crate::value::{RuntimeCell, RuntimeCellHandle};
use std::slice;

/// Extracts array entries into a materialized caller scope and returns the count.
///
/// A negative result is an [`EvalStatus`] code negated for ABI transport. The generated
/// caller converts that status into the same fatal path used by runtime `eval()` failures.
///
/// # Safety
/// `scope` and `array` must be valid bridge/runtime handles. `prefix_ptr` must be readable
/// for `prefix_len` bytes when the length is non-zero.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_extract(
    scope: *mut ElephcEvalScope,
    array: *mut RuntimeCell,
    flags: i64,
    prefix_ptr: *const u8,
    prefix_len: u64,
) -> i64 {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_extract");
    std::panic::catch_unwind(|| unsafe {
        extract_inner(scope, array, flags, prefix_ptr, prefix_len)
    })
    .unwrap_or_else(|_| -i64::from(EvalStatus::RuntimeFatal.code()))
}

/// Runs the `extract()` operation after the exported panic boundary is installed.
///
/// # Safety
/// Mirrors [`__elephc_eval_extract`].
unsafe fn extract_inner(
    scope: *mut ElephcEvalScope,
    array: *mut RuntimeCell,
    flags: i64,
    prefix_ptr: *const u8,
    prefix_len: u64,
) -> i64 {
    let Some(scope) = scope.as_mut() else {
        return extract_error(EvalStatus::RuntimeFatal);
    };
    if array.is_null() || (prefix_len > 0 && prefix_ptr.is_null()) {
        return extract_error(EvalStatus::RuntimeFatal);
    }
    let Ok(prefix_len) = usize::try_from(prefix_len) else {
        return extract_error(EvalStatus::RuntimeFatal);
    };
    let prefix = if prefix_len == 0 {
        &[]
    } else {
        slice::from_raw_parts(prefix_ptr, prefix_len)
    };
    let Ok(prefix) = std::str::from_utf8(prefix) else {
        return extract_error(EvalStatus::RuntimeFatal);
    };
    let mode = flags & !EXTR_REFS;
    if !(0..=EXTR_IF_EXISTS).contains(&mode) || flags & EXTR_REFS != 0 {
        return extract_error(EvalStatus::UnsupportedConstruct);
    }

    let array = RuntimeCellHandle::from_raw(array);
    let mut values = ElephcRuntimeOps::new();
    let Ok(len) = values.array_len(array) else {
        return extract_error(EvalStatus::RuntimeFatal);
    };
    let mut extracted = 0_i64;
    for position in 0..len {
        let Ok(key) = values.array_iter_key(array, position) else {
            return extract_error(EvalStatus::RuntimeFatal);
        };
        let name = extract_key_name(&mut values, key);
        let Ok(Some(name)) = name else {
            let _ = values.release(key);
            if name.is_err() {
                return extract_error(EvalStatus::RuntimeFatal);
            }
            continue;
        };
        let Some(target) = extract_target_name(scope, &name, prefix, mode) else {
            let _ = values.release(key);
            continue;
        };
        let value = values.array_get(array, key);
        let _ = values.release(key);
        let Ok(value) = value else {
            return extract_error(EvalStatus::RuntimeFatal);
        };
        if let Some(replaced) = scope.set(target, value, ScopeCellOwnership::Owned) {
            let _ = values.release(replaced);
        }
        extracted += 1;
    }
    extracted
}

/// Encodes one eval status as a negative ABI return value.
fn extract_error(status: EvalStatus) -> i64 {
    -i64::from(status.code())
}
