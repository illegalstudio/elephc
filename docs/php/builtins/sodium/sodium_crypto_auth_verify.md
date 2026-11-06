---
title: "sodium_crypto_auth_verify()"
description: "Verifies an authentication tag."
---

## sodium_crypto_auth_verify()

```php
function sodium_crypto_auth_verify(string $mac, string $message, string $key): bool
```

Verifies an authentication tag.

**Parameters**:
- `$mac` (`string`)
- `$message` (`string`)
- `$key` (`string`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sodium_crypto_auth_verify` is implemented in the compiler, see [the internals page](../../../internals/builtins/sodium/sodium_crypto_auth_verify.md).
