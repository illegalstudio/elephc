---
title: "sodium_crypto_generichash_update()"
description: "Updates an incremental BLAKE2b hash state."
---

## sodium_crypto_generichash_update()

```php
function sodium_crypto_generichash_update(string &$state, string $message): bool
```

Updates an incremental BLAKE2b hash state.

**Parameters**:
- `$state` (`string`), passed by reference
- `$message` (`string`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sodium_crypto_generichash_update` is implemented in the compiler, see [the internals page](../../../internals/builtins/sodium/sodium_crypto_generichash_update.md).
