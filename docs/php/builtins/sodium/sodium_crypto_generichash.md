---
title: "sodium_crypto_generichash()"
description: "Computes a BLAKE2b hash."
---

## sodium_crypto_generichash()

```php
function sodium_crypto_generichash(string $message, string $key = '', int $length = 32): string
```

Computes a BLAKE2b hash.

**Parameters**:
- `$message` (`string`)
- `$key` (`string`), default `''`, optional
- `$length` (`int`), default `32`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sodium_crypto_generichash` is implemented in the compiler, see [the internals page](../../../internals/builtins/sodium/sodium_crypto_generichash.md).
