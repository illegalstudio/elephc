---
title: "sodium_crypto_sign_publickey_from_secretkey()"
description: "Derives a signature public key from a secret key."
---

## sodium_crypto_sign_publickey_from_secretkey()

```php
function sodium_crypto_sign_publickey_from_secretkey(string $secret_key): string
```

Derives a signature public key from a secret key.

**Parameters**:
- `$secret_key` (`string`)

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sodium_crypto_sign_publickey_from_secretkey` is implemented in the compiler, see [the internals page](../../../internals/builtins/sodium/sodium_crypto_sign_publickey_from_secretkey.md).
