---
title: "stream_socket_sendto()"
description: "Sends a message to a socket, whether it is connected or not."
sidebar:
  order: 222
---

## stream_socket_sendto()

```php
function stream_socket_sendto(resource $socket, string $data, int $flags = 0, string $address = ''): mixed
```

Sends a message to a socket, whether it is connected or not.

**Parameters**:
- `$socket` (`resource`)
- `$data` (`string`)
- `$flags` (`int`), default `0`, optional
- `$address` (`string`), default `''`, optional

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_socket_sendto` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_socket_sendto.md).

