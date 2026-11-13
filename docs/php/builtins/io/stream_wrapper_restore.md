---
title: "stream_wrapper_restore()"
description: "Restores a previously unregistered built-in wrapper."
---

## stream_wrapper_restore()

```php
function stream_wrapper_restore(string $protocol): bool
```

Restores a previously unregistered built-in wrapper.

**Parameters**:
- `$protocol` (`string`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `stream_wrapper_restore` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_wrapper_restore.md).
