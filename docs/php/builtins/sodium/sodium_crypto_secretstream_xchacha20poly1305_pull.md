---
title: "sodium_crypto_secretstream_xchacha20poly1305_pull()"
description: "Performs a secretstream operation."
---

## sodium_crypto_secretstream_xchacha20poly1305_pull()

```php
function sodium_crypto_secretstream_xchacha20poly1305_pull(string &$state, string $ciphertext, string $additional_data = ''): array|false
```

Performs a secretstream operation.

**Parameters**:
- `$state` (`string`), passed by reference
- `$ciphertext` (`string`)
- `$additional_data` (`string`), default `''`, optional

**Returns**: `array|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sodium_crypto_secretstream_xchacha20poly1305_pull` is implemented in the compiler, see [the internals page](../../../internals/builtins/sodium/sodium_crypto_secretstream_xchacha20poly1305_pull.md).
