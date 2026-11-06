---
title: "sodium_crypto_aead_chacha20poly1305_decrypt()"
description: "Encrypts or decrypts with an AEAD cipher."
---

## sodium_crypto_aead_chacha20poly1305_decrypt()

```php
function sodium_crypto_aead_chacha20poly1305_decrypt(string $ciphertext, string $additional_data, string $nonce, string $key): string|false
```

Encrypts or decrypts with an AEAD cipher.

**Parameters**:
- `$ciphertext` (`string`)
- `$additional_data` (`string`)
- `$nonce` (`string`)
- `$key` (`string`)

**Returns**: `string|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sodium_crypto_aead_chacha20poly1305_decrypt` is implemented in the compiler, see [the internals page](../../../internals/builtins/sodium/sodium_crypto_aead_chacha20poly1305_decrypt.md).
