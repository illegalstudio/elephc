---
title: "sodium_crypto_pwhash_scryptsalsa208sha256_str()"
description: "Hashes a password with scrypt."
---

## sodium_crypto_pwhash_scryptsalsa208sha256_str()

```php
function sodium_crypto_pwhash_scryptsalsa208sha256_str(string $password, int $opslimit, int $memlimit): string
```

Hashes a password with scrypt.

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

For how `sodium_crypto_pwhash_scryptsalsa208sha256_str` is implemented in the compiler, see [the internals page](../../../internals/builtins/sodium/sodium_crypto_pwhash_scryptsalsa208sha256_str.md).
