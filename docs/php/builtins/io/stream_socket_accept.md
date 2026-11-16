---
title: "stream_socket_accept()"
description: "Accept a connection on a socket created by stream_socket_server()."
---

## stream_socket_accept()

```php
function stream_socket_accept(resource $socket, float $timeout = null, string $peer_name = null): mixed
```

Accept a connection on a socket created by stream_socket_server().

**Parameters**:
- `$socket` (`resource`)
- `$timeout` (`float`), default `null`, optional
- `$peer_name` (`string`), passed by reference, default `null`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `stream_socket_accept` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_socket_accept.md).
