---
title: "sodium_crypto_stream_xchacha20_xor_ic()"
description: "Generates or applies a stream cipher keystream."
---

## sodium_crypto_stream_xchacha20_xor_ic()

```php
function sodium_crypto_stream_xchacha20_xor_ic(string $message, string $nonce, int $counter, string $key): string
```

Generates or applies a stream cipher keystream.

**Parameters**:
- `$message` (`string`)
- `$nonce` (`string`)
- `$counter` (`int`)
- `$key` (`string`)

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sodium_crypto_stream_xchacha20_xor_ic` is implemented in the compiler, see [the internals page](../../../internals/builtins/sodium/sodium_crypto_stream_xchacha20_xor_ic.md).
