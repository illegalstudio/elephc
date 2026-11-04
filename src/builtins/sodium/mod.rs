//! Purpose:
//! Joins the shared `ext/sodium` contracts to the AOT implementation: one generic EIR
//! lowering boxes raw operands, calls the matching `__elephc_sodium_*` prelude helper,
//! and writes mutating state back through `store_operand_local`.
//!
//! Called from:
//! - Each sodium binding through `builtin!`.
//! - Reachability's call scan through [`prelude_helpers_for`].
//!
//! Key details:
//! - Operands are boxed to `Mixed` before the helper call so PHP 8.5's coercions and
//!   diagnostics live in one PHP implementation shared by AOT and eval.
//! - The compiler-owned strict-types flag is the helper's first argument, so weak and
//!   strict call sites keep their caller-file semantics.
//! - The seven by-reference parameters are write-only outputs: the helper returns the
//!   advanced state and the lowering stores it into the caller's variable, exactly like
//!   php-src's in-place string mutation.
//! - String and array results are fresh; scalars are non-heap.

use crate::builtins::semantics::{
    BuiltinArgumentLowering, BuiltinCallablePolicy, BuiltinEffects, BuiltinLowering,
    BuiltinLoweringContext, BuiltinLoweringError, BuiltinRequirement, BuiltinRequirements,
    BuiltinResultOwnership, BuiltinResultType, BuiltinRuntimeFunctions, BuiltinSemanticInput,
    BuiltinSemantics, BuiltinTargetStrategy, BuiltinTargetSupport, BuiltinValidation,
    LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::errors::CompileError;
use crate::ir::{Effects, Immediate, Op};
use crate::types::PhpType;

mod bindings;

/// The bridge required by every sodium builtin.
const SODIUM_REQUIREMENTS: &[BuiltinRequirement] = &[BuiltinRequirement::Bridge("elephc_sodium")];

/// Every prelude helper a sodium program may need, including the framing helpers.
///
/// Keeping the whole helper set for any sodium call is deliberate: the helpers are called
/// from EIR lowering rather than AST call sites, so a per-name table would add a second
/// place to forget when a function is added.
pub(crate) const HELPERS: &[&str] = &[
    "__elephc_sodium_add",
    "__elephc_sodium_base642bin",
    "__elephc_sodium_bin2base64",
    "__elephc_sodium_bin2hex",
    "__elephc_sodium_compare",
    "__elephc_sodium_crypto_aead_aegis128l_decrypt",
    "__elephc_sodium_crypto_aead_aegis128l_encrypt",
    "__elephc_sodium_crypto_aead_aegis128l_keygen",
    "__elephc_sodium_crypto_aead_aegis256_decrypt",
    "__elephc_sodium_crypto_aead_aegis256_encrypt",
    "__elephc_sodium_crypto_aead_aegis256_keygen",
    "__elephc_sodium_crypto_aead_aes256gcm_decrypt",
    "__elephc_sodium_crypto_aead_aes256gcm_encrypt",
    "__elephc_sodium_crypto_aead_aes256gcm_is_available",
    "__elephc_sodium_crypto_aead_aes256gcm_keygen",
    "__elephc_sodium_crypto_aead_chacha20poly1305_decrypt",
    "__elephc_sodium_crypto_aead_chacha20poly1305_encrypt",
    "__elephc_sodium_crypto_aead_chacha20poly1305_ietf_decrypt",
    "__elephc_sodium_crypto_aead_chacha20poly1305_ietf_encrypt",
    "__elephc_sodium_crypto_aead_chacha20poly1305_ietf_keygen",
    "__elephc_sodium_crypto_aead_chacha20poly1305_keygen",
    "__elephc_sodium_crypto_aead_xchacha20poly1305_ietf_decrypt",
    "__elephc_sodium_crypto_aead_xchacha20poly1305_ietf_encrypt",
    "__elephc_sodium_crypto_aead_xchacha20poly1305_ietf_keygen",
    "__elephc_sodium_crypto_auth",
    "__elephc_sodium_crypto_auth_keygen",
    "__elephc_sodium_crypto_auth_verify",
    "__elephc_sodium_crypto_box",
    "__elephc_sodium_crypto_box_keypair",
    "__elephc_sodium_crypto_box_keypair_from_secretkey_and_publickey",
    "__elephc_sodium_crypto_box_open",
    "__elephc_sodium_crypto_box_publickey",
    "__elephc_sodium_crypto_box_publickey_from_secretkey",
    "__elephc_sodium_crypto_box_seal",
    "__elephc_sodium_crypto_box_seal_open",
    "__elephc_sodium_crypto_box_secretkey",
    "__elephc_sodium_crypto_box_seed_keypair",
    "__elephc_sodium_crypto_core_ristretto255_add",
    "__elephc_sodium_crypto_core_ristretto255_from_hash",
    "__elephc_sodium_crypto_core_ristretto255_is_valid_point",
    "__elephc_sodium_crypto_core_ristretto255_random",
    "__elephc_sodium_crypto_core_ristretto255_scalar_add",
    "__elephc_sodium_crypto_core_ristretto255_scalar_complement",
    "__elephc_sodium_crypto_core_ristretto255_scalar_invert",
    "__elephc_sodium_crypto_core_ristretto255_scalar_mul",
    "__elephc_sodium_crypto_core_ristretto255_scalar_negate",
    "__elephc_sodium_crypto_core_ristretto255_scalar_random",
    "__elephc_sodium_crypto_core_ristretto255_scalar_reduce",
    "__elephc_sodium_crypto_core_ristretto255_scalar_sub",
    "__elephc_sodium_crypto_core_ristretto255_sub",
    "__elephc_sodium_crypto_generichash",
    "__elephc_sodium_crypto_generichash_final",
    "__elephc_sodium_crypto_generichash_init",
    "__elephc_sodium_crypto_generichash_keygen",
    "__elephc_sodium_crypto_generichash_update",
    "__elephc_sodium_crypto_kdf_derive_from_key",
    "__elephc_sodium_crypto_kdf_keygen",
    "__elephc_sodium_crypto_kx_client_session_keys",
    "__elephc_sodium_crypto_kx_keypair",
    "__elephc_sodium_crypto_kx_publickey",
    "__elephc_sodium_crypto_kx_secretkey",
    "__elephc_sodium_crypto_kx_seed_keypair",
    "__elephc_sodium_crypto_kx_server_session_keys",
    "__elephc_sodium_crypto_pwhash",
    "__elephc_sodium_crypto_pwhash_scryptsalsa208sha256",
    "__elephc_sodium_crypto_pwhash_scryptsalsa208sha256_str",
    "__elephc_sodium_crypto_pwhash_scryptsalsa208sha256_str_verify",
    "__elephc_sodium_crypto_pwhash_str",
    "__elephc_sodium_crypto_pwhash_str_needs_rehash",
    "__elephc_sodium_crypto_pwhash_str_verify",
    "__elephc_sodium_crypto_scalarmult",
    "__elephc_sodium_crypto_scalarmult_base",
    "__elephc_sodium_crypto_scalarmult_ristretto255",
    "__elephc_sodium_crypto_scalarmult_ristretto255_base",
    "__elephc_sodium_crypto_secretbox",
    "__elephc_sodium_crypto_secretbox_keygen",
    "__elephc_sodium_crypto_secretbox_open",
    "__elephc_sodium_crypto_secretstream_xchacha20poly1305_init_pull",
    "__elephc_sodium_crypto_secretstream_xchacha20poly1305_init_push",
    "__elephc_sodium_crypto_secretstream_xchacha20poly1305_keygen",
    "__elephc_sodium_crypto_secretstream_xchacha20poly1305_pull",
    "__elephc_sodium_crypto_secretstream_xchacha20poly1305_push",
    "__elephc_sodium_crypto_secretstream_xchacha20poly1305_rekey",
    "__elephc_sodium_crypto_shorthash",
    "__elephc_sodium_crypto_shorthash_keygen",
    "__elephc_sodium_crypto_sign",
    "__elephc_sodium_crypto_sign_detached",
    "__elephc_sodium_crypto_sign_ed25519_pk_to_curve25519",
    "__elephc_sodium_crypto_sign_ed25519_sk_to_curve25519",
    "__elephc_sodium_crypto_sign_keypair",
    "__elephc_sodium_crypto_sign_keypair_from_secretkey_and_publickey",
    "__elephc_sodium_crypto_sign_open",
    "__elephc_sodium_crypto_sign_publickey",
    "__elephc_sodium_crypto_sign_publickey_from_secretkey",
    "__elephc_sodium_crypto_sign_secretkey",
    "__elephc_sodium_crypto_sign_seed_keypair",
    "__elephc_sodium_crypto_sign_verify_detached",
    "__elephc_sodium_crypto_stream",
    "__elephc_sodium_crypto_stream_keygen",
    "__elephc_sodium_crypto_stream_xchacha20",
    "__elephc_sodium_crypto_stream_xchacha20_keygen",
    "__elephc_sodium_crypto_stream_xchacha20_xor",
    "__elephc_sodium_crypto_stream_xchacha20_xor_ic",
    "__elephc_sodium_crypto_stream_xor",
    "__elephc_sodium_hex2bin",
    "__elephc_sodium_increment",
    "__elephc_sodium_memcmp",
    "__elephc_sodium_memzero",
    "__elephc_sodium_pad",
    "__elephc_sodium_unpad",
    "__elephc_sodium_take_state",
    "__elephc_sodium_take_payload",
    "__elephc_sodium_pull_state",
    "__elephc_sodium_pull_result",
];

/// Returns every prelude helper to root when `name` is a sodium builtin.
pub(crate) fn prelude_helpers_for(name: &str) -> impl Iterator<Item = &'static str> {
    let is_sodium = elephc_builtin_contract::lookup(name)
        .is_some_and(|contract| contract.module == elephc_builtin_contract::PhpModule::Sodium);
    HELPERS.iter().copied().filter(move |_| is_sodium)
}

