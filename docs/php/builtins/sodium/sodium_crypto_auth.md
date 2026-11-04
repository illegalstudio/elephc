---
title: "sodium_crypto_auth()"
description: "Computes an authentication tag."
---

## sodium_crypto_auth()

```php
function sodium_crypto_auth(string $message, string $key): string
```

Computes an authentication tag.

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

For how `sodium_crypto_auth` is implemented in the compiler, see [the internals page](../../../internals/builtins/sodium/sodium_crypto_auth.md).
