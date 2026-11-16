---
title: "stream_isatty()"
description: "Checks if a stream is a TTY."
---

## stream_isatty()

```php
function stream_isatty(resource $stream): bool
```

Checks if a stream is a TTY.

**Parameters**:
- `$stream` (`resource`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `stream_isatty` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_isatty.md).
