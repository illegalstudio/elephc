---
title: "sodium_crypto_pwhash_str()"
description: "Hashes a password with Argon2id."
---

## sodium_crypto_pwhash_str()

```php
function sodium_crypto_pwhash_str(string $password, int $opslimit, int $memlimit): string
```

Hashes a password with Argon2id.

**Parameters**:
- `$password` (`string`)
- `$opslimit` (`int`)
- `$memlimit` (`int`)

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sodium_crypto_pwhash_str` is implemented in the compiler, see [the internals page](../../../internals/builtins/sodium/sodium_crypto_pwhash_str.md).
