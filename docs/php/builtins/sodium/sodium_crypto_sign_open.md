---
title: "sodium_crypto_sign_open()"
description: "Verifies a signed message and returns its contents."
---

## sodium_crypto_sign_open()

```php
function sodium_crypto_sign_open(string $signed_message, string $public_key): string|false
```

Verifies a signed message and returns its contents.

**Parameters**:
- `$signed_message` (`string`)
- `$public_key` (`string`)

**Returns**: `string|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sodium_crypto_sign_open` is implemented in the compiler, see [the internals page](../../../internals/builtins/sodium/sodium_crypto_sign_open.md).
