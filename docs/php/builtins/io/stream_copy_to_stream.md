---
title: "stream_copy_to_stream()"
description: "Copies data from one stream to another."
---

## stream_copy_to_stream()

```php
function stream_copy_to_stream(resource $from, resource $to, ?int $length = null, int $offset = -1, mixed $context = null): mixed
```

Copies data from one stream to another.

**Parameters**:
- `$from` (`resource`)
- `$to` (`resource`)
- `$length` (`?int`), default `null`, optional
- `$offset` (`int`), default `-1`, optional
- `$context` (`mixed`), default `null`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `stream_copy_to_stream` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_copy_to_stream.md).
