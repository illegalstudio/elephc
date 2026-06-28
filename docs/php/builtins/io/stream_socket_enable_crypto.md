---
title: "stream_socket_enable_crypto()"
description: "Lowers `stream_socket_enable_crypto(stream, enable, method?, session_stream?)`."
sidebar:
  order: 201
---

## stream_socket_enable_crypto()

```php
function stream_socket_enable_crypto(resource $stream, bool $enable, int $crypto_method, resource $session_stream): bool
```

Lowers `stream_socket_enable_crypto(stream, enable, method?, session_stream?)`.

**Parameters**:
- `$stream` (`resource`)
- `$enable` (`bool`)
- `$crypto_method` (`int`), optional
- `$session_stream` (`resource`), optional

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_socket_enable_crypto` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_socket_enable_crypto.md).

