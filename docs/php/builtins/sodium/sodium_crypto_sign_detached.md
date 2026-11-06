---
title: "sodium_crypto_sign_detached()"
description: "Creates a detached signature."
---

## sodium_crypto_sign_detached()

```php
function sodium_crypto_sign_detached(string $message, string $secret_key): string
```

Creates a detached signature.

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

For how `sodium_crypto_sign_detached` is implemented in the compiler, see [the internals page](../../../internals/builtins/sodium/sodium_crypto_sign_detached.md).
