//! Purpose:
//! Registers every shared `ext/sodium` contract with the one generic AOT sodium semantics.
//!
//! Called from:
//! - The AOT builtin inventory through `builtin!` submissions.
//!
//! Key details:
//! - One direct submission per canonical name keeps the docs extractor's single-source
//!   home-file map and the inventory join keyed on the same `BuiltinId`.
//! - `sodium_semantics()` derives result types from the shared contract, so adding a
//!   function requires only its catalog entry and its prelude helper spec.

builtin! {
    contract: "sodium_add",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_base642bin",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_bin2base64",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_bin2hex",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_compare",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_aead_aegis128l_decrypt",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_aead_aegis128l_encrypt",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_aead_aegis128l_keygen",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_aead_aegis256_decrypt",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_aead_aegis256_encrypt",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_aead_aegis256_keygen",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_aead_aes256gcm_decrypt",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_aead_aes256gcm_encrypt",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_aead_aes256gcm_is_available",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_aead_aes256gcm_keygen",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_aead_chacha20poly1305_decrypt",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_aead_chacha20poly1305_encrypt",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_aead_chacha20poly1305_ietf_decrypt",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_aead_chacha20poly1305_ietf_encrypt",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_aead_chacha20poly1305_ietf_keygen",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_aead_chacha20poly1305_keygen",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_aead_xchacha20poly1305_ietf_decrypt",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_aead_xchacha20poly1305_ietf_encrypt",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_aead_xchacha20poly1305_ietf_keygen",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_auth",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_auth_keygen",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_auth_verify",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_box",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_box_keypair",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_box_keypair_from_secretkey_and_publickey",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_box_open",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_box_publickey",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_box_publickey_from_secretkey",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_box_seal",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_box_seal_open",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_box_secretkey",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_box_seed_keypair",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_core_ristretto255_add",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_core_ristretto255_from_hash",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_core_ristretto255_is_valid_point",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_core_ristretto255_random",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_core_ristretto255_scalar_add",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_core_ristretto255_scalar_complement",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_core_ristretto255_scalar_invert",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_core_ristretto255_scalar_mul",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_core_ristretto255_scalar_negate",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_core_ristretto255_scalar_random",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_core_ristretto255_scalar_reduce",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_core_ristretto255_scalar_sub",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_core_ristretto255_sub",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_generichash",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_generichash_final",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_generichash_init",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_generichash_keygen",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_generichash_update",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_kdf_derive_from_key",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_kdf_keygen",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_kx_client_session_keys",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_kx_keypair",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_kx_publickey",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_kx_secretkey",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_kx_seed_keypair",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_kx_server_session_keys",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_pwhash",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_pwhash_scryptsalsa208sha256",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_pwhash_scryptsalsa208sha256_str",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_pwhash_scryptsalsa208sha256_str_verify",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_pwhash_str",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_pwhash_str_needs_rehash",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_pwhash_str_verify",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_scalarmult",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_scalarmult_base",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_scalarmult_ristretto255",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_scalarmult_ristretto255_base",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_secretbox",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_secretbox_keygen",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_secretbox_open",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_secretstream_xchacha20poly1305_init_pull",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_secretstream_xchacha20poly1305_init_push",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_secretstream_xchacha20poly1305_keygen",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_secretstream_xchacha20poly1305_pull",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_secretstream_xchacha20poly1305_push",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_secretstream_xchacha20poly1305_rekey",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_shorthash",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_shorthash_keygen",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_sign",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_sign_detached",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_sign_ed25519_pk_to_curve25519",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_sign_ed25519_sk_to_curve25519",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_sign_keypair",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_sign_keypair_from_secretkey_and_publickey",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_sign_open",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_sign_publickey",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_sign_publickey_from_secretkey",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_sign_secretkey",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_sign_seed_keypair",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_sign_verify_detached",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_stream",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_stream_keygen",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_stream_xchacha20",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_stream_xchacha20_keygen",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_stream_xchacha20_xor",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_stream_xchacha20_xor_ic",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_crypto_stream_xor",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_hex2bin",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_increment",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_memcmp",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_memzero",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_pad",
    semantics: crate::builtins::sodium::sodium_semantics(),
}

builtin! {
    contract: "sodium_unpad",
    semantics: crate::builtins::sodium::sodium_semantics(),
}
