---
title: "stream_socket_accept()"
description: "Lowers `stream_socket_accept(server, timeout?, peer_name?)`."
sidebar:
  order: 199
---

## stream_socket_accept()

```php
function stream_socket_accept(resource $socket, float $timeout, string $peer_name): mixed
```

Lowers `stream_socket_accept(server, timeout?, peer_name?)`.

**Parameters**:
- `$socket` (`resource`)
- `$timeout` (`float`), optional
- `$peer_name` (`string`), passed by reference, optional

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_socket_accept` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_socket_accept.md).

