---
title: "sodium_crypto_pwhash()"
description: "Derives a key from a password."
---

## sodium_crypto_pwhash()

```php
function sodium_crypto_pwhash(int $length, string $password, string $salt, int $opslimit, int $memlimit, int $algo = 2): string
```

Derives a key from a password.

**Parameters**:
- `$length` (`int`)
- `$password` (`string`)
- `$salt` (`string`)
- `$opslimit` (`int`)
- `$memlimit` (`int`)
- `$algo` (`int`), default `2`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sodium_crypto_pwhash` is implemented in the compiler, see [the internals page](../../../internals/builtins/sodium/sodium_crypto_pwhash.md).
