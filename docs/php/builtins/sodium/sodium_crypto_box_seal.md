---
title: "sodium_crypto_box_seal()"
description: "Encrypts a message with an anonymous sealed box."
---

## sodium_crypto_box_seal()

```php
function sodium_crypto_box_seal(string $message, string $public_key): string
```

Encrypts a message with an anonymous sealed box.

**Parameters**:
- `$message` (`string`)
- `$public_key` (`string`)

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sodium_crypto_box_seal` is implemented in the compiler, see [the internals page](../../../internals/builtins/sodium/sodium_crypto_box_seal.md).
