---
title: "stream_socket_server()"
description: "Lowers `stream_socket_server(address)` and boxes `resource|false`."
sidebar:
  order: 206
---

## stream_socket_server()

```php
function stream_socket_server(string $address, int $error_code, int $error_message): mixed
```

Lowers `stream_socket_server(address)` and boxes `resource|false`.

**Parameters**:
- `$address` (`string`)
- `$error_code` (`int`), passed by reference
- `$error_message` (`int`), passed by reference

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_socket_server` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_socket_server.md).

