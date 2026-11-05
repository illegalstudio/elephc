//! Purpose:
//! Eval homes for PHP's `ext/sodium` (`sodium_*`) surface. None of them re-implements
//! cryptography: every home forwards to the compiled sodium prelude helper the host
//! program registered, so eval runs the same bridge-backed operations as compiled code.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks::{EvalDirectHook, EvalValuesHook}::Sodium`.
//! - The declarative eval builtin registry joined from the shared sodium contracts.
//!
//! Key details:
//! - A host that did not inject the sodium prelude (`--with-sodium`, or a compiled sodium
//!   use) has no helper registered, so a call raises PHP's catchable
//!   `Error: Call to undefined function sodium_*()` instead of a partial surface.
//! - The macro keeps the 110 mechanically identical bindings in one auditable list.

use super::super::*;

mod dispatch;

/// Returns whether the host program registered the compiled sodium prelude in this context.
pub(in crate::interpreter) fn eval_sodium_bridge_linked(context: &ElephcEvalContext) -> bool {
    context.native_function("__elephc_sodium_bin2hex").is_some()
}

/// Returns whether `name` is the `sodium` extension loaded into this eval context.
pub(in crate::interpreter) fn eval_sodium_bridge_extension(
    name: &str,
    context: &ElephcEvalContext,
) -> bool {
    name.eq_ignore_ascii_case("sodium") && eval_sodium_bridge_linked(context)
}

pub(in crate::interpreter) use dispatch::*;

/// Submits one eval binding per canonical sodium name.
macro_rules! sodium_eval_bindings {
    ($($name:literal),* $(,)?) => {
        $( eval_builtin! { contract: $name, area: Sodium, direct: Sodium, values: Sodium } )*
    };
}

sodium_eval_bindings! {
    "sodium_base642bin",
    "sodium_bin2base64",
    "sodium_bin2hex",
    "sodium_compare",
    "sodium_crypto_aead_aegis128l_decrypt",
    "sodium_crypto_aead_aegis128l_encrypt",
    "sodium_crypto_aead_aegis128l_keygen",
    "sodium_crypto_aead_aegis256_decrypt",
    "sodium_crypto_aead_aegis256_encrypt",
    "sodium_crypto_aead_aegis256_keygen",
    "sodium_crypto_aead_aes256gcm_decrypt",
    "sodium_crypto_aead_aes256gcm_encrypt",
    "sodium_crypto_aead_aes256gcm_is_available",
    "sodium_crypto_aead_aes256gcm_keygen",
    "sodium_crypto_aead_chacha20poly1305_decrypt",
    "sodium_crypto_aead_chacha20poly1305_encrypt",
    "sodium_crypto_aead_chacha20poly1305_ietf_decrypt",
    "sodium_crypto_aead_chacha20poly1305_ietf_encrypt",
    "sodium_crypto_aead_chacha20poly1305_ietf_keygen",
    "sodium_crypto_aead_chacha20poly1305_keygen",
    "sodium_crypto_aead_xchacha20poly1305_ietf_decrypt",
    "sodium_crypto_aead_xchacha20poly1305_ietf_encrypt",
    "sodium_crypto_aead_xchacha20poly1305_ietf_keygen",
    "sodium_crypto_auth",
    "sodium_crypto_auth_keygen",
    "sodium_crypto_auth_verify",
    "sodium_crypto_box",
    "sodium_crypto_box_keypair",
    "sodium_crypto_box_keypair_from_secretkey_and_publickey",
    "sodium_crypto_box_open",
    "sodium_crypto_box_publickey",
    "sodium_crypto_box_publickey_from_secretkey",
    "sodium_crypto_box_seal",
    "sodium_crypto_box_seal_open",
    "sodium_crypto_box_secretkey",
    "sodium_crypto_box_seed_keypair",
    "sodium_crypto_core_ristretto255_add",
    "sodium_crypto_core_ristretto255_from_hash",
    "sodium_crypto_core_ristretto255_is_valid_point",
    "sodium_crypto_core_ristretto255_random",
    "sodium_crypto_core_ristretto255_scalar_add",
    "sodium_crypto_core_ristretto255_scalar_complement",
    "sodium_crypto_core_ristretto255_scalar_invert",
    "sodium_crypto_core_ristretto255_scalar_mul",
    "sodium_crypto_core_ristretto255_scalar_negate",
    "sodium_crypto_core_ristretto255_scalar_random",
    "sodium_crypto_core_ristretto255_scalar_reduce",
    "sodium_crypto_core_ristretto255_scalar_sub",
    "sodium_crypto_core_ristretto255_sub",
    "sodium_crypto_generichash",
    "sodium_crypto_generichash_init",
    "sodium_crypto_generichash_keygen",
    "sodium_crypto_kdf_derive_from_key",
    "sodium_crypto_kdf_keygen",
    "sodium_crypto_kx_client_session_keys",
    "sodium_crypto_kx_keypair",
    "sodium_crypto_kx_publickey",
    "sodium_crypto_kx_secretkey",
    "sodium_crypto_kx_seed_keypair",
    "sodium_crypto_kx_server_session_keys",
    "sodium_crypto_pwhash",
    "sodium_crypto_pwhash_scryptsalsa208sha256",
    "sodium_crypto_pwhash_scryptsalsa208sha256_str",
    "sodium_crypto_pwhash_scryptsalsa208sha256_str_verify",
    "sodium_crypto_pwhash_str",
    "sodium_crypto_pwhash_str_needs_rehash",
    "sodium_crypto_pwhash_str_verify",
    "sodium_crypto_scalarmult",
    "sodium_crypto_scalarmult_base",
    "sodium_crypto_scalarmult_ristretto255",
    "sodium_crypto_scalarmult_ristretto255_base",
    "sodium_crypto_secretbox",
    "sodium_crypto_secretbox_keygen",
    "sodium_crypto_secretbox_open",
    "sodium_crypto_secretstream_xchacha20poly1305_init_pull",
    "sodium_crypto_secretstream_xchacha20poly1305_init_push",
    "sodium_crypto_secretstream_xchacha20poly1305_keygen",
    "sodium_crypto_shorthash",
    "sodium_crypto_shorthash_keygen",
    "sodium_crypto_sign",
    "sodium_crypto_sign_detached",
    "sodium_crypto_sign_ed25519_pk_to_curve25519",
    "sodium_crypto_sign_ed25519_sk_to_curve25519",
    "sodium_crypto_sign_keypair",
    "sodium_crypto_sign_keypair_from_secretkey_and_publickey",
    "sodium_crypto_sign_open",
    "sodium_crypto_sign_publickey",
    "sodium_crypto_sign_publickey_from_secretkey",
    "sodium_crypto_sign_secretkey",
    "sodium_crypto_sign_seed_keypair",
    "sodium_crypto_sign_verify_detached",
    "sodium_crypto_stream",
    "sodium_crypto_stream_keygen",
    "sodium_crypto_stream_xchacha20",
    "sodium_crypto_stream_xchacha20_keygen",
    "sodium_crypto_stream_xchacha20_xor",
    "sodium_crypto_stream_xchacha20_xor_ic",
    "sodium_crypto_stream_xor",
    "sodium_hex2bin",
    "sodium_memcmp",
    "sodium_pad",
    "sodium_unpad",
}
