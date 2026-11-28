//! Purpose:
//! Eval registry entry and bridge inventory for `openssl_get_cipher_methods`.
//!
//! Called from:
//! - The shared OpenSSL direct and evaluated-argument hooks.
//!
//! Key details:
//! - The bridge owns the canonical 12-method list and aliases currently return that same list.

eval_builtin! {
    contract: "openssl_get_cipher_methods",
    area: String,
    direct: Openssl,
    values: Openssl,
}

use crate::interpreter::builtins::collection_builder::EvalArrayBuilder;
use super::super::super::*;

/// Evaluates positional `openssl_get_cipher_methods()` expressions.
pub(in crate::interpreter) fn eval_builtin_openssl_get_cipher_methods(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let aliases = match args {
        [] => Vec::new(),
        [aliases] => vec![eval_expr(aliases, context, scope, values)?],
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    eval_openssl_get_cipher_methods_values_result(&aliases, values)
}

/// Builds the indexed cipher-method array from the bridge's packed inventory.
pub(in crate::interpreter) fn eval_openssl_get_cipher_methods_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let aliases = match evaluated_args {
        [] => false,
        [aliases] => values.truthy(*aliases)?,
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let mut packed_length = 0_usize;
    let probe = unsafe {
        elephc_crypto::elephc_crypto_cipher_methods(
            i32::from(aliases),
            std::ptr::null_mut(),
            0,
            &mut packed_length,
        )
    };
    if !openssl_methods_probe_succeeded(probe) {
        return values.array_new(0);
    }
    let mut packed = vec![0_u8; packed_length];
    let count = unsafe {
        elephc_crypto::elephc_crypto_cipher_methods(
            i32::from(aliases),
            packed.as_mut_ptr(),
            packed.len(),
            &mut packed_length,
        )
    };
    let Some(count) = openssl_methods_count(count) else {
        return values.array_new(0);
    };
    if packed_length > packed.len() {
        return Err(EvalStatus::RuntimeFatal);
    }
    packed.truncate(packed_length);
    let methods = packed
        .split(|byte| *byte == 0)
        .filter(|method| !method.is_empty())
        .take(count)
        .collect::<Vec<_>>();
    if methods.len() != count {
        return Err(EvalStatus::RuntimeFatal);
    }
    let mut result = EvalArrayBuilder::indexed(values, methods.len())?;
    for (index, method) in methods.iter().enumerate() {
        result.index(index, |values| values.string_bytes_value(method))?;
    }
    Ok(result.finish())
}

/// Returns whether the size probe produced the bridge's expected retry status.
fn openssl_methods_probe_succeeded(status: isize) -> bool {
    status == elephc_crypto::CIPHER_ERR_OUTPUT_TOO_SMALL as isize
}

/// Converts a successful bridge method count while rejecting negative status codes.
fn openssl_methods_count(status: isize) -> Option<usize> {
    usize::try_from(status).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies anomalous probe and populate statuses map to the empty-array fallback path.
    #[test]
    fn openssl_method_status_helpers_reject_bridge_failures() {
        assert!(openssl_methods_probe_succeeded(
            elephc_crypto::CIPHER_ERR_OUTPUT_TOO_SMALL as isize
        ));
        assert!(!openssl_methods_probe_succeeded(
            elephc_crypto::CIPHER_ERR_INVALID_ARGUMENT as isize
        ));
        assert_eq!(openssl_methods_count(12), Some(12));
        assert_eq!(
            openssl_methods_count(elephc_crypto::CIPHER_ERR_INVALID_ARGUMENT as isize),
            None
        );
    }
}
