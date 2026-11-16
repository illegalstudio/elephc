---
title: "stream_socket_pair()"
description: "Creates a pair of connected, indistinguishable socket streams."
---

## stream_socket_pair()

```php
function stream_socket_pair(int $domain, int $type, int $protocol): mixed
```

Creates a pair of connected, indistinguishable socket streams.

**Parameters**:
- `$domain` (`int`)
- `$type` (`int`)
- `$protocol` (`int`)

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `stream_socket_pair` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_socket_pair.md).
