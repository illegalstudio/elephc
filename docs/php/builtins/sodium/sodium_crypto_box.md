---
title: "sodium_crypto_box()"
description: "Encrypts a message with a box key pair."
---

## sodium_crypto_box()

```php
function sodium_crypto_box(string $message, string $nonce, string $key_pair): string
```

Encrypts a message with a box key pair.

**Parameters**:
- `$message` (`string`)
- `$nonce` (`string`)
- `$key_pair` (`string`)

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sodium_crypto_box` is implemented in the compiler, see [the internals page](../../../internals/builtins/sodium/sodium_crypto_box.md).
