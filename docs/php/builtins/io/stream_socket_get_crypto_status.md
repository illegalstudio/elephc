---
title: "stream_socket_get_crypto_status()"
description: "Returns the current TLS handshake or read status of a stream without blocking."
---

## stream_socket_get_crypto_status()

```php
function stream_socket_get_crypto_status(mixed $stream): int
```

Returns the current TLS handshake or read status of a stream without blocking.

**Parameters**:
- `$stream` (`mixed`)

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `stream_socket_get_crypto_status` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_socket_get_crypto_status.md).
