//! Purpose:
//! Declares the PHP-visible `ext/sodium` procedural surface as shared builtin contracts.
//!
//! Called from:
//! - `crate::registry` while assembling the authoritative builtin catalog.
//! - AOT and Magician home-file joins keyed by the same stable `BuiltinId`s.
//! - The sodium prelude detector, which uses [`FUNCTIONS`] as its exact name set.
//!
//! Key details:
//! - Signatures are php-src 8.5's `ext/sodium` surface against libsodium 1.0.22.
//! - The checker defers value/coercion diagnostics to the runtime helper, so these
//!   types are for arity, named arguments, docs, reflection metadata and strict-PHP
//!   visibility; by-reference parameters are still authoritative for write-back.
//! - Entries stay in canonical lowercase name order, as the registry requires.

use crate::{
    Area, BuiltinContract, BuiltinId, BuiltinKind, DefaultSpec, ParamSpec, PhpModule, TypeSpec,
};

/// Canonical PHP names owned by the sodium extension, in stable order.
pub const FUNCTIONS: &[&str] = &[
    "sodium_add",
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
    "sodium_crypto_generichash_final",
    "sodium_crypto_generichash_init",
    "sodium_crypto_generichash_keygen",
    "sodium_crypto_generichash_update",
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
    "sodium_crypto_secretstream_xchacha20poly1305_pull",
    "sodium_crypto_secretstream_xchacha20poly1305_push",
    "sodium_crypto_secretstream_xchacha20poly1305_rekey",
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
    "sodium_increment",
    "sodium_memcmp",
    "sodium_memzero",
    "sodium_pad",
    "sodium_unpad",
];

/// Returns the canonical names as an iterator for requirements/parity joins.
pub fn contract_names() -> impl Iterator<Item = &'static str> {
    FUNCTIONS.iter().copied()
}

/// One ordinary by-value parameter.
macro_rules! param {
    ($name:literal, $ty:expr) => {
        ParamSpec { name: $name, ty: $ty, default: None, by_ref: false }
    };
    ($name:literal, $ty:expr, $default:expr) => {
        ParamSpec { name: $name, ty: $ty, default: Some($default), by_ref: false }
    };
}

/// One by-reference parameter.
macro_rules! ref_param {
    ($name:literal, $ty:expr) => {
        ParamSpec { name: $name, ty: $ty, default: None, by_ref: true }
    };
}

/// One procedural sodium contract.
macro_rules! sodium_contract {
    ($name:literal, [$($param:expr),*], $returns:expr, $summary:literal) => {
        BuiltinContract {
            id: BuiltinId::from_canonical_name($name),
            name: $name,
            area: Area::Sodium,
            module: PhpModule::Sodium,
            since: None,
            kind: BuiltinKind::Function,
            params: &[$($param),*],
            variadic: None,
            variadic_by_ref: false,
            min_args: None,
            max_args: None,
            arity_error: None,
            returns: $returns,
            by_ref_return: false,
            summary: $summary,
            examples: &[],
            php_manual: None,
            deprecation: None,
            extension: false,
            internal: false,
            requirements: &[],
        }
    };
}

/// One PHP-visible by-reference sodium mutator supplied by the injected prelude.
macro_rules! sodium_prelude_contract {
    ($name:literal, [$($param:expr),*], $returns:expr, $summary:literal) => {
        BuiltinContract {
            id: BuiltinId::from_canonical_name($name),
            name: $name,
            area: Area::Sodium,
            module: PhpModule::Sodium,
            since: None,
            kind: BuiltinKind::PreludeProvided,
            params: &[$($param),*],
            variadic: None,
            variadic_by_ref: false,
            min_args: None,
            max_args: None,
            arity_error: None,
            returns: $returns,
            by_ref_return: false,
            summary: $summary,
            examples: &[],
            php_manual: None,
            deprecation: None,
            extension: false,
            internal: false,
            requirements: &[],
        }
    };
}

