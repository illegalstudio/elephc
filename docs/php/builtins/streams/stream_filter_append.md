---
title: "stream_filter_append()"
description: "Attaches a filter to a stream."
---

## stream_filter_append()

```php
function stream_filter_append(resource $stream, string $filtername, int $read_write = 3, mixed $params = null): mixed
```

Attaches a filter to a stream.

**Parameters**:
- `$stream` (`resource`)
- `$filtername` (`string`)
- `$read_write` (`int`), default `3`, optional
- `$params` (`mixed`), default `null`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `stream_filter_append` is implemented in the compiler, see [the internals page](../../../internals/builtins/streams/stream_filter_append.md).
