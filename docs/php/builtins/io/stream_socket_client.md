---
title: "stream_socket_client()"
description: "Lowers `stream_socket_client(address)` and records the connected host for TLS defaults."
sidebar:
  order: 200
---

## stream_socket_client()

```php
function stream_socket_client(string $address, int $error_code, int $error_message, string $timeout, float $flags): mixed
```

Lowers `stream_socket_client(address)` and records the connected host for TLS defaults.

**Parameters**:
- `$address` (`string`)
- `$error_code` (`int`), passed by reference
- `$error_message` (`int`), passed by reference
- `$timeout` (`string`)
- `$flags` (`float`)

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_socket_client` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_socket_client.md).

