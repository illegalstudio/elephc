---
title: "sodium_crypto_secretbox()"
description: "Encrypts a message with a secret key."
---

## sodium_crypto_secretbox()

```php
function sodium_crypto_secretbox(string $message, string $nonce, string $key): string
```

Encrypts a message with a secret key.

**Parameters**:
- `$message` (`string`)
- `$nonce` (`string`)
- `$key` (`string`)

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sodium_crypto_secretbox` is implemented in the compiler, see [the internals page](../../../internals/builtins/sodium/sodium_crypto_secretbox.md).
