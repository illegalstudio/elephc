---
title: "sodium_crypto_pwhash_str_verify()"
description: "Verifies an Argon2id password hash."
---

## sodium_crypto_pwhash_str_verify()

```php
function sodium_crypto_pwhash_str_verify(string $hash, string $password): bool
```

Verifies an Argon2id password hash.

**Parameters**:
- `$hash` (`string`)
- `$password` (`string`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sodium_crypto_pwhash_str_verify` is implemented in the compiler, see [the internals page](../../../internals/builtins/sodium/sodium_crypto_pwhash_str_verify.md).
