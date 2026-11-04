//! Purpose:
//! Declarative metadata for every PHP `ext/sodium` helper the compiled prelude declares.
//!
//! Called from:
//! - `crate::sodium_prelude::functions::declarations`.
//!
//! Key details:
//! - One entry drives parameter coercion, php-src length checks, bridge dispatch and the
//!   helper's PHP result type; no per-function body is hand-written unless its shape is
//!   genuinely special.
//! - Length checks carry the exact constant spelling php-src puts in `SodiumException`
//!   messages, so the generated text stays byte-identical without duplicating numbers.
//! - By-reference parameters are not marked here: the public shared contract owns them,
//!   and the lowering writes the helper's state result back through `store_operand_local`.

/// PHP-level parameter representation accepted by a sodium helper.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ParamKind {
    /// PHP `string`, with weak-mode coercion handled by the shared validator.
    Str,
    /// PHP `int`, with weak-mode coercion handled by the shared validator.
    Int,
}

/// One optional parameter default rendered as a literal.
#[derive(Clone, Copy)]
pub(super) enum Default {
    /// Empty or non-empty string literal.
    Str(&'static str),
    /// Integer literal.
    Int(i64),
}

/// One helper parameter.
pub(super) struct Param {
    /// PHP-visible parameter name.
    pub(super) name: &'static str,
    /// Accepted representation before coercion.
    pub(super) kind: ParamKind,
    /// Optional default, materialized by the builtin lowering when omitted.
    pub(super) default: Option<Default>,
}

/// The helper's PHP-visible result family.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ResultKind {
    /// Plain string.
    Str,
    /// `string|false`.
    StrOrFalse,
    /// Boolean.
    Bool,
    /// Integer.
    Int,
    /// Void.
    Void,
    /// Indexed array.
    Array,
}

/// How a helper reaches libsodium and shapes its result.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Shape {
    /// Coerce, length-check and call the bridge with the function name.
    Plain,
    /// Validate equal lengths, add, and return the updated string.
    Add,
    /// Increment and return the updated string.
    Increment,
    /// Return the empty string; the lowering stores null.
    Memzero,
    /// Constant-time compare returning an integer.
    Compare,
    /// Constant-time memory compare returning an integer.
    Memcmp,
    /// Decode base64 with a variant check.
    Base642bin,
    /// Encode base64 with a variant check.
    Bin2base64,
    /// Decode hexadecimal.
    Hex2bin,
    /// Pad with a block-size check.
    Pad,
    /// Remove padding with block-size checks.
    Unpad,
    /// One-shot BLAKE2b with output/key range checks.
    Generichash,
    /// Incremental BLAKE2b init with output/key range checks.
    GenerichashInit,
    /// Incremental BLAKE2b update returning the advanced state.
    GenerichashUpdate,
    /// Incremental BLAKE2b final returning the digest.
    GenerichashFinal,
    /// Key-exchange session keys split into an indexed array.
    KxSessionKeys,
    /// Argon2 raw key derivation with php-src's domain checks.
    Pwhash,
    /// Argon2id password hash string.
    PwhashStr,
    /// Argon2 password verification.
    PwhashStrVerify,
    /// Argon2 rehash predicate.
    PwhashStrNeedsRehash,
    /// scrypt raw key derivation with php-src's domain checks.
    Scrypt,
    /// scrypt password hash string.
    ScryptStr,
    /// scrypt password verification.
    ScryptStrVerify,
    /// Secretstream init_push split into `[state, header]`.
    SecretstreamInitPush,
    /// Secretstream push returning `state || ciphertext`.
    SecretstreamPush,
    /// Secretstream pull returning `state || plaintext || tag`, or false.
    SecretstreamPull,
    /// Secretstream rekey returning the advanced state.
    SecretstreamRekey,
}

/// One fixed-length input check.
pub(super) struct LenCheck {
    /// Parameter index.
    pub(super) param: usize,
    /// Required byte length.
    pub(super) len: i64,
    /// Constant spelling used in php-src's error message.
    pub(super) symbol: &'static str,
}

/// One helper specification.
pub(super) struct Spec {
    /// Canonical PHP function name.
    pub(super) name: &'static str,
    /// Parameters in PHP order.
    pub(super) params: Vec<Param>,
    /// Helper result family.
    pub(super) result: ResultKind,
    /// Bridge/result shaping.
    pub(super) shape: Shape,
    /// Fixed-length checks.
    pub(super) lens: Vec<LenCheck>,
}

