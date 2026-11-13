---
title: "stream_set_timeout()"
description: "Sets timeout period on a stream."
---

## stream_set_timeout()

```php
function stream_set_timeout(resource $stream, int $seconds, int $microseconds = 0): bool
```

Sets timeout period on a stream.

**Parameters**:
- `$stream` (`resource`)
- `$seconds` (`int`)
- `$microseconds` (`int`), default `0`, optional

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `stream_set_timeout` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_set_timeout.md).
