---
title: "sodium_crypto_box_open()"
description: "Decrypts a box ciphertext."
---

## sodium_crypto_box_open()

```php
function sodium_crypto_box_open(string $ciphertext, string $nonce, string $key_pair): string|false
```

Decrypts a box ciphertext.

**Parameters**:
- `$ciphertext` (`string`)
- `$nonce` (`string`)
- `$key_pair` (`string`)

**Returns**: `string|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sodium_crypto_box_open` is implemented in the compiler, see [the internals page](../../../internals/builtins/sodium/sodium_crypto_box_open.md).
