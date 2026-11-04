---
title: "sodium_crypto_secretbox_open()"
description: "Decrypts a secret-key ciphertext."
---

## sodium_crypto_secretbox_open()

```php
function sodium_crypto_secretbox_open(string $ciphertext, string $nonce, string $key): string|false
```

Decrypts a secret-key ciphertext.

**Parameters**:
- `$ciphertext` (`string`)
- `$nonce` (`string`)
- `$key` (`string`)

**Returns**: `string|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sodium_crypto_secretbox_open` is implemented in the compiler, see [the internals page](../../../internals/builtins/sodium/sodium_crypto_secretbox_open.md).