pub(crate) static CONTRACTS: &[BuiltinContract] = &[
    sodium_prelude_contract!("sodium_add", [ref_param!("string1", TypeSpec::Str), param!("string2", TypeSpec::Str)], TypeSpec::Void, "Adds two strings as little-endian integers."),
    sodium_contract!("sodium_base642bin", [param!("string", TypeSpec::Str), param!("id", TypeSpec::Int), param!("ignore", TypeSpec::Str, DefaultSpec::Str(""))], TypeSpec::Str, "Decodes a base64 string."),
    sodium_contract!("sodium_bin2base64", [param!("string", TypeSpec::Str), param!("id", TypeSpec::Int)], TypeSpec::Str, "Encodes a binary string as base64."),
    sodium_contract!("sodium_bin2hex", [param!("string", TypeSpec::Str)], TypeSpec::Str, "Encodes a binary string as hexadecimal."),
    sodium_contract!("sodium_compare", [param!("string1", TypeSpec::Str), param!("string2", TypeSpec::Str)], TypeSpec::Int, "Compares two strings in constant time."),
    sodium_contract!("sodium_crypto_aead_aegis128l_decrypt", [param!("ciphertext", TypeSpec::Str), param!("additional_data", TypeSpec::Str), param!("nonce", TypeSpec::Str), param!("key", TypeSpec::Str)], TypeSpec::Union(&[TypeSpec::Str, TypeSpec::False]), "Encrypts or decrypts with an AEAD cipher."),
    sodium_contract!("sodium_crypto_aead_aegis128l_encrypt", [param!("message", TypeSpec::Str), param!("additional_data", TypeSpec::Str), param!("nonce", TypeSpec::Str), param!("key", TypeSpec::Str)], TypeSpec::Str, "Encrypts or decrypts with an AEAD cipher."),
    sodium_contract!("sodium_crypto_aead_aegis128l_keygen", [], TypeSpec::Str, "Generates an AEAD key."),
    sodium_contract!("sodium_crypto_aead_aegis256_decrypt", [param!("ciphertext", TypeSpec::Str), param!("additional_data", TypeSpec::Str), param!("nonce", TypeSpec::Str), param!("key", TypeSpec::Str)], TypeSpec::Union(&[TypeSpec::Str, TypeSpec::False]), "Encrypts or decrypts with an AEAD cipher."),
    sodium_contract!("sodium_crypto_aead_aegis256_encrypt", [param!("message", TypeSpec::Str), param!("additional_data", TypeSpec::Str), param!("nonce", TypeSpec::Str), param!("key", TypeSpec::Str)], TypeSpec::Str, "Encrypts or decrypts with an AEAD cipher."),
    sodium_contract!("sodium_crypto_aead_aegis256_keygen", [], TypeSpec::Str, "Generates an AEAD key."),
    sodium_contract!("sodium_crypto_aead_aes256gcm_decrypt", [param!("ciphertext", TypeSpec::Str), param!("additional_data", TypeSpec::Str), param!("nonce", TypeSpec::Str), param!("key", TypeSpec::Str)], TypeSpec::Union(&[TypeSpec::Str, TypeSpec::False]), "Encrypts or decrypts with an AEAD cipher."),
    sodium_contract!("sodium_crypto_aead_aes256gcm_encrypt", [param!("message", TypeSpec::Str), param!("additional_data", TypeSpec::Str), param!("nonce", TypeSpec::Str), param!("key", TypeSpec::Str)], TypeSpec::Str, "Encrypts or decrypts with an AEAD cipher."),
    sodium_contract!("sodium_crypto_aead_aes256gcm_is_available", [], TypeSpec::Bool, "Reports AEAD cipher availability."),
    sodium_contract!("sodium_crypto_aead_aes256gcm_keygen", [], TypeSpec::Str, "Generates an AEAD key."),
    sodium_contract!("sodium_crypto_aead_chacha20poly1305_decrypt", [param!("ciphertext", TypeSpec::Str), param!("additional_data", TypeSpec::Str), param!("nonce", TypeSpec::Str), param!("key", TypeSpec::Str)], TypeSpec::Union(&[TypeSpec::Str, TypeSpec::False]), "Encrypts or decrypts with an AEAD cipher."),
    sodium_contract!("sodium_crypto_aead_chacha20poly1305_encrypt", [param!("message", TypeSpec::Str), param!("additional_data", TypeSpec::Str), param!("nonce", TypeSpec::Str), param!("key", TypeSpec::Str)], TypeSpec::Str, "Encrypts or decrypts with an AEAD cipher."),
    sodium_contract!("sodium_crypto_aead_chacha20poly1305_ietf_decrypt", [param!("ciphertext", TypeSpec::Str), param!("additional_data", TypeSpec::Str), param!("nonce", TypeSpec::Str), param!("key", TypeSpec::Str)], TypeSpec::Union(&[TypeSpec::Str, TypeSpec::False]), "Encrypts or decrypts with an AEAD cipher."),
    sodium_contract!("sodium_crypto_aead_chacha20poly1305_ietf_encrypt", [param!("message", TypeSpec::Str), param!("additional_data", TypeSpec::Str), param!("nonce", TypeSpec::Str), param!("key", TypeSpec::Str)], TypeSpec::Str, "Encrypts or decrypts with an AEAD cipher."),
    sodium_contract!("sodium_crypto_aead_chacha20poly1305_ietf_keygen", [], TypeSpec::Str, "Generates an AEAD key."),
    sodium_contract!("sodium_crypto_aead_chacha20poly1305_keygen", [], TypeSpec::Str, "Generates an AEAD key."),
    sodium_contract!("sodium_crypto_aead_xchacha20poly1305_ietf_decrypt", [param!("ciphertext", TypeSpec::Str), param!("additional_data", TypeSpec::Str), param!("nonce", TypeSpec::Str), param!("key", TypeSpec::Str)], TypeSpec::Union(&[TypeSpec::Str, TypeSpec::False]), "Encrypts or decrypts with an AEAD cipher."),
    sodium_contract!("sodium_crypto_aead_xchacha20poly1305_ietf_encrypt", [param!("message", TypeSpec::Str), param!("additional_data", TypeSpec::Str), param!("nonce", TypeSpec::Str), param!("key", TypeSpec::Str)], TypeSpec::Str, "Encrypts or decrypts with an AEAD cipher."),
    sodium_contract!("sodium_crypto_aead_xchacha20poly1305_ietf_keygen", [], TypeSpec::Str, "Generates an AEAD key."),
    sodium_contract!("sodium_crypto_auth", [param!("message", TypeSpec::Str), param!("key", TypeSpec::Str)], TypeSpec::Str, "Computes an authentication tag."),
    sodium_contract!("sodium_crypto_auth_keygen", [], TypeSpec::Str, "Generates an authentication key."),
    sodium_contract!("sodium_crypto_auth_verify", [param!("mac", TypeSpec::Str), param!("message", TypeSpec::Str), param!("key", TypeSpec::Str)], TypeSpec::Bool, "Verifies an authentication tag."),
    sodium_contract!("sodium_crypto_box", [param!("message", TypeSpec::Str), param!("nonce", TypeSpec::Str), param!("key_pair", TypeSpec::Str)], TypeSpec::Str, "Encrypts a message with a box key pair."),
    sodium_contract!("sodium_crypto_box_keypair", [], TypeSpec::Str, "Generates a box key pair."),
    sodium_contract!("sodium_crypto_box_keypair_from_secretkey_and_publickey", [param!("secret_key", TypeSpec::Str), param!("public_key", TypeSpec::Str)], TypeSpec::Str, "Builds a box key pair from its two halves."),
    sodium_contract!("sodium_crypto_box_open", [param!("ciphertext", TypeSpec::Str), param!("nonce", TypeSpec::Str), param!("key_pair", TypeSpec::Str)], TypeSpec::Union(&[TypeSpec::Str, TypeSpec::False]), "Decrypts a box ciphertext."),
    sodium_contract!("sodium_crypto_box_publickey", [param!("key_pair", TypeSpec::Str)], TypeSpec::Str, "Extracts the public key from a box key pair."),
    sodium_contract!("sodium_crypto_box_publickey_from_secretkey", [param!("secret_key", TypeSpec::Str)], TypeSpec::Str, "Derives a box public key from a secret key."),
    sodium_contract!("sodium_crypto_box_seal", [param!("message", TypeSpec::Str), param!("public_key", TypeSpec::Str)], TypeSpec::Str, "Encrypts a message with an anonymous sealed box."),
    sodium_contract!("sodium_crypto_box_seal_open", [param!("ciphertext", TypeSpec::Str), param!("key_pair", TypeSpec::Str)], TypeSpec::Union(&[TypeSpec::Str, TypeSpec::False]), "Decrypts a sealed box."),
    sodium_contract!("sodium_crypto_box_secretkey", [param!("key_pair", TypeSpec::Str)], TypeSpec::Str, "Extracts the secret key from a box key pair."),
    sodium_contract!("sodium_crypto_box_seed_keypair", [param!("seed", TypeSpec::Str)], TypeSpec::Str, "Derives a box key pair from a seed."),
    sodium_contract!("sodium_crypto_core_ristretto255_add", [param!("p", TypeSpec::Str), param!("q", TypeSpec::Str)], TypeSpec::Str, "Performs a Ristretto255 group operation."),
    sodium_contract!("sodium_crypto_core_ristretto255_from_hash", [param!("s", TypeSpec::Str)], TypeSpec::Str, "Performs a Ristretto255 group operation."),
    sodium_contract!("sodium_crypto_core_ristretto255_is_valid_point", [param!("s", TypeSpec::Str)], TypeSpec::Bool, "Performs a Ristretto255 group operation."),
    sodium_contract!("sodium_crypto_core_ristretto255_random", [], TypeSpec::Str, "Performs a Ristretto255 group operation."),
    sodium_contract!("sodium_crypto_core_ristretto255_scalar_add", [param!("x", TypeSpec::Str), param!("y", TypeSpec::Str)], TypeSpec::Str, "Performs a Ristretto255 scalar operation."),
    sodium_contract!("sodium_crypto_core_ristretto255_scalar_complement", [param!("s", TypeSpec::Str)], TypeSpec::Str, "Performs a Ristretto255 scalar operation."),
    sodium_contract!("sodium_crypto_core_ristretto255_scalar_invert", [param!("s", TypeSpec::Str)], TypeSpec::Str, "Performs a Ristretto255 scalar operation."),
    sodium_contract!("sodium_crypto_core_ristretto255_scalar_mul", [param!("x", TypeSpec::Str), param!("y", TypeSpec::Str)], TypeSpec::Str, "Performs a Ristretto255 scalar operation."),
    sodium_contract!("sodium_crypto_core_ristretto255_scalar_negate", [param!("s", TypeSpec::Str)], TypeSpec::Str, "Performs a Ristretto255 scalar operation."),
    sodium_contract!("sodium_crypto_core_ristretto255_scalar_random", [], TypeSpec::Str, "Performs a Ristretto255 scalar operation."),
    sodium_contract!("sodium_crypto_core_ristretto255_scalar_reduce", [param!("s", TypeSpec::Str)], TypeSpec::Str, "Performs a Ristretto255 scalar operation."),
    sodium_contract!("sodium_crypto_core_ristretto255_scalar_sub", [param!("x", TypeSpec::Str), param!("y", TypeSpec::Str)], TypeSpec::Str, "Performs a Ristretto255 scalar operation."),
    sodium_contract!("sodium_crypto_core_ristretto255_sub", [param!("p", TypeSpec::Str), param!("q", TypeSpec::Str)], TypeSpec::Str, "Performs a Ristretto255 group operation."),
    sodium_contract!("sodium_crypto_generichash", [param!("message", TypeSpec::Str), param!("key", TypeSpec::Str, DefaultSpec::Str("")), param!("length", TypeSpec::Int, DefaultSpec::Int(32))], TypeSpec::Str, "Computes a BLAKE2b hash."),
    sodium_prelude_contract!("sodium_crypto_generichash_final", [ref_param!("state", TypeSpec::Str), param!("length", TypeSpec::Int, DefaultSpec::Int(32))], TypeSpec::Str, "Finalizes an incremental BLAKE2b hash state."),
    sodium_contract!("sodium_crypto_generichash_init", [param!("key", TypeSpec::Str, DefaultSpec::Str("")), param!("length", TypeSpec::Int, DefaultSpec::Int(32))], TypeSpec::Str, "Initializes an incremental BLAKE2b hash state."),
    sodium_contract!("sodium_crypto_generichash_keygen", [], TypeSpec::Str, "Generates a BLAKE2b key."),
    sodium_prelude_contract!("sodium_crypto_generichash_update", [ref_param!("state", TypeSpec::Str), param!("message", TypeSpec::Str)], TypeSpec::Bool, "Updates an incremental BLAKE2b hash state."),
    sodium_contract!("sodium_crypto_kdf_derive_from_key", [param!("subkey_length", TypeSpec::Int), param!("subkey_id", TypeSpec::Int), param!("context", TypeSpec::Str), param!("key", TypeSpec::Str)], TypeSpec::Str, "Derives a subkey from a master key."),
    sodium_contract!("sodium_crypto_kdf_keygen", [], TypeSpec::Str, "Generates a KDF master key."),
    sodium_contract!("sodium_crypto_kx_client_session_keys", [param!("client_key_pair", TypeSpec::Str), param!("server_key", TypeSpec::Str)], TypeSpec::Array, "Performs a key-exchange operation."),
    sodium_contract!("sodium_crypto_kx_keypair", [], TypeSpec::Str, "Performs a key-exchange operation."),
    sodium_contract!("sodium_crypto_kx_publickey", [param!("key_pair", TypeSpec::Str)], TypeSpec::Str, "Performs a key-exchange operation."),
    sodium_contract!("sodium_crypto_kx_secretkey", [param!("key_pair", TypeSpec::Str)], TypeSpec::Str, "Performs a key-exchange operation."),
    sodium_contract!("sodium_crypto_kx_seed_keypair", [param!("seed", TypeSpec::Str)], TypeSpec::Str, "Performs a key-exchange operation."),
    sodium_contract!("sodium_crypto_kx_server_session_keys", [param!("server_key_pair", TypeSpec::Str), param!("client_key", TypeSpec::Str)], TypeSpec::Array, "Performs a key-exchange operation."),
    sodium_contract!("sodium_crypto_pwhash", [param!("length", TypeSpec::Int), param!("password", TypeSpec::Str), param!("salt", TypeSpec::Str), param!("opslimit", TypeSpec::Int), param!("memlimit", TypeSpec::Int), param!("algo", TypeSpec::Int, DefaultSpec::Int(2))], TypeSpec::Str, "Derives a key from a password."),
    sodium_contract!("sodium_crypto_pwhash_scryptsalsa208sha256", [param!("length", TypeSpec::Int), param!("password", TypeSpec::Str), param!("salt", TypeSpec::Str), param!("opslimit", TypeSpec::Int), param!("memlimit", TypeSpec::Int)], TypeSpec::Str, "Derives a key with scrypt."),
    sodium_contract!("sodium_crypto_pwhash_scryptsalsa208sha256_str", [param!("password", TypeSpec::Str), param!("opslimit", TypeSpec::Int), param!("memlimit", TypeSpec::Int)], TypeSpec::Str, "Hashes a password with scrypt."),
    sodium_contract!("sodium_crypto_pwhash_scryptsalsa208sha256_str_verify", [param!("hash", TypeSpec::Str), param!("password", TypeSpec::Str)], TypeSpec::Bool, "Verifies a scrypt password hash."),
    sodium_contract!("sodium_crypto_pwhash_str", [param!("password", TypeSpec::Str), param!("opslimit", TypeSpec::Int), param!("memlimit", TypeSpec::Int)], TypeSpec::Str, "Hashes a password with Argon2id."),
    sodium_contract!("sodium_crypto_pwhash_str_needs_rehash", [param!("password", TypeSpec::Str), param!("opslimit", TypeSpec::Int), param!("memlimit", TypeSpec::Int)], TypeSpec::Bool, "Reports whether a password hash needs rehashing."),
    sodium_contract!("sodium_crypto_pwhash_str_verify", [param!("hash", TypeSpec::Str), param!("password", TypeSpec::Str)], TypeSpec::Bool, "Verifies an Argon2id password hash."),
    sodium_contract!("sodium_crypto_scalarmult", [param!("n", TypeSpec::Str), param!("p", TypeSpec::Str)], TypeSpec::Str, "Performs a scalar multiplication."),
    sodium_contract!("sodium_crypto_scalarmult_base", [param!("secret_key", TypeSpec::Str)], TypeSpec::Str, "Computes a scalar multiplication base point."),
    sodium_contract!("sodium_crypto_scalarmult_ristretto255", [param!("n", TypeSpec::Str), param!("p", TypeSpec::Str)], TypeSpec::Str, "Performs a scalar multiplication."),
    sodium_contract!("sodium_crypto_scalarmult_ristretto255_base", [param!("n", TypeSpec::Str)], TypeSpec::Str, "Computes a scalar multiplication base point."),
    sodium_contract!("sodium_crypto_secretbox", [param!("message", TypeSpec::Str), param!("nonce", TypeSpec::Str), param!("key", TypeSpec::Str)], TypeSpec::Str, "Encrypts a message with a secret key."),
    sodium_contract!("sodium_crypto_secretbox_keygen", [], TypeSpec::Str, "Generates a secretbox key."),
    sodium_contract!("sodium_crypto_secretbox_open", [param!("ciphertext", TypeSpec::Str), param!("nonce", TypeSpec::Str), param!("key", TypeSpec::Str)], TypeSpec::Union(&[TypeSpec::Str, TypeSpec::False]), "Decrypts a secret-key ciphertext."),
    sodium_contract!("sodium_crypto_secretstream_xchacha20poly1305_init_pull", [param!("header", TypeSpec::Str), param!("key", TypeSpec::Str)], TypeSpec::Str, "Performs a secretstream operation."),
    sodium_contract!("sodium_crypto_secretstream_xchacha20poly1305_init_push", [param!("key", TypeSpec::Str)], TypeSpec::Array, "Performs a secretstream operation."),
    sodium_contract!("sodium_crypto_secretstream_xchacha20poly1305_keygen", [], TypeSpec::Str, "Performs a secretstream operation."),
    sodium_prelude_contract!("sodium_crypto_secretstream_xchacha20poly1305_pull", [ref_param!("state", TypeSpec::Str), param!("ciphertext", TypeSpec::Str), param!("additional_data", TypeSpec::Str, DefaultSpec::Str(""))], TypeSpec::Union(&[TypeSpec::Array, TypeSpec::False]), "Performs a secretstream operation."),
    sodium_prelude_contract!("sodium_crypto_secretstream_xchacha20poly1305_push", [ref_param!("state", TypeSpec::Str), param!("message", TypeSpec::Str), param!("additional_data", TypeSpec::Str, DefaultSpec::Str("")), param!("tag", TypeSpec::Int, DefaultSpec::Int(0))], TypeSpec::Str, "Performs a secretstream operation."),
    sodium_prelude_contract!("sodium_crypto_secretstream_xchacha20poly1305_rekey", [ref_param!("state", TypeSpec::Str)], TypeSpec::Void, "Performs a secretstream operation."),
    sodium_contract!("sodium_crypto_shorthash", [param!("message", TypeSpec::Str), param!("key", TypeSpec::Str)], TypeSpec::Str, "Computes a short hash."),
    sodium_contract!("sodium_crypto_shorthash_keygen", [], TypeSpec::Str, "Generates a short-hash key."),
    sodium_contract!("sodium_crypto_sign", [param!("message", TypeSpec::Str), param!("secret_key", TypeSpec::Str)], TypeSpec::Str, "Signs a message and returns the signed message."),
    sodium_contract!("sodium_crypto_sign_detached", [param!("message", TypeSpec::Str), param!("secret_key", TypeSpec::Str)], TypeSpec::Str, "Creates a detached signature."),
    sodium_contract!("sodium_crypto_sign_ed25519_pk_to_curve25519", [param!("public_key", TypeSpec::Str)], TypeSpec::Str, "Converts an Ed25519 public key to Curve25519."),
    sodium_contract!("sodium_crypto_sign_ed25519_sk_to_curve25519", [param!("secret_key", TypeSpec::Str)], TypeSpec::Str, "Converts an Ed25519 secret key to Curve25519."),
    sodium_contract!("sodium_crypto_sign_keypair", [], TypeSpec::Str, "Generates a signature key pair."),
    sodium_contract!("sodium_crypto_sign_keypair_from_secretkey_and_publickey", [param!("secret_key", TypeSpec::Str), param!("public_key", TypeSpec::Str)], TypeSpec::Str, "Builds a signature key pair from its two halves."),
    sodium_contract!("sodium_crypto_sign_open", [param!("signed_message", TypeSpec::Str), param!("public_key", TypeSpec::Str)], TypeSpec::Union(&[TypeSpec::Str, TypeSpec::False]), "Verifies a signed message and returns its contents."),
    sodium_contract!("sodium_crypto_sign_publickey", [param!("key_pair", TypeSpec::Str)], TypeSpec::Str, "Extracts the public key from a signature key pair."),
    sodium_contract!("sodium_crypto_sign_publickey_from_secretkey", [param!("secret_key", TypeSpec::Str)], TypeSpec::Str, "Derives a signature public key from a secret key."),
    sodium_contract!("sodium_crypto_sign_secretkey", [param!("key_pair", TypeSpec::Str)], TypeSpec::Str, "Extracts the secret key from a signature key pair."),
    sodium_contract!("sodium_crypto_sign_seed_keypair", [param!("seed", TypeSpec::Str)], TypeSpec::Str, "Derives a signature key pair from a seed."),
    sodium_contract!("sodium_crypto_sign_verify_detached", [param!("signature", TypeSpec::Str), param!("message", TypeSpec::Str), param!("public_key", TypeSpec::Str)], TypeSpec::Bool, "Verifies a detached signature."),
    sodium_contract!("sodium_crypto_stream", [param!("length", TypeSpec::Int), param!("nonce", TypeSpec::Str), param!("key", TypeSpec::Str)], TypeSpec::Str, "Generates or applies a stream cipher keystream."),
    sodium_contract!("sodium_crypto_stream_keygen", [], TypeSpec::Str, "Generates or applies a stream cipher keystream."),
    sodium_contract!("sodium_crypto_stream_xchacha20", [param!("length", TypeSpec::Int), param!("nonce", TypeSpec::Str), param!("key", TypeSpec::Str)], TypeSpec::Str, "Generates or applies a stream cipher keystream."),
    sodium_contract!("sodium_crypto_stream_xchacha20_keygen", [], TypeSpec::Str, "Generates or applies a stream cipher keystream."),
    sodium_contract!("sodium_crypto_stream_xchacha20_xor", [param!("message", TypeSpec::Str), param!("nonce", TypeSpec::Str), param!("key", TypeSpec::Str)], TypeSpec::Str, "Generates or applies a stream cipher keystream."),
    sodium_contract!("sodium_crypto_stream_xchacha20_xor_ic", [param!("message", TypeSpec::Str), param!("nonce", TypeSpec::Str), param!("counter", TypeSpec::Int), param!("key", TypeSpec::Str)], TypeSpec::Str, "Generates or applies a stream cipher keystream."),
    sodium_contract!("sodium_crypto_stream_xor", [param!("message", TypeSpec::Str), param!("nonce", TypeSpec::Str), param!("key", TypeSpec::Str)], TypeSpec::Str, "Generates or applies a stream cipher keystream."),
    sodium_contract!("sodium_hex2bin", [param!("string", TypeSpec::Str), param!("ignore", TypeSpec::Str, DefaultSpec::Str(""))], TypeSpec::Str, "Decodes a hexadecimal string."),
    sodium_prelude_contract!("sodium_increment", [ref_param!("string", TypeSpec::Str)], TypeSpec::Void, "Increments a little-endian integer string."),
    sodium_contract!("sodium_memcmp", [param!("string1", TypeSpec::Str), param!("string2", TypeSpec::Str)], TypeSpec::Int, "Compares two strings in constant time."),
    sodium_prelude_contract!("sodium_memzero", [ref_param!("string", TypeSpec::Str)], TypeSpec::Void, "Overwrites a string with zero bytes."),
    sodium_contract!("sodium_pad", [param!("string", TypeSpec::Str), param!("block_size", TypeSpec::Int)], TypeSpec::Str, "Pads a string to a block size."),
    sodium_contract!("sodium_unpad", [param!("string", TypeSpec::Str), param!("block_size", TypeSpec::Int)], TypeSpec::Str, "Removes padding from a block-padded string."),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies every name has exactly one contract and the list stays alphabetical.
    #[test]
    fn contracts_match_the_function_list() {
        assert_eq!(CONTRACTS.len(), FUNCTIONS.len());
        for (contract, name) in CONTRACTS.iter().zip(FUNCTIONS) {
            assert_eq!(contract.name, *name);
            assert_eq!(contract.module, PhpModule::Sodium);
        }
        let mut sorted = FUNCTIONS.to_vec();
        sorted.sort_unstable();
        assert_eq!(sorted, FUNCTIONS);
    }

    /// Verifies the public by-reference parameters match php-src 8.5.
    #[test]
    fn by_reference_parameters_match_php_src() {
        let mut refs: Vec<(&str, &str)> = Vec::new();
        for contract in CONTRACTS {
            for param in contract.params.iter().filter(|param| param.by_ref) {
                assert_eq!(
                    param.ty,
                    TypeSpec::Str,
                    "{} ${} must keep php-src's string by-reference contract",
                    contract.name,
                    param.name,
                );
                refs.push((contract.name, param.name));
            }
        }
        assert_eq!(
            refs,
            vec![
                ("sodium_add", "string1"),
                ("sodium_crypto_generichash_final", "state"),
                ("sodium_crypto_generichash_update", "state"),
                ("sodium_crypto_secretstream_xchacha20poly1305_pull", "state"),
                ("sodium_crypto_secretstream_xchacha20poly1305_push", "state"),
                ("sodium_crypto_secretstream_xchacha20poly1305_rekey", "state"),
                ("sodium_increment", "string"),
                ("sodium_memzero", "string"),
            ]
        );
    }
}
