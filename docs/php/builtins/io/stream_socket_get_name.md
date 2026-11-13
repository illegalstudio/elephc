---
title: "stream_socket_get_name()"
description: "Retrieve the name of the local or remote sockets."
---

## stream_socket_get_name()

```php
function stream_socket_get_name(resource $socket, bool $remote): mixed
```

Retrieve the name of the local or remote sockets.

**Parameters**:
- `$socket` (`resource`)
- `$remote` (`bool`)

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `stream_socket_get_name` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_socket_get_name.md).
