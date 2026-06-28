---
title: "stream_socket_sendto()"
description: "Lowers `stream_socket_sendto(socket, data, flags?, address?)` and boxes `int|false`."
sidebar:
  order: 205
---

## stream_socket_sendto()

```php
function stream_socket_sendto(resource $socket, string $data, int $flags, string $address): mixed
```

Lowers `stream_socket_sendto(socket, data, flags?, address?)` and boxes `int|false`.

**Parameters**:
- `$socket` (`resource`)
- `$data` (`string`)
- `$flags` (`int`), optional
- `$address` (`string`), optional

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_socket_sendto` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_socket_sendto.md).

