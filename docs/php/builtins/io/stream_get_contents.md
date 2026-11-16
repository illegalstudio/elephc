---
title: "stream_get_contents()"
description: "Reads remainder of a stream into a string."
---

## stream_get_contents()

```php
function stream_get_contents(resource $stream, ?int $length = null, int $offset = -1): mixed
```

Reads remainder of a stream into a string.

**Parameters**:
- `$stream` (`resource`)
- `$length` (`?int`), default `null`, optional
- `$offset` (`int`), default `-1`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `stream_get_contents` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_get_contents.md).
