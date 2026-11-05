---
title: "Sodium"
description: "PHP's ext/sodium cryptography surface over a pinned libsodium 1.0.22, including all 110 functions, 102 constants, by-reference state functions and the SodiumException class."
sidebar:
  order: 26
---

elephc implements PHP's **Sodium** extension by linking the same library php-src
uses: a pinned **libsodium 1.0.22** static archive managed by the native-package
catalog. The PHP-visible surface is the complete php-src 8.5 one — all 110
procedural functions, every `SODIUM_*` constant, the `SodiumException` class,
and the eight by-reference state functions (`sodium_add`, `sodium_increment`,
`sodium_memzero`, `sodium_crypto_generichash_update`/`_final`, and the three
secretstream state functions).

The extension is pay-for-use. A program that never names a `sodium_*` function
or `SodiumException` does not inject the prelude, does not link `libsodium`, and
does not require the managed package.

## Enable Sodium

Detection is automatic when the program calls a `sodium_*` function or names
`SodiumException`. Install the managed package once per target:

```bash
elephc native add libsodium
```

Force the whole surface for dynamic code whose calls are not visible in the
source (an interpreted string or a callable table behind `mixed`):

```bash
elephc --with-sodium program.php
```

A missing package fails closed with the `elephc native add libsodium` recovery;
there is no system `-lsodium` fallback. The iOS targets link the same static
archive and need no extra system framework.

## Example

```php
<?php
$key = sodium_crypto_secretbox_keygen();
$nonce = random_bytes(SODIUM_CRYPTO_SECRETBOX_NONCEBYTES);

$ciphertext = sodium_crypto_secretbox('hello', $nonce, $key);
echo sodium_crypto_secretbox_open($ciphertext, $nonce, $key), "\n";

$hash = sodium_crypto_pwhash_str(
    'correct horse battery staple',
    SODIUM_CRYPTO_PWHASH_OPSLIMIT_INTERACTIVE,
    SODIUM_CRYPTO_PWHASH_MEMLIMIT_INTERACTIVE,
);
var_dump(sodium_crypto_pwhash_str_verify($hash, 'correct horse battery staple'));
```

## Compatibility

- Binary strings are fully supported: every input and output crosses the bridge
  as binary data, never as a NUL-terminated C string.
- The eight by-reference functions mutate their argument exactly like php-src for
  locals and array elements. Object-property targets are refused with a compile-time
  diagnostic because the wrappers use an untyped reference cell to reproduce php-src's
  own zval checks, instead of silently losing the writeback. `sodium_memzero()` wipes
  a uniquely owned string payload before converting the argument to `null`; BLAKE2b
  finalization and secretstream states also reset their reference to `null` after use.
- Argument validation, exception classes, and messages match php-src 8.5,
  including `SodiumException` versus `TypeError` boundaries and the
  `password_hash()` Argon2 and bcrypt string formats.
- `sodium_crypto_pwhash_str()` produces libsodium's `$argon2id$...` format, and
  `sodium_crypto_pwhash_str_verify()` also accepts the `$argon2i$...` hashes
  produced by `password_hash()`.
- Uncaught sodium errors hide the generated prelude helper frames, so secret
  operands do not reach stderr. The two php-src backtrace fixtures compare the
  uncaught exception class and message because their frame numbering is
  target-specific.

The core `password` extension's provider is implemented on top of the same
bridge, so `password_hash()`, `password_verify()`, `password_get_info()`,
`password_needs_rehash()`, `password_algos()` and the `PASSWORD_*` constants are
available whenever the Sodium prelude is loaded. Argon2i and Argon2id use
libsodium's string format and option validation; bcrypt (`2y`) is implemented by
the bridge's Rust provider, so `PASSWORD_DEFAULT` and `password_hash($password)`
work. `PASSWORD_ARGON2_PROVIDER` reports `"sodium"`, matching the libsodium
backed provider. libsodium's Argon2i limit applies (`time_cost >= 3` for Argon2i
at the smallest memory sizes); the php-src sodium fixtures use `time_cost` 4 and 8.

The full 31-case php-src Sodium corpus now runs, including the three
`password_hash()` interoperability fixtures and `bug78516`.

## Inside `eval()`

A host program that linked Sodium registers the compiled prelude helpers in its
eval context, so `eval()` code can call the non-mutating `sodium_*` functions
exactly as compiled code can. The eight by-reference mutators (`sodium_add`,
`sodium_increment`, `sodium_memzero`, the incremental BLAKE2b pair, and the
three secretstream state functions) are ordinary PHP functions with `&$`
parameters. Eval therefore mutates the same local, array element, or property
the AOT call mutates, including through named and dynamic calls. A program that never linked Sodium keeps PHP's catchable
`Error: Call to undefined function sodium_*()` for the whole surface.
