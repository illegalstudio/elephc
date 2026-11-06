---
title: "sodium_crypto_sign_verify_detached()"
description: "Verifies a detached signature."
---

## sodium_crypto_sign_verify_detached()

```php
function sodium_crypto_sign_verify_detached(string $signature, string $message, string $public_key): bool
```

Verifies a detached signature.

**Parameters**:
- `$signature` (`string`)
- `$message` (`string`)
- `$public_key` (`string`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sodium_crypto_sign_verify_detached` is implemented in the compiler, see [the internals page](../../../internals/builtins/sodium/sodium_crypto_sign_verify_detached.md).
