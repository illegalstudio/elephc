---
title: "Sodium"
description: "PHP's ext/sodium surface in elephc: secretbox, box, sign, generichash, AEAD, secretstream, XOFs, IP conversion, and the pure-Rust elephc-sodium bridge."
sidebar:
  order: 24
---

elephc exposes `ext/sodium` through an injected PHP prelude backed by the
pure-Rust `elephc-sodium` bridge. The bridge implements the NaCl/libsodium
primitives directly, so it needs no system `libsodium` and works on every
supported target.

## Delivered surface

| Area | Functions | Notes |
|---|---|---|
| Secretbox | `sodium_crypto_secretbox()`, `sodium_crypto_secretbox_open()`, `sodium_crypto_secretbox_keygen()` | XSalsa20-Poly1305; byte-compatible with libsodium's `crypto_secretbox_easy` / `_open_easy`. |
| Box | `sodium_crypto_box()`, `_open()`, `_keypair()`, `_seed_keypair()`, `_keypair_from_secretkey_and_publickey()`, `_secretkey()`, `_publickey()`, `_publickey_from_secretkey()`, `_seal()`, `_seal_open()` | Curve25519-XSalsa20-Poly1305 plus anonymous sealed boxes. |
| Sign | `sodium_crypto_sign()`, `_open()`, `_detached()`, `_verify_detached()`, `_keypair()`, `_seed_keypair()`, `_keypair_from_secretkey_and_publickey()`, `_secretkey()`, `_publickey()`, `_publickey_from_secretkey()`, the Ed25519-to-Curve25519 conversions | Ed25519; libsodium's 64-byte `seed \|\| public` secret keys and 96-byte keypairs. |
| Generichash | `sodium_crypto_generichash()`, `_init()`, `_update()`, `_final()`, `_keygen()` | BLAKE2b with 16–64-byte output, 0/16–64-byte keys, and value-semantics streaming states. |
| Auth | `sodium_crypto_auth()`, `_verify()`, `_keygen()` | HMAC-SHA512 truncated to 32 bytes, like libsodium. |
| KDF | `sodium_crypto_kdf_derive_from_key()`, `_keygen()` | BLAKE2b with libsodium's salt/personal layout. |
| KX | `sodium_crypto_kx_keypair()`, `_seed_keypair()`, `_secretkey()`, `_publickey()`, `_client_session_keys()`, `_server_session_keys()` | X25519 plus BLAKE2b-512 session-key derivation. |
| AEAD | `sodium_crypto_aead_chacha20poly1305_*`, `_chacha20poly1305_ietf_*`, `_xchacha20poly1305_ietf_*`, `_aes256gcm_*`, `_aegis128l_*`, `_aegis256_*` | Legacy 8-byte-nonce ChaCha20-Poly1305, IETF, XChaCha20-IETF, AES-256-GCM, and PHP 8.6's AEGIS-128L/AEGIS-256; `_aes256gcm_is_available()` is always true. |
| Secretstream | `sodium_crypto_secretstream_xchacha20poly1305_keygen()`, `_init_push()`, `_init_pull()`, `_push()`, `_pull()`, `_rekey()` | PHP-style numeric state/header arrays and by-reference states. |
| Stream | `sodium_crypto_stream()`, `_xor()`, `_keygen()`, `sodium_crypto_stream_xchacha20()`, `_xor()`, `_xor_ic()`, `_keygen()` | XSalsa20 and XChaCha20 keystreams. |
| Scalarmult | `sodium_crypto_scalarmult()`, `_base()` | X25519 with libsodium's all-zero rejection. |
| Shorthash | `sodium_crypto_shorthash()`, `_keygen()` | SipHash-2-4. |
| Ristretto255 | `sodium_crypto_core_ristretto255_*`, `sodium_crypto_scalarmult_ristretto255*` | Point add/sub/hash/validity, scalar arithmetic/reduction, and scalar multiplication; the scalar complement follows libsodium's `1 - s` convention. |
| Password hash helpers | `sodium_crypto_pwhash_str_needs_rehash()` | Argon2id parameter check; `$7$`/unknown hashes report `true`, matching PHP. |
| Helpers | `sodium_compare()`, `sodium_memcmp()`, `sodium_add()`, `sodium_increment()`, `sodium_memzero()`, `sodium_pad()`, `sodium_unpad()`, `sodium_bin2hex()`, `sodium_hex2bin()`, `sodium_bin2base64()`, `sodium_base642bin()` | The four base64 variants, constant-time comparison, and PHP's `SodiumException` diagnostics. |
| SHAKE one-shot | `sodium_crypto_xof_shake128()`, `sodium_crypto_xof_shake256()` | PHP 8.6; FIPS 202 output. |
| SHAKE state | `sodium_crypto_xof_shake128_init/update/squeeze()`, `sodium_crypto_xof_shake256_init/update/squeeze()` | PHP 8.6; interleaved streaming squeezes; an `update()` after squeezing raises `SodiumException`, matching libsodium. |
| IP conversion | `sodium_bin2ip()`, `sodium_ip2bin()` | PHP 8.6; canonical IPv6 text, dotted IPv4 for the mapped prefix, and 16-byte IPv4-mapped IPv6 binary. |
| IPcrypt | `sodium_crypto_ipcrypt_*`, `sodium_crypto_ipcrypt_nd_*`, `sodium_crypto_ipcrypt_ndx_*`, `sodium_crypto_ipcrypt_pfx_*` | PHP 8.6; plain AES-128, 8-byte-tweak ND, 16-byte-tweak NDX, and format-preserving PFX. Checked against libsodium's IPcrypt vectors. |
| TurboSHAKE | `sodium_crypto_xof_turboshake128/256()`, `_init/update/squeeze()` and the `SodiumCryptoXofTurboshake128State`/`Turboshake256State` classes | PHP 8.6; Keccak-p[1600,12] with streaming squeezes and domain separation. |
| X-Wing KEM | `sodium_crypto_kem_keypair()`, `_seed_keypair()`, `_secretkey()`, `_publickey()`, `_enc()`, `_dec()` | PHP 8.6; hybrid ML-KEM-768 + X25519, libsodium's `secret || public` keypair layout. |
| ML-KEM-768 KEM | `sodium_crypto_kem_mlkem768_keypair()`, `_seed_keypair()`, `_secretkey()`, `_publickey()`, `_enc()`, `_dec()` | PHP 8.6; FIPS 203, expanded 2400-byte secret-key encoding. |
| Password hashing | `sodium_crypto_pwhash()`, `_str()`, `_str_verify()`, the Argon2i/Argon2id aliases, and `sodium_crypto_pwhash_scryptsalsa208sha256()`, `_str()`, `_str_verify()` | Argon2id/i and scrypt-salsa20/poly1305; PHC and `$7$` strings verify against PHP's installed ext/sodium. PHP 8.6 range errors raise `ValueError`. |

