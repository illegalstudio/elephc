---
title: "sodium_crypto_stream_xchacha20()"
description: "Generates or applies a stream cipher keystream."
---

## sodium_crypto_stream_xchacha20()

```php
function sodium_crypto_stream_xchacha20(int $length, string $nonce, string $key): string
```

Generates or applies a stream cipher keystream.

**Parameters**:
- `$length` (`int`)
- `$nonce` (`string`)
- `$key` (`string`)

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sodium_crypto_stream_xchacha20` is implemented in the compiler, see [the internals page](../../../internals/builtins/sodium/sodium_crypto_stream_xchacha20.md).
