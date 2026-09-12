//! Purpose:
//! Eval registry entry and shared OpenSSL cipher dispatch for `openssl_encrypt`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks` and direct by-reference call handling.
//!
//! Key details:
//! - The crypto bridge consumes raw bytes; this module applies PHP Base64 semantics.
//! - Successful GCM calls write a fresh tag into the captured caller lvalue.

eval_builtin! {
    contract: "openssl_encrypt",
    area: String,
    direct: Openssl,
    values: Openssl,
}

use super::super::super::*;

const OPENSSL_RAW_DATA: u32 = 1;
const OPENSSL_ENCRYPT_ARG_COUNT: usize = 8;

/// Evaluates positional `openssl_encrypt()` expressions while preserving the tag lvalue.
pub(in crate::interpreter) fn eval_builtin_openssl_encrypt(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let args = args.iter().cloned().map(EvalCallArg::positional).collect::<Vec<_>>();
    eval_builtin_openssl_encrypt_call(&args, context, scope, values)
}

/// Evaluates a full `openssl_encrypt()` call with named arguments and tag writeback metadata.
pub(in crate::interpreter) fn eval_builtin_openssl_encrypt_call(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    with_eval_call_arguments(args, context, scope, values, |evaluated_args, context, _, values| {
        let (bound_args, _) = bind_evaluated_ref_builtin_args(
            &[
                "data",
                "cipher_algo",
                "passphrase",
                "options",
                "iv",
                "tag",
                "aad",
                "tag_length",
            ],
            &evaluated_args,
            false,
        )?;
        let mut bound = [None; OPENSSL_ENCRYPT_ARG_COUNT];
        for (index, arg) in bound_args.iter().enumerate() {
            bound[index] = arg.as_ref().map(|arg| arg.value);
        }
        for index in 0..3 {
            required_openssl_arg(&bound, index)?;
        }
        let tag_target = bound_args
            .get(5)
            .and_then(Option::as_ref)
            .map(|arg| arg.ref_target.as_ref().ok_or(EvalStatus::RuntimeFatal))
            .transpose()?;
        eval_openssl_encrypt_bound_result(&bound, tag_target, context, values)
    })
}

/// Applies by-value callable semantics for `openssl_encrypt()` after registry binding.
pub(in crate::interpreter) fn eval_openssl_encrypt_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(3..=OPENSSL_ENCRYPT_ARG_COUNT).contains(&evaluated_args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    if evaluated_args.len() >= 6 {
        values.warning(
            "openssl_encrypt(): Argument #6 ($tag) must be passed by reference, value given",
        )?;
    }
    let mut bound = [None; OPENSSL_ENCRYPT_ARG_COUNT];
    for (index, arg) in evaluated_args.iter().copied().enumerate() {
        bound[index] = Some(arg);
    }
    eval_openssl_encrypt_bound_result(&bound, None, context, values)
}

