---
title: "stream_socket_shutdown()"
description: "Shutdown a full-duplex connection."
---

## stream_socket_shutdown()

```php
function stream_socket_shutdown(resource $stream, int $mode): bool
```

Shutdown a full-duplex connection.

**Parameters**:
- `$stream` (`resource`)
- `$mode` (`int`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `stream_socket_shutdown` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_socket_shutdown.md).
