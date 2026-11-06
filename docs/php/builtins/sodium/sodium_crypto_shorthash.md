---
title: "sodium_crypto_shorthash()"
description: "Computes a short hash."
---

## sodium_crypto_shorthash()

```php
function sodium_crypto_shorthash(string $message, string $key): string
```

Computes a short hash.

**Parameters**:
- `$message` (`string`)
- `$key` (`string`)

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sodium_crypto_shorthash` is implemented in the compiler, see [the internals page](../../../internals/builtins/sodium/sodium_crypto_shorthash.md).
