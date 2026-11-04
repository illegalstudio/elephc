---
title: "sodium_crypto_generichash_init()"
description: "Initializes an incremental BLAKE2b hash state."
---

## sodium_crypto_generichash_init()

```php
function sodium_crypto_generichash_init(string $key = '', int $length = 32): string
```

Initializes an incremental BLAKE2b hash state.

**Parameters**:
- `$key` (`string`), default `''`, optional
- `$length` (`int`), default `32`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sodium_crypto_generichash_init` is implemented in the compiler, see [the internals page](../../../internals/builtins/sodium/sodium_crypto_generichash_init.md).