/// Builds the shared descriptor for every sodium builtin.
const fn sodium_semantics() -> BuiltinSemantics {
    BuiltinSemantics {
        validation: BuiltinValidation::Shared(accept_any_arguments),
        result_type: BuiltinResultType::Shared(declared_result),
        effects: BuiltinEffects::Static(Effects::all()),
        result_ownership: BuiltinResultOwnership::Fresh,
        requirements: BuiltinRequirements::Static(SODIUM_REQUIREMENTS),
        target_strategy: BuiltinTargetStrategy::EirGraph,
        target_support: BuiltinTargetSupport::All,
        runtime_functions: BuiltinRuntimeFunctions::None,
        argument_lowering: BuiltinArgumentLowering::Standard,
        callable: BuiltinCallablePolicy::Dynamic(
            crate::builtins::semantics::callable_accepts_any_source,
        ),
        lowering: BuiltinLowering::Eir(lower),
    }
}

/// Defers every operand diagnostic to the PHP helper, which implements php-src's rules.
fn accept_any_arguments(_input: &BuiltinSemanticInput<'_>) -> Result<(), CompileError> {
    Ok(())
}

/// Resolves the shared contract's declared return type for one call.
fn declared_result(input: &BuiltinSemanticInput<'_>) -> PhpType {
    elephc_builtin_contract::lookup(input.name)
        .map(|contract| crate::builtins::convert::type_spec_to_php(&contract.returns))
        .unwrap_or(PhpType::Mixed)
}