/// Dispatches one OpenSSL builtin from unevaluated positional expressions.
pub(in crate::interpreter) fn eval_builtin_openssl_declared_call(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "openssl_encrypt" => eval_builtin_openssl_encrypt(args, context, scope, values),
        "openssl_decrypt" => {
            super::openssl_decrypt::eval_builtin_openssl_decrypt(args, context, scope, values)
        }
        "openssl_cipher_iv_length" => {
            super::openssl_cipher_iv_length::eval_builtin_openssl_cipher_iv_length(
                args, context, scope, values,
            )
        }
        "openssl_get_cipher_methods" => {
            super::openssl_get_cipher_methods::eval_builtin_openssl_get_cipher_methods(
                args, context, scope, values,
            )
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Dispatches one OpenSSL builtin from already evaluated registry arguments.
pub(in crate::interpreter) fn eval_openssl_declared_values_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "openssl_encrypt" => {
            eval_openssl_encrypt_values_result(evaluated_args, context, values)
        }
        "openssl_decrypt" => {
            super::openssl_decrypt::eval_openssl_decrypt_values_result(evaluated_args, values)
        }
        "openssl_cipher_iv_length" => super::openssl_cipher_iv_length::
            eval_openssl_cipher_iv_length_values_result(evaluated_args, values),
        "openssl_get_cipher_methods" => super::openssl_get_cipher_methods::
            eval_openssl_get_cipher_methods_values_result(evaluated_args, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Encrypts bound PHP arguments through `elephc-crypto` and performs optional tag writeback.
fn eval_openssl_encrypt_bound_result(
    bound: &[Option<RuntimeCellHandle>; OPENSSL_ENCRYPT_ARG_COUNT],
    tag_target: Option<&EvalReferenceTarget>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let data = values.string_bytes(required_openssl_arg(bound, 0)?)?;
    let cipher = values.string_bytes(required_openssl_arg(bound, 1)?)?;
    let passphrase = values.string_bytes(required_openssl_arg(bound, 2)?)?;
    let options = optional_openssl_int_arg(bound, 3, 0, values)? as u32;
    let iv = optional_openssl_bytes_arg(bound, 4, values)?;
    let aad = optional_openssl_bytes_arg(bound, 6, values)?;
    let tag_length = optional_openssl_int_arg(bound, 7, 16, values)?;
    let tag_length = usize::try_from(tag_length).unwrap_or(usize::MAX);
    let output_capacity = data.len().checked_add(16).ok_or(EvalStatus::RuntimeFatal)?;
    let mut output = vec![0_u8; output_capacity];
    let mut output_length = 0_usize;
    let mut tag = vec![0_u8; 16];
    let mut tag_output_length = 0_usize;
    let status = unsafe {
        elephc_crypto::elephc_crypto_encrypt(
            cipher.as_ptr(),
            cipher.len(),
            data.as_ptr(),
            data.len(),
            passphrase.as_ptr(),
            passphrase.len(),
            iv.as_ptr(),
            iv.len(),
            options,
            aad.as_ptr(),
            aad.len(),
            tag_length,
            output.as_mut_ptr(),
            output.len(),
            &mut output_length,
            tag.as_mut_ptr(),
            tag.len(),
            &mut tag_output_length,
        )
    };
    if status != elephc_crypto::CIPHER_OK {
        return values.bool_value(false);
    }
    if output_length > output.len() || tag_output_length > tag.len() {
        return Err(EvalStatus::RuntimeFatal);
    }
    output.truncate(output_length);
    tag.truncate(tag_output_length);
    if !tag.is_empty() && tag_target.is_none() {
        return values.bool_value(false);
    }
    let result = if options & OPENSSL_RAW_DATA != 0 {
        values.string_bytes_value(&output)?
    } else {
        let output = super::base64_encode::eval_base64_encode_bytes(&output);
        values.string(&output)?
    };
    if let Some(target) = tag_target.filter(|_| !tag.is_empty()) {
        let tag = values.string_bytes_value(&tag)?;
        eval_write_direct_ref_target(
            target,
            tag,
            context,
            values,
            Some(ScopeCellOwnership::Owned),
        )?;
    }
    Ok(result)
}

/// Returns one required bound OpenSSL argument.
fn required_openssl_arg(
    bound: &[Option<RuntimeCellHandle>; OPENSSL_ENCRYPT_ARG_COUNT],
    index: usize,
) -> Result<RuntimeCellHandle, EvalStatus> {
    bound
        .get(index)
        .and_then(|arg| *arg)
        .ok_or(EvalStatus::RuntimeFatal)
}

/// Converts one optional OpenSSL integer argument or returns its PHP default.
fn optional_openssl_int_arg(
    bound: &[Option<RuntimeCellHandle>; OPENSSL_ENCRYPT_ARG_COUNT],
    index: usize,
    default: i64,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    bound[index].map_or(Ok(default), |value| eval_int_value(value, values))
}

/// Converts one optional OpenSSL string argument or returns an empty byte string.
fn optional_openssl_bytes_arg(
    bound: &[Option<RuntimeCellHandle>; OPENSSL_ENCRYPT_ARG_COUNT],
    index: usize,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<u8>, EvalStatus> {
    bound[index].map_or_else(|| Ok(Vec::new()), |value| values.string_bytes(value))
}
