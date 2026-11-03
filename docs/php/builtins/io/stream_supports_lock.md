---
title: "stream_supports_lock()"
description: "Tells whether the stream supports locking."
---

## stream_supports_lock()

```php
function stream_supports_lock(resource $stream): bool
```

Tells whether the stream supports locking.

**Parameters**:
- `$stream` (`resource`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `stream_supports_lock` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_supports_lock.md).