/// Lowers one public sodium builtin by calling its prelude helper with boxed raw operands.
fn lower(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    let strict = ctx.emit_value(
        Op::ConstBool,
        Vec::new(),
        Some(Immediate::Bool(crate::source::current_strict_types())),
        PhpType::Bool,
        Op::ConstBool.default_effects(),
        Some(call.span),
    );
    let contract = elephc_builtin_contract::lookup(call.name).ok_or_else(|| {
        BuiltinLoweringError::new(format!("{} has no shared sodium contract", call.name))
    })?;
    let mut supplied = call.operands.to_vec();
    for index in supplied.len()..contract.params.len() {
        let default = contract.params[index].default.ok_or_else(|| {
            BuiltinLoweringError::new(format!(
                "{} lowering is missing required operand {index}",
                call.name
            ))
        })?;
        supplied.push(emit_default(ctx, default, call.span)?);
    }
    let mut operands = Vec::with_capacity(supplied.len() + 1);
    operands.push(strict.value);
    let mut boxes = Vec::new();
    for operand in &supplied {
        let representation = ctx.value_php_type(*operand).codegen_repr();
        if matches!(representation, PhpType::Mixed | PhpType::Union(_)) {
            operands.push(*operand);
            continue;
        }
        let boxed = ctx.emit_value(
            Op::MixedBox,
            vec![*operand],
            None,
            PhpType::Mixed,
            Op::MixedBox.default_effects(),
            Some(call.span),
        );
        boxes.push(boxed.value);
        operands.push(boxed.value);
    }
    let helper = format!(
        "__elephc_sodium_{}",
        call.name.trim_start_matches("sodium_")
    );
    let result_type = call.result_type.clone();
    let raw = ctx.emit_user_call(&helper, operands, helper_value_type(call), Some(call.span));
    let lowered = shape_result(ctx, call, &raw, &result_type, &supplied)?;
    for boxed in boxes {
        ctx.emit_void(
            Op::Release,
            vec![boxed],
            None,
            Op::Release.default_effects(),
            Some(call.span),
        );
    }
    Ok(lowered)
}

