---
title: "sodium_crypto_kdf_derive_from_key()"
description: "Derives a subkey from a master key."
---

## sodium_crypto_kdf_derive_from_key()

```php
function sodium_crypto_kdf_derive_from_key(int $subkey_length, int $subkey_id, string $context, string $key): string
```

Derives a subkey from a master key.

**Parameters**:
- `$subkey_length` (`int`)
- `$subkey_id` (`int`)
- `$context` (`string`)
- `$key` (`string`)

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sodium_crypto_kdf_derive_from_key` is implemented in the compiler, see [the internals page](../../../internals/builtins/sodium/sodium_crypto_kdf_derive_from_key.md).