The matching `SODIUM_CRYPTO_*` and `SODIUM_BASE64_VARIANT_*` constants are
declared as well.

```php
$key = sodium_crypto_secretbox_keygen();
$nonce = random_bytes(SODIUM_CRYPTO_SECRETBOX_NONCEBYTES);
$cipher = sodium_crypto_secretbox("hello", $nonce, $key);
echo sodium_crypto_secretbox_open($cipher, $nonce, $key), "\n";

$keypair = sodium_crypto_box_keypair();
$sealed = sodium_crypto_box_seal("hello", sodium_crypto_box_publickey($keypair));
echo sodium_crypto_box_seal_open($sealed, $keypair), "\n";
```

`SodiumException` carries php-src's invalid-length and invalid-domain
diagnostics for the delivered functions.

## Ed25519 conversion validation

`sodium_crypto_sign_ed25519_pk_to_curve25519()` rejects non-canonical encodings,
small-order points, and points outside the main subgroup with libsodium's
`SodiumException("conversion failed")`. The accepted/rejected set and the
converted bytes are locked to libsodium vectors for identity, low-order,
non-canonical, and valid seed-derived keys.

## Linking

The prelude carries an `extern "elephc_sodium"` block. A program that names a
delivered `sodium_*` function or `SODIUM_*` constant links
`libelephc_sodium.a` automatically; `--with-sodium` force-injects the prelude and
bridge for indirect calls.
