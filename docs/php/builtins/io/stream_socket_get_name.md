---
title: "stream_socket_get_name()"
description: "Lowers `stream_socket_get_name(socket, remote)` and boxes `string|false`."
sidebar:
  order: 202
---

## stream_socket_get_name()

```php
function stream_socket_get_name(resource $socket, bool $remote): mixed
```

Lowers `stream_socket_get_name(socket, remote)` and boxes `string|false`.

**Parameters**:
- `$socket` (`resource`)
- `$remote` (`bool`)

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_socket_get_name` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_socket_get_name.md).

