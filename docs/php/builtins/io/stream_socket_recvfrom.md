---
title: "stream_socket_recvfrom()"
description: "Lowers `stream_socket_recvfrom(socket, length, flags?, address?)`."
sidebar:
  order: 204
---

## stream_socket_recvfrom()

```php
function stream_socket_recvfrom(resource $socket, int $length, int $flags, string $address): mixed
```

Lowers `stream_socket_recvfrom(socket, length, flags?, address?)`.

**Parameters**:
- `$socket` (`resource`)
- `$length` (`int`)
- `$flags` (`int`), optional
- `$address` (`string`), passed by reference, optional

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_socket_recvfrom` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_socket_recvfrom.md).

