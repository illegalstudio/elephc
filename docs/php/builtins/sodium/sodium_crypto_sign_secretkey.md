---
title: "sodium_crypto_sign_secretkey()"
description: "Extracts the secret key from a signature key pair."
---

## sodium_crypto_sign_secretkey()

```php
function sodium_crypto_sign_secretkey(string $key_pair): string
```

Extracts the secret key from a signature key pair.

**Parameters**:
- `$key_pair` (`string`)

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sodium_crypto_sign_secretkey` is implemented in the compiler, see [the internals page](../../../internals/builtins/sodium/sodium_crypto_sign_secretkey.md).