/// Returns the helper's declared value family for one call.
fn helper_value_type(call: &NormalizedBuiltinCall<'_>) -> PhpType {
    match call.name {
        "sodium_crypto_secretstream_xchacha20poly1305_pull" => {
            PhpType::Union(vec![PhpType::Str, PhpType::False])
        }
        "sodium_crypto_secretstream_xchacha20poly1305_init_push"
        | "sodium_crypto_kx_client_session_keys"
        | "sodium_crypto_kx_server_session_keys" => PhpType::Array(Box::new(PhpType::Mixed)),
        name if name == "sodium_add" || name == "sodium_increment"
            || name == "sodium_memzero"
            || name == "sodium_crypto_generichash_update"
            || name == "sodium_crypto_generichash_final"
            || name == "sodium_crypto_secretstream_xchacha20poly1305_push"
            || name == "sodium_crypto_secretstream_xchacha20poly1305_rekey" => PhpType::Str,
        _ => call.result_type.clone(),
    }
}

/// Turns the helper result into the builtin's observable result and writes state back.
fn shape_result(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
    raw: &LoweredBuiltinValue,
    result_type: &PhpType,
    supplied: &[crate::ir::ValueId],
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    match call.name {
        "sodium_add" | "sodium_increment" => {
            store_first(ctx, call, raw.value, PhpType::Str);
            Ok(LoweredBuiltinValue { value: emit_null(ctx, call.span) })
        }
        "sodium_memzero" => {
            let null = emit_null(ctx, call.span);
            store_first(ctx, call, null, PhpType::Mixed);
            Ok(LoweredBuiltinValue { value: null })
        }
        "sodium_crypto_generichash_update" => {
            store_first(ctx, call, raw.value, PhpType::Str);
            let value = ctx.emit_value(
                Op::ConstBool,
                Vec::new(),
                Some(Immediate::Bool(true)),
                PhpType::Bool,
                Op::ConstBool.default_effects(),
                Some(call.span),
            );
            Ok(value)
        }
        "sodium_crypto_generichash_final" => {
            let null = emit_null(ctx, call.span);
            store_first(ctx, call, null, PhpType::Mixed);
            Ok(LoweredBuiltinValue { value: raw.value })
        }
        "sodium_crypto_secretstream_xchacha20poly1305_push" => {
            let state = ctx.emit_user_call(
                "__elephc_sodium_take_state",
                vec![raw.value],
                PhpType::Str,
                Some(call.span),
            );
            store_first(ctx, call, state.value, PhpType::Str);
            let payload = ctx.emit_user_call(
                "__elephc_sodium_take_payload",
                vec![raw.value],
                PhpType::Str,
                Some(call.span),
            );
            Ok(payload)
        }
        "sodium_crypto_secretstream_xchacha20poly1305_pull" => {
            let original = supplied.first().copied().ok_or_else(|| {
                BuiltinLoweringError::new("sodium secretstream pull is missing its state operand")
            })?;
            let state = ctx.emit_user_call(
                "__elephc_sodium_pull_state",
                vec![raw.value, original],
                PhpType::Str,
                Some(call.span),
            );
            store_first(ctx, call, state.value, PhpType::Str);
            Ok(ctx.emit_user_call(
                "__elephc_sodium_pull_result",
                vec![raw.value],
                result_type.clone(),
                Some(call.span),
            ))
        }
        "sodium_crypto_secretstream_xchacha20poly1305_rekey" => {
            store_first(ctx, call, raw.value, PhpType::Str);
            Ok(LoweredBuiltinValue { value: emit_null(ctx, call.span) })
        }
        "sodium_crypto_secretstream_xchacha20poly1305_init_push" => {
            Ok(LoweredBuiltinValue { value: raw.value })
        }
        _ => Ok(LoweredBuiltinValue { value: raw.value }),
    }
}

