---
title: "stream_is_local()"
description: "Checks if a stream is a local stream."
---

## stream_is_local()

```php
function stream_is_local(resource $stream, mixed $context = null): bool
```

Checks if a stream is a local stream.

**Parameters**:
- `$stream` (`resource`)
- `$context` (`mixed`), default `null`, optional

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `stream_is_local` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_is_local.md).
