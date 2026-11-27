---
title: "Sodium"
description: "PHP's ext/sodium surface in elephc: secretbox, SHAKE XOFs, IP conversion, and the pure-Rust elephc-sodium bridge."
sidebar:
  order: 24
---

elephc exposes a growing subset of `ext/sodium` through an injected PHP prelude
backed by the pure-Rust `elephc-sodium` bridge. The bridge implements the
NaCl/libsodium primitives directly, so it needs no system `libsodium` and works
on every supported target.

## Delivered surface

| Area | Functions | Notes |
|---|---|---|
| Secretbox | `sodium_crypto_secretbox()`, `sodium_crypto_secretbox_open()`, `sodium_crypto_secretbox_keygen()` | XSalsa20-Poly1305; byte-compatible with libsodium's `crypto_secretbox_easy` / `_open_easy`. |
| SHAKE one-shot | `sodium_crypto_xof_shake128()`, `sodium_crypto_xof_shake256()` | PHP 8.6; FIPS 202 output. |
| SHAKE state | `sodium_crypto_xof_shake128_init/update/squeeze()`, `sodium_crypto_xof_shake256_init/update/squeeze()` | PHP 8.6; interleaved streaming squeezes; an `update()` after squeezing raises `SodiumException`, matching libsodium. |
| IP conversion | `sodium_bin2ip()`, `sodium_ip2bin()` | PHP 8.6; canonical IPv6 text, dotted IPv4 for the mapped prefix, and 16-byte IPv4-mapped IPv6 binary. |
| IPcrypt | `sodium_crypto_ipcrypt_*`, `sodium_crypto_ipcrypt_nd_*`, `sodium_crypto_ipcrypt_ndx_*`, `sodium_crypto_ipcrypt_pfx_*` | PHP 8.6; plain AES-128, 8-byte-tweak ND, 16-byte-tweak NDX, and format-preserving PFX. Checked against libsodium's IPcrypt vectors. |
| TurboSHAKE | `sodium_crypto_xof_turboshake128/256()`, `_init/update/squeeze()` and the `SodiumCryptoXofTurboshake128State`/`Turboshake256State` classes | PHP 8.6; Keccak-p[1600,12] with streaming squeezes and domain separation. |
| X-Wing KEM | `sodium_crypto_kem_keypair()`, `_seed_keypair()`, `_secretkey()`, `_publickey()`, `_enc()`, `_dec()` | PHP 8.6; hybrid ML-KEM-768 + X25519, libsodium's `secret || public` keypair layout. |
| ML-KEM-768 KEM | `sodium_crypto_kem_mlkem768_keypair()`, `_seed_keypair()`, `_secretkey()`, `_publickey()`, `_enc()`, `_dec()` | PHP 8.6; FIPS 203, expanded 2400-byte secret-key encoding. |
| Password hashing | `sodium_crypto_pwhash()`, `_str()`, `_str_verify()`, the Argon2i/Argon2id aliases, and `sodium_crypto_pwhash_scryptsalsa208sha256()`, `_str()`, `_str_verify()` | Argon2id/i and scrypt-salsa20/poly1305; PHC and `$7$` strings verify against PHP's installed ext/sodium. PHP 8.6 range errors raise `ValueError`. |

The matching `SODIUM_CRYPTO_SECRETBOX_*` and `SODIUM_CRYPTO_XOF_SHAKE*` constants
are declared as well.

```php
$key = sodium_crypto_secretbox_keygen();
$nonce = random_bytes(SODIUM_CRYPTO_SECRETBOX_NONCEBYTES);
$cipher = sodium_crypto_secretbox("hello", $nonce, $key);
echo sodium_crypto_secretbox_open($cipher, $nonce, $key), "\n";
```

`SodiumException` carries php-src's invalid-length and invalid-domain
diagnostics for the delivered functions.

## Deferred ext/sodium surface

The remaining `ext/sodium` surface is the base families that predate PHP 8.6:
`crypto_box`, `crypto_sign`, `crypto_generichash`, `crypto_kdf`, `crypto_kx`,
`crypto_auth`, `crypto_aead`, the secretstream helpers, and the
`pwhash_str_needs_rehash` family. None of those names are declared, so a
program that calls them fails at compile time rather than silently returning a
wrong value.

## Linking

The prelude carries an `extern "elephc_sodium"` block. A program that names a
delivered `sodium_*` function or `SODIUM_*` constant links `libelephc_sodium.a`
automatically; `--with-sodium` force-injects the prelude and bridge for indirect
calls.
