---
title: "sodium_crypto_stream_xor()"
description: "Generates or applies a stream cipher keystream."
---

## sodium_crypto_stream_xor()

```php
function sodium_crypto_stream_xor(string $message, string $nonce, string $key): string
```

Generates or applies a stream cipher keystream.

**Parameters**:
- `$message` (`string`)
- `$nonce` (`string`)
- `$key` (`string`)

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sodium_crypto_stream_xor` is implemented in the compiler, see [the internals page](../../../internals/builtins/sodium/sodium_crypto_stream_xor.md).
