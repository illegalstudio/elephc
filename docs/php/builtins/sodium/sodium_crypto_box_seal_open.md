---
title: "sodium_crypto_box_seal_open()"
description: "Decrypts a sealed box."
---

## sodium_crypto_box_seal_open()

```php
function sodium_crypto_box_seal_open(string $ciphertext, string $key_pair): string|false
```

Decrypts a sealed box.

**Parameters**:
- `$ciphertext` (`string`)
- `$key_pair` (`string`)

**Returns**: `string|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sodium_crypto_box_seal_open` is implemented in the compiler, see [the internals page](../../../internals/builtins/sodium/sodium_crypto_box_seal_open.md).
