---
title: "sodium_crypto_secretstream_xchacha20poly1305_push()"
description: "Performs a secretstream operation."
---

## sodium_crypto_secretstream_xchacha20poly1305_push()

```php
function sodium_crypto_secretstream_xchacha20poly1305_push(string &$state, string $message, string $additional_data = '', int $tag = 0): string
```

Performs a secretstream operation.

**Parameters**:
- `$state` (`string`), passed by reference
- `$message` (`string`)
- `$additional_data` (`string`), default `''`, optional
- `$tag` (`int`), default `0`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sodium_crypto_secretstream_xchacha20poly1305_push` is implemented in the compiler, see [the internals page](../../../internals/builtins/sodium/sodium_crypto_secretstream_xchacha20poly1305_push.md).
