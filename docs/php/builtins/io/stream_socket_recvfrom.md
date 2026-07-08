---
title: "stream_socket_recvfrom()"
description: "Receives data from a socket, connected or not."
sidebar:
  order: 221
---

## stream_socket_recvfrom()

```php
function stream_socket_recvfrom(resource $socket, int $length, int $flags = 0, string $address = ''): mixed
```

Receives data from a socket, connected or not.

**Parameters**:
- `$socket` (`resource`)
- `$length` (`int`)
- `$flags` (`int`), default `0`, optional
- `$address` (`string`), passed by reference, default `''`, optional

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_socket_recvfrom` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_socket_recvfrom.md).

