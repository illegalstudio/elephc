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
- The by-reference functions mutate their argument exactly like php-src, including
  `sodium_memzero()` and `sodium_crypto_generichash_final()` converting the
  argument to `null` after use.
- Argument validation, exception classes, and messages match php-src 8.5,
  including `SodiumException` versus `TypeError` boundaries and the
  `password_hash()` Argon2 string formats.
- `sodium_crypto_pwhash_str()` produces libsodium's `$argon2id$...` format, and
  `sodium_crypto_pwhash_str_verify()` also accepts the `$argon2i$...` hashes
  produced by `password_hash()`.
- The compiled runtime does not render PHP's call stack, so the two php-src
  fixtures that assert only backtrace redaction compare the uncaught exception
  class and message instead of the omitted frames.

The core `password` extension's Argon2 provider is implemented on top of the
same bridge, so `password_hash()`, `password_verify()`, `password_get_info()`,
`password_needs_rehash()`, `password_algos()` and the `PASSWORD_ARGON2*`
constants are available whenever the Sodium prelude is loaded. The Argon2i /
Argon2id string formats and option validation follow php-src 8.5's Argon2
password provider. libsodium's own Argon2i limit applies (`time_cost >= 3` for Argon2i at
the smallest memory sizes); the php-src sodium fixtures use `time_cost` 4 and 8.
bcrypt (`2y`) is reported by `password_algos()`/`password_get_info()` but not
implemented.

The full 31-case php-src Sodium corpus now runs, including the three
`password_hash()` interoperability fixtures and `bug78516`.

## Inside `eval()`

A host program that linked Sodium registers the compiled prelude helpers in its
eval context, so `eval()` code can call the non-mutating `sodium_*` functions
exactly as compiled code can. The eight by-reference mutators (`sodium_add`,
`sodium_increment`, `sodium_memzero`, the incremental BLAKE2b pair, and the
three secretstream state functions) run through hidden prelude wrappers whose
parameters are ordinary PHP references, so eval mutates the caller's variable
exactly like AOT. A program that never linked Sodium keeps PHP's catchable
`Error: Call to undefined function sodium_*()` for the whole surface.
