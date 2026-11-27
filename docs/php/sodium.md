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
| IP conversion | `sodium_bin2ip()`, `sodium_ip2bin()` | PHP 8.6; canonical IPv6 text and 16-byte IPv4-mapped IPv6 binary. |

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

The rest of `ext/sodium` remains to be delivered, including the base
`crypto_box`/`crypto_sign`/`crypto_generichash`/`crypto_pwhash` families and the
PHP 8.6 IPcrypt-ND/NDX/PFX families, TurboSHAKE128/256, and the X-Wing/ML-KEM768
KEM functions. None of those names are declared yet, so a program that calls
them fails at compile time rather than silently returning a wrong value.

## Linking

The prelude carries an `extern "elephc_sodium"` block. A program that names a
delivered `sodium_*` function or `SODIUM_*` constant links `libelephc_sodium.a`
automatically; `--with-sodium` force-injects the prelude and bridge for indirect
calls.