/// One required string parameter.
const fn s(name: &'static str) -> Param {
    Param { name, kind: ParamKind::Str, default: None }
}

/// One required integer parameter.
const fn i(name: &'static str) -> Param {
    Param { name, kind: ParamKind::Int, default: None }
}

/// One optional string parameter.
const fn sd(name: &'static str, default: &'static str) -> Param {
    Param { name, kind: ParamKind::Str, default: Some(Default::Str(default)) }
}

/// One optional integer parameter.
const fn id(name: &'static str, default: i64) -> Param {
    Param { name, kind: ParamKind::Int, default: Some(Default::Int(default)) }
}

/// One fixed-length check.
const fn len(param: usize, len: i64, symbol: &'static str) -> LenCheck {
    LenCheck { param, len, symbol }
}

/// Builds the complete helper table.
pub(super) fn specs() -> Vec<Spec> {
    vec![
        Spec { name: "sodium_add", params: vec![s("string1"), s("string2")], result: ResultKind::Void, shape: Shape::Add, lens: vec![] },
        Spec { name: "sodium_base642bin", params: vec![s("string"), i("id"), sd("ignore", "")], result: ResultKind::Str, shape: Shape::Base642bin, lens: vec![] },
        Spec { name: "sodium_bin2base64", params: vec![s("string"), i("id")], result: ResultKind::Str, shape: Shape::Bin2base64, lens: vec![] },
        Spec { name: "sodium_bin2hex", params: vec![s("string")], result: ResultKind::Str, shape: Shape::Plain, lens: vec![] },
        Spec { name: "sodium_compare", params: vec![s("string1"), s("string2")], result: ResultKind::Int, shape: Shape::Compare, lens: vec![] },
        aead("sodium_crypto_aead_aegis128l_encrypt", "SODIUM_CRYPTO_AEAD_AEGIS128L", 16, 16),
        aead("sodium_crypto_aead_aegis128l_decrypt", "SODIUM_CRYPTO_AEAD_AEGIS128L", 16, 16),
        keygen("sodium_crypto_aead_aegis128l_keygen"),
        aead("sodium_crypto_aead_aegis256_encrypt", "SODIUM_CRYPTO_AEAD_AEGIS256", 32, 32),
        aead("sodium_crypto_aead_aegis256_decrypt", "SODIUM_CRYPTO_AEAD_AEGIS256", 32, 32),
        keygen("sodium_crypto_aead_aegis256_keygen"),
        aead("sodium_crypto_aead_aes256gcm_encrypt", "SODIUM_CRYPTO_AEAD_AES256GCM", 12, 32),
        aead("sodium_crypto_aead_aes256gcm_decrypt", "SODIUM_CRYPTO_AEAD_AES256GCM", 12, 32),
        Spec { name: "sodium_crypto_aead_aes256gcm_is_available", params: vec![], result: ResultKind::Bool, shape: Shape::Plain, lens: vec![] },
        keygen("sodium_crypto_aead_aes256gcm_keygen"),
        aead("sodium_crypto_aead_chacha20poly1305_encrypt", "SODIUM_CRYPTO_AEAD_CHACHA20POLY1305", 8, 32),
        aead("sodium_crypto_aead_chacha20poly1305_decrypt", "SODIUM_CRYPTO_AEAD_CHACHA20POLY1305", 8, 32),
        aead("sodium_crypto_aead_chacha20poly1305_ietf_encrypt", "SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_IETF", 12, 32),
        aead("sodium_crypto_aead_chacha20poly1305_ietf_decrypt", "SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_IETF", 12, 32),
        keygen("sodium_crypto_aead_chacha20poly1305_ietf_keygen"),
        keygen("sodium_crypto_aead_chacha20poly1305_keygen"),
        aead("sodium_crypto_aead_xchacha20poly1305_ietf_encrypt", "SODIUM_CRYPTO_AEAD_XCHACHA20POLY1305_IETF", 24, 32),
        aead("sodium_crypto_aead_xchacha20poly1305_ietf_decrypt", "SODIUM_CRYPTO_AEAD_XCHACHA20POLY1305_IETF", 24, 32),
        keygen("sodium_crypto_aead_xchacha20poly1305_ietf_keygen"),
        Spec { name: "sodium_crypto_auth", params: vec![s("message"), s("key")], result: ResultKind::Str, shape: Shape::Plain, lens: vec![len(1, 32, "SODIUM_CRYPTO_AUTH_KEYBYTES")] },
        keygen("sodium_crypto_auth_keygen"),
        Spec { name: "sodium_crypto_auth_verify", params: vec![s("mac"), s("message"), s("key")], result: ResultKind::Bool, shape: Shape::Plain, lens: vec![len(2, 32, "SODIUM_CRYPTO_AUTH_KEYBYTES"), len(0, 32, "SODIUM_CRYPTO_AUTH_BYTES")] },
        Spec { name: "sodium_crypto_box", params: vec![s("message"), s("nonce"), s("key_pair")], result: ResultKind::Str, shape: Shape::Plain, lens: vec![len(1, 24, "SODIUM_CRYPTO_BOX_NONCEBYTES"), len(2, 64, "SODIUM_CRYPTO_BOX_KEYPAIRBYTES")] },
        keygen("sodium_crypto_box_keypair"),
        Spec { name: "sodium_crypto_box_keypair_from_secretkey_and_publickey", params: vec![s("secret_key"), s("public_key")], result: ResultKind::Str, shape: Shape::Plain, lens: vec![len(0, 32, "SODIUM_CRYPTO_BOX_SECRETKEYBYTES"), len(1, 32, "SODIUM_CRYPTO_BOX_PUBLICKEYBYTES")] },
        Spec { name: "sodium_crypto_box_open", params: vec![s("ciphertext"), s("nonce"), s("key_pair")], result: ResultKind::StrOrFalse, shape: Shape::Plain, lens: vec![len(1, 24, "SODIUM_CRYPTO_BOX_NONCEBYTES"), len(2, 64, "SODIUM_CRYPTO_BOX_KEYPAIRBYTES")] },
        Spec { name: "sodium_crypto_box_publickey", params: vec![s("key_pair")], result: ResultKind::Str, shape: Shape::Plain, lens: vec![len(0, 64, "SODIUM_CRYPTO_BOX_KEYPAIRBYTES")] },
        Spec { name: "sodium_crypto_box_publickey_from_secretkey", params: vec![s("secret_key")], result: ResultKind::Str, shape: Shape::Plain, lens: vec![len(0, 32, "SODIUM_CRYPTO_BOX_SECRETKEYBYTES")] },
        Spec { name: "sodium_crypto_box_seal", params: vec![s("message"), s("public_key")], result: ResultKind::Str, shape: Shape::Plain, lens: vec![len(1, 32, "SODIUM_CRYPTO_BOX_PUBLICKEYBYTES")] },
        Spec { name: "sodium_crypto_box_seal_open", params: vec![s("ciphertext"), s("key_pair")], result: ResultKind::StrOrFalse, shape: Shape::Plain, lens: vec![len(1, 64, "SODIUM_CRYPTO_BOX_KEYPAIRBYTES")] },
        Spec { name: "sodium_crypto_box_secretkey", params: vec![s("key_pair")], result: ResultKind::Str, shape: Shape::Plain, lens: vec![len(0, 64, "SODIUM_CRYPTO_BOX_KEYPAIRBYTES")] },
        Spec { name: "sodium_crypto_box_seed_keypair", params: vec![s("seed")], result: ResultKind::Str, shape: Shape::Plain, lens: vec![len(0, 32, "SODIUM_CRYPTO_BOX_SEEDBYTES")] },
        Spec { name: "sodium_crypto_core_ristretto255_add", params: vec![s("p"), s("q")], result: ResultKind::Str, shape: Shape::Plain, lens: vec![len(0, 32, "SODIUM_CRYPTO_CORE_RISTRETTO255_BYTES"), len(1, 32, "SODIUM_CRYPTO_CORE_RISTRETTO255_BYTES")] },
        Spec { name: "sodium_crypto_core_ristretto255_from_hash", params: vec![s("s")], result: ResultKind::Str, shape: Shape::Plain, lens: vec![len(0, 64, "SODIUM_CRYPTO_CORE_RISTRETTO255_HASHBYTES")] },
        Spec { name: "sodium_crypto_core_ristretto255_is_valid_point", params: vec![s("s")], result: ResultKind::Bool, shape: Shape::Plain, lens: vec![len(0, 32, "SODIUM_CRYPTO_CORE_RISTRETTO255_BYTES")] },
        keygen("sodium_crypto_core_ristretto255_random"),
        Spec { name: "sodium_crypto_core_ristretto255_scalar_add", params: vec![s("x"), s("y")], result: ResultKind::Str, shape: Shape::Plain, lens: vec![len(0, 32, "SODIUM_CRYPTO_CORE_RISTRETTO255_SCALARBYTES"), len(1, 32, "SODIUM_CRYPTO_CORE_RISTRETTO255_SCALARBYTES")] },
        Spec { name: "sodium_crypto_core_ristretto255_scalar_complement", params: vec![s("s")], result: ResultKind::Str, shape: Shape::Plain, lens: vec![len(0, 32, "SODIUM_CRYPTO_CORE_RISTRETTO255_SCALARBYTES")] },
        Spec { name: "sodium_crypto_core_ristretto255_scalar_invert", params: vec![s("s")], result: ResultKind::Str, shape: Shape::Plain, lens: vec![len(0, 32, "SODIUM_CRYPTO_CORE_RISTRETTO255_SCALARBYTES")] },
        Spec { name: "sodium_crypto_core_ristretto255_scalar_mul", params: vec![s("x"), s("y")], result: ResultKind::Str, shape: Shape::Plain, lens: vec![len(0, 32, "SODIUM_CRYPTO_CORE_RISTRETTO255_SCALARBYTES"), len(1, 32, "SODIUM_CRYPTO_CORE_RISTRETTO255_SCALARBYTES")] },
        Spec { name: "sodium_crypto_core_ristretto255_scalar_negate", params: vec![s("s")], result: ResultKind::Str, shape: Shape::Plain, lens: vec![len(0, 32, "SODIUM_CRYPTO_CORE_RISTRETTO255_SCALARBYTES")] },
        keygen("sodium_crypto_core_ristretto255_scalar_random"),
        Spec { name: "sodium_crypto_core_ristretto255_scalar_reduce", params: vec![s("s")], result: ResultKind::Str, shape: Shape::Plain, lens: vec![len(0, 64, "SODIUM_CRYPTO_CORE_RISTRETTO255_NONREDUCEDSCALARBYTES")] },
        Spec { name: "sodium_crypto_core_ristretto255_scalar_sub", params: vec![s("x"), s("y")], result: ResultKind::Str, shape: Shape::Plain, lens: vec![len(0, 32, "SODIUM_CRYPTO_CORE_RISTRETTO255_SCALARBYTES"), len(1, 32, "SODIUM_CRYPTO_CORE_RISTRETTO255_SCALARBYTES")] },
        Spec { name: "sodium_crypto_core_ristretto255_sub", params: vec![s("p"), s("q")], result: ResultKind::Str, shape: Shape::Plain, lens: vec![len(0, 32, "SODIUM_CRYPTO_CORE_RISTRETTO255_BYTES"), len(1, 32, "SODIUM_CRYPTO_CORE_RISTRETTO255_BYTES")] },
        Spec { name: "sodium_crypto_generichash", params: vec![s("message"), sd("key", ""), id("length", 32)], result: ResultKind::Str, shape: Shape::Generichash, lens: vec![] },
        Spec { name: "sodium_crypto_generichash_final", params: vec![s("state"), id("length", 32)], result: ResultKind::Str, shape: Shape::GenerichashFinal, lens: vec![] },
        Spec { name: "sodium_crypto_generichash_init", params: vec![sd("key", ""), id("length", 32)], result: ResultKind::Str, shape: Shape::GenerichashInit, lens: vec![] },
        keygen("sodium_crypto_generichash_keygen"),
        Spec { name: "sodium_crypto_generichash_update", params: vec![s("state"), s("message")], result: ResultKind::Str, shape: Shape::GenerichashUpdate, lens: vec![] },
        Spec { name: "sodium_crypto_kdf_derive_from_key", params: vec![i("subkey_length"), i("subkey_id"), s("context"), s("key")], result: ResultKind::Str, shape: Shape::Plain, lens: vec![len(2, 8, "SODIUM_CRYPTO_KDF_CONTEXTBYTES"), len(3, 32, "SODIUM_CRYPTO_KDF_KEYBYTES")] },
        keygen("sodium_crypto_kdf_keygen"),
        Spec { name: "sodium_crypto_kx_client_session_keys", params: vec![s("client_key_pair"), s("server_key")], result: ResultKind::Array, shape: Shape::KxSessionKeys, lens: vec![len(0, 64, "SODIUM_CRYPTO_KX_KEYPAIRBYTES"), len(1, 32, "SODIUM_CRYPTO_KX_PUBLICKEYBYTES")] },
        keygen("sodium_crypto_kx_keypair"),
        Spec { name: "sodium_crypto_kx_publickey", params: vec![s("key_pair")], result: ResultKind::Str, shape: Shape::Plain, lens: vec![len(0, 64, "SODIUM_CRYPTO_KX_KEYPAIRBYTES")] },
        Spec { name: "sodium_crypto_kx_secretkey", params: vec![s("key_pair")], result: ResultKind::Str, shape: Shape::Plain, lens: vec![len(0, 64, "SODIUM_CRYPTO_KX_KEYPAIRBYTES")] },
        Spec { name: "sodium_crypto_kx_seed_keypair", params: vec![s("seed")], result: ResultKind::Str, shape: Shape::Plain, lens: vec![len(0, 32, "SODIUM_CRYPTO_KX_SEEDBYTES")] },
        Spec { name: "sodium_crypto_kx_server_session_keys", params: vec![s("server_key_pair"), s("client_key")], result: ResultKind::Array, shape: Shape::KxSessionKeys, lens: vec![len(0, 64, "SODIUM_CRYPTO_KX_KEYPAIRBYTES"), len(1, 32, "SODIUM_CRYPTO_KX_PUBLICKEYBYTES")] },
        Spec { name: "sodium_crypto_pwhash", params: vec![i("length"), s("password"), s("salt"), i("opslimit"), i("memlimit"), id("algo", 2)], result: ResultKind::Str, shape: Shape::Pwhash, lens: vec![] },
        Spec { name: "sodium_crypto_pwhash_scryptsalsa208sha256", params: vec![i("length"), s("password"), s("salt"), i("opslimit"), i("memlimit")], result: ResultKind::Str, shape: Shape::Scrypt, lens: vec![] },
        Spec { name: "sodium_crypto_pwhash_scryptsalsa208sha256_str", params: vec![s("password"), i("opslimit"), i("memlimit")], result: ResultKind::Str, shape: Shape::ScryptStr, lens: vec![] },
        Spec { name: "sodium_crypto_pwhash_scryptsalsa208sha256_str_verify", params: vec![s("hash"), s("password")], result: ResultKind::Bool, shape: Shape::ScryptStrVerify, lens: vec![] },
        Spec { name: "sodium_crypto_pwhash_str", params: vec![s("password"), i("opslimit"), i("memlimit")], result: ResultKind::Str, shape: Shape::PwhashStr, lens: vec![] },
        Spec { name: "sodium_crypto_pwhash_str_needs_rehash", params: vec![s("password"), i("opslimit"), i("memlimit")], result: ResultKind::Bool, shape: Shape::PwhashStrNeedsRehash, lens: vec![] },
        Spec { name: "sodium_crypto_pwhash_str_verify", params: vec![s("hash"), s("password")], result: ResultKind::Bool, shape: Shape::PwhashStrVerify, lens: vec![] },
        Spec { name: "sodium_crypto_scalarmult", params: vec![s("n"), s("p")], result: ResultKind::Str, shape: Shape::Plain, lens: vec![len(0, 32, "SODIUM_CRYPTO_SCALARMULT_SCALARBYTES"), len(1, 32, "SODIUM_CRYPTO_SCALARMULT_BYTES")] },
        Spec { name: "sodium_crypto_scalarmult_base", params: vec![s("secret_key")], result: ResultKind::Str, shape: Shape::Plain, lens: vec![len(0, 32, "SODIUM_CRYPTO_SCALARMULT_SCALARBYTES")] },
        Spec { name: "sodium_crypto_scalarmult_ristretto255", params: vec![s("n"), s("p")], result: ResultKind::Str, shape: Shape::Plain, lens: vec![len(0, 32, "SODIUM_CRYPTO_SCALARMULT_RISTRETTO255_SCALARBYTES"), len(1, 32, "SODIUM_CRYPTO_SCALARMULT_RISTRETTO255_BYTES")] },
        Spec { name: "sodium_crypto_scalarmult_ristretto255_base", params: vec![s("n")], result: ResultKind::Str, shape: Shape::Plain, lens: vec![len(0, 32, "SODIUM_CRYPTO_SCALARMULT_RISTRETTO255_SCALARBYTES")] },
        Spec { name: "sodium_crypto_secretbox", params: vec![s("message"), s("nonce"), s("key")], result: ResultKind::Str, shape: Shape::Plain, lens: vec![len(1, 24, "SODIUM_CRYPTO_SECRETBOX_NONCEBYTES"), len(2, 32, "SODIUM_CRYPTO_SECRETBOX_KEYBYTES")] },
        keygen("sodium_crypto_secretbox_keygen"),
        Spec { name: "sodium_crypto_secretbox_open", params: vec![s("ciphertext"), s("nonce"), s("key")], result: ResultKind::StrOrFalse, shape: Shape::Plain, lens: vec![len(1, 24, "SODIUM_CRYPTO_SECRETBOX_NONCEBYTES"), len(2, 32, "SODIUM_CRYPTO_SECRETBOX_KEYBYTES")] },
        Spec { name: "sodium_crypto_secretstream_xchacha20poly1305_init_pull", params: vec![s("header"), s("key")], result: ResultKind::Str, shape: Shape::Plain, lens: vec![len(0, 24, "SODIUM_CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_HEADERBYTES"), len(1, 32, "SODIUM_CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_KEYBYTES")] },
        Spec { name: "sodium_crypto_secretstream_xchacha20poly1305_init_push", params: vec![s("key")], result: ResultKind::Array, shape: Shape::SecretstreamInitPush, lens: vec![len(0, 32, "SODIUM_CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_KEYBYTES")] },
        keygen("sodium_crypto_secretstream_xchacha20poly1305_keygen"),
        Spec { name: "sodium_crypto_secretstream_xchacha20poly1305_pull", params: vec![s("state"), s("ciphertext"), sd("additional_data", "")], result: ResultKind::Array, shape: Shape::SecretstreamPull, lens: vec![] },
        Spec { name: "sodium_crypto_secretstream_xchacha20poly1305_push", params: vec![s("state"), s("message"), sd("additional_data", ""), id("tag", 0)], result: ResultKind::Str, shape: Shape::SecretstreamPush, lens: vec![] },
        Spec { name: "sodium_crypto_secretstream_xchacha20poly1305_rekey", params: vec![s("state")], result: ResultKind::Void, shape: Shape::SecretstreamRekey, lens: vec![] },
        Spec { name: "sodium_crypto_shorthash", params: vec![s("message"), s("key")], result: ResultKind::Str, shape: Shape::Plain, lens: vec![len(1, 16, "SODIUM_CRYPTO_SHORTHASH_KEYBYTES")] },
        keygen("sodium_crypto_shorthash_keygen"),
        Spec { name: "sodium_crypto_sign", params: vec![s("message"), s("secret_key")], result: ResultKind::Str, shape: Shape::Plain, lens: vec![len(1, 64, "SODIUM_CRYPTO_SIGN_SECRETKEYBYTES")] },
        Spec { name: "sodium_crypto_sign_detached", params: vec![s("message"), s("secret_key")], result: ResultKind::Str, shape: Shape::Plain, lens: vec![len(1, 64, "SODIUM_CRYPTO_SIGN_SECRETKEYBYTES")] },
        Spec { name: "sodium_crypto_sign_ed25519_pk_to_curve25519", params: vec![s("public_key")], result: ResultKind::Str, shape: Shape::Plain, lens: vec![len(0, 32, "SODIUM_CRYPTO_SIGN_PUBLICKEYBYTES")] },
        Spec { name: "sodium_crypto_sign_ed25519_sk_to_curve25519", params: vec![s("secret_key")], result: ResultKind::Str, shape: Shape::Plain, lens: vec![len(0, 64, "SODIUM_CRYPTO_SIGN_SECRETKEYBYTES")] },
        keygen("sodium_crypto_sign_keypair"),
        Spec { name: "sodium_crypto_sign_keypair_from_secretkey_and_publickey", params: vec![s("secret_key"), s("public_key")], result: ResultKind::Str, shape: Shape::Plain, lens: vec![len(0, 64, "SODIUM_CRYPTO_SIGN_SECRETKEYBYTES"), len(1, 32, "SODIUM_CRYPTO_SIGN_PUBLICKEYBYTES")] },
        Spec { name: "sodium_crypto_sign_open", params: vec![s("signed_message"), s("public_key")], result: ResultKind::StrOrFalse, shape: Shape::Plain, lens: vec![len(1, 32, "SODIUM_CRYPTO_SIGN_PUBLICKEYBYTES")] },
        Spec { name: "sodium_crypto_sign_publickey", params: vec![s("key_pair")], result: ResultKind::Str, shape: Shape::Plain, lens: vec![len(0, 96, "SODIUM_CRYPTO_SIGN_KEYPAIRBYTES")] },
        Spec { name: "sodium_crypto_sign_publickey_from_secretkey", params: vec![s("secret_key")], result: ResultKind::Str, shape: Shape::Plain, lens: vec![len(0, 64, "SODIUM_CRYPTO_SIGN_SECRETKEYBYTES")] },
        Spec { name: "sodium_crypto_sign_secretkey", params: vec![s("key_pair")], result: ResultKind::Str, shape: Shape::Plain, lens: vec![len(0, 96, "SODIUM_CRYPTO_SIGN_KEYPAIRBYTES")] },
        Spec { name: "sodium_crypto_sign_seed_keypair", params: vec![s("seed")], result: ResultKind::Str, shape: Shape::Plain, lens: vec![len(0, 32, "SODIUM_CRYPTO_SIGN_SEEDBYTES")] },
        Spec { name: "sodium_crypto_sign_verify_detached", params: vec![s("signature"), s("message"), s("public_key")], result: ResultKind::Bool, shape: Shape::Plain, lens: vec![len(0, 64, "SODIUM_CRYPTO_SIGN_BYTES"), len(2, 32, "SODIUM_CRYPTO_SIGN_PUBLICKEYBYTES")] },
        Spec { name: "sodium_crypto_stream", params: vec![i("length"), s("nonce"), s("key")], result: ResultKind::Str, shape: Shape::Plain, lens: vec![len(1, 24, "SODIUM_CRYPTO_STREAM_NONCEBYTES"), len(2, 32, "SODIUM_CRYPTO_STREAM_KEYBYTES")] },
        keygen("sodium_crypto_stream_keygen"),
        Spec { name: "sodium_crypto_stream_xchacha20", params: vec![i("length"), s("nonce"), s("key")], result: ResultKind::Str, shape: Shape::Plain, lens: vec![len(1, 24, "SODIUM_CRYPTO_STREAM_XCHACHA20_NONCEBYTES"), len(2, 32, "SODIUM_CRYPTO_STREAM_XCHACHA20_KEYBYTES")] },
        keygen("sodium_crypto_stream_xchacha20_keygen"),
        Spec { name: "sodium_crypto_stream_xchacha20_xor", params: vec![s("message"), s("nonce"), s("key")], result: ResultKind::Str, shape: Shape::Plain, lens: vec![len(1, 24, "SODIUM_CRYPTO_STREAM_XCHACHA20_NONCEBYTES"), len(2, 32, "SODIUM_CRYPTO_STREAM_XCHACHA20_KEYBYTES")] },
        Spec { name: "sodium_crypto_stream_xchacha20_xor_ic", params: vec![s("message"), s("nonce"), i("counter"), s("key")], result: ResultKind::Str, shape: Shape::Plain, lens: vec![len(1, 24, "SODIUM_CRYPTO_STREAM_XCHACHA20_NONCEBYTES"), len(3, 32, "SODIUM_CRYPTO_STREAM_XCHACHA20_KEYBYTES")] },
        Spec { name: "sodium_crypto_stream_xor", params: vec![s("message"), s("nonce"), s("key")], result: ResultKind::Str, shape: Shape::Plain, lens: vec![len(1, 24, "SODIUM_CRYPTO_STREAM_NONCEBYTES"), len(2, 32, "SODIUM_CRYPTO_STREAM_KEYBYTES")] },
        Spec { name: "sodium_hex2bin", params: vec![s("string"), sd("ignore", "")], result: ResultKind::Str, shape: Shape::Hex2bin, lens: vec![] },
        Spec { name: "sodium_increment", params: vec![s("string")], result: ResultKind::Void, shape: Shape::Increment, lens: vec![] },
        Spec { name: "sodium_memcmp", params: vec![s("string1"), s("string2")], result: ResultKind::Int, shape: Shape::Memcmp, lens: vec![] },
        Spec { name: "sodium_memzero", params: vec![s("string")], result: ResultKind::Void, shape: Shape::Memzero, lens: vec![] },
        Spec { name: "sodium_pad", params: vec![s("string"), i("block_size")], result: ResultKind::Str, shape: Shape::Pad, lens: vec![] },
        Spec { name: "sodium_unpad", params: vec![s("string"), i("block_size")], result: ResultKind::Str, shape: Shape::Unpad, lens: vec![] },
    ]
}

