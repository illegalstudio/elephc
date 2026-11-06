---
title: "stream_set_blocking()"
description: "Sets blocking/non-blocking mode on a stream."
---

## stream_set_blocking()

```php
function stream_set_blocking(resource $stream, bool $enable): bool
```

Sets blocking/non-blocking mode on a stream.

**Parameters**:
- `$stream` (`resource`)
- `$enable` (`bool`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `stream_set_blocking` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_set_blocking.md).
