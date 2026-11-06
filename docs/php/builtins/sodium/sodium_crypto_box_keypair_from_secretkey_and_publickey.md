---
title: "sodium_crypto_box_keypair_from_secretkey_and_publickey()"
description: "Builds a box key pair from its two halves."
---

## sodium_crypto_box_keypair_from_secretkey_and_publickey()

```php
function sodium_crypto_box_keypair_from_secretkey_and_publickey(string $secret_key, string $public_key): string
```

Builds a box key pair from its two halves.

**Parameters**:
- `$secret_key` (`string`)
- `$public_key` (`string`)

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sodium_crypto_box_keypair_from_secretkey_and_publickey` is implemented in the compiler, see [the internals page](../../../internals/builtins/sodium/sodium_crypto_box_keypair_from_secretkey_and_publickey.md).
