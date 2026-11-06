---
title: "sodium_crypto_generichash_final()"
description: "Finalizes an incremental BLAKE2b hash state."
---

## sodium_crypto_generichash_final()

```php
function sodium_crypto_generichash_final(string &$state, int $length = 32): string
```

Finalizes an incremental BLAKE2b hash state.

**Parameters**:
- `$state` (`string`), passed by reference
- `$length` (`int`), default `32`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sodium_crypto_generichash_final` is implemented in the compiler, see [the internals page](../../../internals/builtins/sodium/sodium_crypto_generichash_final.md).
