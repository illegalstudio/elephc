---
title: "sodium_crypto_aead_aegis256_encrypt()"
description: "Encrypts or decrypts with an AEAD cipher."
---

## sodium_crypto_aead_aegis256_encrypt()

```php
function sodium_crypto_aead_aegis256_encrypt(string $message, string $additional_data, string $nonce, string $key): string
```

Encrypts or decrypts with an AEAD cipher.

**Parameters**:
- `$message` (`string`)
- `$additional_data` (`string`)
- `$nonce` (`string`)
- `$key` (`string`)

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sodium_crypto_aead_aegis256_encrypt` is implemented in the compiler, see [the internals page](../../../internals/builtins/sodium/sodium_crypto_aead_aegis256_encrypt.md).