/// Builds one AEAD encrypt/decrypt spec with its nonce and key lengths.
fn aead(name: &'static str, prefix: &'static str, nonce: i64, key: i64) -> Spec {
    Spec { name, params: vec![s("message"), s("additional_data"), s("nonce"), s("key")], result: if name.ends_with("_decrypt") { ResultKind::StrOrFalse } else { ResultKind::Str }, shape: Shape::Plain, lens: vec![len(2, nonce, aead_symbol(prefix, "NPUBBYTES")), len(3, key, aead_symbol(prefix, "KEYBYTES"))] }
}

/// Returns the constant spelling for one AEAD length.
fn aead_symbol(prefix: &'static str, suffix: &'static str) -> &'static str {
    match (prefix, suffix) {
        ("SODIUM_CRYPTO_AEAD_AEGIS128L", "NPUBBYTES") => "SODIUM_CRYPTO_AEAD_AEGIS128L_NPUBBYTES",
        ("SODIUM_CRYPTO_AEAD_AEGIS128L", _) => "SODIUM_CRYPTO_AEAD_AEGIS128L_KEYBYTES",
        ("SODIUM_CRYPTO_AEAD_AEGIS256", "NPUBBYTES") => "SODIUM_CRYPTO_AEAD_AEGIS256_NPUBBYTES",
        ("SODIUM_CRYPTO_AEAD_AEGIS256", _) => "SODIUM_CRYPTO_AEAD_AEGIS256_KEYBYTES",
        ("SODIUM_CRYPTO_AEAD_AES256GCM", "NPUBBYTES") => "SODIUM_CRYPTO_AEAD_AES256GCM_NPUBBYTES",
        ("SODIUM_CRYPTO_AEAD_AES256GCM", _) => "SODIUM_CRYPTO_AEAD_AES256GCM_KEYBYTES",
        ("SODIUM_CRYPTO_AEAD_CHACHA20POLY1305", "NPUBBYTES") => "SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_NPUBBYTES",
        ("SODIUM_CRYPTO_AEAD_CHACHA20POLY1305", _) => "SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_KEYBYTES",
        ("SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_IETF", "NPUBBYTES") => "SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_IETF_NPUBBYTES",
        ("SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_IETF", _) => "SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_IETF_KEYBYTES",
        ("SODIUM_CRYPTO_AEAD_XCHACHA20POLY1305_IETF", "NPUBBYTES") => "SODIUM_CRYPTO_AEAD_XCHACHA20POLY1305_IETF_NPUBBYTES",
        ("SODIUM_CRYPTO_AEAD_XCHACHA20POLY1305_IETF", _) => "SODIUM_CRYPTO_AEAD_XCHACHA20POLY1305_IETF_KEYBYTES",
        _ => "SODIUM_CRYPTO_AEAD_KEYBYTES",
    }
}

/// Builds one key-generation spec.
fn keygen(name: &'static str) -> Spec {
    Spec { name, params: vec![], result: ResultKind::Str, shape: Shape::Plain, lens: vec![] }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// Verifies the table covers exactly the shared contract surface with unique names.
    #[test]
    fn specs_cover_every_contract() {
        let specs = specs();
        let names: HashSet<&str> = specs.iter().map(|spec| spec.name).collect();
        assert_eq!(names.len(), specs.len());
        assert_eq!(names.len(), elephc_builtin_contract::SODIUM_FUNCTIONS.len());
        for name in elephc_builtin_contract::SODIUM_FUNCTIONS {
            assert!(names.contains(name), "missing helper spec for {name}");
        }
    }

    /// Verifies every fixed-length message names the constant it enforces.
    #[test]
    fn length_checks_use_constant_spellings() {
        for spec in specs() {
            for check in spec.lens {
                assert!(spec.params.len() > check.param);
                assert!(check.symbol.starts_with("SODIUM_"), "{}", spec.name);
                assert!(check.symbol.contains("BYTES"), "{}", spec.name);
            }
        }
    }
}
