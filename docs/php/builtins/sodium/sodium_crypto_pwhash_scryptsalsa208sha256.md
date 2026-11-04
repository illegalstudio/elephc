---
title: "sodium_crypto_pwhash_scryptsalsa208sha256()"
description: "Derives a key with scrypt."
---

## sodium_crypto_pwhash_scryptsalsa208sha256()

```php
function sodium_crypto_pwhash_scryptsalsa208sha256(int $length, string $password, string $salt, int $opslimit, int $memlimit): string
```

Derives a key with scrypt.

**Parameters**:
- `$length` (`int`)
- `$password` (`string`)
- `$salt` (`string`)
- `$opslimit` (`int`)
- `$memlimit` (`int`)

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sodium_crypto_pwhash_scryptsalsa208sha256` is implemented in the compiler, see [the internals page](../../../internals/builtins/sodium/sodium_crypto_pwhash_scryptsalsa208sha256.md).
