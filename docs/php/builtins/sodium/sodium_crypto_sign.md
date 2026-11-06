---
title: "sodium_crypto_sign()"
description: "Signs a message and returns the signed message."
---

## sodium_crypto_sign()

```php
function sodium_crypto_sign(string $message, string $secret_key): string
```

Signs a message and returns the signed message.

**Parameters**:
- `$message` (`string`)
- `$secret_key` (`string`)

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sodium_crypto_sign` is implemented in the compiler, see [the internals page](../../../internals/builtins/sodium/sodium_crypto_sign.md).