/// Stores a helper's advanced state into the first (by-reference) operand.
fn store_first(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
    value: crate::ir::ValueId,
    php_type: PhpType,
) {
    if let Some(target) = call.operands.first().copied() {
        let _ = ctx.store_operand_local(target, value, php_type, Some(call.span));
    }
}

/// Emits a null constant.
fn emit_null(ctx: &mut dyn BuiltinLoweringContext, span: crate::span::Span) -> crate::ir::ValueId {
    ctx.emit_value(
        Op::ConstNull,
        Vec::new(),
        None,
        PhpType::Void,
        Op::ConstNull.default_effects(),
        Some(span),
    )
    .value
}

/// Emits one omitted optional argument's default value for the helper ABI.
fn emit_default(
    ctx: &mut dyn BuiltinLoweringContext,
    default: elephc_builtin_contract::DefaultSpec,
    span: crate::span::Span,
) -> Result<crate::ir::ValueId, BuiltinLoweringError> {
    let (op, immediate, php_type) = match default {
        elephc_builtin_contract::DefaultSpec::Null => (Op::ConstNull, None, PhpType::Void),
        elephc_builtin_contract::DefaultSpec::Int(value) => {
            (Op::ConstI64, Some(Immediate::I64(value)), PhpType::Int)
        }
        elephc_builtin_contract::DefaultSpec::Bool(value) => {
            (Op::ConstBool, Some(Immediate::Bool(value)), PhpType::Bool)
        }
        elephc_builtin_contract::DefaultSpec::Str(value) => {
            let data = ctx.intern_string(value);
            (Op::ConstStr, Some(Immediate::Data(data)), PhpType::Str)
        }
        other => {
            return Err(BuiltinLoweringError::new(format!(
                "unsupported sodium helper default {other:?}"
            )))
        }
    };
    Ok(ctx.emit_value(op, Vec::new(), immediate, php_type, op.default_effects(), Some(span)).value)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies every contract roots the whole helper set and unrelated names root none.
    #[test]
    fn every_sodium_contract_has_a_helper_root() {
        for name in elephc_builtin_contract::SODIUM_FUNCTIONS {
            let helpers: Vec<_> = prelude_helpers_for(name).collect();
            assert_eq!(helpers.len(), HELPERS.len(), "{name} must root every helper");
        }
        assert!(prelude_helpers_for("strlen").next().is_none());
    }

    /// Verifies the declared result families survive the shared type conversion.
    #[test]
    fn declared_result_families_are_distinct() {
        let input = |name| BuiltinSemanticInput {
            name,
            args: &[],
            arg_types: &[],
            span: crate::span::Span::dummy(),
        };
        assert_eq!(declared_result(&input("sodium_bin2hex")), PhpType::Str);
        assert_eq!(declared_result(&input("sodium_memcmp")), PhpType::Int);
        assert_eq!(declared_result(&input("sodium_crypto_auth_verify")), PhpType::Bool);
        assert!(matches!(
            declared_result(&input("sodium_crypto_secretbox_open")),
            PhpType::Union(_)
        ));
        assert!(matches!(
            declared_result(&input("sodium_crypto_kx_client_session_keys")),
            PhpType::Array(_)
        ));
    }
}
