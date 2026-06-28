---
title: "stream_socket_pair()"
description: "Lowers `stream_socket_pair(domain, type, protocol)` and boxes `array|false`."
sidebar:
  order: 203
---

## stream_socket_pair()

```php
function stream_socket_pair(int $domain, int $type, int $protocol): mixed
```

Lowers `stream_socket_pair(domain, type, protocol)` and boxes `array|false`.

**Parameters**:
- `$domain` (`int`)
- `$type` (`int`)
- `$protocol` (`int`)

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_socket_pair` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_socket_pair.md).

