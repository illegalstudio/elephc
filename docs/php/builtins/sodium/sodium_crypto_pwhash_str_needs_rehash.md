---
title: "sodium_crypto_pwhash_str_needs_rehash()"
description: "Reports whether a password hash needs rehashing."
---

## sodium_crypto_pwhash_str_needs_rehash()

```php
function sodium_crypto_pwhash_str_needs_rehash(string $password, int $opslimit, int $memlimit): bool
```

Reports whether a password hash needs rehashing.

**Parameters**:
- `$password` (`string`)
- `$opslimit` (`int`)
- `$memlimit` (`int`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sodium_crypto_pwhash_str_needs_rehash` is implemented in the compiler, see [the internals page](../../../internals/builtins/sodium/sodium_crypto_pwhash_str_needs_rehash.md).
